//! Reviewing an isolated delegation from the native app's changes list
//! (`handleApplyDelegation` / `handleDiscardDelegation`): apply the child's
//! worktree onto the task's checkout and approve the delegation, or throw
//! the worktree away and cancel it, in the desktop's order.
use super::*;
use dcc_core::domain::delegation::DelegationId;
use dcc_core::ports::DelegationRepo;
use dcc_tauri::commands::delegation_commands::{
    self, ApproveDelegationInput, CancelDelegationInput,
};
use dcc_tauri::commands::workspace_commands::{
    self, WorkspaceApplyDelegationWorktreeInput, WorkspaceRemoveDelegationWorktreeInput,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReviewInput {
    /// The task's checkout the delegation applies to.
    workspace_root: String,
}

pub(super) async fn apply(
    State(backend): State<Arc<HttpBackend>>,
    Path(delegation_id): Path<String>,
    Json(input): Json<ReviewInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    let id = DelegationId(delegation_id);
    let delegation = DelegationRepo::get_delegation(&backend.sessions, &id)
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .ok_or_else(|| HttpApiError::not_found("delegation not found"))?;
    let applied = workspace_commands::workspace_apply_delegation_worktree_with_state(
        &backend.workspace,
        WorkspaceApplyDelegationWorktreeInput {
            workspace_root: input.workspace_root,
            delegation_id: id.clone(),
        },
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    delegation_commands::approve_delegation_with_state(
        &backend.sessions,
        ApproveDelegationInput {
            delegation_id: id,
            summary: delegation.result_summary,
        },
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "changedFiles": applied.changed_files })))
}

pub(super) async fn discard(
    State(backend): State<Arc<HttpBackend>>,
    Path(delegation_id): Path<String>,
    Json(input): Json<ReviewInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    let id = DelegationId(delegation_id);
    workspace_commands::workspace_remove_delegation_worktree_with_state(
        &backend.workspace,
        WorkspaceRemoveDelegationWorktreeInput {
            workspace_root: input.workspace_root,
            delegation_id: Some(id.clone()),
            operation_id: None,
            remove_branch: true,
            discard_changes: true,
        },
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    delegation_commands::cancel_delegation_with_state(
        &backend.sessions,
        CancelDelegationInput {
            delegation_id: id,
            reason: Some("Discarded isolated implementation worktree.".to_string()),
        },
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}
