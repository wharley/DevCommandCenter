//! In `dcc_overview`, the tasks the agent itself opened that are still open
//! — from any of its rooms — with their ids, so it can continue one with
//! `dcc_task_message` from the room it talks in now (the task then moves to
//! that room, `task_adoption`).

use dcc_core::domain::delegation::{DelegationKind, DelegationStatus};
use dcc_core::domain::session::SessionId;
use dcc_core::ports::DelegationRepo;
use dcc_tauri::state::SessionCommandState;
use serde_json::{json, Value};

/// `yourOpenTasks`, for the agent whose conversation `session_id` is.
pub async fn own_open_tasks(sessions: &SessionCommandState, session_id: &str) -> Vec<Value> {
    let Ok(Some(agent)) = sessions.session_agent(&SessionId(session_id.to_string())) else {
        return Vec::new();
    };
    let Ok(bindings) = sessions.list_agent_session_bindings() else {
        return Vec::new();
    };
    let mine: Vec<(String, Option<String>)> = bindings
        .into_iter()
        .filter(|binding| binding.agent_id == agent.id)
        .map(|binding| (binding.session_id, binding.room))
        .collect();
    let Ok(delegations) = DelegationRepo::list_delegations(sessions, None, None).await else {
        return Vec::new();
    };
    delegations
        .into_iter()
        .filter(|task| task.kind == DelegationKind::Task && task.status == DelegationStatus::Running)
        .filter_map(|task| {
            let (_, room) = mine
                .iter()
                .find(|(session, _)| *session == task.parent_session_id.0)?;
            let target = task.budget.new_task.as_ref()?;
            target.workspace_id.as_ref()?;
            Some(json!({
                "taskId": task.id.0,
                "task": task.title.clone().unwrap_or_default(),
                "project": target.project,
                "openedIn": room.clone().map_or("Geral".to_string(), |room| format!("room of {room}")),
            }))
        })
        .collect()
}
