//! The review's forge reads and actions for the native app: the checkout's
//! forge context and accounts (who commits and pushes), the pull request's
//! review state, reviewer candidates and requests, and the last delivery
//! failure with its recovery. They run the desktop's own commands (the
//! failure lives in this process); a checkout must be a task's.
use super::*;
use dcc_tauri::commands::forge_commands::{
    self, ForgeCliAccountsInput, WorkspaceForgeContextInput, WorkspaceRequestReviewersInput,
    WorkspaceReviewStateInput, WorkspaceReviewerCandidatesInput,
};
use dcc_tauri::delivery_failure::{WorkspaceDeliveryFailureInput, WorkspaceDeliveryRecoveryInput};

fn reply<T: Serialize>(result: Result<T, String>) -> Result<Json<Value>, HttpApiError> {
    let value = result.map_err(HttpApiError::bad_request)?;
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

pub(super) async fn context(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceForgeContextInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(forge_commands::workspace_forge_context_with_state(&backend.workspace, input).await)
}

pub(super) async fn accounts(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ForgeCliAccountsInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(forge_commands::workspace_forge_cli_accounts_with_state(&backend.workspace, input).await)
}

pub(super) async fn review_state(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceReviewStateInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(forge_commands::workspace_review_state_with_state(&backend.workspace, input).await)
}

pub(super) async fn reviewer_candidates(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceReviewerCandidatesInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(forge_commands::workspace_reviewer_candidates_with_state(&backend.workspace, input).await)
}

pub(super) async fn request_reviewers(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceRequestReviewersInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(forge_commands::workspace_request_reviewers_with_state(&backend.workspace, input).await)
}

pub(super) async fn delivery_failure(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceDeliveryFailureInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(
        dcc_tauri::delivery_failure::workspace_delivery_failure_snapshot_with_state(
            &backend.workspace,
            input,
        )
        .await,
    )
}

pub(super) async fn delivery_recovery(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceDeliveryRecoveryInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(
        dcc_tauri::commands::workspace_commands::workspace_delivery_recovery_execute_with_state(
            &backend.workspace,
            input,
        )
        .await,
    )
}
