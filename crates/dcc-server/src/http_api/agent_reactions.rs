//! 👍 / 👎 on an agent's answer, from the team room (desktop or phone).
//! Local only: the relay never forwards `/api/v1/agents/`.
use super::*;
use dcc_core::domain::agent_reaction::Reaction;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReactionInput {
    session_id: String,
    line_key: String,
    /// `None` takes it back.
    #[serde(default)]
    reaction: Option<Reaction>,
}

pub(super) async fn set(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(input): Json<ReactionInput>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .set_agent_reaction(
            agent_id.trim(),
            input.session_id.trim(),
            input.line_key.trim(),
            input.reaction,
        )
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}
