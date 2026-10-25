//! Saving a file from the native app's editor: the desktop's own
//! `write_workspace_file`, a compare-and-swap run in the workspace's
//! mutation queue (so it never lands in the middle of an agent's edit),
//! for a task's checkout only.
use super::*;
use dcc_tauri::commands::workspace_commands::{self, WriteWorkspaceFileInput};

pub(super) async fn write(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WriteWorkspaceFileInput>,
) -> Result<Json<Value>, HttpApiError> {
    git_changes::task_checkout(&backend, &input.workspace_root).await?;
    let output = workspace_commands::write_workspace_file_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    serde_json::to_value(output)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}
