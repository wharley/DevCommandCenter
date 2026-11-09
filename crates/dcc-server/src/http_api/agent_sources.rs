//! An agent's sources from its page and the room (`agent_source`): create
//! one the agent proposed, switch it, delete it, and read it now (the app's
//! clock, once it is due). Local only: the relay never forwards
//! `/api/v1/agents/`.
use super::*;

#[derive(Deserialize)]
pub(super) struct SwitchInput {
    enabled: bool,
}

fn found(changed: bool) -> Result<Json<Value>, HttpApiError> {
    if changed {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err(HttpApiError::not_found("source not found"))
    }
}

pub(super) async fn switch(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, source_id)): Path<(String, String)>,
    Json(input): Json<SwitchInput>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .set_agent_source(agent_id.trim(), source_id.trim(), input.enabled)
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    found(changed)
}

pub(super) async fn delete(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, source_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .delete_agent_source(agent_id.trim(), source_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    found(changed)
}

pub(super) async fn read(
    State(backend): State<Arc<HttpBackend>>,
    Path((_agent_id, source_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let gateway = backend
        .app
        .get()
        .and_then(|app| app.connectors())
        .map(|(_, gateway)| gateway)
        .ok_or_else(|| HttpApiError::not_found("connectors are not available"))?;
    let read = dcc_tauri::source_read::read_source(&backend.sessions, &gateway, source_id.trim())
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(read)))
}
