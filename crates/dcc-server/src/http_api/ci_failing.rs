//! A trigger any agent can have on, from the server: the checks of a task's
//! pull request failed (`docs/PLANO_MAQUINA_SEM_TELA`, slice 5b). The pull
//! requests' search (`pr_watch`) already lists the person's own failing pull
//! requests with their task, so no `gh` is added to notice it. Each failure
//! (pull request + commit) wakes each agent watching for it once, where it
//! works and its ceilings allow (`agent_triggers`): it reads the failing
//! checks and, in its own conversation in that task, says what broke and
//! the smallest fix, read-only, so who fixes it stays the person's call.
//! A failure outside the agent's projects, or held by a ceiling, is only
//! marked seen. It ran in the native app's window (`ci_trigger`).

use dcc_core::domain::agent::ResidentAgent;
use dcc_core::domain::delivery_failure_prompt::{
    build_delivery_failure_prompt, DeliveryFailurePrompt,
};
use dcc_infra::db::SqliteSessionRepo;
use dcc_tauri::agent_triggers::Trigger;
use dcc_tauri::commands::pr_hub_failing::PullRequestHubFailing;

use super::room_feed::read_team;
use super::*;

/// What the agent is asked; `checks` is DCC's read of the failure.
pub(super) fn diagnosis_prompt(pr: &PullRequestHubFailing, checks: Option<&str>) -> String {
    let sha: String = pr.head_sha.chars().take(7).collect();
    let checks = checks.map(str::trim).filter(|checks| !checks.is_empty());
    let evidence = match checks {
        Some(checks) => format!("What DCC read of the failing checks:\n\n{checks}"),
        None => "DCC could not read the failing checks' logs; work from the task's code and its latest changes.".to_string(),
    };
    format!(
        "[DCC] The checks of this task's pull request {}#{} failed on {sha} (branch {}).\n\nFind the cause: say what broke, where, and the smallest fix. Work read-only: do not edit files, commit or push; the person decides who fixes it. Answer in the person's language, short.\n\n{evidence}",
        pr.repository, pr.number, pr.head_branch
    )
}

