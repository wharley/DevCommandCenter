//! Settings → Git → "Automação do projeto" for the native app: the task's
//! `.dcc.toml` (setup, delivery policy, checks and fixes), saving it against
//! the hash it was read with, and running its tasks in the checkout. The
//! checkout must be a task's.
use super::*;
use dcc_tauri::commands::workspace_commands::{
    self as commands, WorkspaceGitConflictStateInput, WorkspaceRunProjectTasksInput,
    WorkspaceSaveProjectAutomationInput,
};

fn reply<T: Serialize>(result: Result<T, String>) -> Result<Json<Value>, HttpApiError> {
    let value = result.map_err(HttpApiError::bad_request)?;
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

pub(super) async fn config(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceGitConflictStateInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(commands::workspace_project_automation_config_with_state(&backend.workspace, input).await)
}

pub(super) async fn save(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceSaveProjectAutomationInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(commands::workspace_save_project_automation_with_state(&backend.workspace, input).await)
}

pub(super) async fn run(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceRunProjectTasksInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    reply(commands::workspace_run_project_tasks_with_state(&backend.workspace, input).await)
}
