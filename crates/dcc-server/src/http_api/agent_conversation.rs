//! A resident agent's own conversation for its page: its home folder,
//! made the first time the team room writes to it (`agent_conversation`).
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
