//! Resident agents for the native app: a session started for an agent is
//! bound to it (`agents_bind_session`), so it runs with the agent's role
//! and shows its mascot. A session keeps its first agent. The agent's page
//! saves what the person changes (`agents_save`); "Novo agente" creates one.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BindInput {
    agent_id: String,
}

pub(super) async fn bind_session(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(input): Json<BindInput>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .bind_session_agent(&SessionId(session_id), input.agent_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

/// "Novo agente": an agent of the person's own, with the role they wrote.
pub(super) async fn create(
    State(backend): State<Arc<HttpBackend>>,
    Json(draft): Json<dcc_core::domain::agent::ResidentAgentDraft>,
) -> Result<Json<Value>, HttpApiError> {
    let agent = backend
        .sessions
        .save_resident_agent(None, draft)
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "agent": agent })))
}

/// `agents_save`: the agent's name, mascot, provider, extra instructions
/// and offer; its role stays DCC's.
pub(super) async fn save(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(draft): Json<dcc_core::domain::agent::ResidentAgentDraft>,
) -> Result<Json<Value>, HttpApiError> {
    let agent = backend
        .sessions
        .save_resident_agent(Some(agent_id.trim()), draft)
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "agent": agent })))
}
