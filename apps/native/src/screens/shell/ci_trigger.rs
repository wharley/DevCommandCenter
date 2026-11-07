//! The Reviewer's second trigger: the checks of a task's pull request
//! failed. The same search `PrWatch` already makes lists the person's own
//! failing pull requests with their task (`pr_hub_failing`), so no `gh` is
//! added to notice it. Each failure (pull request + commit) wakes the
//! Reviewer once, when the person turned this on and its ceilings allow
//! (`AgentTriggers`): it reads the failing checks (the code column's "fix
//! checks" read) and, in its own conversation in that task, says what broke
//! and the smallest fix — read-only, so who fixes it stays the person's call.

use chrono::Utc;
use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{App, BorrowAppContext, Context};

use super::Shell;
use super::review_trigger::review_run;
use super::reviewer_in_task::TaskAsk;
use crate::data::agent_triggers::{AgentTriggers, Trigger};
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_hub::Failing;
use crate::i18n::{Args, Translate};

/// What the Reviewer is asked; `checks` is DCC's read of the failure.
pub(super) fn diagnosis_prompt(pr: &Failing, checks: Option<&str>) -> String {
    let sha: String = pr.head_sha.chars().take(7).collect();
    let checks = checks.map(str::trim).filter(|checks| !checks.is_empty());
    let evidence = match checks {
        Some(checks) => format!("What DCC read of the failing checks:\n\n{checks}"),
        None => "DCC could not read the failing checks' logs; work from the task's code and its latest changes.".to_string(),
    };
    format!(
        "[DCC] The checks of this task's pull request {}#{} failed on {sha} (branch {}).\n\nAs the Reviewer, find the cause: say what broke, where, and the smallest fix. Work read-only: do not edit files, commit or push; the person decides who fixes it. Answer in the person's language, short.\n\n{evidence}",
        pr.repository, pr.number, pr.head_branch
    )
}

fn announce(reviewer: &str, pr: &Failing, done: &Result<(), String>, cx: &App) {
    let title = cx.t_with(
        "agents.triggers.ciFailed.done",
        &Args::default().with("agent", reviewer.to_string()),
    );
    let what = match done {
        Ok(()) => cx.t("agents.triggers.ciFailed.ready").to_string(),
        Err(_) => cx.t("agents.triggers.ciFailed.failed").to_string(),
    };
    let body = format!("{} #{} · {what}", pr.repository, pr.number);
    crate::ui::attention::notify_away(&title, &body);
}

