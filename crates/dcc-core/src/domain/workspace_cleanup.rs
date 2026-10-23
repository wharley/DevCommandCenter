//! What deleting a completed task would free and what it could lose.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceCleanupSafety {
    /// Clean worktree; every commit is on a remote or on the local base branch.
    Safe,
    /// Changes that were never committed, untracked files included. Lost on delete.
    Uncommitted,
    /// Commits only on the task's branch. Kept on the branch, but easy to forget.
    Unpushed,
    /// The task ran in the project checkout, or its folder is already gone.
    NoWorktree,
    /// Git could not inspect the worktree.
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCleanupEntry {
    pub workspace_id: String,
    pub bytes: u64,
    pub safety: WorkspaceCleanupSafety,
    /// Files with uncommitted changes, when `safety` is `uncommitted`.
    pub changed_files: u32,
    /// Commits on no remote and not on the base branch, when `safety` is `unpushed`.
    pub unpushed_commits: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCleanupScanOutput {
    pub workspaces: Vec<WorkspaceCleanupEntry>,
    /// Every distinct worktree counted once, even when tasks share one.
    pub total_bytes: u64,
}
