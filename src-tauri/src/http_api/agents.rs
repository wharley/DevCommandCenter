//! Resident agents for the native app: a session started for an agent is
//! bound to it (`agents_bind_session`), so it runs with the agent's role
//! and shows its mascot. A session keeps its first agent.
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
