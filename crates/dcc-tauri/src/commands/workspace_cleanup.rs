//! What deleting a completed task would cost and free
//! (`dcc_infra::workspace_cleanup`, shared with the native app).

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

pub use dcc_infra::workspace_cleanup::{
    WorkspaceCleanupEntry, WorkspaceCleanupSafety, WorkspaceCleanupScanOutput,
};

use crate::state::WorkspaceCommandState;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCleanupScanInput {
    pub workspace_ids: Vec<String>,
}

/// Size and safety of each task's worktree and its child tasks' worktrees.
/// Unknown ids are skipped.
#[tauri::command]
pub async fn workspace_cleanup_scan(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCleanupScanInput,
) -> Result<WorkspaceCleanupScanOutput, String> {
    let targets =
        dcc_infra::workspace_cleanup::cleanup_targets(&state.db_path, input.workspace_ids).await?;
    tauri::async_runtime::spawn_blocking(move || {
        dcc_infra::workspace_cleanup::measure_cleanup(targets)
    })
    .await
    .map_err(|error| format!("failed to inspect completed worktrees: {error}"))?
}
