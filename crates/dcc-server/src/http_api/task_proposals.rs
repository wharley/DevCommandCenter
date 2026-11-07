//! A task proposal's card in the desktop's conversation: "Começar", with the
//! brief as the person left it, and "Descartar". The phone answers the same
//! proposals through `/api/v1/mobile/task-proposals`.
use super::*;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartProposal {
    /// The brief as edited on the card; omitted: as the agent wrote it.
    #[serde(default)]
    brief: Option<String>,
}

fn proposal_error(error: String) -> HttpApiError {
    if error.contains("already started or dismissed") || error.contains("already starting") {
        HttpApiError::Conflict(error, Value::Null)
    } else if error.contains("not found") {
        HttpApiError::not_found(error)
    } else {
        HttpApiError::bad_request(error)
    }
}

pub(super) async fn start(
    State(backend): State<Arc<HttpBackend>>,
    Path(proposal_id): Path<String>,
    Json(input): Json<StartProposal>,
) -> Result<Json<Value>, HttpApiError> {
    let id = dcc_core::domain::delegation::DelegationId(proposal_id);
    let task =
        dcc_tauri::delegation_runtime::start_task_proposal(&backend.sessions, &id, input.brief)
            .await
            .map_err(proposal_error)?;
    // A task opened from an agent's conversation lives in its own project.
    let workspace_id = task
        .budget
        .new_task
        .as_ref()
        .and_then(|target| target.workspace_id.clone())
        .unwrap_or_else(|| task.workspace_id.0.clone());
    Ok(Json(json!({
        "taskId": task.id.0,
        "workspaceId": workspace_id,
        "childSessionId": task.child_session_id.map(|id| id.0),
    })))
}

pub(super) async fn dismiss(
    State(backend): State<Arc<HttpBackend>>,
    Path(proposal_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let id = dcc_core::domain::delegation::DelegationId(proposal_id);
    attention::dismiss_task_proposal(&backend.sessions, id)
        .await
        .map_err(proposal_error)?;
    Ok(Json(json!({ "ok": true })))
}
