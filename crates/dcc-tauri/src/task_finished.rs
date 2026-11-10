//! A task the person finished takes no more messages: once it is completed
//! (merged, moved to "Concluídas"), archived or deleted, a follow-up from the
//! agent that opened it would change code nobody is looking at — the
//! Reviewer's "Dev altered the merged task". The agent opens a new task
//! from the project's up-to-date base instead.

use dcc_core::domain::session::SessionId;
use dcc_core::domain::workspace::WorkspaceState;
use dcc_core::ports::{SessionRepo, WorkspaceRepo};
use dcc_infra::db::SqliteWorkspaceRepo;

use crate::state::SessionCommandState;

/// Why the task behind `session_id` is finished, if it is.
pub(crate) async fn finished_task(
    state: &SessionCommandState,
    session_id: &SessionId,
) -> Result<Option<&'static str>, String> {
    let Some(session) = SessionRepo::get_session(state, session_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(Some("deleted"));
    };
    // The durable workspace store lives in its own repository.
    let workspaces =
        SqliteWorkspaceRepo::open(state.db_path()).map_err(|error| error.to_string())?;
    let workspace = WorkspaceRepo::get_workspace(&workspaces, &session.workspace_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(match workspace.map(|workspace| workspace.state) {
        None => Some("deleted"),
        Some(WorkspaceState::Completed) => Some("completed"),
        Some(WorkspaceState::Archived) => Some("archived"),
        Some(_) => None,
    })
}

/// What the agent is told when it writes to a finished task.
pub(crate) fn finished_error(why: &str) -> String {
    format!(
        "this task is {why} — the person finished it (merged or set it aside), so it takes no more messages. Open a new task with dcc_open_task in the same project, from its up-to-date base branch, with a brief of what is left; tell the person you did."
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_agent_is_told_to_open_a_new_task() {
        let error = super::finished_error("completed");
        assert!(error.contains("completed"));
        assert!(error.contains("dcc_open_task"));
    }
}
