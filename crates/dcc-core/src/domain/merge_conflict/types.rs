//! A merge's conflict state as the desktop reads it from git (the
//! `workspace_git_conflict_state` / `workspace_git_complete_merge` contract).

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceGitConflictOperation {
    None,
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceGitConflictKind {
    BothModified,
    BothAdded,
    DeletedByCurrent,
    DeletedByIncoming,
    BothDeleted,
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitConflictContent {
    pub exists: bool,
    pub binary: bool,
    pub truncated: bool,
    pub byte_count: u64,
    pub mode: Option<String>,
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitConflictEntry {
    pub path: String,
    pub kind: WorkspaceGitConflictKind,
    pub base: WorkspaceGitConflictContent,
    pub current: WorkspaceGitConflictContent,
    pub incoming: WorkspaceGitConflictContent,
    pub result: WorkspaceGitConflictContent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitConflictStateOutput {
    pub operation: WorkspaceGitConflictOperation,
    pub current_branch: Option<String>,
    pub incoming_ref: Option<String>,
    pub conflicts: Vec<WorkspaceGitConflictEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceGitConflictSide {
    Current,
    Incoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceGitValidationStatus {
    NotConfigured,
    Passed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitValidationStep {
    pub command: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub output: String,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitValidationReport {
    pub status: WorkspaceGitValidationStatus,
    pub source_path: Option<String>,
    pub steps: Vec<WorkspaceGitValidationStep>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitValidationConfigOutput {
    pub commands: Vec<String>,
    pub source_path: Option<String>,
    pub timeout_seconds: u64,
    pub config_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitCompleteMergeOutput {
    pub completed: bool,
    pub validation: WorkspaceGitValidationReport,
}
