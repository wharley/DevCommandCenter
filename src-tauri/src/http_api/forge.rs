//! The review's forge reads and actions for the native app: the checkout's
//! forge context and accounts (who commits and pushes), the pull request's
//! review state, reviewer candidates and requests, replying to and
//! resolving its conversations, and the last delivery failure with its
//! recovery. They run the desktop's own commands (the
//! failure lives in this process); a checkout must be a task's.
use super::*;
use dcc_tauri::commands::forge_commands::{
    self, ForgeCliAccountsInput, ForgeCliHostsInput, ForgeCliSelectLoginInput, ForgeCliStatusInput,
    WorkspaceForgeContextInput, WorkspaceRequestReviewersInput, WorkspaceReviewStateInput,
    WorkspaceReviewerCandidatesInput,
};
use dcc_tauri::commands::workspace_commands::WorkspaceChangeRequestContextInput;
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

/// The "Criar PR" dialog's route and default title (`workspaceChangeRequestContext`).
pub(super) async fn change_request_context(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceChangeRequestContextInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(
        forge_commands::workspace_change_request_context_with_state(&backend.workspace, input)
            .await,
    )
}

pub(super) async fn accounts(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ForgeCliAccountsInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(forge_commands::workspace_forge_cli_accounts_with_state(&backend.workspace, input).await)
}

/// Settings → Conta: who the CLI is signed in as on a host.
pub(super) async fn cli_status(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ForgeCliStatusInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(forge_commands::workspace_forge_cli_status_with_state(
        &backend.workspace,
        input,
    ))
}

/// The hosts the CLI is signed in to.
pub(super) async fn hosts(
    Json(input): Json<ForgeCliHostsInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(forge_commands::forge_cli_hosts(input))
}

/// The login DCC uses on a host.
pub(super) async fn select_login(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ForgeCliSelectLoginInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(
        forge_commands::workspace_forge_cli_select_login_with_state(&backend.workspace, input)
            .map(|()| json!({ "ok": true })),
    )
}

/// After a new sign-in: the repositories without a login get it.
pub(super) async fn backfill_bindings(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    reply(
        forge_commands::workspace_backfill_forge_repo_bindings_with_state(&backend.workspace)
            .map(|bound| json!({ "bound": bound })),
    )
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

pub(super) async fn review_comments(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<forge_commands::WorkspacePrReviewCommentsInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(forge_commands::workspace_pr_review_comments_with_state(&backend.workspace, input).await)
}

pub(super) async fn pipeline(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<forge_commands::WorkspacePipelineStatusInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(forge_commands::workspace_pipeline_status_with_state(&backend.workspace, input).await)
}

pub(super) async fn pipeline_job_log(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<forge_commands::WorkspacePipelineJobInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(forge_commands::workspace_pipeline_job_log_with_state(&backend.workspace, input).await)
}

pub(super) async fn pipeline_job_retry(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<forge_commands::WorkspacePipelineJobInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    forge_commands::workspace_pipeline_job_retry_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn reply_thread(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<forge_commands::PullRequestHubThreadReplyInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.repository_root).await?;
    reply(forge_commands::pull_request_hub_reply_thread_with_state(&backend.workspace, input).await)
}

pub(super) async fn resolve_thread(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<forge_commands::PullRequestHubThreadResolveInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.repository_root).await?;
    reply(
        forge_commands::pull_request_hub_resolve_thread_with_state(&backend.workspace, input).await,
    )
}
