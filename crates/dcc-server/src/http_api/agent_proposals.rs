//! An agent another agent proposed (`dcc_propose_agent`): "Criar" makes it
//! from the stored draft and "Descartar" answers the proposal (also after
//! the person created it from the editor). Local only: the relay never
//! forwards `/api/v1/agents/`.
use super::*;

pub(super) async fn accept(
    State(backend): State<Arc<HttpBackend>>,
    Path((_agent_id, proposal_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let agent = backend
        .sessions
        .accept_agent_proposal(proposal_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?
        .ok_or_else(|| HttpApiError::not_found("proposal not found"))?;
    Ok(Json(json!({ "agentId": agent.id })))
}

pub(super) async fn dismiss(
    State(backend): State<Arc<HttpBackend>>,
    Path((_agent_id, proposal_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let answered = backend
        .sessions
        .dismiss_agent_proposal(proposal_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "answered": answered })))
}
