//! Git and pull-request status of a task's checkout, as every client reads it.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitChangeEntry {
    /// Repo-relative path (posix separators).
    pub path: String,
    pub name: String,
    pub absolute_path: String,
    /// Single-letter status (index/worktree combined display).
    pub status: String,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitStatusOutput {
    pub staged: Vec<WorkspaceGitChangeEntry>,
    pub unstaged: Vec<WorkspaceGitChangeEntry>,
    pub staged_fingerprint: String,
    pub current_branch: Option<String>,
    pub ahead_of_remote_count: u32,
    pub behind_of_remote_count: u32,
    pub conflict_count: u32,
    pub merge_in_progress: bool,
    /// The repository has at least one remote, so push and pull requests have
    /// somewhere to go. Without one, delivery stops at a local commit.
    pub has_remote: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitBranchDiffOutput {
    pub changes: Vec<WorkspaceGitChangeEntry>,
    pub base_branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrStatusOutput {
    pub provider: Option<String>,
    pub host: Option<String>,
    pub number: Option<u32>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub head_branch: Option<String>,
    pub base_branch: Option<String>,
    pub state: Option<String>,
    pub is_draft: bool,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
}

/// A remote branch a deletion may also remove, as it was when the dialog
/// offered it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRemoteBranchDeletionTarget {
    pub remote: String,
    pub branch: String,
    /// The local worktree HEAD observed when the destructive action was offered.
    /// Older clients do not send this field, which intentionally makes deletion
    /// fail closed until the confirmation dialog is reopened.
    #[serde(default)]
    pub expected_oid: String,
    /// The effective push URL observed when the destructive action was offered.
    /// This is always redacted before it can leave the backend.
    #[serde(default)]
    pub push_url: String,
}
