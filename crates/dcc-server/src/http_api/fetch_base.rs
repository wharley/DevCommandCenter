//! "Nova tarefa" opened: the project's base branch fetched from its remote
//! (refs only, nothing checked out or merged), so the task created a moment
//! later starts from it without its creation waiting on the network.
use super::*;
use dcc_tauri::commands::workspace_commands::{self, WorkspaceFetchBaseInput};

pub(super) async fn fetch_base(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<WorkspaceFetchBaseInput>,
) -> Result<Json<Value>, HttpApiError> {
    pull_request_hub::known_repository(&backend, &input.repository_root).await?;
    workspace_commands::workspace_fetch_base_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}
