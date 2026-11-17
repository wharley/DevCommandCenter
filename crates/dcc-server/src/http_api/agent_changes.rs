//! A change an agent proposed (`dcc_propose_change`): "Aplicar" writes it
//! and "Descartar" answers it. Local only: the relay never forwards
//! `/api/v1/agents/`.
use super::*;

pub(super) async fn accept(
    State(backend): State<Arc<HttpBackend>>,
    Path(change_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let applied = backend
        .sessions
        .accept_agent_change(change_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?
        .ok_or_else(|| HttpApiError::not_found("change not found"))?;
    Ok(Json(json!({ "agentId": applied.change.about() })))
}

pub(super) async fn dismiss(
    State(backend): State<Arc<HttpBackend>>,
    Path(change_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let answered = backend
        .sessions
        .dismiss_agent_change(change_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "answered": answered })))
}
