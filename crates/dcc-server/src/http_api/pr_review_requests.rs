//! The app took a pull request an agent asked it to review
//! (`dcc_review_pull_request`) and runs the hub's review now. Local only:
//! the relay never forwards `/api/v1/agents/`.
use super::*;

pub(super) async fn taken(
    State(backend): State<Arc<HttpBackend>>,
    Path((_agent_id, request_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .take_pr_review_request(request_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!({ "taken": changed })))
}
