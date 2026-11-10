//! A task an agent opened belongs to the agent, not to one of its
//! conversations (`docs/PLANO_SALA_POR_PROJETO`): asked from another room —
//! the Dev, in the Checkout's room, about a task it opened from Geral — it
//! takes the task over, and the task's next statuses and its report come to
//! that conversation. Only the same agent's own conversations, and the
//! usual rules of a message still hold (a person's turn, a task not done).

use dcc_core::domain::delegation::{Delegation, DelegationId};
use dcc_core::domain::session::SessionId;
use dcc_core::ports::DelegationRepo;

use crate::state::SessionCommandState;

/// The task `task_id` for `parent`: its own, or one the same agent opened
/// from another of its conversations, to move to `parent` (with `moved`
/// true) once the message is allowed — `keep_moved` saves it.
pub(crate) async fn task_for_agent(
    state: &SessionCommandState,
    parent: &SessionId,
    task_id: &str,
) -> Result<(Delegation, bool), String> {
    let text = |error: dcc_core::CoreError| error.to_string();
    let not_found = || format!("task not found: {task_id}");
    let mut task = DelegationRepo::get_delegation(state, &DelegationId(task_id.trim().to_string()))
        .await
        .map_err(text)?
        .ok_or_else(not_found)?;
    if task.parent_session_id == *parent {
        return Ok((task, false));
    }
    let here = state.session_agent(parent).map_err(text)?;
    let opener = state.session_agent(&task.parent_session_id).map_err(text)?;
    let same_agent = matches!((&here, &opener), (Some(here), Some(opener)) if here.id == opener.id);
    let both_its_own = state.is_agent_conversation(parent).map_err(text)?
        && state
            .is_agent_conversation(&task.parent_session_id)
            .map_err(text)?;
    if !same_agent || !both_its_own {
        return Err(not_found());
    }
    task.parent_session_id = parent.clone();
    Ok((task, true))
}

/// Saves a task moved to the conversation that now carries it.
pub(crate) async fn keep_moved(
    state: &SessionCommandState,
    task: &Delegation,
) -> Result<(), String> {
    DelegationRepo::save_delegation(state, task)
        .await
        .map_err(|error| error.to_string())
}
