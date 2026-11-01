//! A local-direct task's branch (`POST /api/v1/workspaces/local-branches`,
//! `…/switch`): the branches its checkout can move to before the
//! conversation starts, and the move. Git runs on the server, behind the
//! same lock as a conversation's start. Blocking: call it off the UI thread.

use dcc_tauri::commands::local_branches::LocalBranchesOutput;
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi};

/// Where the checkout goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchTarget {
    /// A branch from the list, by its full ref (local or remote).
    Existing(String),
    /// A new branch made from the current one.
    New(String),
}

impl DesktopApi {
    /// `refresh` fetches the remotes first; a failed fetch still answers
    /// with the branches already known (`refreshError`).
    pub fn local_branches(
        &self,
        task_id: &str,
        refresh: bool,
    ) -> Result<LocalBranchesOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/workspaces/local-branches",
            &json!({ "workspaceId": task_id, "refresh": refresh }),
        )?)
    }

    /// Refused when the checkout moved since `expected` was read, the
    /// conversation started or an agent works in the folder.
    pub fn switch_local_branch(
        &self,
        task_id: &str,
        expected: Option<&str>,
        target: &BranchTarget,
    ) -> Result<LocalBranchesOutput, ApiError> {
        let (reference, new_branch) = match target {
            BranchTarget::Existing(reference) => (Some(reference.as_str()), None),
            BranchTarget::New(name) => (None, Some(name.as_str())),
        };
        Self::decoded(self.post(
            "/api/v1/workspaces/local-branches/switch",
            &json!({
                "workspaceId": task_id,
                "expectedBranch": expected,
                "reference": reference,
                "newBranch": new_branch,
            }),
        )?)
    }
}
