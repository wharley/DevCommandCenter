//! What deleting a completed task would cost and free
//! (`dcc_infra::workspace_cleanup`, shared with the native app).

use serde::{Deserialize, Serialize};
use specta::Type;

pub use dcc_infra::workspace_cleanup::{
    WorkspaceCleanupEntry, WorkspaceCleanupSafety, WorkspaceCleanupScanOutput,
};


#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCleanupScanInput {
    pub workspace_ids: Vec<String>,
}

