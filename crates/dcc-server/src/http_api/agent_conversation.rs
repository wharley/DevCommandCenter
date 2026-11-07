//! A resident agent's own conversation for its page: its home folder,
//! made on the first "Conversar" (`agent_conversation`). The native app
//! then opens the task there with the agent, like a Researcher's idea.
//! Local only: the relay never forwards `/api/v1/agents/`.
use super::*;
use dcc_tauri::commands::agent_conversation::agent_home_with_state;

pub(super) async fn home(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let home = agent_home_with_state(&backend.sessions, &agent_id)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(home)))
}