/// The failure DCC read in the task's folder, as "Enviar ao agente" words it.
fn failure_read(snapshot: &Value) -> Option<String> {
    let text = |key: &str| snapshot.get(key).and_then(Value::as_str);
    let changed: Vec<String> = snapshot
        .get("changedFiles")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let push = snapshot.get("pushTarget").and_then(|target| {
        Some((
            target.get("remote")?.as_str()?,
            target.get("branch")?.as_str()?,
        ))
    });
    Some(build_delivery_failure_prompt(&DeliveryFailurePrompt {
        operation: text("operation")?,
        classification: text("classification")?,
        branch: text("branch"),
        head_sha: text("headSha"),
        remote: text("remote"),
        operation_target: text("operationTarget"),
        push_target: push,
        created_at: text("createdAt").unwrap_or_default(),
        attempt_token: text("attemptToken").unwrap_or_default(),
        output: text("output").unwrap_or_default(),
        changed_files: &changed,
        changed_files_truncated: snapshot
            .get("changedFilesTruncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }))
}

/// Reads the pipeline again (it records the failure) and then the failure.
async fn read_checks(backend: &HttpBackend, path: &str) -> Option<String> {
    let pipeline =
        serde_json::from_value(json!({ "workspaceRoot": path, "forgeLogin": null })).ok()?;
    let _ = dcc_tauri::commands::forge_commands::workspace_pipeline_status_with_state(
        &backend.workspace,
        pipeline,
    )
    .await;
    let input = serde_json::from_value(json!({ "workspaceRoot": path })).ok()?;
    let read = dcc_tauri::delivery_failure::workspace_delivery_failure_snapshot_with_state(
        &backend.workspace,
        input,
    )
    .await
    .ok()?;
    let snapshot = serde_json::to_value(read).ok()?;
    failure_read(snapshot.get("snapshot")?)
}

/// The failing pull requests of this search, for every agent watching.
pub(super) async fn react(backend: &Arc<HttpBackend>, failing: &[PullRequestHubFailing]) {
    let db_path = backend.workspace.db_path.clone();
    let agents = tokio::task::spawn_blocking(move || {
        let repo =
            SqliteSessionRepo::open_read_only(&db_path).map_err(|error| error.to_string())?;
        Ok::<_, String>(read_team(&repo)?.agents)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|read| read);
    let agents = match agents {
        Ok(agents) => agents,
        Err(error) => {
            eprintln!("[DCC][ci-failed] agents unreadable: {error}");
            return;
        }
    };
    let watching: Vec<ResidentAgent> = agents
        .into_iter()
        .filter(|agent| {
            super::agent_clock::with_triggers(backend, |triggers| {
                triggers.is_on(&agent.id, Trigger::CiFailed)
            })
            .unwrap_or(false)
        })
        .collect();
    let urls: Vec<&str> = failing.iter().map(|pr| pr.url.as_str()).collect();
    for agent in &watching {
        super::agent_clock::with_triggers(backend, |triggers| {
            triggers.keep_failures(&agent.id, &urls)
        });
        for pr in failing {
            let (Some(task), Some(project), Some(path)) = (
                pr.workspace_id.clone(),
                pr.project_id.clone(),
                pr.workspace_path.clone(),
            ) else {
                continue;
            };
            let now = chrono::Utc::now();
            // Looked at, or held: either way this failure is done.
            let allowed = super::agent_clock::with_triggers(backend, |triggers| {
                if !triggers.is_new_failure(&agent.id, &pr.url, &pr.head_sha) {
                    return None;
                }
                let allowed = agent.works_in(&project)
                    && triggers
                        .check(&agent.id, Trigger::CiFailed, &task, now)
                        .is_ok();
                triggers.mark_failure(&agent.id, &pr.url, &pr.head_sha);
                if allowed {
                    triggers.record(&agent.id, Trigger::CiFailed, &task, now);
                }
                Some(allowed)
            })
            .flatten();
            if allowed != Some(true) {
                continue;
            }
            let (backend, agent, pr) = (backend.clone(), agent.clone(), pr.clone());
            crate::runtime::spawn(async move {
                let checks = read_checks(&backend, &path).await;
                let prompt = diagnosis_prompt(&pr, checks.as_deref());
                if let Err(error) =
                    super::agent_task_ask::ask_in_task(&backend, &agent, &task, &project, &prompt)
                        .await
                {
                    eprintln!("[DCC][ci-failed] {}#{}: {error}", pr.repository, pr.number);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_asked_read_only_with_what_dcc_read() {
        let pr = PullRequestHubFailing {
            url: "https://github.com/acme/app/pull/7".into(),
            number: 7,
            repository: "acme/app".into(),
            title: String::new(),
            head_branch: "feat/rastreio".into(),
            head_sha: "abcdef0123".into(),
            workspace_id: None,
            project_id: None,
            workspace_path: None,
        };
        let prompt = diagnosis_prompt(&pr, Some("## Failing: build\nerror[E0308]"));
        assert!(prompt.starts_with(
            "[DCC] The checks of this task's pull request acme/app#7 failed on abcdef0"
        ));
        assert!(prompt.contains("read-only"));
        assert!(prompt.contains("error[E0308]"));
        assert!(diagnosis_prompt(&pr, None).contains("could not read"));
    }

    #[test]
    fn a_failure_reads_as_send_to_agent_words_it() {
        let snapshot = json!({
            "operation": "pipeline",
            "classification": "checks-failed",
            "branch": "feat/x",
            "output": "error[E0308]: mismatched types",
            "changedFiles": ["src/main.rs"],
        });
        let read = failure_read(&snapshot).unwrap();
        assert!(read.contains("error[E0308]"), "{read}");
        assert!(failure_read(&json!({})).is_none());
    }
}
