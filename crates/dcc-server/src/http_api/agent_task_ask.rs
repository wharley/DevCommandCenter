//! Asking a resident agent something inside a task, from the server: its
//! newest conversation in that task, or a new one there bound to it, on its
//! own provider, model, effort and account (`dcc_core::domain::agent_run`,
//! as the phone's room picks them), read-only. A conversation with a turn
//! open gets it in its queue. The native app did this through the API from
//! its window (`reviewer_in_task`); the triggers that react to the pull
//! requests' search (`pr_watch`) do it here, with or without one.

use dcc_core::domain::agent::ResidentAgent;
use dcc_core::domain::agent_run::{agent_run_effort, agent_run_pick, agent_run_runtime};
use dcc_core::domain::provider::{HealthStatus, ProviderApprovalPolicy};
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo};

use super::room_feed::read_team;
use super::*;

/// The agent's newest conversation in `task`.
fn conversation_in(bindings: &[AgentSessionBinding], agent_id: &str, task: &str) -> Option<String> {
    bindings
        .iter()
        .filter(|binding| binding.agent_id == agent_id && binding.workspace_id == task)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
        .map(|binding| binding.session_id.clone())
}

/// Sends `prompt` to `agent` in `task` (of `project`); the conversation it
/// went to.
pub(super) async fn ask_in_task(
    backend: &HttpBackend,
    agent: &ResidentAgent,
    task: &str,
    project: &str,
    prompt: &str,
) -> Result<String, String> {
    let state = &backend.sessions;
    let db_path = backend.workspace.db_path.clone();
    let (agent_id, task_id) = (agent.id.clone(), task.to_string());
    let existing = tokio::task::spawn_blocking(move || {
        let repo =
            SqliteSessionRepo::open_read_only(&db_path).map_err(|error| error.to_string())?;
        let team = read_team(&repo)?;
        Ok::<_, String>(conversation_in(&team.bindings, &agent_id, &task_id))
    })
    .await
    .map_err(|error| error.to_string())??;
    let current = match &existing {
        Some(id) => state
            .peek_session(&SessionId(id.clone()))
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let providers = dcc_tauri::commands::provider_commands::list_providers_for_state(state)
        .await?
        .catalog
        .providers;
    // Where it is written: what this conversation ran on, else the first
    // provider that can run.
    let fallback = current
        .as_ref()
        .map(|session| (Some(session.provider_id.clone()), session.model.clone()))
        .or_else(|| {
            providers
                .iter()
                .find(|provider| {
                    provider.enabled && !matches!(provider.health, HealthStatus::Unhealthy { .. })
                })
                .map(|provider| (Some(provider.id.0.clone()), None))
        })
        .unwrap_or((None, None));
    let pick = agent_run_pick(
        agent.provider_id.as_deref(),
        agent.model.as_deref(),
        &providers,
        (fallback.0.as_deref(), fallback.1.as_deref()),
    )
    .map_err(|error| format!("{} has no provider to run on ({error:?})", agent.name))?;
    let own_account = (agent.provider_id.as_deref() == Some(pick.provider_id.as_str())
        && providers.iter().any(|provider| {
            provider.id.0 == pick.provider_id && provider.capabilities.supports_accounts
        }))
    .then(|| agent.account_id.clone());
    let keeps = current
        .as_ref()
        .is_some_and(|session| session.provider_id == pick.provider_id);
    let runtime = (!keeps).then(|| agent_run_runtime(None, own_account).unwrap_or_default());
    let session = match existing {
        Some(session) => session,
        None => {
            let mut start: StartThreadInput = serde_json::from_value(json!({
                "workspaceId": task,
                "projectId": project,
                "providerId": pick.provider_id,
                "model": pick.model,
                "title": agent.name,
            }))
            .map_err(|error| error.to_string())?;
            start.provider_runtime = runtime.clone();
            let session = start_thread_with_state(state, start).await?.session.id;
            if let Err(error) = state.bind_session_agent(&session, &agent.id) {
                // Without the binding it would be a plain session with no role.
                let _ = close_session(
                    state,
                    state,
                    state,
                    CloseSessionInput {
                        session_id: session.clone(),
                        delete_history: true,
                    },
                )
                .await;
                return Err(error.to_string());
            }
            session.0
        }
    };
    let mut input: SendTurnInput = serde_json::from_value(json!({
        "sessionId": session,
        "prompt": prompt,
        "providerId": pick.provider_id,
        "model": pick.model,
        "effort": agent_run_effort(agent.provider_id.as_deref(), agent.effort.as_deref())
            .or_else(|| Some("medium".to_string())),
        "planMode": false,
        "fastMode": false,
        "approvalPolicy": ProviderApprovalPolicy::ReadOnly,
    }))
    .map_err(|error| error.to_string())?;
    input.provider_runtime = runtime;
    match send_turn_with_state(state, input.clone()).await {
        Ok(_) => Ok(session),
        // Busy: after the turn it is in.
        Err(error) if error.contains("already has an active turn") => {
            dcc_tauri::commands::session_commands::queue_turn_with_state(
                state,
                dcc_core::application::QueueTurnInput { turn: input },
            )
            .await
            .map(|_| session)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(session: &str, agent: &str, task: &str, at: &str) -> AgentSessionBinding {
        AgentSessionBinding {
            session_id: session.into(),
            agent_id: agent.into(),
            workspace_id: task.into(),
            project_id: "p".into(),
            title: None,
            updated_at: at.into(),
            room: None,
        }
    }

    #[test]
    fn the_agents_newest_conversation_in_that_task() {
        let bindings = [
            binding("s-old", "rev", "w-1", "1"),
            binding("s-new", "rev", "w-1", "2"),
            binding("s-other", "rev", "w-2", "3"),
            binding("s-dev", "dev", "w-1", "4"),
        ];
        assert_eq!(
            conversation_in(&bindings, "rev", "w-1").as_deref(),
            Some("s-new")
        );
        assert_eq!(conversation_in(&bindings, "rev", "w-3"), None);
    }
}
