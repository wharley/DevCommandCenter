//! The changes list's git actions for the native app: stage, unstage and
//! discard one file, and stage everything. They run through the desktop's
//! own commands, so they wait for an agent's edit to the same checkout
//! (`run_git_workspace_mutation`). Only a task's checkout is accepted.
use super::*;
use dcc_tauri::commands::workspace_commands::{self, WorkspaceGitPathInput};

/// The folder must be where a task runs (its worktree or its repository).
pub(super) async fn task_checkout(backend: &HttpBackend, root: &str) -> Result<(), HttpApiError> {
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(&backend.workspace.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    dcc_infra::workspace_git::find_workspace_by_root(&repo, root)
        .await
        .map_err(HttpApiError::internal)?
        .map(|_| ())
        .ok_or_else(|| HttpApiError::bad_request("not a task's checkout"))
}

async fn run<F, Fut>(
    backend: Arc<HttpBackend>,
    input: WorkspaceGitPathInput,
    action: F,
) -> Result<Json<Value>, HttpApiError>
where
    F: FnOnce(Arc<HttpBackend>, WorkspaceGitPathInput) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    task_checkout(&backend, &input.workspace_root).await?;
    action(backend, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn stage_file(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitPathInput>,
) -> Result<Json<Value>, HttpApiError> {
    run(backend, input, |backend, input| async move {
        workspace_commands::workspace_git_stage_file_with_state(&backend.workspace, input).await
    })
    .await
}

pub(super) async fn unstage_file(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitPathInput>,
) -> Result<Json<Value>, HttpApiError> {
    run(backend, input, |backend, input| async move {
        workspace_commands::workspace_git_unstage_file_with_state(&backend.workspace, input).await
    })
    .await
}

pub(super) async fn discard_file(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitPathInput>,
) -> Result<Json<Value>, HttpApiError> {
    run(backend, input, |backend, input| async move {
        workspace_commands::workspace_git_discard_file_with_state(&backend.workspace, input).await
    })
    .await
}

pub(super) async fn stage_all(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitPathInput>,
) -> Result<Json<Value>, HttpApiError> {
    run(backend, input, |backend, input| async move {
        workspace_commands::workspace_git_stage_all_with_state(&backend.workspace, input).await
    })
    .await
}