impl Shell {
    /// The failing pull requests of this search: the Reviewer looks at each
    /// new failure of a task its trigger, its ceilings and its target allow.
    pub(super) fn checks_failing(&mut self, failing: &[Failing], cx: &mut Context<Self>) {
        let Some(reviewer) = self.sidebar.read(cx).reviewer() else {
            return;
        };
        if !cx.has_global::<AgentTriggers>()
            || !cx
                .global::<AgentTriggers>()
                .is_on(&reviewer.id, Trigger::CiFailed)
        {
            return;
        }
        let urls: Vec<&str> = failing.iter().map(|pr| pr.url.as_str()).collect();
        cx.update_global::<AgentTriggers, _>(|triggers, _| {
            triggers.keep_failures(&reviewer.id, &urls)
        });
        let composer = self.conversation.read(cx).composer.read(cx);
        let providers: std::sync::Arc<Vec<ProviderDescriptor>> =
            composer.catalog.clone().unwrap_or_default();
        let pick = (
            composer.session.provider_id.clone(),
            composer.session.model.clone(),
        );
        for pr in failing {
            let (Some(task), Some(project), Some(path)) = (
                pr.workspace_id.clone(),
                pr.project_id.clone(),
                pr.workspace_path.clone(),
            ) else {
                continue;
            };
            let triggers = cx.global::<AgentTriggers>();
            if !triggers.is_new_failure(&reviewer.id, &pr.url, &pr.head_sha) {
                continue;
            }
            let now = Utc::now();
            // Outside the agent's projects the failure is only marked seen.
            let allowed = if reviewer.works_in(&project) {
                triggers
                    .check(&reviewer.id, Trigger::CiFailed, &task, now)
                    .map_err(|_| ())
            } else {
                Err(())
            };
            let Some(run) = review_run(&reviewer, &providers, &pick, &path, String::new()) else {
                continue;
            };
            // Looked at, or held by a ceiling: either way this failure is done.
            cx.update_global::<AgentTriggers, _>(|triggers, _| {
                triggers.mark_failure(&reviewer.id, &pr.url, &pr.head_sha);
                if allowed.is_ok() {
                    triggers.record(&reviewer.id, Trigger::CiFailed, &task, now);
                }
            });
            if allowed.is_err() {
                continue;
            }
            let ask = TaskAsk::new(&reviewer, &task, &project, run);
            let (name, pr) = (reviewer.name.clone(), pr.clone());
            cx.spawn(async move |_, cx| {
                let done = cx
                    .background_executor()
                    .spawn({
                        let pr = pr.clone();
                        async move {
                            let api = DesktopApi::load()
                                .ok_or_else(|| "DCC is not running".to_string())?;
                            let _ = api.pipeline(&path, None);
                            let failure = api.delivery_failure(&path).ok().flatten();
                            let prompt = diagnosis_prompt(
                                &pr,
                                failure
                                    .as_ref()
                                    .map(|failure| failure.agent_prompt())
                                    .as_deref(),
                            );
                            ask.ask(&api, &prompt)
                        }
                    })
                    .await;
                let _ = cx.update(|cx| announce(&name, &pr, &done, cx));
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::reviewer_in_task::conversation_in;
    use super::*;
    use crate::data::agents::{AgentRecord, AgentSession, AgentState};

    #[test]
    fn the_reviewer_is_asked_read_only_in_its_one_conversation_of_the_task() {
        let pr = Failing {
            url: "https://github.com/acme/app/pull/7".into(),
            number: 7,
            repository: "acme/app".into(),
            head_branch: "feat/rastreio".into(),
            head_sha: "abcdef0123".into(),
            ..Failing::default()
        };
        let prompt = diagnosis_prompt(&pr, Some("## Failing: build\nerror[E0308]"));
        assert!(prompt.starts_with(
            "[DCC] The checks of this task's pull request acme/app#7 failed on abcdef0"
        ));
        assert!(prompt.contains("read-only"));
        assert!(prompt.contains("error[E0308]"));
        assert!(diagnosis_prompt(&pr, None).contains("could not read"));

        let session = |id: &str, task: &str, at: &str| AgentSession {
            session_id: id.into(),
            workspace_id: task.into(),
            state: AgentState::Idle,
            completed_at: None,
            project_id: "p".into(),
            title: None,
            updated_at: at.into(),
        };
        let reviewer = AgentRecord {
            projects: Vec::new(),
            id: "rev".into(),
            name: "Revisor".into(),
            preset: Some("reviewer".into()),
            avatar: dcc_core::domain::agent::AgentAvatar {
                shape: "octopus".to_string(),
                accessory: "none".to_string(),
                color: "violet".into(),
                arms: 3,
                eyes: "round".into(),
            },
            provider_id: None,
            model: None,
            effort: None,
            kickoff_prompt: String::new(),
            role: String::new(),
            offer_prompt: String::new(),
            extra_instructions: String::new(),
            sessions: vec![
                session("s-old", "w-1", "1"),
                session("s-new", "w-1", "2"),
                session("s-other", "w-2", "3"),
            ],
        };
        assert_eq!(conversation_in(&reviewer, "w-1").as_deref(), Some("s-new"));
        assert_eq!(conversation_in(&reviewer, "w-3"), None);
    }
}
