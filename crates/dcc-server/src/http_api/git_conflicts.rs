//! The merge conflict resolver for the native app: the conflict state, one
//! side taken for a file, a file marked resolved (or deleted), the merge
//! aborted, its validation commands, and the merge completed after them.
//! They run the desktop's own commands, in its mutation queue; a checkout
//! must be a task's.
use super::*;
use dcc_tauri::commands::workspace_commands::{
    self, WorkspaceGitAcceptConflictInput, WorkspaceGitCompleteMergeInput,
    WorkspaceGitConflictStateInput, WorkspaceGitMarkConflictResolvedInput,
};

fn reply<T: Serialize>(result: Result<T, String>) -> Result<Json<Value>, HttpApiError> {
    let value = result.map_err(HttpApiError::bad_request)?;
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

pub(super) async fn conflict_state(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitConflictStateInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(
        workspace_commands::workspace_git_conflict_state_with_state(&backend.workspace, input)
            .await,
    )
}

pub(super) async fn accept_conflict(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitAcceptConflictInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    workspace_commands::workspace_git_accept_conflict_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn mark_conflict_resolved(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitMarkConflictResolvedInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    workspace_commands::workspace_git_mark_conflict_resolved_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn abort_merge(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitConflictStateInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    workspace_commands::workspace_git_abort_merge_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn validation_config(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitConflictStateInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(
        workspace_commands::workspace_git_validation_config_with_state(&backend.workspace, input)
            .await,
    )
}

pub(super) async fn complete_merge(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitCompleteMergeInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(
        workspace_commands::workspace_git_complete_merge_with_state(&backend.workspace, input)
            .await,
    )
}
