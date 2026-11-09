//! A resident agent's routines from its page (`agent_routine`): save one,
//! delete it, create one the agent proposed, and mark one ran (the app's
//! scheduler, before it sends the turn). Local only: the relay never
//! forwards `/api/v1/agents/`.
use super::*;
use dcc_core::domain::agent_routine::RoutineDraft;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SaveInput {
    #[serde(default)]
    id: Option<String>,
    #[serde(flatten)]
    draft: RoutineDraft,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Deserialize)]
pub(super) struct RanInput {
    at: String,
}

pub(super) async fn save(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(input): Json<SaveInput>,
) -> Result<Json<Value>, HttpApiError> {
    let routine = backend
        .sessions
        .save_agent_routine(
            agent_id.trim(),
            input.id.as_deref(),
            input.draft,
            input.enabled,
        )
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "routine": routine })))
}

fn found(changed: bool) -> Result<Json<Value>, HttpApiError> {
    if changed {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err(HttpApiError::not_found("routine not found"))
    }
}

pub(super) async fn delete(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, routine_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .delete_agent_routine(agent_id.trim(), routine_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    found(changed)
}

pub(super) async fn accept(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, routine_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .accept_agent_routine(agent_id.trim(), routine_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    found(changed)
}

pub(super) async fn ran(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, routine_id)): Path<(String, String)>,
    Json(input): Json<RanInput>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .mark_agent_routine_ran(agent_id.trim(), routine_id.trim(), input.at.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    found(changed)
}
