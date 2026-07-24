use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use specta::Type;
use tauri::{AppHandle, State};
use uuid::Uuid;

use dcc_core::{
    application::{
        create_workspace_bundle as run_create_workspace_bundle,
        create_workspace_for_repo as run_create_workspace_for_repo,
        create_workspace_from_url as run_create_workspace_from_url,
        finalize_workspace_for_repo as run_finalize_workspace_for_repo,
        prepare_workspace_for_repo as run_prepare_workspace_for_repo, CreateWorkspaceForRepoInput,
        CreateWorkspaceFromUrlInput,
    },
    domain::{
        repository::{Repository, RepositoryId},
        workspace::{
            Workspace, WorkspaceId, WorkspacePushTarget, WorkspaceSetupReport,
            WorkspaceSetupStatus, WorkspaceSetupStepReport, WorkspaceSource, WorkspaceSourceKind,
            WorkspaceState,
        },
        workspace_bundle::{WorkspaceBundleId, WorkspaceBundleState, WorkspaceBundleSummary},
    },
    ports::{DelegationRepo, RepositoryRepo, SessionRepo, WorkspaceBundleRepo, WorkspaceRepo},
};
#[cfg(test)]
use dcc_infra::git::read_workspace_validation_config;
use dcc_infra::{
    db::{SqliteSessionRepo, SqliteWorkspaceRepo},
    git::{
        broken_worktree_reason, create_worktree_branch_from_ref,
        detect_workspace_setup_suggestions, is_git_repo, list_local_branch_names,
        read_workspace_automation_config, remove_worktree, validate_workspace_automation_config,
        CommandGitOps, RepoAutomationConfig, RepoAutomationTask, RepoTaskKind,
    },
};
use toml_edit::{
    value as toml_value, Array as TomlArray, Document as TomlDocument, Item as TomlItem,
    Table as TomlTable,
};
use url::Url;

#[cfg(test)]
use crate::workspace_setup::run_workspace_validation_command;
use crate::{
    commands::forge::{
        github, gitlab,
        remote::{
            resolve_workspace_forge_target, resolve_workspace_remote_info, WorkspaceForgeTarget,
        },
    },
    commands::workspace_support::{
        broken_workspace_message, cleanup_workspace_files, ensure_pushable_branch,
        find_workspace_by_root, next_available_branch_name, preflight_workspace_root,
        purge_broken_workspace_by_root, push_branch_refspec, push_branch_refspec_to_remote,
        resolve_branch_diff_base, resolve_current_branch_name, resolve_current_commit_sha,
        resolve_default_remote_name, resolve_workspace_active_root,
        resolve_workspace_broken_reason, resolve_workspace_setup_root,
        resolve_workspace_target_branch, run_git_network_output_with_workspace_auth,
    },
    delivery_failure::{
        capture_workspace_delivery_failure, clear_workspace_delivery_failure,
        CaptureDeliveryFailureOptions, WorkspaceDeliveryFailureOperation,
    },
    events::TauriEventBus,
    git::{
        configure_git_command, git_command_succeeds, git_output_err, parse_name_status_z,
        parse_numstat_z, run_git_network_output, run_git_output, run_git_output_owned,
        split_null_terminated_fields,
    },
    state::WorkspaceCommandState,
    workspace_setup::{
        run_detected_workspace_setup, run_workspace_task_command, WORKSPACE_VALIDATION_TIMEOUT,
    },
};

const DCC_SPEC_CONTEXT_START: &str = "<!-- dcc:spec:start -->";
const DCC_SPEC_CONTEXT_END: &str = "<!-- dcc:spec:end -->";
const DCC_SPEC_CONTEXT_MANIFEST_PATH: &str = ".devcommandcenter/context.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MissionSpecContextTargetKind {
    MarkdownSection,
    ManifestJson,
}

#[derive(Clone, Copy, Debug)]
struct MissionSpecContextTarget {
    relative_path: &'static str,
    provider_id: &'static str,
    kind: MissionSpecContextTargetKind,
}

const DCC_SPEC_CONTEXT_TARGETS: [MissionSpecContextTarget; 3] = [
    MissionSpecContextTarget {
        relative_path: "AGENTS.md",
        provider_id: "codex",
        kind: MissionSpecContextTargetKind::MarkdownSection,
    },
    MissionSpecContextTarget {
        relative_path: "GEMINI.md",
        provider_id: "gemini",
        kind: MissionSpecContextTargetKind::MarkdownSection,
    },
    MissionSpecContextTarget {
        relative_path: DCC_SPEC_CONTEXT_MANIFEST_PATH,
        provider_id: "dcc",
        kind: MissionSpecContextTargetKind::ManifestJson,
    },
];

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceForRepoOutput {
    pub workspace: Workspace,
    pub setup_hints: Vec<WorkspaceSetupHint>,
    pub setup_report: WorkspaceSetupReport,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceFromUrlOutput {
    pub workspace: Workspace,
    pub setup_hints: Vec<WorkspaceSetupHint>,
    pub setup_report: WorkspaceSetupReport,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveWorkspaceSourceUrlInput {
    pub workspace_root: String,
    pub url: String,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceFromSourceUrlInput {
    pub project_id: dcc_core::domain::project::ProjectId,
    pub workspace_root: String,
    pub url: String,
    pub name: Option<String>,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSourceUrlResolution {
    pub kind: WorkspaceSourceKind,
    pub url: String,
    pub provider: String,
    pub host: String,
    pub repository: String,
    pub head_branch: String,
    pub head_sha: String,
    pub base_branch: String,
    pub change_request_number: Option<u32>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub state: Option<String>,
    pub source_repository: Option<String>,
    pub is_cross_repository: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceBundleForReposInput {
    pub name: String,
    pub projects: Vec<CreateWorkspaceForRepoInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceBundleForReposOutput {
    pub summary: WorkspaceBundleSummary,
    pub workspaces: Vec<CreateWorkspaceForRepoOutput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRunSetupInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRunSetupOutput {
    pub workspace: Workspace,
    pub setup_hints: Vec<WorkspaceSetupHint>,
    pub setup_report: WorkspaceSetupReport,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSetupHint {
    pub label: String,
    pub command: String,
    pub source_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspacesOutput {
    pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListRepositoriesOutput {
    pub repositories: Vec<Repository>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspaceBundlesOutput {
    pub bundles: Vec<WorkspaceBundleSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBundleIdInput {
    pub bundle_id: WorkspaceBundleId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBundleStateOutput {
    pub summary: WorkspaceBundleSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalBranchesInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalBranchesOutput {
    pub branches: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListGitTrackedFilesInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListGitTrackedFilesOutput {
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadWorkspaceFileInput {
    pub workspace_root: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadWorkspaceFileOutput {
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WriteWorkspaceFileInput {
    pub workspace_root: String,
    pub relative_path: String,
    pub content: String,
    /// When set, the write only proceeds if the file on disk still equals this
    /// (compare-and-swap). A mismatch returns `conflicted` instead of overwriting.
    pub expected_previous: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WriteWorkspaceFileOutput {
    pub bytes_written: u32,
    /// True when `expected_previous` no longer matched the disk; nothing was written.
    pub conflicted: bool,
    /// The current on-disk content, present only when `conflicted` is true.
    pub disk_content: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchWorkspaceInput {
    pub workspace_root: String,
    pub query: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchWorkspaceMatch {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchWorkspaceOutput {
    pub matches: Vec<SearchWorkspaceMatch>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMissionSpecsInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionSpecEntry {
    pub relative_path: String,
    pub name: String,
    pub content: String,
    pub validation: Option<MissionValidationEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionValidationEntry {
    pub relative_path: String,
    pub content: String,
    pub history_relative_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMissionSpecsOutput {
    pub specs: Vec<MissionSpecEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveMissionValidationInput {
    pub workspace_root: String,
    pub spec_relative_path: String,
    pub report_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveMissionValidationOutput {
    pub relative_path: String,
    pub history_relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompileMissionSpecContextInput {
    pub workspace_root: String,
    pub spec_relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompiledMissionSpecContextFile {
    pub relative_path: String,
    pub created: bool,
    pub updated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompileMissionSpecContextOutput {
    pub files: Vec<CompiledMissionSpecContextFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionSpecContextStatusInput {
    pub workspace_root: String,
    pub spec_relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MissionSpecContextFileState {
    Current,
    Missing,
    Stale,
    Invalid,
    Symlink,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionSpecContextFileStatus {
    pub relative_path: String,
    pub state: MissionSpecContextFileState,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionSpecContextStatusOutput {
    pub current: bool,
    pub files: Vec<MissionSpecContextFileStatus>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListChildDirectoriesInput {
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListChildDirectoriesOutput {
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIdInput {
    pub repository_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitStatusInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitStatusOutput {
    pub staged: Vec<WorkspaceGitChangeEntry>,
    pub unstaged: Vec<WorkspaceGitChangeEntry>,
    pub current_branch: Option<String>,
    pub ahead_of_remote_count: u32,
    pub behind_of_remote_count: u32,
    pub conflict_count: u32,
    pub merge_in_progress: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrepareDelegationWorktreeInput {
    pub workspace_root: String,
    pub delegation_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrepareDelegationWorktreeOutput {
    pub worktree_path: String,
    pub branch: String,
    pub base_commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRemoveDelegationWorktreeInput {
    pub workspace_root: String,
    pub worktree_path: String,
    #[serde(default)]
    pub remove_branch: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceApplyDelegationWorktreeInput {
    pub workspace_root: String,
    pub worktree_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceApplyDelegationWorktreeOutput {
    pub changed_files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitPathInput {
    pub workspace_root: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitCommitPushInput {
    pub workspace_root: String,
    pub message: String,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitPushInput {
    pub workspace_root: String,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitCompleteMergeInput {
    pub workspace_root: String,
    pub forge_login: Option<String>,
    pub validation_config_hash: Option<String>,
    pub validation_commands: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceGitValidationStatus {
    NotConfigured,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitValidationReport {
    pub status: WorkspaceGitValidationStatus,
    pub source_path: Option<String>,
    pub steps: Vec<WorkspaceGitValidationStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitValidationConfigOutput {
    pub commands: Vec<String>,
    pub source_path: Option<String>,
    pub timeout_seconds: u64,
    pub config_hash: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceProjectTaskKind {
    Check,
    Fix,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProjectTask {
    pub id: String,
    pub label: Option<String>,
    pub command: String,
    pub kind: WorkspaceProjectTaskKind,
    pub cwd: Option<String>,
    pub timeout_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProjectAutomationConfigOutput {
    pub setup_command: Option<String>,
    pub tasks: Vec<WorkspaceProjectTask>,
    pub before_merge: Vec<String>,
    pub before_push: Vec<String>,
    pub source_path: String,
    pub config_hash: Option<String>,
    pub tracked_in_git: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSaveProjectAutomationInput {
    pub workspace_root: String,
    pub setup_command: Option<String>,
    pub tasks: Vec<WorkspaceProjectTask>,
    pub before_merge: Vec<String>,
    pub before_push: Vec<String>,
    pub expected_config_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRunProjectTasksInput {
    pub workspace_root: String,
    pub task_ids: Vec<String>,
    pub expected_config_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRunProjectTasksOutput {
    pub report: WorkspaceGitValidationReport,
    pub changed_files: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitCompleteMergeOutput {
    pub completed: bool,
    pub validation: WorkspaceGitValidationReport,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitSyncBaseInput {
    pub workspace_root: String,
    pub base_branch: Option<String>,
    pub forge_login: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitSyncBaseOutput {
    pub branch: String,
    pub base_branch: String,
    pub remote: String,
    pub updated: bool,
    pub conflict_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitConflictStateInput {
    pub workspace_root: String,
}

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

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitConflictContent {
    pub exists: bool,
    pub binary: bool,
    pub truncated: bool,
    pub byte_count: u64,
    pub mode: Option<String>,
    pub text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitConflictEntry {
    pub path: String,
    pub kind: WorkspaceGitConflictKind,
    pub base: WorkspaceGitConflictContent,
    pub current: WorkspaceGitConflictContent,
    pub incoming: WorkspaceGitConflictContent,
    pub result: WorkspaceGitConflictContent,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitAcceptConflictInput {
    pub workspace_root: String,
    pub relative_path: String,
    pub side: WorkspaceGitConflictSide,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitMarkConflictResolvedInput {
    pub workspace_root: String,
    pub relative_path: String,
    #[serde(default)]
    pub delete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceGitPreviewScope {
    Staged,
    Unstaged,
    Committed,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitFilePreviewInput {
    pub workspace_root: String,
    pub relative_path: String,
    pub status: String,
    pub scope: WorkspaceGitPreviewScope,
    pub base_branch: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitFilePreviewContentOutput {
    pub original_text: String,
    pub modified_text: String,
    pub inline: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceContinueFromBaseBranchInput {
    pub workspace_root: String,
    pub base_branch: Option<String>,
    pub target_branch: Option<String>,
    pub new_branch_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceContinueFromBaseBranchOutput {
    pub success: bool,
    pub branch: String,
    pub workspace_root: String,
    pub previous_workspace_root: String,
    pub workspace: Workspace,
}

fn normalize_git_relative_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn is_path_inside(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

fn collect_workspace_setup_hints(workspace: &Workspace) -> Vec<WorkspaceSetupHint> {
    detect_workspace_setup_suggestions(resolve_workspace_setup_root(workspace))
        .into_iter()
        .map(|suggestion| WorkspaceSetupHint {
            label: suggestion.label,
            command: suggestion.command,
            source_path: suggestion.source_path,
        })
        .collect()
}

fn collect_workspace_setup_suggestions(
    workspace: &Workspace,
) -> Vec<dcc_infra::git::WorkspaceSetupSuggestion> {
    detect_workspace_setup_suggestions(resolve_workspace_setup_root(workspace))
}

async fn execute_workspace_setup_report(workspace: &Workspace) -> WorkspaceSetupReport {
    let setup_suggestions = collect_workspace_setup_suggestions(workspace);
    match run_detected_workspace_setup(
        resolve_workspace_setup_root(workspace).to_string(),
        setup_suggestions,
    )
    .await
    {
        Ok(report) => report,
        Err(error) => WorkspaceSetupReport {
            status: WorkspaceSetupStatus::Failed,
            steps: Vec::new(),
            message: Some(format!(
                "Workspace was created, but the automatic setup runner failed: {error}"
            )),
        },
    }
}

async fn persist_workspace_setup_outcome(
    repo: &SqliteWorkspaceRepo,
    workspace: &mut Workspace,
    setup_report: &WorkspaceSetupReport,
) -> Result<(), String> {
    workspace.state = match setup_report.status {
        WorkspaceSetupStatus::Completed | WorkspaceSetupStatus::Skipped => WorkspaceState::Ready,
        WorkspaceSetupStatus::Warning | WorkspaceSetupStatus::Failed => {
            WorkspaceState::SetupPending
        }
    };
    workspace.setup_report = Some(setup_report.clone());
    workspace.updated_at = Utc::now().to_rfc3339();
    repo.save_workspace(workspace)
        .await
        .map_err(|error| error.to_string())
}

fn compile_active_mission_spec_context_for_workspace(
    workspace: &Workspace,
) -> Result<Option<String>, String> {
    match select_active_mission_spec_relative_path(
        resolve_workspace_active_root(workspace),
        &workspace.base_branch,
    )
    .and_then(|spec_relative_path| {
        spec_relative_path
            .map(|path| {
                compile_mission_spec_context_for_path(
                    resolve_workspace_active_root(workspace),
                    &path,
                )
            })
            .transpose()
    }) {
        Ok(_) => Ok(None),
        Err(error) => {
            eprintln!("[dcc] mission spec setup compile failed: {error}");
            Ok(Some(error))
        }
    }
}

fn append_mission_spec_compile_warning(
    setup_report: &WorkspaceSetupReport,
    compile_error: Option<String>,
) -> WorkspaceSetupReport {
    let Some(error) = compile_error.filter(|value| !value.trim().is_empty()) else {
        return setup_report.clone();
    };

    let detail =
        format!("Workspace setup completed, but mission spec context auto-compile failed: {error}");
    let mut next = setup_report.clone();
    next.steps.push(WorkspaceSetupStepReport {
        label: "Compile mission spec context".to_string(),
        command: "compile_mission_spec_context".to_string(),
        source_path: DCC_SPEC_CONTEXT_MANIFEST_PATH.to_string(),
        status: WorkspaceSetupStatus::Warning,
        detail: Some(detail.clone()),
    });
    if matches!(
        next.status,
        WorkspaceSetupStatus::Completed | WorkspaceSetupStatus::Skipped
    ) {
        next.status = WorkspaceSetupStatus::Warning;
    }
    if next.message.is_none() {
        next.message = Some(detail);
    }
    next
}

fn validate_git_relative_path(path: &str) -> Result<String, String> {
    let p = normalize_git_relative_path(path);
    if p.is_empty() {
        return Err("path is empty".to_string());
    }
    if p.contains('\0') {
        return Err("invalid path".to_string());
    }
    let path = Path::new(&p);
    if path.is_absolute() {
        return Err("invalid path".to_string());
    }
    let has_normal_component = path.components().any(|component| match component {
        Component::Normal(_) => true,
        Component::CurDir => false,
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => false,
    });
    if !has_normal_component {
        return Err("invalid path".to_string());
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("invalid path".to_string());
    }
    Ok(p)
}

fn resolve_default_branch_name(root: &str) -> Result<String, String> {
    let output = run_git_output(
        root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )?;
    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Some((_, branch)) = raw.rsplit_once('/') {
            if !branch.trim().is_empty() {
                return Ok(branch.trim().to_string());
            }
        }
    }
    Ok("main".to_string())
}

fn resolve_upstream_branch_counts(root: &str) -> Result<(u32, u32), String> {
    let output = run_git_output(
        root,
        &["rev-list", "--left-right", "--count", "@{u}...HEAD"],
    )?;
    if !output.status.success() {
        return Ok((0, 0));
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut parts = raw.split_whitespace();
    let behind = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let ahead = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    Ok((ahead, behind))
}

fn resolve_conflict_count(root: &str) -> Result<u32, String> {
    let output = run_git_output(root, &["diff", "--name-only", "--diff-filter=U"])?;
    if !output.status.success() {
        return Ok(0);
    }

    let count = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count();
    Ok(count as u32)
}

const MAX_CONFLICT_TEXT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnmergedIndexStage {
    mode: String,
    object_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct UnmergedIndexEntry {
    path: String,
    stages: BTreeMap<u8, UnmergedIndexStage>,
}

fn parse_unmerged_index_entries(stdout: &[u8]) -> Result<Vec<UnmergedIndexEntry>, String> {
    let mut entries = BTreeMap::<String, UnmergedIndexEntry>::new();

    for record in stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let separator = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| "invalid git ls-files -u record: missing path separator".to_string())?;
        let metadata = std::str::from_utf8(&record[..separator])
            .map_err(|_| "invalid git ls-files -u metadata".to_string())?;
        let mut fields = metadata.split_whitespace();
        let mode = fields
            .next()
            .ok_or_else(|| "invalid git ls-files -u record: missing mode".to_string())?;
        let object_id = fields
            .next()
            .ok_or_else(|| "invalid git ls-files -u record: missing object id".to_string())?;
        let stage = fields
            .next()
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|value| (1..=3).contains(value))
            .ok_or_else(|| "invalid git ls-files -u record: invalid stage".to_string())?;
        if fields.next().is_some()
            || !object_id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || object_id.len() < 40
        {
            return Err("invalid git ls-files -u metadata".to_string());
        }

        let path = String::from_utf8_lossy(&record[(separator + 1)..]).to_string();
        validate_git_relative_path(&path)?;
        let entry = entries
            .entry(path.clone())
            .or_insert_with(|| UnmergedIndexEntry {
                path,
                stages: BTreeMap::new(),
            });
        entry.stages.insert(
            stage,
            UnmergedIndexStage {
                mode: mode.to_string(),
                object_id: object_id.to_string(),
            },
        );
    }

    Ok(entries.into_values().collect())
}

fn classify_unmerged_entry(entry: &UnmergedIndexEntry) -> WorkspaceGitConflictKind {
    match (
        entry.stages.contains_key(&1),
        entry.stages.contains_key(&2),
        entry.stages.contains_key(&3),
    ) {
        (true, true, true) => WorkspaceGitConflictKind::BothModified,
        (false, true, true) => WorkspaceGitConflictKind::BothAdded,
        (true, false, true) => WorkspaceGitConflictKind::DeletedByCurrent,
        (true, true, false) => WorkspaceGitConflictKind::DeletedByIncoming,
        (true, false, false) => WorkspaceGitConflictKind::BothDeleted,
        _ => WorkspaceGitConflictKind::Other,
    }
}

fn conflict_content(bytes: Option<Vec<u8>>, mode: Option<String>) -> WorkspaceGitConflictContent {
    let Some(bytes) = bytes else {
        return WorkspaceGitConflictContent {
            exists: false,
            binary: false,
            truncated: false,
            byte_count: 0,
            mode,
            text: None,
        };
    };

    let byte_count = bytes.len() as u64;
    let truncated = bytes.len() > MAX_CONFLICT_TEXT_BYTES;
    let contains_nul = bytes.contains(&0);
    let utf8_valid = std::str::from_utf8(&bytes).is_ok();
    let text = if truncated || contains_nul || !utf8_valid {
        None
    } else {
        String::from_utf8(bytes).ok()
    };
    let binary = contains_nul || !utf8_valid;

    WorkspaceGitConflictContent {
        exists: true,
        binary,
        truncated,
        byte_count,
        mode,
        text,
    }
}

fn read_unmerged_stage(
    root: &str,
    stage: Option<&UnmergedIndexStage>,
) -> Result<WorkspaceGitConflictContent, String> {
    let Some(stage) = stage else {
        return Ok(conflict_content(None, None));
    };
    let output = run_git_output(root, &["cat-file", "blob", &stage.object_id])?;
    if !output.status.success() {
        return Err(git_output_err("git cat-file blob", &output.stderr));
    }
    Ok(conflict_content(
        Some(output.stdout),
        Some(stage.mode.clone()),
    ))
}

fn read_conflict_result(
    root: &str,
    relative_path: &str,
) -> Result<WorkspaceGitConflictContent, String> {
    let candidate = PathBuf::from(root).join(relative_path);
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Ok(WorkspaceGitConflictContent {
                exists: true,
                binary: true,
                truncated: false,
                byte_count: 0,
                mode: Some("120000".to_string()),
                text: None,
            });
        }
        Ok(metadata) if metadata.is_dir() => {
            return Ok(WorkspaceGitConflictContent {
                exists: true,
                binary: true,
                truncated: false,
                byte_count: 0,
                mode: Some("160000".to_string()),
                text: None,
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(conflict_content(None, None));
        }
        Err(error) => return Err(error.to_string()),
    }
    let Some(path) = resolve_worktree_file_path(root, relative_path)? else {
        return Ok(conflict_content(None, None));
    };
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(conflict_content(Some(bytes), None))
}

fn git_internal_path_exists(root: &str, name: &str) -> bool {
    let Ok(output) = run_git_output(root, &["rev-parse", "--git-path", name]) else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return false;
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path.exists()
    } else {
        PathBuf::from(root).join(path).exists()
    }
}

fn resolve_conflict_operation(root: &str, has_conflicts: bool) -> WorkspaceGitConflictOperation {
    if git_internal_path_exists(root, "MERGE_HEAD") {
        WorkspaceGitConflictOperation::Merge
    } else if git_internal_path_exists(root, "rebase-merge")
        || git_internal_path_exists(root, "rebase-apply")
    {
        WorkspaceGitConflictOperation::Rebase
    } else if git_internal_path_exists(root, "CHERRY_PICK_HEAD") {
        WorkspaceGitConflictOperation::CherryPick
    } else if git_internal_path_exists(root, "REVERT_HEAD") {
        WorkspaceGitConflictOperation::Revert
    } else if has_conflicts {
        WorkspaceGitConflictOperation::Unknown
    } else {
        WorkspaceGitConflictOperation::None
    }
}

fn resolve_merge_incoming_ref(root: &str) -> Option<String> {
    let merge_head = run_git_output(root, &["rev-parse", "--verify", "MERGE_HEAD"]).ok()?;
    if !merge_head.status.success() {
        return None;
    }
    let object_id = String::from_utf8_lossy(&merge_head.stdout)
        .trim()
        .to_string();
    if object_id.is_empty() {
        return None;
    }
    let name = run_git_output(
        root,
        &["name-rev", "--name-only", "--no-undefined", &object_id],
    )
    .ok()?;
    if !name.status.success() {
        return Some(object_id);
    }
    let value = String::from_utf8_lossy(&name.stdout).trim().to_string();
    if value.is_empty() {
        Some(object_id)
    } else {
        Some(value.strip_prefix("remotes/").unwrap_or(&value).to_string())
    }
}

fn workspace_git_conflict_state_inner(
    root: &str,
) -> Result<WorkspaceGitConflictStateOutput, String> {
    let output = run_git_output(root, &["ls-files", "-u", "-z"])?;
    if !output.status.success() {
        return Err(git_output_err("git ls-files -u", &output.stderr));
    }
    let unmerged = parse_unmerged_index_entries(&output.stdout)?;
    let operation = resolve_conflict_operation(root, !unmerged.is_empty());
    let incoming_ref = if operation == WorkspaceGitConflictOperation::Merge {
        resolve_merge_incoming_ref(root)
    } else {
        None
    };

    let mut conflicts = Vec::with_capacity(unmerged.len());
    for entry in unmerged {
        let base = read_unmerged_stage(root, entry.stages.get(&1))?;
        let current = read_unmerged_stage(root, entry.stages.get(&2))?;
        let incoming = read_unmerged_stage(root, entry.stages.get(&3))?;
        let result = read_conflict_result(root, &entry.path)?;
        conflicts.push(WorkspaceGitConflictEntry {
            kind: classify_unmerged_entry(&entry),
            path: entry.path,
            base,
            current,
            incoming,
            result,
        });
    }

    Ok(WorkspaceGitConflictStateOutput {
        operation,
        current_branch: resolve_current_branch_name(root).ok(),
        incoming_ref,
        conflicts,
    })
}

#[tauri::command]
pub async fn workspace_git_conflict_state(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitConflictStateInput,
) -> Result<WorkspaceGitConflictStateOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    workspace_git_conflict_state_inner(root)
}

fn load_unmerged_index_entries(root: &str) -> Result<Vec<UnmergedIndexEntry>, String> {
    let output = run_git_output(root, &["ls-files", "-u", "-z"])?;
    if !output.status.success() {
        return Err(git_output_err("git ls-files -u", &output.stderr));
    }
    parse_unmerged_index_entries(&output.stdout)
}

fn require_merge_conflict_entry(root: &str, path: &str) -> Result<UnmergedIndexEntry, String> {
    if resolve_conflict_operation(root, true) != WorkspaceGitConflictOperation::Merge {
        return Err("conflict actions currently support merge operations only".to_string());
    }
    load_unmerged_index_entries(root)?
        .into_iter()
        .find(|entry| entry.path == path)
        .ok_or_else(|| format!("path is not an unresolved merge conflict: {path}"))
}

#[tauri::command]
pub async fn workspace_git_accept_conflict(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitAcceptConflictInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let path = validate_git_relative_path(&input.relative_path)?;
    workspace_git_accept_conflict_inner(root, &path, input.side)
}

fn workspace_git_accept_conflict_inner(
    root: &str,
    path: &str,
    side: WorkspaceGitConflictSide,
) -> Result<(), String> {
    let entry = require_merge_conflict_entry(root, &path)?;
    let (stage, checkout_flag) = match side {
        WorkspaceGitConflictSide::Current => (2, "--ours"),
        WorkspaceGitConflictSide::Incoming => (3, "--theirs"),
    };

    if entry.stages.contains_key(&stage) {
        let checkout = run_git_output(root, &["checkout", checkout_flag, "--", &path])?;
        if !checkout.status.success() {
            return Err(git_output_err(
                "git checkout conflict side",
                &checkout.stderr,
            ));
        }
        let add = run_git_output(root, &["add", "--", &path])?;
        if add.status.success() {
            Ok(())
        } else {
            Err(git_output_err("git add", &add.stderr))
        }
    } else {
        let remove = run_git_output(root, &["rm", "-f", "--", &path])?;
        if remove.status.success() {
            Ok(())
        } else {
            Err(git_output_err("git rm", &remove.stderr))
        }
    }
}

#[tauri::command]
pub async fn workspace_git_mark_conflict_resolved(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitMarkConflictResolvedInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let path = validate_git_relative_path(&input.relative_path)?;
    workspace_git_mark_conflict_resolved_inner(root, &path, input.delete)
}

fn workspace_git_mark_conflict_resolved_inner(
    root: &str,
    path: &str,
    delete: bool,
) -> Result<(), String> {
    require_merge_conflict_entry(root, &path)?;

    let output = if delete {
        run_git_output(root, &["rm", "-f", "--", &path])?
    } else {
        let absolute = PathBuf::from(root).join(&path);
        if !absolute.is_file() {
            return Err("resolved file does not exist; choose deletion explicitly".to_string());
        }
        run_git_output(root, &["add", "--", &path])?
    };
    if output.status.success() {
        Ok(())
    } else if delete {
        Err(git_output_err("git rm", &output.stderr))
    } else {
        Err(git_output_err("git add", &output.stderr))
    }
}

#[tauri::command]
pub async fn workspace_git_abort_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitConflictStateInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    workspace_git_abort_merge_inner(root)
}

fn workspace_git_abort_merge_inner(root: &str) -> Result<(), String> {
    if resolve_conflict_operation(root, false) != WorkspaceGitConflictOperation::Merge {
        return Err("no merge is in progress".to_string());
    }
    let output = run_git_output(root, &["merge", "--abort"])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_output_err("git merge --abort", &output.stderr))
    }
}

#[tauri::command]
pub async fn workspace_git_validation_config(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitConflictStateInput,
) -> Result<WorkspaceGitValidationConfigOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let automation = read_workspace_automation_config(Path::new(root))?;
    let automation_source_path = automation.as_ref().map(|config| config.source_path.clone());
    let configured_tasks = automation
        .as_ref()
        .map(|config| {
            let by_id = config
                .tasks
                .iter()
                .map(|task| (task.id.as_str(), task))
                .collect::<BTreeMap<_, _>>();
            config
                .before_merge
                .iter()
                .map(|id| {
                    by_id
                        .get(id.as_str())
                        .copied()
                        .ok_or_else(|| format!("Unknown before_merge task `{id}`."))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(WorkspaceGitValidationConfigOutput {
        commands: configured_tasks
            .iter()
            .map(|task| task.command.clone())
            .collect(),
        source_path: if configured_tasks.is_empty() {
            None
        } else {
            automation_source_path
        },
        timeout_seconds: configured_tasks
            .iter()
            .map(|task| task.timeout_seconds)
            .max()
            .unwrap_or(WORKSPACE_VALIDATION_TIMEOUT.as_secs()),
        config_hash: workspace_validation_config_hash(root)?,
    })
}

fn project_task_from_repo(task: RepoAutomationTask) -> WorkspaceProjectTask {
    WorkspaceProjectTask {
        id: task.id,
        label: task.label,
        command: task.command,
        kind: match task.kind {
            RepoTaskKind::Check => WorkspaceProjectTaskKind::Check,
            RepoTaskKind::Fix => WorkspaceProjectTaskKind::Fix,
        },
        cwd: task.cwd,
        timeout_seconds: task.timeout_seconds,
    }
}

fn repo_task_from_project(task: &WorkspaceProjectTask) -> RepoAutomationTask {
    RepoAutomationTask {
        id: task.id.trim().to_string(),
        label: task
            .label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        command: task.command.trim().to_string(),
        kind: match task.kind {
            WorkspaceProjectTaskKind::Check => RepoTaskKind::Check,
            WorkspaceProjectTaskKind::Fix => RepoTaskKind::Fix,
        },
        cwd: task
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        timeout_seconds: task.timeout_seconds,
    }
}

#[tauri::command]
pub async fn workspace_project_automation_config(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitConflictStateInput,
) -> Result<WorkspaceProjectAutomationConfigOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let source_path = Path::new(root)
        .join(".dcc.toml")
        .to_string_lossy()
        .to_string();
    let tracked_in_git = workspace_automation_config_is_tracked(root);
    let config = read_workspace_automation_config(Path::new(root))?;
    Ok(match config {
        Some(config) => WorkspaceProjectAutomationConfigOutput {
            setup_command: config.setup_command,
            tasks: config
                .tasks
                .into_iter()
                .map(project_task_from_repo)
                .collect(),
            before_merge: config.before_merge,
            before_push: config.before_push,
            source_path: config.source_path,
            config_hash: workspace_validation_config_hash(root)?,
            tracked_in_git,
        },
        None => WorkspaceProjectAutomationConfigOutput {
            setup_command: None,
            tasks: Vec::new(),
            before_merge: Vec::new(),
            before_push: Vec::new(),
            source_path,
            config_hash: None,
            tracked_in_git,
        },
    })
}

fn workspace_automation_config_is_tracked(root: &str) -> bool {
    run_git_output(root, &["ls-files", "--error-unmatch", "--", ".dcc.toml"])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn workspace_save_project_automation(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceSaveProjectAutomationInput,
) -> Result<WorkspaceProjectAutomationConfigOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    if workspace_validation_config_hash(root)? != input.expected_config_hash {
        return Err("The .dcc.toml configuration changed. Reload it before saving.".to_string());
    }
    let source_path = Path::new(root).join(".dcc.toml");
    let tasks = input
        .tasks
        .iter()
        .map(repo_task_from_project)
        .collect::<Vec<_>>();
    let normalized = RepoAutomationConfig {
        setup_command: input
            .setup_command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        tasks,
        before_merge: input.before_merge.clone(),
        before_push: input.before_push.clone(),
        source_path: source_path.to_string_lossy().to_string(),
    };
    validate_workspace_automation_config(&normalized)?;

    let raw = match fs::read_to_string(&source_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read .dcc.toml: {error}")),
    };
    let mut document = if raw.trim().is_empty() {
        TomlDocument::new()
    } else {
        raw.parse::<TomlDocument>()
            .map_err(|error| format!("invalid .dcc.toml: {error}"))?
    };
    document.remove("setup_command");
    document.remove("validation_commands");
    if !document.contains_key("scripts") {
        document["scripts"] = TomlItem::Table(TomlTable::new());
    }
    if let Some(scripts) = document["scripts"].as_table_mut() {
        scripts.remove("validate");
        match normalized.setup_command.as_deref() {
            Some(command) => scripts["setup"] = toml_value(command),
            None => {
                scripts.remove("setup");
            }
        }
    }

    if normalized.tasks.is_empty() {
        document.remove("tasks");
    } else {
        let mut table = TomlTable::new();
        for task in &normalized.tasks {
            let mut item = TomlTable::new();
            item["command"] = toml_value(&task.command);
            item["kind"] = toml_value(match task.kind {
                RepoTaskKind::Check => "check",
                RepoTaskKind::Fix => "fix",
            });
            if let Some(label) = task.label.as_deref() {
                item["label"] = toml_value(label);
            }
            if let Some(cwd) = task.cwd.as_deref() {
                item["cwd"] = toml_value(cwd);
            }
            if task.timeout_seconds != WORKSPACE_VALIDATION_TIMEOUT.as_secs() {
                item["timeout_seconds"] = toml_value(task.timeout_seconds as i64);
            }
            table[&task.id] = TomlItem::Table(item);
        }
        document["tasks"] = TomlItem::Table(table);
    }

    if normalized.before_merge.is_empty() && normalized.before_push.is_empty() {
        document.remove("hooks");
    } else {
        let mut hooks = TomlTable::new();
        for (name, ids) in [
            ("before_merge", &normalized.before_merge),
            ("before_push", &normalized.before_push),
        ] {
            let mut values = TomlArray::new();
            for id in ids {
                values.push(id.as_str());
            }
            hooks[name] = toml_value(values);
        }
        document["hooks"] = TomlItem::Table(hooks);
    }

    fs::write(&source_path, document.to_string())
        .map_err(|error| format!("failed to write .dcc.toml: {error}"))?;
    workspace_project_automation_config(
        state,
        WorkspaceGitConflictStateInput {
            workspace_root: root.to_string(),
        },
    )
    .await
}

fn resolve_task_execution_root(root: &str, task: &RepoAutomationTask) -> Result<String, String> {
    let root_path = Path::new(root)
        .canonicalize()
        .map_err(|error| format!("failed to resolve workspace root: {error}"))?;
    let execution_path = match task.cwd.as_deref() {
        Some(cwd) => root_path.join(cwd),
        None => root_path.clone(),
    }
    .canonicalize()
    .map_err(|error| format!("task `{}` cwd is unavailable: {error}", task.id))?;
    if !execution_path.starts_with(&root_path) {
        return Err(format!("task `{}` cwd escapes the workspace", task.id));
    }
    Ok(execution_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn workspace_run_project_tasks(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceRunProjectTasksInput,
) -> Result<WorkspaceRunProjectTasksOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim().to_string();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    if workspace_validation_config_hash(&root)? != input.expected_config_hash {
        return Err(
            "The .dcc.toml configuration changed. Review the tasks and try again.".to_string(),
        );
    }
    let config = read_workspace_automation_config(Path::new(&root))?
        .ok_or_else(|| "No project automation is configured.".to_string())?;
    let by_id = config
        .tasks
        .into_iter()
        .map(|task| (task.id.clone(), task))
        .collect::<BTreeMap<_, _>>();
    let mut requested = Vec::new();
    let mut seen = BTreeSet::new();
    for id in input.task_ids {
        if seen.insert(id.clone()) {
            requested.push(
                by_id
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| format!("Unknown project task `{id}`."))?,
            );
        }
    }
    if requested.is_empty() {
        return Err("Select at least one project task.".to_string());
    }
    if requested.len() > 20 {
        return Err("At most 20 project tasks can run at once.".to_string());
    }
    let source_path = Some(config.source_path);
    let root_for_run = root.clone();
    let (report, changed_files) = tauri::async_runtime::spawn_blocking(
        move || -> Result<(WorkspaceGitValidationReport, bool), String> {
            let mut steps = Vec::new();
            let mut changed_files = false;
            for task in requested {
                let before_task = workspace_validation_fingerprint(&root_for_run)?;
                let execution_root = resolve_task_execution_root(&root_for_run, &task)?;
                let execution = run_workspace_task_command(
                    &execution_root,
                    &task.command,
                    task.timeout_seconds,
                )?;
                let success = execution.success;
                steps.push(WorkspaceGitValidationStep {
                    command: task.command,
                    success,
                    exit_code: execution.exit_code,
                    output: execution.output,
                    timed_out: execution.timed_out,
                    duration_ms: execution.duration_ms,
                    truncated: execution.truncated,
                });
                let after_task = workspace_validation_fingerprint(&root_for_run)?;
                let task_changed_files = before_task != after_task;
                changed_files |= task_changed_files;
                if task_changed_files && task.kind == RepoTaskKind::Check {
                    steps.push(WorkspaceGitValidationStep {
                        command: "DCC workspace consistency check".to_string(),
                        success: false,
                        exit_code: None,
                        output: "A check task changed tracked files or the index. Review the changes before continuing.".to_string(),
                        timed_out: false,
                        duration_ms: 0,
                        truncated: false,
                    });
                    return Ok((
                        WorkspaceGitValidationReport {
                            status: WorkspaceGitValidationStatus::Failed,
                            source_path,
                            steps,
                        },
                        changed_files,
                    ));
                }
                if !success {
                    return Ok((
                        WorkspaceGitValidationReport {
                            status: WorkspaceGitValidationStatus::Failed,
                            source_path,
                            steps,
                        },
                        changed_files,
                    ));
                }
            }
            Ok((
                WorkspaceGitValidationReport {
                    status: WorkspaceGitValidationStatus::Passed,
                    source_path,
                    steps,
                },
                changed_files,
            ))
        },
    )
    .await
    .map_err(|error| error.to_string())??;
    Ok(WorkspaceRunProjectTasksOutput {
        report,
        changed_files,
    })
}

#[tauri::command]
pub async fn workspace_git_complete_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitCompleteMergeInput,
) -> Result<WorkspaceGitCompleteMergeOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    require_merge_ready_to_complete(root)?;
    let configured_automation = read_workspace_automation_config(Path::new(root))?;
    let configured_tasks = configured_automation
        .as_ref()
        .map(|config| {
            let by_id = config
                .tasks
                .iter()
                .map(|task| (task.id.as_str(), task))
                .collect::<BTreeMap<_, _>>();
            config
                .before_merge
                .iter()
                .map(|id| {
                    by_id
                        .get(id.as_str())
                        .cloned()
                        .cloned()
                        .ok_or_else(|| format!("Unknown before_merge task `{id}`."))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();
    let configured_commands = configured_tasks
        .iter()
        .map(|task| task.command.clone())
        .collect::<Vec<_>>();
    if workspace_validation_config_hash(root)? != input.validation_config_hash
        || configured_commands != input.validation_commands
    {
        return Err(
            "The .dcc.toml validation configuration changed. Review the commands and confirm again."
                .to_string(),
        );
    }

    let validation_root = root.to_string();
    let validation_source_path = configured_automation.map(|config| config.source_path);
    let (mut validation, validated_fingerprint) = tauri::async_runtime::spawn_blocking(move || {
        workspace_git_run_automation_validations_inner(
            &validation_root,
            configured_tasks,
            validation_source_path,
        )
    })
    .await
    .map_err(|error| error.to_string())??;
    if validation.status == WorkspaceGitValidationStatus::Failed {
        return Ok(WorkspaceGitCompleteMergeOutput {
            completed: false,
            validation,
        });
    }

    require_merge_ready_to_complete(root)?;
    if workspace_validation_config_hash(root)? != input.validation_config_hash {
        return Err(
            "The .dcc.toml validation configuration changed while validations were running. Review the commands and confirm again."
                .to_string(),
        );
    }
    if let Some(validated_fingerprint) = validated_fingerprint {
        let current_fingerprint = workspace_validation_fingerprint(root)?;
        if current_fingerprint != validated_fingerprint {
            validation.status = WorkspaceGitValidationStatus::Failed;
            validation.steps.push(WorkspaceGitValidationStep {
                command: "DCC workspace consistency check".to_string(),
                success: false,
                exit_code: None,
                output: "The staged result or tracked working-tree changes changed while validations were running. Run validations again.".to_string(),
                timed_out: false,
                duration_ms: 0,
                truncated: false,
            });
            return Ok(WorkspaceGitCompleteMergeOutput {
                completed: false,
                validation,
            });
        }
    }
    workspace_git_complete_merge_commit_inner(root)?;

    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    push_current_branch(
        &state,
        root,
        protected_branch.as_deref(),
        input.forge_login.as_deref(),
    )
    .await?;
    Ok(WorkspaceGitCompleteMergeOutput {
        completed: true,
        validation,
    })
}

fn workspace_git_run_automation_validations_inner(
    root: &str,
    tasks: Vec<RepoAutomationTask>,
    source_path: Option<String>,
) -> Result<(WorkspaceGitValidationReport, Option<String>), String> {
    if tasks.is_empty() {
        return Ok((
            WorkspaceGitValidationReport {
                status: WorkspaceGitValidationStatus::NotConfigured,
                source_path,
                steps: Vec::new(),
            },
            None,
        ));
    }
    let fingerprint = workspace_validation_fingerprint(root)?;
    let mut steps = Vec::with_capacity(tasks.len());
    for task in tasks {
        let execution_root = resolve_task_execution_root(root, &task)?;
        let execution =
            run_workspace_task_command(&execution_root, &task.command, task.timeout_seconds)?;
        let success = execution.success;
        steps.push(WorkspaceGitValidationStep {
            command: task.command,
            success,
            exit_code: execution.exit_code,
            output: execution.output,
            timed_out: execution.timed_out,
            duration_ms: execution.duration_ms,
            truncated: execution.truncated,
        });
        if !success {
            return Ok((
                WorkspaceGitValidationReport {
                    status: WorkspaceGitValidationStatus::Failed,
                    source_path,
                    steps,
                },
                Some(fingerprint),
            ));
        }
    }
    let after = workspace_validation_fingerprint(root)?;
    if after != fingerprint {
        steps.push(WorkspaceGitValidationStep {
            command: "DCC workspace consistency check".to_string(),
            success: false,
            exit_code: None,
            output: "A validation command changed the staged result or tracked working-tree files. Review the changes, stage the intended result, and validate again.".to_string(),
            timed_out: false,
            duration_ms: 0,
            truncated: false,
        });
        return Ok((
            WorkspaceGitValidationReport {
                status: WorkspaceGitValidationStatus::Failed,
                source_path,
                steps,
            },
            Some(fingerprint),
        ));
    }
    Ok((
        WorkspaceGitValidationReport {
            status: WorkspaceGitValidationStatus::Passed,
            source_path,
            steps,
        },
        Some(fingerprint),
    ))
}

fn workspace_git_complete_merge_commit_inner(root: &str) -> Result<(), String> {
    require_merge_ready_to_complete(root)?;

    let commit = run_git_output(root, &["commit", "--no-edit"])?;
    if !commit.status.success() {
        return Err(git_output_err("git commit --no-edit", &commit.stderr));
    }
    Ok(())
}

fn require_merge_ready_to_complete(root: &str) -> Result<(), String> {
    if resolve_conflict_operation(root, false) != WorkspaceGitConflictOperation::Merge {
        return Err("no merge is in progress".to_string());
    }
    let unresolved = resolve_conflict_count(root)?;
    if unresolved > 0 {
        return Err(format!(
            "resolve {unresolved} remaining conflict{} before completing the merge",
            if unresolved == 1 { "" } else { "s" }
        ));
    }

    Ok(())
}

#[cfg(test)]
fn workspace_git_run_validations_inner(
    root: &str,
) -> Result<(WorkspaceGitValidationReport, Option<String>), String> {
    let Some(config) = read_workspace_validation_config(Path::new(root))? else {
        return workspace_git_run_confirmed_validations_inner(root, Vec::new(), None);
    };
    workspace_git_run_confirmed_validations_inner(root, config.commands, Some(config.source_path))
}

#[cfg(test)]
fn workspace_git_run_confirmed_validations_inner(
    root: &str,
    commands: Vec<String>,
    source_path: Option<String>,
) -> Result<(WorkspaceGitValidationReport, Option<String>), String> {
    if commands.is_empty() {
        return Ok((
            WorkspaceGitValidationReport {
                status: WorkspaceGitValidationStatus::NotConfigured,
                source_path,
                steps: Vec::new(),
            },
            None,
        ));
    }
    let fingerprint = workspace_validation_fingerprint(root)?;
    let mut steps = Vec::with_capacity(commands.len());
    for command in commands {
        let execution = run_workspace_validation_command(root, &command)?;
        let success = execution.success;
        steps.push(WorkspaceGitValidationStep {
            command,
            success,
            exit_code: execution.exit_code,
            output: execution.output,
            timed_out: execution.timed_out,
            duration_ms: execution.duration_ms,
            truncated: execution.truncated,
        });
        if !success {
            return Ok((
                WorkspaceGitValidationReport {
                    status: WorkspaceGitValidationStatus::Failed,
                    source_path,
                    steps,
                },
                Some(fingerprint),
            ));
        }
    }
    let after = workspace_validation_fingerprint(root)?;
    if after != fingerprint {
        steps.push(WorkspaceGitValidationStep {
            command: "DCC workspace consistency check".to_string(),
            success: false,
            exit_code: None,
            output: "A validation command changed the staged result or tracked working-tree files. Review the changes, stage the intended result, and validate again.".to_string(),
            timed_out: false,
            duration_ms: 0,
            truncated: false,
        });
        return Ok((
            WorkspaceGitValidationReport {
                status: WorkspaceGitValidationStatus::Failed,
                source_path,
                steps,
            },
            Some(fingerprint),
        ));
    }
    Ok((
        WorkspaceGitValidationReport {
            status: WorkspaceGitValidationStatus::Passed,
            source_path,
            steps,
        },
        Some(fingerprint),
    ))
}

fn workspace_validation_fingerprint(root: &str) -> Result<String, String> {
    let tree = run_git_output(root, &["write-tree"])?;
    if !tree.status.success() {
        return Err(git_output_err("git write-tree", &tree.stderr));
    }
    let worktree = run_git_output(root, &["diff", "--binary", "--no-ext-diff"])?;
    if !worktree.status.success() {
        return Err(git_output_err("git diff", &worktree.stderr));
    }
    let mut hasher = Sha256::new();
    hasher.update(&tree.stdout);
    hasher.update([0]);
    hasher.update(&worktree.stdout);
    hasher.update([0]);
    match fs::read(Path::new(root).join(".dcc.toml")) {
        Ok(config) => hasher.update(config),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("failed to fingerprint .dcc.toml: {error}")),
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn workspace_validation_config_hash(root: &str) -> Result<Option<String>, String> {
    match fs::read(Path::new(root).join(".dcc.toml")) {
        Ok(config) => {
            let mut hasher = Sha256::new();
            hasher.update(config);
            Ok(Some(format!("{:x}", hasher.finalize())))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to fingerprint .dcc.toml: {error}")),
    }
}

async fn refresh_repository_forge_metadata(
    repo: &SqliteWorkspaceRepo,
    workspace: &Workspace,
) -> Result<(), String> {
    let repository_id = RepositoryId(workspace.root_path.clone());
    let Some(mut repository) = repo
        .get_repository(&repository_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };

    let remote_info = resolve_workspace_remote_info(&workspace.root_path)?;
    repository.remote = remote_info.as_ref().map(|info| info.remote_name.clone());
    repository.remote_url = remote_info.as_ref().map(|info| info.remote_url.clone());
    repository.forge_provider = remote_info.as_ref().map(|info| match info.provider {
        crate::commands::forge_commands::ForgeCliProvider::Github => "github".to_string(),
        crate::commands::forge_commands::ForgeCliProvider::Gitlab => "gitlab".to_string(),
    });
    repo.save_repository(&repository)
        .await
        .map_err(|error| error.to_string())?;

    if repository
        .forge_login
        .as_deref()
        .is_some_and(|login| !login.trim().is_empty())
    {
        return Ok(());
    }

    let _ = crate::commands::forge::accounts::auto_bind_repository(repo, &repository_id);

    Ok(())
}

async fn push_current_branch_inner(
    db_path: &Path,
    root: &str,
    protected_branch: Option<&str>,
    forge_login: Option<&str>,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    if let Some(source) = find_workspace_by_root(&repo, root)
        .await?
        .and_then(|workspace| workspace.source)
    {
        let push_target = source.push_target.unwrap_or(WorkspacePushTarget {
            remote_name: source.remote_name,
            branch_name: source.head_branch,
            remote_url: None,
            remote_created: false,
        });
        let remote_tracking_ref = format!(
            "refs/remotes/{}/{}",
            push_target.remote_name, push_target.branch_name
        );
        let source_ref = format!("refs/heads/{}", push_target.branch_name);
        let refspec = format!("+{source_ref}:{remote_tracking_ref}");
        let fetch = run_git_network_output_with_workspace_auth(
            db_path,
            root,
            &["fetch", &push_target.remote_name, &refspec],
            forge_login,
        )?;
        if !fetch.status.success() {
            return Err(git_output_err("git fetch source branch", &fetch.stderr));
        }
        let ancestry = run_git_output(
            root,
            &["merge-base", "--is-ancestor", &remote_tracking_ref, "HEAD"],
        )?;
        if !ancestry.status.success() {
            return Err(format!(
                "The remote branch `{}` changed after this workspace was opened. Sync or merge its latest commits before pushing.",
                push_target.branch_name
            ));
        }
        return push_branch_refspec_to_remote(
            db_path,
            root,
            &push_target.remote_name,
            &push_target.branch_name,
            forge_login,
        );
    }

    let branch = ensure_pushable_branch(root, protected_branch)?;
    push_branch_refspec(db_path, root, &branch, forge_login)
}

pub(crate) async fn push_current_branch(
    state: &WorkspaceCommandState,
    root: &str,
    protected_branch: Option<&str>,
    forge_login: Option<&str>,
) -> Result<(), String> {
    let result =
        push_current_branch_inner(&state.db_path, root, protected_branch, forge_login).await;
    match result {
        Ok(()) => {
            clear_workspace_delivery_failure(state, root, WorkspaceDeliveryFailureOperation::Push);
            Ok(())
        }
        Err(error) => {
            capture_workspace_delivery_failure(
                state,
                root,
                WorkspaceDeliveryFailureOperation::Push,
                &error,
                CaptureDeliveryFailureOptions::default(),
            )
            .await;
            Err(error)
        }
    }
}

fn path_is_tracked(root: &str, rel: &str) -> bool {
    git_command_succeeds(root, &["ls-files", "--error-unmatch", "--", rel])
}

#[tauri::command]
pub async fn workspace_git_stage_all(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let output = run_git_output(root, &["add", "-A"])?;
    if output.status.success() {
        return Ok(());
    }

    Err(git_output_err("git add -A", &output.stderr))
}

fn git_diff_output_text(output: std::process::Output, command: &str) -> Result<String, String> {
    if output.status.success() || output.status.code() == Some(1) {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    Err(git_output_err(command, &output.stderr))
}

fn run_git_diff_text(root: &str, args: &[&str], command: &str) -> Result<String, String> {
    let output = run_git_output(root, args)?;
    git_diff_output_text(output, command)
}

fn run_git_show_text(
    root: &str,
    revision_path: &str,
    command: &str,
) -> Result<Option<String>, String> {
    let output = run_git_output(root, &["show", revision_path])?;
    if output.status.success() {
        return Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let missing_markers = [
        "Path '",
        "path '",
        "does not exist",
        "exists on disk, but not in",
        "does not have a commit",
    ];
    if missing_markers.iter().any(|marker| stderr.contains(marker)) {
        return Ok(None);
    }

    Err(git_output_err(command, &output.stderr))
}

fn resolve_worktree_file_path(root: &str, rel: &str) -> Result<Option<PathBuf>, String> {
    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let candidate = root_canonical.join(rel);
    let metadata = match fs::symlink_metadata(&candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() && !metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !is_path_inside(&canonical, &root_canonical) {
        return Err("path escapes workspace".to_string());
    }
    if !canonical.is_file() {
        return Ok(None);
    }
    Ok(Some(canonical))
}

fn resolve_worktree_write_path(root: &str, rel: &str) -> Result<PathBuf, String> {
    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let candidate = root_canonical.join(rel);
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) => {
            if !metadata.is_file() && !metadata.file_type().is_symlink() {
                return Err("path is not a file".to_string());
            }
            let canonical = candidate
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !is_path_inside(&canonical, &root_canonical) {
                return Err("path escapes workspace".to_string());
            }
            if !canonical.is_file() {
                return Err("path is not a file".to_string());
            }
            Ok(canonical)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = candidate
                .parent()
                .ok_or_else(|| "invalid path".to_string())?;
            let parent_canonical = parent.canonicalize().map_err(|error| error.to_string())?;
            if !is_path_inside(&parent_canonical, &root_canonical) {
                return Err("path escapes workspace".to_string());
            }
            let file_name = candidate
                .file_name()
                .ok_or_else(|| "invalid path".to_string())?;
            Ok(parent_canonical.join(file_name))
        }
        Err(error) => Err(error.to_string()),
    }
}

fn read_worktree_file_text(root: &str, rel: &str) -> Result<Option<String>, String> {
    let Some(path) = resolve_worktree_file_path(root, rel)? else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
}

/// `git add -- path`.
#[tauri::command]
pub async fn workspace_git_stage_file(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let path = validate_git_relative_path(&input.relative_path)?;
    let output = run_git_output(root, &["add", "--", &path])?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_err("git add", &output.stderr))
}

/// `git restore --staged` with `git reset HEAD --` fallback.
#[tauri::command]
pub async fn workspace_git_unstage_file(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let path = validate_git_relative_path(&input.relative_path)?;
    let output = run_git_output(root, &["restore", "--staged", "--", &path])?;
    if output.status.success() {
        return Ok(());
    }
    let fallback = run_git_output(root, &["reset", "HEAD", "--", &path])?;
    if fallback.status.success() {
        return Ok(());
    }
    Err(git_output_err("git reset", &fallback.stderr))
}

/// Tracked: `git checkout HEAD -- path`; untracked file: remove.
#[tauri::command]
pub async fn workspace_git_discard_file(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let path = validate_git_relative_path(&input.relative_path)?;
    let absolute = PathBuf::from(root).join(&path);

    if path_is_tracked(root, &path) {
        let output = run_git_output(root, &["checkout", "HEAD", "--", &path])?;
        if output.status.success() {
            return Ok(());
        }
        return Err(git_output_err("git checkout", &output.stderr));
    }

    if absolute.is_file() {
        fs::remove_file(&absolute).map_err(|e| e.to_string())?;
        return Ok(());
    }

    Err("cannot discard: path is not a tracked file or a single untracked file".to_string())
}

fn parse_git_numstat_maps(root: &str, cached: bool) -> Result<HashMap<String, (u32, u32)>, String> {
    let output = if cached {
        run_git_output(root, &["diff", "--cached", "--numstat", "-z"])?
    } else {
        run_git_output(root, &["diff", "--numstat", "-z"])?
    };
    if !output.status.success() {
        return Ok(HashMap::new());
    }
    Ok(parse_numstat_z(&output.stdout))
}

fn join_workspace_path(root: &str, rel: &str) -> String {
    PathBuf::from(root).join(rel).to_string_lossy().to_string()
}

/// `git diff --cached --quiet` → `true` if there is at least one staged change.
fn git_has_staged_changes(root: &str) -> Result<bool, String> {
    let output = run_git_output(root, &["diff", "--cached", "--quiet"])?;
    let code = output.status.code();
    if code == Some(0) {
        return Ok(false);
    }
    if code == Some(1) {
        return Ok(true);
    }
    Err(format!(
        "git diff --cached failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Commit staged changes and push (requires at least one staged path).
#[tauri::command]
pub async fn workspace_git_commit_push(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitCommitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let message = input.message.trim();
    if message.is_empty() {
        return Err("commit message is empty".to_string());
    }

    if !git_has_staged_changes(root)? {
        return Err("nothing to commit — stage changes first".to_string());
    }

    let commit = run_git_output(root, &["commit", "-m", message])?;
    if !commit.status.success() {
        let error = git_output_err("git commit", &commit.stderr);
        capture_workspace_delivery_failure(
            &state,
            root,
            WorkspaceDeliveryFailureOperation::Push,
            &error,
            CaptureDeliveryFailureOptions::default(),
        )
        .await;
        return Err(error);
    }

    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    push_current_branch(
        &state,
        root,
        protected_branch.as_deref(),
        input.forge_login.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn workspace_git_push(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    push_current_branch(
        &state,
        root,
        protected_branch.as_deref(),
        input.forge_login.as_deref(),
    )
    .await
}

fn normalize_base_branch_for_sync(value: &str, remote: &str) -> Option<String> {
    let mut trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "HEAD" {
        return None;
    }

    if let Some(stripped) = trimmed.strip_prefix("refs/heads/") {
        trimmed = stripped.trim();
    } else if let Some(stripped) = trimmed.strip_prefix("refs/remotes/") {
        trimmed = stripped.trim();
    }

    for prefix in [format!("{remote}/"), "origin/".to_string()] {
        if let Some(stripped) = trimmed.strip_prefix(prefix.as_str()) {
            trimmed = stripped.trim();
            break;
        }
    }

    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn validate_branch_for_fetch(root: &str, branch: &str) -> Result<(), String> {
    let output = run_git_output(root, &["check-ref-format", "--branch", branch])?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_err(
        "git check-ref-format --branch",
        &output.stderr,
    ))
}

fn remote_tracking_ref(remote: &str, branch: &str) -> String {
    format!("refs/remotes/{remote}/{branch}")
}

fn remote_branch_fetch_refspec(remote: &str, branch: &str) -> String {
    format!(
        "+refs/heads/{branch}:{}",
        remote_tracking_ref(remote, branch)
    )
}

async fn persist_workspace_base_branch(
    state: &State<'_, WorkspaceCommandState>,
    workspace_root: &str,
    base_branch: &str,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let Some(mut workspace) = find_workspace_by_root(&repo, workspace_root).await? else {
        return Ok(());
    };
    if workspace.base_branch.trim() == base_branch {
        return Ok(());
    }

    workspace.base_branch = base_branch.to_string();
    workspace.updated_at = Utc::now().to_rfc3339();
    repo.save_workspace(&workspace)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn workspace_git_sync_base(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitSyncBaseInput,
) -> Result<WorkspaceGitSyncBaseOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    if git_command_succeeds(root, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]) {
        return Err(
            "a merge is already in progress; resolve it before updating the base".to_string(),
        );
    }

    let remote = resolve_default_remote_name(root)?;
    let workspace_target_branch = resolve_workspace_target_branch(&state, root).await;
    let default_branch = resolve_default_branch_name(root).ok();
    let base_branch = input
        .base_branch
        .as_deref()
        .and_then(|branch| normalize_base_branch_for_sync(branch, &remote))
        .or_else(|| {
            workspace_target_branch
                .as_deref()
                .and_then(|branch| normalize_base_branch_for_sync(branch, &remote))
        })
        .or_else(|| {
            default_branch
                .as_deref()
                .and_then(|branch| normalize_base_branch_for_sync(branch, &remote))
        })
        .unwrap_or_else(|| "main".to_string());

    validate_branch_for_fetch(root, &base_branch)?;
    let base_ref = remote_tracking_ref(&remote, &base_branch);
    let fetch_refspec = remote_branch_fetch_refspec(&remote, &base_branch);
    let branch = resolve_current_branch_name(root)?;
    let before = resolve_current_commit_sha(root)?.unwrap_or_default();
    let fetch = match run_git_network_output_with_workspace_auth(
        &state.db_path,
        root,
        &["fetch", &remote, &fetch_refspec],
        input.forge_login.as_deref(),
    ) {
        Ok(fetch) => fetch,
        Err(error) => {
            capture_workspace_delivery_failure(
                &state,
                root,
                WorkspaceDeliveryFailureOperation::Fetch,
                &error,
                CaptureDeliveryFailureOptions {
                    remote: Some(remote.clone()),
                    external_url: None,
                },
            )
            .await;
            return Err(error);
        }
    };
    if !fetch.status.success() {
        let error = git_output_err("git fetch", &fetch.stderr);
        capture_workspace_delivery_failure(
            &state,
            root,
            WorkspaceDeliveryFailureOperation::Fetch,
            &error,
            CaptureDeliveryFailureOptions {
                remote: Some(remote.clone()),
                external_url: None,
            },
        )
        .await;
        return Err(error);
    }
    clear_workspace_delivery_failure(&state, root, WorkspaceDeliveryFailureOperation::Fetch);

    let merge = match run_git_output(root, &["merge", "--no-edit", &base_ref]) {
        Ok(merge) => merge,
        Err(error) => {
            capture_workspace_delivery_failure(
                &state,
                root,
                WorkspaceDeliveryFailureOperation::Pull,
                &error,
                CaptureDeliveryFailureOptions {
                    remote: Some(remote.clone()),
                    external_url: None,
                },
            )
            .await;
            return Err(error);
        }
    };
    let conflict_count = resolve_conflict_count(root).unwrap_or(0);
    if !merge.status.success() {
        let mut detail = git_output_err("git merge", &merge.stderr);
        if conflict_count > 0 {
            detail = format!(
                "{detail}\nMerge left {conflict_count} conflicting file(s) in the worktree."
            );
        }
        capture_workspace_delivery_failure(
            &state,
            root,
            WorkspaceDeliveryFailureOperation::Pull,
            &detail,
            CaptureDeliveryFailureOptions {
                remote: Some(remote.clone()),
                external_url: None,
            },
        )
        .await;
        return Err(detail);
    }
    clear_workspace_delivery_failure(&state, root, WorkspaceDeliveryFailureOperation::Pull);

    let after = resolve_current_commit_sha(root)?.unwrap_or_default();
    persist_workspace_base_branch(&state, root, &base_branch).await?;
    Ok(WorkspaceGitSyncBaseOutput {
        branch,
        base_branch,
        remote,
        updated: before != after,
        conflict_count,
    })
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// `git status --porcelain` → staged / unstaged rows.
fn workspace_git_status_inner(workspace_root: &str) -> Result<WorkspaceGitStatusOutput, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Ok(WorkspaceGitStatusOutput {
            staged: vec![],
            unstaged: vec![],
            current_branch: None,
            ahead_of_remote_count: 0,
            behind_of_remote_count: 0,
            conflict_count: 0,
            merge_in_progress: false,
        });
    }

    let cached_stats = parse_git_numstat_maps(root, true)?;
    let unstaged_stats = parse_git_numstat_maps(root, false)?;

    let staged_output = run_git_output(root, &["diff", "--cached", "--name-status", "-z"])?;
    if !staged_output.status.success() {
        return Err(format!(
            "git diff --cached --name-status failed: {}",
            String::from_utf8_lossy(&staged_output.stderr)
        ));
    }
    let unstaged_output = run_git_output(root, &["diff", "--name-status", "-z"])?;
    if !unstaged_output.status.success() {
        return Err(format!(
            "git diff --name-status failed: {}",
            String::from_utf8_lossy(&unstaged_output.stderr)
        ));
    }
    let untracked_output =
        run_git_output(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    if !untracked_output.status.success() {
        return Err(format!(
            "git ls-files --others failed: {}",
            String::from_utf8_lossy(&untracked_output.stderr)
        ));
    }

    let mut staged: Vec<WorkspaceGitChangeEntry> = parse_name_status_z(&staged_output.stdout)
        .into_iter()
        .map(|entry| {
            let (ins, del) = cached_stats.get(&entry.path).copied().unwrap_or((0, 0));
            WorkspaceGitChangeEntry {
                path: entry.path.clone(),
                name: file_name_from_path(&entry.path),
                absolute_path: join_workspace_path(root, &entry.path),
                status: entry.status,
                insertions: ins,
                deletions: del,
            }
        })
        .collect();

    let mut unstaged: Vec<WorkspaceGitChangeEntry> = parse_name_status_z(&unstaged_output.stdout)
        .into_iter()
        .map(|entry| {
            let (ins, del) = unstaged_stats.get(&entry.path).copied().unwrap_or((0, 0));
            WorkspaceGitChangeEntry {
                path: entry.path.clone(),
                name: file_name_from_path(&entry.path),
                absolute_path: join_workspace_path(root, &entry.path),
                status: entry.status,
                insertions: ins,
                deletions: del,
            }
        })
        .collect();

    for path in split_null_terminated_fields(&untracked_output.stdout) {
        unstaged.push(WorkspaceGitChangeEntry {
            path: path.clone(),
            name: file_name_from_path(&path),
            absolute_path: join_workspace_path(root, &path),
            status: "?".to_string(),
            insertions: 0,
            deletions: 0,
        });
    }

    staged.sort_by(|a, b| a.path.cmp(&b.path));
    unstaged.sort_by(|a, b| a.path.cmp(&b.path));

    let current_branch = resolve_current_branch_name(root).ok();
    let (ahead_of_remote_count, behind_of_remote_count) =
        resolve_upstream_branch_counts(root).unwrap_or((0, 0));
    let conflict_count = resolve_conflict_count(root).unwrap_or(0);
    let merge_in_progress = git_internal_path_exists(root, "MERGE_HEAD");

    Ok(WorkspaceGitStatusOutput {
        staged,
        unstaged,
        current_branch,
        ahead_of_remote_count,
        behind_of_remote_count,
        conflict_count,
        merge_in_progress,
    })
}

#[tauri::command]
pub async fn workspace_git_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitStatusInput,
) -> Result<WorkspaceGitStatusOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    workspace_git_status_inner(&input.workspace_root)
}

fn delegation_worktrees_root(active_root: &Path) -> PathBuf {
    let parent = active_root.parent().unwrap_or(active_root);
    let worktrees_root =
        if parent.file_name().and_then(|name| name.to_str()) == Some(".dcc-worktrees") {
            parent.to_path_buf()
        } else {
            parent.join(".dcc-worktrees")
        };
    worktrees_root.join(".dcc-delegations")
}

fn delegation_key_suffix(key: Option<&str>) -> String {
    let suffix: String = key
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(12)
        .collect();
    if suffix.is_empty() {
        Uuid::new_v4().simple().to_string()[..12].to_string()
    } else {
        suffix
    }
}

fn validate_delegation_worktree_path(root: &str, value: &str) -> Result<PathBuf, String> {
    let worktree_path = PathBuf::from(value.trim());
    if !worktree_path.is_absolute() {
        return Err("delegation worktree path must be absolute".to_string());
    }
    let allowed_root = delegation_worktrees_root(Path::new(root));
    let canonical_allowed_root =
        fs::canonicalize(&allowed_root).unwrap_or_else(|_| allowed_root.clone());
    let canonical_worktree_path =
        fs::canonicalize(&worktree_path).unwrap_or_else(|_| worktree_path.clone());
    if !canonical_worktree_path.starts_with(&canonical_allowed_root) {
        return Err("refusing to use a path outside the DCC delegation worktree root".to_string());
    }
    Ok(worktree_path)
}

#[tauri::command]
pub async fn workspace_prepare_delegation_worktree(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrepareDelegationWorktreeInput,
) -> Result<WorkspacePrepareDelegationWorktreeOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let status = workspace_git_status_inner(root)?;
    if status.conflict_count > 0 {
        return Err(format!(
            "resolve {} conflict{} before creating a delegation worktree",
            status.conflict_count,
            if status.conflict_count == 1 { "" } else { "s" },
        ));
    }
    let changed_count = status.staged.len() + status.unstaged.len();
    if changed_count > 0 {
        return Err(format!(
            "commit, stash, or discard {changed_count} existing worktree change{} before creating a delegation worktree",
            if changed_count == 1 { "" } else { "s" },
        ));
    }

    let base_commit = resolve_current_commit_sha(root)?
        .filter(|commit| !commit.trim().is_empty())
        .ok_or_else(|| "failed to resolve current HEAD".to_string())?;
    let suffix = delegation_key_suffix(input.delegation_key.as_deref());
    let raw_branch = format!("dcc/delegation/{suffix}");
    let branch = next_available_branch_name(root, &raw_branch);
    let worktree_root = delegation_worktrees_root(Path::new(root));
    let worktree_path = worktree_root.join(branch.replace('/', "-"));

    create_worktree_branch_from_ref(Path::new(root), &worktree_path, &branch, &base_commit)
        .map_err(|error| error.to_string())?;

    Ok(WorkspacePrepareDelegationWorktreeOutput {
        worktree_path: worktree_path.to_string_lossy().to_string(),
        branch,
        base_commit,
    })
}

#[tauri::command]
pub async fn workspace_remove_delegation_worktree(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceRemoveDelegationWorktreeInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let worktree_path = validate_delegation_worktree_path(root, &input.worktree_path)?;

    if worktree_path.exists() {
        remove_worktree(Path::new(root), &worktree_path).map_err(|error| error.to_string())?;
        if worktree_path.exists() {
            fs::remove_dir_all(&worktree_path).map_err(|error| error.to_string())?;
        }
    }

    if input.remove_branch {
        let branch = worktree_path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("dcc-delegation-"))
            .map(|suffix| format!("dcc/delegation/{suffix}"));
        if let Some(branch) = branch {
            let _ = run_git_output(root, &["branch", "-D", &branch]);
        }
    }

    Ok(())
}

fn apply_patch_to_worktree(root: &str, patch: &[u8]) -> Result<(), String> {
    let mut command = Command::new("git");
    configure_git_command(&mut command);
    command
        .current_dir(root)
        .arg("apply")
        .arg("--whitespace=nowarn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to run git apply: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "failed to open git apply stdin".to_string())?
        .write_all(patch)
        .map_err(|error| format!("failed to write git apply patch: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("git apply failed: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(git_output_err("git apply", &output.stderr))
}

fn list_untracked_files(root: &str) -> Result<Vec<String>, String> {
    let output = run_git_output(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    if !output.status.success() {
        return Err(git_output_err("git ls-files --others", &output.stderr));
    }
    split_null_terminated_fields(&output.stdout)
        .into_iter()
        .map(|path| validate_git_relative_path(&path))
        .collect()
}

fn preflight_untracked_delegation_files(
    source_root: &Path,
    destination_root: &Path,
    paths: &[String],
) -> Result<(), String> {
    for path in paths {
        let rel = validate_git_relative_path(path)?;
        let source_path = source_root.join(&rel);
        let destination_path = destination_root.join(&rel);
        let source_metadata = fs::symlink_metadata(&source_path).map_err(|error| {
            format!(
                "failed to read delegation file {}: {}",
                source_path.display(),
                error
            )
        })?;
        if !source_metadata.is_file() {
            return Err(format!(
                "delegation untracked path is not a regular file: {}",
                source_path.display()
            ));
        }
        if fs::symlink_metadata(&destination_path).is_ok() {
            return Err(format!(
                "destination already contains untracked delegation file: {}",
                destination_path.display()
            ));
        }
    }
    Ok(())
}

fn copy_untracked_delegation_files(
    source_root: &Path,
    destination_root: &Path,
    paths: &[String],
) -> Result<(), String> {
    preflight_untracked_delegation_files(source_root, destination_root, paths)?;
    for path in paths {
        let rel = validate_git_relative_path(path)?;
        let source_path = source_root.join(&rel);
        let destination_path = destination_root.join(&rel);
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create destination directory {}: {}",
                    parent.display(),
                    error
                )
            })?;
        }
        fs::copy(&source_path, &destination_path).map_err(|error| {
            format!(
                "failed to copy delegation file {} to {}: {}",
                source_path.display(),
                destination_path.display(),
                error
            )
        })?;
    }
    Ok(())
}

#[tauri::command]
pub async fn workspace_apply_delegation_worktree(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceApplyDelegationWorktreeInput,
) -> Result<WorkspaceApplyDelegationWorktreeOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let worktree_path = validate_delegation_worktree_path(root, &input.worktree_path)?;
    let worktree_root = worktree_path.to_string_lossy().to_string();

    let status = workspace_git_status_inner(root)?;
    let changed_count = status.staged.len() + status.unstaged.len();
    if status.conflict_count > 0 || changed_count > 0 {
        return Err(
            "apply requires a clean destination worktree; commit, stash, or discard local changes first"
                .to_string(),
        );
    }

    let destination_head = resolve_current_commit_sha(root)?
        .filter(|commit| !commit.trim().is_empty())
        .ok_or_else(|| "failed to resolve destination HEAD".to_string())?;
    let delegation_head = resolve_current_commit_sha(&worktree_root)?
        .filter(|commit| !commit.trim().is_empty())
        .ok_or_else(|| "failed to resolve delegation worktree HEAD".to_string())?;
    if destination_head != delegation_head {
        return Err(
            "destination worktree HEAD differs from delegation baseline; rebase or recreate the delegation before applying"
                .to_string(),
        );
    }

    let changed_output = run_git_output(
        &worktree_root,
        &["diff", "HEAD", "--name-only", "-z", "--", "."],
    )?;
    if !changed_output.status.success() {
        return Err(git_output_err(
            "git diff HEAD --name-only",
            &changed_output.stderr,
        ));
    }
    let untracked_files = list_untracked_files(&worktree_root)?;
    let mut changed_files_set: BTreeSet<String> =
        split_null_terminated_fields(&changed_output.stdout)
            .into_iter()
            .map(|path| validate_git_relative_path(&path))
            .collect::<Result<_, _>>()?;
    changed_files_set.extend(untracked_files.iter().cloned());
    let changed_files = changed_files_set.into_iter().collect::<Vec<_>>();
    if changed_files.is_empty() {
        return Err("delegation worktree has no changes to apply".to_string());
    }
    preflight_untracked_delegation_files(
        Path::new(&worktree_root),
        Path::new(root),
        &untracked_files,
    )?;

    let diff_output = run_git_output(
        &worktree_root,
        &["diff", "HEAD", "--binary", "--full-index", "--", "."],
    )?;
    if !diff_output.status.success() {
        return Err(git_output_err("git diff HEAD", &diff_output.stderr));
    }
    if !diff_output.stdout.is_empty() {
        apply_patch_to_worktree(root, &diff_output.stdout)?;
    }
    copy_untracked_delegation_files(Path::new(&worktree_root), Path::new(root), &untracked_files)?;

    Ok(WorkspaceApplyDelegationWorktreeOutput { changed_files })
}

#[tauri::command]
pub async fn workspace_continue_from_base_branch(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceContinueFromBaseBranchInput,
) -> Result<WorkspaceContinueFromBaseBranchOutput, String> {
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    if let Some(reason) = purge_broken_workspace_by_root(&repo, root).await? {
        return Err(broken_workspace_message(&reason));
    }
    let Some(mut workspace) = find_workspace_by_root(&repo, root).await? else {
        return Err(format!("workspace not found for path: {root}"));
    };

    let active_root = resolve_workspace_active_root(&workspace).to_string();

    // Resolve the target branch to branch off from (the PR's base branch, e.g. "main")
    let target_branch = input
        .target_branch
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .or_else(|| {
            input
                .base_branch
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
        })
        .map(|v| v.to_string())
        .unwrap_or_else(|| {
            resolve_default_branch_name(&workspace.root_path).unwrap_or_else(|_| "main".to_string())
        });

    // Fetch latest remote state for the target branch
    let _ = run_git_network_output(&workspace.root_path, &["fetch", "origin", &target_branch]);
    let start_point = resolve_continue_start_point(&workspace.root_path, &target_branch)?;

    // Derive a sanitized base name for the new branch from the workspace name
    let raw_name = input
        .new_branch_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| sanitize_branch_name(v))
        .unwrap_or_else(|| {
            workspace
                .name
                .as_deref()
                .map(sanitize_branch_name)
                .or_else(|| {
                    std::path::Path::new(active_root.as_str())
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(sanitize_branch_name)
                })
                .unwrap_or_else(|| "workspace".to_string())
        });

    // Find the first available branch name (raw_name, raw_name-2, raw_name-3, …)
    let new_branch = next_available_branch_name(&workspace.root_path, &raw_name);
    let old_branch = resolve_current_branch_name(&active_root)
        .map_err(|error| format!("failed to resolve current workspace branch: {error}"))?;
    let switch = run_git_output(&active_root, &["switch", "-c", &new_branch, &start_point])?;
    if !switch.status.success() {
        return Err(
            "Continue could not move your local changes onto the target branch. Commit, stash, or discard the conflicting changes, then try again."
                .to_string(),
        );
    }
    let _ = run_git_output(&active_root, &["branch", "--unset-upstream", &new_branch]);

    // `base_branch` must stay the PR/diff target branch (e.g. `main`), not the
    // working branch. Storing `new_branch` here corrupts it: `gh pr create`
    // then uses the working branch as the PR base, and `ensure_pushable_branch`
    // sees the current branch as "protected" and materializes a spurious branch.
    workspace.base_branch = target_branch.clone();
    workspace.updated_at = Utc::now().to_rfc3339();
    if let Err(error) = repo.save_workspace(&workspace).await {
        rollback_continue_branch(&active_root, &old_branch, &new_branch);
        return Err(error.to_string());
    }

    Ok(WorkspaceContinueFromBaseBranchOutput {
        success: true,
        branch: new_branch,
        workspace_root: active_root.clone(),
        previous_workspace_root: active_root,
        workspace,
    })
}

fn rollback_continue_branch(root: &str, old_branch: &str, new_branch: &str) {
    if let Ok(output) = run_git_output(root, &["switch", old_branch]) {
        if output.status.success() {
            let _ = run_git_output(root, &["branch", "-D", new_branch]);
        }
    }
}

fn resolve_continue_start_point(root: &str, target_branch: &str) -> Result<String, String> {
    let remote_ref = format!("origin/{target_branch}");
    let remote_exists = run_git_output(root, &["rev-parse", "--verify", &remote_ref])?;
    if remote_exists.status.success() {
        return Ok(remote_ref);
    }

    let local_exists = run_git_output(root, &["rev-parse", "--verify", target_branch])?;
    if local_exists.status.success() {
        return Ok(target_branch.to_string());
    }

    Err(format!(
        "could not resolve base branch `{target_branch}` locally or on origin"
    ))
}

fn sanitize_branch_name(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    // Collapse multiple dashes and trim from edges
    let mut result = String::new();
    let mut last_dash = true;
    for c in slug.chars() {
        if c == '-' {
            if !last_dash {
                result.push('-');
            }
            last_dash = true;
        } else {
            result.push(c);
            last_dash = false;
        }
    }
    let result = result.trim_end_matches('-').to_string();
    if result.is_empty() {
        "workspace".to_string()
    } else {
        result.chars().take(50).collect()
    }
}

/// File-level preview for staged / unstaged / committed tree rows.
#[tauri::command]
pub async fn workspace_git_file_preview(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitFilePreviewInput,
) -> Result<String, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let rel = validate_git_relative_path(&input.relative_path)?;
    let absolute = PathBuf::from(root).join(&rel);
    let status = input.status.trim();
    let patch = match input.scope {
        WorkspaceGitPreviewScope::Staged => run_git_diff_text(
            root,
            &[
                "diff",
                "--cached",
                "--unified=80",
                "--no-ext-diff",
                "--",
                &rel,
            ],
            "git diff --cached",
        )?,
        WorkspaceGitPreviewScope::Unstaged => {
            if status == "?" {
                if !absolute.is_file() {
                    return Err("file not found".to_string());
                }
                let absolute = absolute.to_string_lossy().to_string();
                run_git_diff_text(
                    root,
                    &[
                        "diff",
                        "--no-index",
                        "--unified=80",
                        "--no-ext-diff",
                        "/dev/null",
                        &absolute,
                    ],
                    "git diff --no-index",
                )?
            } else {
                run_git_diff_text(
                    root,
                    &["diff", "--unified=80", "--no-ext-diff", "--", &rel],
                    "git diff",
                )?
            }
        }
        WorkspaceGitPreviewScope::Committed => {
            let base = input
                .base_branch
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "base_branch is required for committed previews".to_string())?;
            let range = format!("{base}...HEAD");
            run_git_diff_text(
                root,
                &["diff", "--unified=80", "--no-ext-diff", &range, "--", &rel],
                "git diff committed",
            )?
        }
    };

    if patch.trim().is_empty() {
        return Err("no diff available for this file".to_string());
    }

    Ok(patch)
}

/// File-level code snapshot used by the Monaco diff surface.
#[tauri::command]
pub async fn workspace_git_file_preview_content(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitFilePreviewInput,
) -> Result<WorkspaceGitFilePreviewContentOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let rel = validate_git_relative_path(&input.relative_path)?;
    let absolute = PathBuf::from(root).join(&rel);
    let status = input.status.trim();
    let base_branch = input
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    let (original_text, modified_text, inline) = match input.scope {
        WorkspaceGitPreviewScope::Committed => {
            let base = base_branch
                .clone()
                .ok_or_else(|| "base_branch is required for committed previews".to_string())?;
            let original = run_git_show_text(root, &format!("{base}:{rel}"), "git show base")?
                .unwrap_or_default();
            let modified = run_git_show_text(root, &format!("HEAD:{rel}"), "git show HEAD")?
                .unwrap_or_default();
            (original, modified, matches!(status, "A" | "D" | "?"))
        }
        WorkspaceGitPreviewScope::Staged | WorkspaceGitPreviewScope::Unstaged => {
            let original = if status == "?" {
                String::new()
            } else {
                run_git_show_text(root, &format!("HEAD:{rel}"), "git show HEAD")?
                    .unwrap_or_default()
            };

            let modified = match status {
                "D" => String::new(),
                "?" => read_worktree_file_text(root, &rel)?.unwrap_or_default(),
                _ => read_worktree_file_text(root, &rel)?.unwrap_or_else(|| {
                    run_git_show_text(root, &format!(":{rel}"), "git show index")
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                }),
            };

            let inline = matches!(status, "A" | "D" | "?");
            (original, modified, inline)
        }
    };

    if original_text.is_empty() && modified_text.is_empty() {
        return Err("no content available for this file".to_string());
    }

    if !absolute.is_file() && input.scope != WorkspaceGitPreviewScope::Committed && status != "D" {
        return Err("file not found".to_string());
    }

    Ok(WorkspaceGitFilePreviewContentOutput {
        original_text,
        modified_text,
        inline,
    })
}

#[tauri::command]
pub async fn create_workspace_for_repo(
    state: State<'_, WorkspaceCommandState>,
    app: AppHandle,
    input: CreateWorkspaceForRepoInput,
) -> Result<CreateWorkspaceForRepoOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    create_workspace_for_repo_with_repo(&repo, &app, input).await
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ParsedWorkspaceSourceKind {
    Branch(String),
    PullRequest(u32),
}

#[derive(Clone, Debug)]
struct ResolvedWorkspaceSource {
    public: WorkspaceSourceUrlResolution,
    remote_name: String,
    effective_login: Option<String>,
    requested_push_target: RequestedWorkspacePushTarget,
}

#[derive(Clone, Debug)]
struct RequestedWorkspacePushTarget {
    preferred_remote_name: String,
    branch_name: String,
    remote_url: Option<String>,
}

fn sanitize_fork_remote_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches(['-', '.']).to_ascii_lowercase();
    if sanitized.is_empty() {
        "contributor".to_string()
    } else {
        sanitized
    }
}

fn preferred_fork_remote_name(source_repository: &str) -> String {
    let namespace = source_repository
        .rsplit_once('/')
        .map(|(namespace, _)| namespace)
        .unwrap_or(source_repository);
    format!("dcc-{}", sanitize_fork_remote_segment(namespace))
}

fn https_repository_url(host: &str, repository: &str) -> String {
    format!(
        "https://{}/{}.git",
        host.trim().trim_end_matches('/'),
        repository.trim().trim_matches('/'),
    )
}

fn percent_decode_url_path(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err("URL contains an invalid percent-encoded path.".to_string());
        }
        let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
            .map_err(|_| "URL contains an invalid percent-encoded path.".to_string())?;
        let byte = u8::from_str_radix(hex, 16)
            .map_err(|_| "URL contains an invalid percent-encoded path.".to_string())?;
        decoded.push(byte);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|_| "URL path is not valid UTF-8.".to_string())
}

fn parse_workspace_source_url(
    raw_url: &str,
    target: &WorkspaceForgeTarget,
) -> Result<ParsedWorkspaceSourceKind, String> {
    let url = Url::parse(raw_url.trim())
        .map_err(|_| "Enter a valid HTTP or HTTPS URL for a branch or pull request.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only HTTP or HTTPS branch and pull request URLs are supported.".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URLs containing credentials are not supported.".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "The URL does not contain a repository host.".to_string())?;
    if !host.eq_ignore_ascii_case(&target.remote.host) {
        return Err(format!(
            "This URL belongs to `{host}`, but the current project uses `{}`.",
            target.remote.host
        ));
    }

    let decoded_path = percent_decode_url_path(url.path())?;
    let path = decoded_path.trim_matches('/');
    let repository_path = format!("{}/{}", target.remote.namespace, target.remote.repo);
    let prefix = format!("{repository_path}/");
    let suffix = path
        .strip_prefix(&prefix)
        .or_else(|| {
            path.get(..prefix.len())
                .filter(|candidate| candidate.eq_ignore_ascii_case(&prefix))
                .and_then(|_| path.get(prefix.len()..))
        })
        .ok_or_else(|| {
            format!("This URL belongs to a different repository. Expected `{repository_path}`.")
        })?;

    match target.provider {
        crate::commands::forge_commands::ForgeCliProvider::Github => {
            if let Some(rest) = suffix.strip_prefix("pull/") {
                let number = rest
                    .split('/')
                    .next()
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|number| *number > 0)
                    .ok_or_else(|| "The GitHub pull request number is invalid.".to_string())?;
                return Ok(ParsedWorkspaceSourceKind::PullRequest(number));
            }
            let branch = suffix
                .strip_prefix("tree/")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "Use a GitHub branch URL (`/tree/...`) or pull request URL (`/pull/...`)."
                        .to_string()
                })?;
            Ok(ParsedWorkspaceSourceKind::Branch(branch.to_string()))
        }
        crate::commands::forge_commands::ForgeCliProvider::Gitlab => {
            if let Some(rest) = suffix.strip_prefix("-/merge_requests/") {
                let number = rest
                    .split('/')
                    .next()
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|number| *number > 0)
                    .ok_or_else(|| "The GitLab merge request number is invalid.".to_string())?;
                return Ok(ParsedWorkspaceSourceKind::PullRequest(number));
            }
            let branch = suffix
                .strip_prefix("-/tree/")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "Use a GitLab branch URL (`/-/tree/...`) or merge request URL (`/-/merge_requests/...`)."
                        .to_string()
                })?;
            Ok(ParsedWorkspaceSourceKind::Branch(branch.to_string()))
        }
    }
}

fn parse_remote_heads(output: &[u8]) -> Vec<(String, String)> {
    let mut heads = String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| {
            let (sha, reference) = line.split_once('\t')?;
            let branch = reference.strip_prefix("refs/heads/")?;
            Some((branch.to_string(), sha.to_string()))
        })
        .collect::<Vec<_>>();
    heads.sort_by(|left, right| left.0.cmp(&right.0));
    heads.dedup_by(|left, right| left.0 == right.0);
    heads
}

fn resolve_branch_from_url_path(
    url_branch_path: &str,
    heads: &[(String, String)],
) -> Result<(String, String), String> {
    let exact = heads
        .iter()
        .find(|(branch, _)| branch == url_branch_path)
        .cloned()
        .ok_or_else(|| {
            "The URL does not resolve to an existing remote branch. File and folder URLs are not accepted."
                .to_string()
        })?;
    let ambiguous = heads.iter().any(|(branch, _)| {
        branch != &exact.0
            && exact
                .0
                .strip_prefix(branch)
                .is_some_and(|suffix| suffix.starts_with('/'))
    });
    if ambiguous {
        return Err(
            "The URL is ambiguous between a branch and a path inside another branch. Copy the branch URL from the forge branch list."
                .to_string(),
        );
    }
    Ok(exact)
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn resolve_current_commit_sha_for_ref(root: &str, reference: &str) -> Result<String, String> {
    let output = run_git_output(root, &["rev-parse", "--verify", reference])?;
    if !output.status.success() {
        return Err(git_output_err("git rev-parse --verify", &output.stderr));
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!("Git did not resolve `{reference}` to a commit."));
    }
    Ok(sha)
}

fn list_workspace_remote_names(root: &str) -> Result<Vec<String>, String> {
    let output = run_git_output(root, &["remote"])?;
    if !output.status.success() {
        return Err(git_output_err("git remote", &output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn find_workspace_remote_for_url(root: &str, remote_url: &str) -> Result<Option<String>, String> {
    let expected = crate::commands::forge::remote::parse_remote(remote_url);
    for remote_name in list_workspace_remote_names(root)? {
        let output = run_git_output(root, &["remote", "get-url", &remote_name])?;
        if !output.status.success() {
            continue;
        }
        let candidate_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let same_repository = expected
            .as_ref()
            .zip(crate::commands::forge::remote::parse_remote(&candidate_url).as_ref())
            .is_some_and(|(expected, candidate)| {
                expected.host.eq_ignore_ascii_case(&candidate.host)
                    && expected
                        .namespace
                        .eq_ignore_ascii_case(&candidate.namespace)
                    && expected.repo.eq_ignore_ascii_case(&candidate.repo)
            });
        if same_repository || candidate_url == remote_url {
            return Ok(Some(remote_name));
        }
    }
    Ok(None)
}

fn next_available_workspace_remote_name(root: &str, preferred: &str) -> Result<String, String> {
    let existing = list_workspace_remote_names(root)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    if !existing.contains(preferred) {
        return Ok(preferred.to_string());
    }
    for suffix in 2..100 {
        let candidate = format!("{preferred}-{suffix}");
        if !existing.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "Could not find an available Git remote name for `{preferred}`."
    ))
}

fn prepare_workspace_push_target(
    root: &str,
    requested: &RequestedWorkspacePushTarget,
) -> Result<WorkspacePushTarget, String> {
    let Some(remote_url) = requested.remote_url.as_deref() else {
        let output = run_git_output(
            root,
            &["remote", "get-url", &requested.preferred_remote_name],
        )?;
        if !output.status.success() {
            return Err(format!(
                "The source remote `{}` is no longer available.",
                requested.preferred_remote_name
            ));
        }
        return Ok(WorkspacePushTarget {
            remote_name: requested.preferred_remote_name.clone(),
            branch_name: requested.branch_name.clone(),
            remote_url: None,
            remote_created: false,
        });
    };

    if let Some(remote_name) = find_workspace_remote_for_url(root, remote_url)? {
        return Ok(WorkspacePushTarget {
            remote_name,
            branch_name: requested.branch_name.clone(),
            remote_url: Some(remote_url.to_string()),
            remote_created: false,
        });
    }

    let remote_name = next_available_workspace_remote_name(root, &requested.preferred_remote_name)?;
    let output = run_git_output(root, &["remote", "add", &remote_name, remote_url])?;
    if !output.status.success() {
        return Err(git_output_err("git remote add", &output.stderr));
    }
    Ok(WorkspacePushTarget {
        remote_name,
        branch_name: requested.branch_name.clone(),
        remote_url: Some(remote_url.to_string()),
        remote_created: true,
    })
}

fn cleanup_prepared_workspace_push_target(root: &str, target: &WorkspacePushTarget) {
    if !target.remote_created {
        return;
    }
    let Some(expected_url) = target.remote_url.as_deref() else {
        return;
    };
    let output = match run_git_output(root, &["remote", "get-url", &target.remote_name]) {
        Ok(output) if output.status.success() => output,
        _ => return,
    };
    let configured_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if configured_url != expected_url {
        return;
    }
    let _ = run_git_output(root, &["remote", "remove", &target.remote_name]);
}

async fn inherit_workspace_push_target_ownership(
    repo: &SqliteWorkspaceRepo,
    root: &str,
    target: &mut WorkspacePushTarget,
) -> Result<(), String> {
    if target.remote_created || target.remote_url.is_none() {
        return Ok(());
    }
    let is_owned = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|workspace| workspace.root_path == root)
        .filter_map(|workspace| workspace.source?.push_target)
        .any(|known| {
            known.remote_created
                && (known.remote_name == target.remote_name
                    || known.remote_url.as_deref() == target.remote_url.as_deref())
        });
    target.remote_created = is_owned;
    Ok(())
}

fn git_branch_config_uses_remote(root: &str, remote_name: &str) -> bool {
    let Ok(output) = run_git_output(
        root,
        &[
            "config",
            "--get-regexp",
            r"^branch\..*\.(remote|pushRemote)$",
        ],
    ) else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .any(|value| value == remote_name)
}

async fn cleanup_unused_workspace_push_target(
    repo: &SqliteWorkspaceRepo,
    removed: &Workspace,
) -> Result<(), String> {
    let Some(target) = removed
        .source
        .as_ref()
        .and_then(|source| source.push_target.as_ref())
    else {
        return Ok(());
    };
    if !target.remote_created
        || target.remote_url.is_none()
        || matches!(target.remote_name.as_str(), "origin" | "upstream")
    {
        return Ok(());
    }
    let used_by_another_workspace = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|workspace| workspace.id != removed.id && workspace.root_path == removed.root_path)
        .filter_map(|workspace| workspace.source?.push_target)
        .any(|known| {
            known.remote_name == target.remote_name
                || known.remote_url.as_deref() == target.remote_url.as_deref()
        });
    if used_by_another_workspace
        || git_branch_config_uses_remote(&removed.root_path, &target.remote_name)
    {
        return Ok(());
    }

    let output = run_git_output(
        &removed.root_path,
        &["remote", "get-url", &target.remote_name],
    )?;
    if !output.status.success() {
        return Ok(());
    }
    let configured_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if target.remote_url.as_deref() != Some(configured_url.as_str()) {
        return Ok(());
    }
    let output = run_git_output(
        &removed.root_path,
        &["remote", "remove", &target.remote_name],
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_output_err("git remote remove", &output.stderr))
    }
}

async fn resolve_workspace_source_url_inner(
    db_path: &Path,
    repo: &SqliteWorkspaceRepo,
    input: &ResolveWorkspaceSourceUrlInput,
) -> Result<ResolvedWorkspaceSource, String> {
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let target = resolve_workspace_forge_target(root)?.ok_or_else(|| {
        "The current project does not have a supported GitHub or GitLab remote.".to_string()
    })?;
    let parsed = parse_workspace_source_url(&input.url, &target)?;
    let forge_context = crate::commands::forge::context::resolve_workspace_forge_context(
        db_path,
        root,
        input.forge_login.as_deref(),
    )?;
    let effective_login = forge_context
        .as_ref()
        .and_then(|context| context.effective_login.clone());
    let repository = repo
        .get_repository(&RepositoryId(root.to_string()))
        .await
        .map_err(|error| error.to_string())?;
    let default_base = repository
        .as_ref()
        .map(|repository| repository.base_branch.trim())
        .filter(|branch| !branch.is_empty())
        .unwrap_or("main")
        .to_string();
    let repository_name = format!("{}/{}", target.remote.namespace, target.remote.repo);
    let provider = match target.provider {
        crate::commands::forge_commands::ForgeCliProvider::Github => "github",
        crate::commands::forge_commands::ForgeCliProvider::Gitlab => "gitlab",
    }
    .to_string();
    let mut source_remote_url = None;

    let public = match parsed {
        ParsedWorkspaceSourceKind::Branch(url_branch_path) => {
            let output = run_git_network_output_with_workspace_auth(
                db_path,
                root,
                &["ls-remote", "--heads", &target.remote_name],
                effective_login.as_deref(),
            )?;
            if !output.status.success() {
                return Err(git_output_err("git ls-remote --heads", &output.stderr));
            }
            let heads = parse_remote_heads(&output.stdout);
            let (head_branch, head_sha) = resolve_branch_from_url_path(&url_branch_path, &heads)?;
            if head_branch == default_base {
                return Err(format!(
                    "`{head_branch}` is the project base branch. Choose a feature branch URL instead."
                ));
            }
            WorkspaceSourceUrlResolution {
                kind: WorkspaceSourceKind::Branch,
                url: input.url.trim().to_string(),
                provider,
                host: target.remote.host.clone(),
                repository: repository_name.clone(),
                head_branch,
                head_sha,
                base_branch: default_base,
                change_request_number: None,
                title: None,
                author: None,
                state: None,
                source_repository: Some(repository_name.clone()),
                is_cross_repository: false,
            }
        }
        ParsedWorkspaceSourceKind::PullRequest(number) => match target.provider {
            crate::commands::forge_commands::ForgeCliProvider::Github => {
                let raw = github::resolve_change_request_url_json(
                    root,
                    &target.remote.host,
                    input.url.trim(),
                    effective_login.as_deref(),
                )?;
                let resolved_number = raw
                    .get("number")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32)
                    .ok_or_else(|| "GitHub did not return a pull request number.".to_string())?;
                if resolved_number != number {
                    return Err("The resolved pull request does not match the URL.".to_string());
                }
                let state = json_string(&raw, "state")
                    .map(|value| value.to_ascii_lowercase())
                    .unwrap_or_default();
                if state != "open" {
                    return Err(
                        "Only open pull requests can be opened as editable workspaces.".to_string(),
                    );
                }
                let head_repository = raw
                    .get("headRepository")
                    .and_then(|value| value.get("nameWithOwner"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .or_else(|| {
                        let owner = raw
                            .get("headRepositoryOwner")
                            .and_then(|value| value.get("login"))
                            .and_then(Value::as_str)?;
                        let name = raw
                            .get("headRepository")
                            .and_then(|value| value.get("name"))
                            .and_then(Value::as_str)?;
                        Some(format!("{owner}/{name}"))
                    })
                    .ok_or_else(|| {
                        "GitHub did not provide enough repository identity to verify the pull request source."
                            .to_string()
                    })?;
                let is_cross_repository = !head_repository.eq_ignore_ascii_case(&repository_name);
                WorkspaceSourceUrlResolution {
                    kind: WorkspaceSourceKind::PullRequest,
                    url: json_string(&raw, "url").unwrap_or_else(|| input.url.trim().to_string()),
                    provider,
                    host: target.remote.host.clone(),
                    repository: repository_name.clone(),
                    head_branch: json_string(&raw, "headRefName").ok_or_else(|| {
                        "GitHub did not return the pull request branch.".to_string()
                    })?,
                    head_sha: json_string(&raw, "headRefOid").ok_or_else(|| {
                        "GitHub did not return the pull request head commit.".to_string()
                    })?,
                    base_branch: json_string(&raw, "baseRefName").unwrap_or(default_base),
                    change_request_number: Some(number),
                    title: json_string(&raw, "title"),
                    author: raw
                        .get("author")
                        .and_then(|value| value.get("login"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    state: Some(state),
                    source_repository: Some(head_repository),
                    is_cross_repository,
                }
            }
            crate::commands::forge_commands::ForgeCliProvider::Gitlab => {
                let raw = gitlab::resolve_change_request_url_json(
                    root,
                    &target,
                    number,
                    effective_login.as_deref(),
                )?;
                let source_project = raw.get("source_project_id").and_then(Value::as_u64);
                let target_project = raw.get("target_project_id").and_then(Value::as_u64);
                let (source_project, target_project) =
                    source_project.zip(target_project).ok_or_else(|| {
                        "GitLab did not provide enough project identity to verify the merge request source."
                            .to_string()
                    })?;
                let is_cross_repository = source_project != target_project;
                let source_project_metadata = if is_cross_repository {
                    Some(gitlab::resolve_project_json(
                        root,
                        &target.remote.host,
                        source_project,
                        effective_login.as_deref(),
                    )?)
                } else {
                    None
                };
                let source_repository = source_project_metadata
                    .as_ref()
                    .and_then(|project| json_string(project, "path_with_namespace"))
                    .unwrap_or_else(|| repository_name.clone());
                source_remote_url = source_project_metadata.as_ref().and_then(|project| {
                    json_string(project, "http_url_to_repo")
                        .or_else(|| json_string(project, "ssh_url_to_repo"))
                });
                let state = json_string(&raw, "state")
                    .map(|value| value.to_ascii_lowercase())
                    .unwrap_or_default();
                if state != "opened" && state != "open" {
                    return Err(
                        "Only open merge requests can be opened as editable workspaces."
                            .to_string(),
                    );
                }
                let head_sha = json_string(&raw, "sha")
                    .or_else(|| {
                        raw.get("diff_refs")
                            .and_then(|value| value.get("head_sha"))
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
                    .ok_or_else(|| {
                        "GitLab did not return the merge request head commit.".to_string()
                    })?;
                WorkspaceSourceUrlResolution {
                    kind: WorkspaceSourceKind::PullRequest,
                    url: json_string(&raw, "web_url")
                        .unwrap_or_else(|| input.url.trim().to_string()),
                    provider,
                    host: target.remote.host.clone(),
                    repository: repository_name.clone(),
                    head_branch: json_string(&raw, "source_branch").ok_or_else(|| {
                        "GitLab did not return the merge request branch.".to_string()
                    })?,
                    head_sha,
                    base_branch: json_string(&raw, "target_branch").unwrap_or(default_base),
                    change_request_number: Some(number),
                    title: json_string(&raw, "title"),
                    author: raw
                        .get("author")
                        .and_then(|value| value.get("username"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                    state: Some("open".to_string()),
                    source_repository: Some(source_repository),
                    is_cross_repository,
                }
            }
        },
    };

    let source_repository = public
        .source_repository
        .clone()
        .unwrap_or_else(|| repository_name.clone());
    let requested_push_target = if public.is_cross_repository {
        RequestedWorkspacePushTarget {
            preferred_remote_name: preferred_fork_remote_name(&source_repository),
            branch_name: public.head_branch.clone(),
            remote_url: Some(
                source_remote_url
                    .unwrap_or_else(|| https_repository_url(&public.host, &source_repository)),
            ),
        }
    } else {
        RequestedWorkspacePushTarget {
            preferred_remote_name: target.remote_name.clone(),
            branch_name: public.head_branch.clone(),
            remote_url: None,
        }
    };

    Ok(ResolvedWorkspaceSource {
        public,
        remote_name: target.remote_name,
        effective_login,
        requested_push_target,
    })
}

#[tauri::command]
pub async fn resolve_workspace_source_url(
    state: State<'_, WorkspaceCommandState>,
    input: ResolveWorkspaceSourceUrlInput,
) -> Result<WorkspaceSourceUrlResolution, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    Ok(
        resolve_workspace_source_url_inner(&state.db_path, &repo, &input)
            .await?
            .public,
    )
}

#[tauri::command]
pub async fn create_workspace_from_source_url(
    state: State<'_, WorkspaceCommandState>,
    app: AppHandle,
    input: CreateWorkspaceFromSourceUrlInput,
) -> Result<CreateWorkspaceForRepoOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim().to_string();
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let repository_id = RepositoryId(root.clone());
    let existing_repository = repo
        .get_repository(&repository_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "The project must already be open in DCC before importing a branch or pull request."
                .to_string()
        })?;
    if existing_repository.project_id != input.project_id {
        return Err("The URL must be opened from the matching DCC project.".to_string());
    }

    let resolved = resolve_workspace_source_url_inner(
        &state.db_path,
        &repo,
        &ResolveWorkspaceSourceUrlInput {
            workspace_root: root.clone(),
            url: input.url.clone(),
            forge_login: input.forge_login.clone(),
        },
    )
    .await?;

    let mut push_target = prepare_workspace_push_target(&root, &resolved.requested_push_target)?;
    let prepared_push_target = push_target.clone();
    if let Err(error) =
        inherit_workspace_push_target_ownership(&repo, &root, &mut push_target).await
    {
        cleanup_prepared_workspace_push_target(&root, &prepared_push_target);
        return Err(error);
    }
    let source_ref = format!("refs/heads/{}", push_target.branch_name);
    let tracking_ref = format!(
        "refs/remotes/{}/{}",
        push_target.remote_name, push_target.branch_name
    );
    let refspec = format!("+{source_ref}:{tracking_ref}");
    let fetch = match run_git_network_output_with_workspace_auth(
        &state.db_path,
        &root,
        &["fetch", &push_target.remote_name, &refspec],
        resolved.effective_login.as_deref(),
    ) {
        Ok(fetch) => fetch,
        Err(error) => {
            cleanup_prepared_workspace_push_target(&root, &prepared_push_target);
            return Err(error);
        }
    };
    if !fetch.status.success() {
        cleanup_prepared_workspace_push_target(&root, &prepared_push_target);
        return Err(git_output_err("git fetch branch", &fetch.stderr));
    }
    let fetched_sha = match resolve_current_commit_sha_for_ref(&root, &tracking_ref) {
        Ok(sha) => sha,
        Err(error) => {
            cleanup_prepared_workspace_push_target(&root, &prepared_push_target);
            return Err(error);
        }
    };
    if !fetched_sha.eq_ignore_ascii_case(&resolved.public.head_sha) {
        cleanup_prepared_workspace_push_target(&root, &prepared_push_target);
        return Err(format!(
            "The source branch changed while DCC was opening this workspace. Expected `{}`, but fetched `{fetched_sha}`. Validate the PR or MR again.",
            resolved.public.head_sha
        ));
    }

    let workspace_name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .or_else(|| resolved.public.title.clone())
        .or_else(|| Some(resolved.public.head_branch.clone()));
    let git = CommandGitOps::new();
    let events = TauriEventBus::new(app.clone());
    let mut prepared = match run_prepare_workspace_for_repo(
        &git,
        &events,
        CreateWorkspaceForRepoInput {
            project_id: input.project_id,
            workspace_root: root.clone(),
            base_branch: tracking_ref,
            name: workspace_name,
        },
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            cleanup_prepared_workspace_push_target(&root, &prepared_push_target);
            return Err(error.to_string());
        }
    };
    prepared.workspace.base_branch = resolved.public.base_branch.clone();
    prepared.workspace.source = Some(WorkspaceSource {
        kind: resolved.public.kind,
        url: resolved.public.url,
        provider: resolved.public.provider,
        remote_name: resolved.remote_name,
        head_branch: resolved.public.head_branch,
        head_sha: fetched_sha,
        base_branch: resolved.public.base_branch,
        change_request_number: resolved.public.change_request_number,
        title: resolved.public.title,
        author: resolved.public.author,
        source_repository: resolved.public.source_repository,
        push_target: Some(push_target.clone()),
    });
    let finalized = match run_finalize_workspace_for_repo(&repo, &events, prepared).await {
        Ok(finalized) => finalized,
        Err(error) => {
            cleanup_prepared_workspace_push_target(&root, &prepared_push_target);
            return Err(error.to_string());
        }
    };
    let mut workspace = finalized.workspace;

    let mut repository = existing_repository;
    repository.base_branch = workspace.base_branch.clone();
    repository.updated_at = Utc::now().to_rfc3339();
    repo.save_repository(&repository)
        .await
        .map_err(|error| error.to_string())?;
    refresh_repository_forge_metadata(&repo, &workspace).await?;

    let setup_hints = collect_workspace_setup_hints(&workspace);
    let setup_report = execute_workspace_setup_report(&workspace).await;
    let compile_warning = compile_active_mission_spec_context_for_workspace(&workspace)?;
    let setup_report = append_mission_spec_compile_warning(&setup_report, compile_warning);
    persist_workspace_setup_outcome(&repo, &mut workspace, &setup_report).await?;

    Ok(CreateWorkspaceForRepoOutput {
        workspace,
        setup_hints,
        setup_report,
    })
}

async fn create_workspace_for_repo_with_repo(
    repo: &SqliteWorkspaceRepo,
    app: &AppHandle,
    input: CreateWorkspaceForRepoInput,
) -> Result<CreateWorkspaceForRepoOutput, String> {
    let git = CommandGitOps::new();
    let events = TauriEventBus::new(app.clone());

    let finalized = run_create_workspace_for_repo(repo, &git, &events, input)
        .await
        .map_err(|error| error.to_string())?;
    refresh_repository_forge_metadata(repo, &finalized.workspace).await?;
    let setup_hints = collect_workspace_setup_hints(&finalized.workspace);
    let setup_report = execute_workspace_setup_report(&finalized.workspace).await;
    let mut workspace = finalized.workspace;
    let compile_warning = compile_active_mission_spec_context_for_workspace(&workspace)?;
    let setup_report = append_mission_spec_compile_warning(&setup_report, compile_warning);
    persist_workspace_setup_outcome(repo, &mut workspace, &setup_report).await?;

    Ok(CreateWorkspaceForRepoOutput {
        workspace,
        setup_hints,
        setup_report,
    })
}

fn validate_bundle_projects(
    repositories: &[Repository],
    input: &CreateWorkspaceBundleForReposInput,
) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("workspace bundle name cannot be empty".to_string());
    }
    if input.projects.len() < 2 {
        return Err("workspace bundle requires at least two projects".to_string());
    }

    let mut roots = BTreeSet::new();
    for project in &input.projects {
        let root = project.workspace_root.trim();
        if root.is_empty() {
            return Err("workspace bundle project root cannot be empty".to_string());
        }
        let normalized_root = root.replace('\\', "/");
        if !roots.insert(normalized_root) {
            return Err(format!(
                "workspace bundle contains duplicate project root: {root}"
            ));
        }
        let registered = repositories.iter().any(|repository| {
            repository.root_path.trim() == root && repository.project_id == project.project_id
        });
        if !registered {
            return Err(format!(
                "project must be opened in DCC before it can join a multi-workspace: {root}"
            ));
        }
    }
    Ok(())
}

async fn rollback_bundle_workspaces(
    repo: &SqliteWorkspaceRepo,
    workspaces: &[Workspace],
) -> Vec<String> {
    let mut errors = Vec::new();
    for workspace in workspaces.iter().rev() {
        if let Err(error) = cleanup_workspace_files(workspace) {
            errors.push(format!(
                "failed to clean workspace {}: {error}",
                workspace.id.0
            ));
            continue;
        }
        if let Err(error) = cleanup_unused_workspace_push_target(repo, workspace).await {
            errors.push(format!(
                "failed to clean workspace remote {}: {error}",
                workspace.id.0
            ));
            continue;
        }
        if let Err(error) = repo.delete_workspace(&workspace.id).await {
            errors.push(format!(
                "failed to remove workspace record {}: {error}",
                workspace.id.0
            ));
        }
    }
    errors
}

#[tauri::command]
pub async fn create_workspace_bundle_for_repos(
    state: State<'_, WorkspaceCommandState>,
    app: AppHandle,
    input: CreateWorkspaceBundleForReposInput,
) -> Result<CreateWorkspaceBundleForReposOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let repositories = repo
        .list_repositories()
        .await
        .map_err(|error| error.to_string())?;
    validate_bundle_projects(&repositories, &input)?;
    let initial_workspace_ids = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|workspace| workspace.id.0)
        .collect::<BTreeSet<_>>();
    let bundle_roots = input
        .projects
        .iter()
        .map(|project| project.workspace_root.trim().to_string())
        .collect::<BTreeSet<_>>();

    let mut created = Vec::with_capacity(input.projects.len());
    for mut project in input.projects {
        if project
            .name
            .as_deref()
            .is_none_or(|name| name.trim().is_empty())
        {
            project.name = repositories
                .iter()
                .find(|repository| {
                    repository.project_id == project.project_id
                        && repository.root_path.trim() == project.workspace_root.trim()
                })
                .map(|repository| repository.name.clone());
        }
        match create_workspace_for_repo_with_repo(&repo, &app, project).await {
            Ok(output) => created.push(output),
            Err(error) => {
                let rollback_targets = repo
                    .list_workspaces()
                    .await
                    .map_err(|list_error| {
                        format!("{error}; failed to discover workspaces for rollback: {list_error}")
                    })?
                    .into_iter()
                    .filter(|workspace| {
                        !initial_workspace_ids.contains(&workspace.id.0)
                            && bundle_roots.contains(workspace.root_path.trim())
                    })
                    .collect::<Vec<_>>();
                let rollback_errors = rollback_bundle_workspaces(&repo, &rollback_targets).await;
                if rollback_errors.is_empty() {
                    return Err(error);
                }
                return Err(format!(
                    "{error}; multi-workspace rollback was incomplete: {}",
                    rollback_errors.join("; ")
                ));
            }
        }
    }

    let workspaces = created
        .iter()
        .map(|output| output.workspace.clone())
        .collect::<Vec<_>>();
    let summary = match run_create_workspace_bundle(&repo, &input.name, &workspaces).await {
        Ok(summary) => summary,
        Err(error) => {
            let rollback_targets = created
                .iter()
                .map(|output| output.workspace.clone())
                .collect::<Vec<_>>();
            let rollback_errors = rollback_bundle_workspaces(&repo, &rollback_targets).await;
            if rollback_errors.is_empty() {
                return Err(error.to_string());
            }
            return Err(format!(
                "{error}; multi-workspace rollback was incomplete: {}",
                rollback_errors.join("; ")
            ));
        }
    };

    Ok(CreateWorkspaceBundleForReposOutput {
        summary,
        workspaces: created,
    })
}

#[tauri::command]
pub async fn create_workspace_from_url(
    state: State<'_, WorkspaceCommandState>,
    app: AppHandle,
    input: CreateWorkspaceFromUrlInput,
) -> Result<CreateWorkspaceFromUrlOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let git = CommandGitOps::new();
    let events = TauriEventBus::new(app);

    let finalized = run_create_workspace_from_url(&repo, &git, &events, input)
        .await
        .map_err(|error| error.to_string())?;
    refresh_repository_forge_metadata(&repo, &finalized.workspace).await?;
    let setup_hints = collect_workspace_setup_hints(&finalized.workspace);
    let setup_report = execute_workspace_setup_report(&finalized.workspace).await;
    let mut workspace = finalized.workspace;
    let compile_warning = compile_active_mission_spec_context_for_workspace(&workspace)?;
    let setup_report = append_mission_spec_compile_warning(&setup_report, compile_warning);
    persist_workspace_setup_outcome(&repo, &mut workspace, &setup_report).await?;

    Ok(CreateWorkspaceFromUrlOutput {
        workspace,
        setup_hints,
        setup_report,
    })
}

pub async fn workspace_run_setup(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceRunSetupInput,
) -> Result<WorkspaceRunSetupOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let mut workspace = find_workspace_by_root(&repo, &input.workspace_root)
        .await?
        .ok_or_else(|| format!("workspace not found for root {}", input.workspace_root))?;

    let setup_hints = collect_workspace_setup_hints(&workspace);
    let setup_report = execute_workspace_setup_report(&workspace).await;
    let compile_warning = compile_active_mission_spec_context_for_workspace(&workspace)?;
    let setup_report = append_mission_spec_compile_warning(&setup_report, compile_warning);
    persist_workspace_setup_outcome(&repo, &mut workspace, &setup_report).await?;

    Ok(WorkspaceRunSetupOutput {
        workspace,
        setup_hints,
        setup_report,
    })
}

#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListWorkspacesOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let workspaces = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?;
    let mut healthy_workspaces = Vec::with_capacity(workspaces.len());
    for workspace in workspaces {
        if resolve_workspace_broken_reason(&workspace).is_some() {
            repo.delete_workspace(&workspace.id)
                .await
                .map_err(|error| error.to_string())?;
            continue;
        }
        healthy_workspaces.push(workspace);
    }

    Ok(ListWorkspacesOutput {
        workspaces: healthy_workspaces,
    })
}

#[tauri::command]
pub async fn list_repositories(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListRepositoriesOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let repositories = repo
        .list_repositories()
        .await
        .map_err(|error| error.to_string())?;

    Ok(ListRepositoriesOutput { repositories })
}

#[tauri::command]
pub async fn list_workspace_bundles(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListWorkspaceBundlesOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let bundles = repo
        .list_workspace_bundles()
        .await
        .map_err(|error| error.to_string())?;
    Ok(ListWorkspaceBundlesOutput { bundles })
}

async fn set_workspace_bundle_state(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceBundleIdInput,
    bundle_state: WorkspaceBundleState,
) -> Result<WorkspaceBundleStateOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let summary = repo
        .set_workspace_bundle_state(&input.bundle_id, bundle_state, Utc::now().to_rfc3339())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("workspace bundle not found: {}", input.bundle_id.0))?;
    Ok(WorkspaceBundleStateOutput { summary })
}

#[tauri::command]
pub async fn archive_workspace_bundle(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceBundleIdInput,
) -> Result<WorkspaceBundleStateOutput, String> {
    set_workspace_bundle_state(state, input, WorkspaceBundleState::Archived).await
}

#[tauri::command]
pub async fn restore_workspace_bundle(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceBundleIdInput,
) -> Result<WorkspaceBundleStateOutput, String> {
    set_workspace_bundle_state(state, input, WorkspaceBundleState::Ready).await
}

#[tauri::command]
pub async fn delete_workspace_bundle(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceBundleIdInput,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let session_repo =
        SqliteSessionRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let summary = repo
        .get_workspace_bundle(&input.bundle_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("workspace bundle not found: {}", input.bundle_id.0))?;
    let mut created_workspaces = Vec::new();
    for member in &summary.members {
        if !member.created_for_bundle {
            continue;
        }
        let workspace = repo
            .get_workspace(&member.workspace_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("workspace not found: {}", member.workspace_id.0))?;
        created_workspaces.push(workspace);
    }

    let mut cleanup_errors = Vec::new();
    for workspace in created_workspaces.iter().rev() {
        if let Err(error) = cleanup_delegation_worktrees(&session_repo, workspace).await {
            cleanup_errors.push(format!("{} delegations: {error}", workspace.id.0));
            continue;
        }
        if let Err(error) = cleanup_workspace_files(workspace) {
            cleanup_errors.push(format!("{}: {error}", workspace.id.0));
        }
    }
    if !cleanup_errors.is_empty() {
        return Err(format!(
            "multi-workspace cleanup was incomplete: {}",
            cleanup_errors.join("; ")
        ));
    }

    let mut bundle_session_ids = Vec::new();
    for workspace in &created_workspaces {
        let sessions = session_repo
            .list_workspace_sessions(&workspace.id)
            .map_err(|error| error.to_string())?;
        bundle_session_ids.extend(sessions.into_iter().map(|summary| summary.session.id));
    }
    for session_id in bundle_session_ids {
        SessionRepo::delete_session(&session_repo, &session_id)
            .await
            .map_err(|error| {
                format!(
                    "failed to remove multi-workspace session {}: {error}",
                    session_id.0
                )
            })?;
    }

    repo.delete_workspace_bundle(&input.bundle_id)
        .await
        .map_err(|error| error.to_string())?;
    for workspace in created_workspaces {
        cleanup_unused_workspace_push_target(&repo, &workspace).await?;
        repo.delete_workspace(&workspace.id)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_local_branches(
    state: State<'_, WorkspaceCommandState>,
    input: ListLocalBranchesInput,
) -> Result<ListLocalBranchesOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let branches =
        list_local_branch_names(&input.workspace_root).map_err(|error| error.to_string())?;
    Ok(ListLocalBranchesOutput { branches })
}

/// Paths tracked by git (`git ls-files`), repo-relative forward slashes.
/// Empty vec if not a git worktree or git fails.
#[tauri::command]
pub async fn list_git_tracked_files(
    state: State<'_, WorkspaceCommandState>,
    input: ListGitTrackedFilesInput,
) -> Result<ListGitTrackedFilesOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(ListGitTrackedFilesOutput { paths: Vec::new() });
    }

    let output = run_git_output(root, &["ls-files", "-z"])?;

    if !output.status.success() {
        return Ok(ListGitTrackedFilesOutput { paths: Vec::new() });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut paths: Vec<String> = stdout
        .split('\0')
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();
    paths.sort();
    paths.dedup();

    Ok(ListGitTrackedFilesOutput { paths })
}

/// Reads the current working-tree body of an arbitrary worktree file. The path is
/// confined to the workspace root (no `..` escapes). Used by Quick Open and the
/// read-only file surface to open files that may not have pending changes.
#[tauri::command]
pub async fn read_workspace_file(
    state: State<'_, WorkspaceCommandState>,
    input: ReadWorkspaceFileInput,
) -> Result<ReadWorkspaceFileOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let rel = validate_git_relative_path(&input.relative_path)?;
    let content =
        read_worktree_file_text(root, &rel)?.ok_or_else(|| "file not found".to_string())?;

    Ok(ReadWorkspaceFileOutput { content })
}

/// Writes `content` to an existing worktree file (path confined to the workspace
/// root, no `..` escapes). Used by the editable file surface. The reconciliation
/// against concurrent agent edits is handled client-side before this is called.
#[tauri::command]
pub async fn write_workspace_file(
    state: State<'_, WorkspaceCommandState>,
    input: WriteWorkspaceFileInput,
) -> Result<WriteWorkspaceFileOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let rel = validate_git_relative_path(&input.relative_path)?;
    let path = resolve_worktree_write_path(root, &rel)?;

    // Compare-and-swap: verify the disk still matches what the caller last saw
    // before overwriting. Doing the read+compare+write in one command shrinks the
    // window where a concurrent agent edit could be clobbered.
    if let Some(expected) = &input.expected_previous {
        let current = read_worktree_file_text(root, &rel)?.unwrap_or_default();
        if &current != expected {
            return Ok(WriteWorkspaceFileOutput {
                bytes_written: 0,
                conflicted: true,
                disk_content: Some(current),
            });
        }
    }

    let bytes = input.content.into_bytes();
    let bytes_written = bytes.len() as u32;
    fs::write(&path, &bytes).map_err(|error| error.to_string())?;

    Ok(WriteWorkspaceFileOutput {
        bytes_written,
        conflicted: false,
        disk_content: None,
    })
}

const SEARCH_WORKSPACE_MAX_RESULTS: usize = 200;

fn parse_git_grep_z_output(stdout: &[u8], max_results: usize) -> SearchWorkspaceOutput {
    let mut matches = Vec::new();
    let mut truncated = false;
    let stdout = String::from_utf8_lossy(stdout);
    for record in stdout.split('\n') {
        if record.is_empty() {
            continue;
        }
        // Record format: "<path>\0<line>:<text>"
        let Some((path, rest)) = record.split_once('\0') else {
            continue;
        };
        let Some((line_str, text)) = rest.split_once(':') else {
            continue;
        };
        let Ok(line) = line_str.trim().parse::<u32>() else {
            continue;
        };
        if matches.len() >= max_results {
            truncated = true;
            break;
        }
        matches.push(SearchWorkspaceMatch {
            path: path.to_string(),
            line,
            text: text.chars().take(400).collect(),
        });
    }

    SearchWorkspaceOutput { matches, truncated }
}

#[cfg(test)]
mod editor_workspace_file_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("dcc-{name}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn as_str(&self) -> &str {
            self.path.to_str().expect("utf-8 temp path")
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn registered_repository(project_id: &str, root: &str) -> Repository {
        Repository {
            id: RepositoryId(root.to_string()),
            project_id: dcc_core::domain::project::ProjectId(project_id.to_string()),
            name: project_id.to_string(),
            root_path: root.to_string(),
            base_branch: "main".to_string(),
            remote: None,
            remote_url: None,
            forge_provider: None,
            forge_login: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn bundle_project(project_id: &str, root: &str) -> CreateWorkspaceForRepoInput {
        CreateWorkspaceForRepoInput {
            project_id: dcc_core::domain::project::ProjectId(project_id.to_string()),
            workspace_root: root.to_string(),
            base_branch: "main".to_string(),
            name: None,
        }
    }

    fn forge_target(
        provider: crate::commands::forge_commands::ForgeCliProvider,
    ) -> WorkspaceForgeTarget {
        WorkspaceForgeTarget {
            provider,
            remote_name: "origin".to_string(),
            remote: crate::commands::forge::remote::parse_remote(
                "https://github.com/acme/widgets.git",
            )
            .expect("parsed remote"),
        }
    }

    #[test]
    fn source_url_parser_accepts_only_the_current_github_repository() {
        let target = forge_target(crate::commands::forge_commands::ForgeCliProvider::Github);
        assert_eq!(
            parse_workspace_source_url("https://github.com/acme/widgets/pull/42", &target)
                .expect("pull request URL"),
            ParsedWorkspaceSourceKind::PullRequest(42)
        );
        assert_eq!(
            parse_workspace_source_url(
                "https://github.com/acme/widgets/tree/feature/review",
                &target,
            )
            .expect("branch URL"),
            ParsedWorkspaceSourceKind::Branch("feature/review".to_string())
        );
        assert!(
            parse_workspace_source_url("https://github.com/acme/other/pull/42", &target).is_err()
        );
    }

    #[test]
    fn source_branch_resolution_rejects_path_ambiguity() {
        let heads = vec![
            ("feature".to_string(), "111".to_string()),
            ("feature/docs".to_string(), "222".to_string()),
        ];
        assert!(resolve_branch_from_url_path("feature/docs", &heads).is_err());
        assert_eq!(
            resolve_branch_from_url_path(
                "feature/docs",
                &[("feature/docs".to_string(), "222".to_string())],
            )
            .expect("unambiguous branch"),
            ("feature/docs".to_string(), "222".to_string())
        );
    }

    fn initialize_remote_test_repository(root: &str) {
        let output = run_git_output(root, &["init"]).expect("git init");
        assert!(output.status.success());
    }

    fn imported_fork_workspace(id: &str, root: &str, remote_name: &str) -> Workspace {
        Workspace {
            id: WorkspaceId(id.to_string()),
            project_id: dcc_core::domain::project::ProjectId("project-1".to_string()),
            name: Some(id.to_string()),
            root_path: root.to_string(),
            base_branch: "main".to_string(),
            worktree_path: None,
            source: Some(WorkspaceSource {
                kind: WorkspaceSourceKind::PullRequest,
                url: "https://github.com/acme/widgets/pull/42".to_string(),
                provider: "github".to_string(),
                remote_name: "origin".to_string(),
                head_branch: "feature/review".to_string(),
                head_sha: "abc123".to_string(),
                base_branch: "main".to_string(),
                change_request_number: Some(42),
                title: Some("Review".to_string()),
                author: Some("wharley".to_string()),
                source_repository: Some("wharley/widgets".to_string()),
                push_target: Some(WorkspacePushTarget {
                    remote_name: remote_name.to_string(),
                    branch_name: "feature/review".to_string(),
                    remote_url: Some("https://github.com/wharley/widgets.git".to_string()),
                    remote_created: true,
                }),
            }),
            state: WorkspaceState::Ready,
            setup_report: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn fork_remote_names_are_deterministic_and_sanitized() {
        assert_eq!(
            preferred_fork_remote_name("Wharley.Ornelas/widgets"),
            "dcc-wharley.ornelas"
        );
        assert_eq!(
            preferred_fork_remote_name("company/platform team/widgets"),
            "dcc-company-platform-team"
        );
    }

    #[test]
    fn push_target_reuses_an_existing_remote_for_the_same_repository() {
        let dir = TestDir::new("reuse-fork-remote");
        initialize_remote_test_repository(dir.as_str());
        let output = run_git_output(
            dir.as_str(),
            &[
                "remote",
                "add",
                "contributor",
                "git@github.com:wharley/widgets.git",
            ],
        )
        .expect("add existing remote");
        assert!(output.status.success());

        let target = prepare_workspace_push_target(
            dir.as_str(),
            &RequestedWorkspacePushTarget {
                preferred_remote_name: "dcc-wharley".to_string(),
                branch_name: "feature/review".to_string(),
                remote_url: Some("https://github.com/wharley/widgets.git".to_string()),
            },
        )
        .expect("reuse matching remote");

        assert_eq!(target.remote_name, "contributor");
        assert!(!target.remote_created);
        assert_eq!(list_workspace_remote_names(dir.as_str()).unwrap().len(), 1);
    }

    #[test]
    fn push_target_uses_a_unique_name_and_cleans_up_only_its_remote() {
        let dir = TestDir::new("create-fork-remote");
        initialize_remote_test_repository(dir.as_str());
        let output = run_git_output(
            dir.as_str(),
            &[
                "remote",
                "add",
                "dcc-wharley",
                "https://github.com/other/widgets.git",
            ],
        )
        .expect("add colliding remote");
        assert!(output.status.success());

        let target = prepare_workspace_push_target(
            dir.as_str(),
            &RequestedWorkspacePushTarget {
                preferred_remote_name: "dcc-wharley".to_string(),
                branch_name: "feature/review".to_string(),
                remote_url: Some("https://github.com/wharley/widgets.git".to_string()),
            },
        )
        .expect("create unique remote");

        assert_eq!(target.remote_name, "dcc-wharley-2");
        assert!(target.remote_created);
        cleanup_prepared_workspace_push_target(dir.as_str(), &target);
        assert_eq!(
            list_workspace_remote_names(dir.as_str()).unwrap(),
            vec!["dcc-wharley".to_string()]
        );
    }

    #[test]
    fn fork_remote_cleanup_waits_for_the_last_workspace() {
        use std::sync::{Arc, Mutex};

        let dir = TestDir::new("cleanup-fork-remote");
        initialize_remote_test_repository(dir.as_str());
        let output = run_git_output(
            dir.as_str(),
            &[
                "remote",
                "add",
                "dcc-wharley",
                "https://github.com/wharley/widgets.git",
            ],
        )
        .expect("add DCC remote");
        assert!(output.status.success());

        let connection = rusqlite::Connection::open_in_memory().expect("in-memory database");
        let repo = SqliteWorkspaceRepo::from_connection(Arc::new(Mutex::new(connection)))
            .expect("workspace repository");
        let first = imported_fork_workspace("fork-1", dir.as_str(), "dcc-wharley");
        let second = imported_fork_workspace("fork-2", dir.as_str(), "dcc-wharley");
        futures::executor::block_on(repo.save_workspace(&first)).expect("save first workspace");
        futures::executor::block_on(repo.save_workspace(&second)).expect("save second workspace");

        futures::executor::block_on(cleanup_unused_workspace_push_target(&repo, &first))
            .expect("keep shared remote");
        assert!(
            run_git_output(dir.as_str(), &["remote", "get-url", "dcc-wharley"])
                .unwrap()
                .status
                .success()
        );

        futures::executor::block_on(repo.delete_workspace(&first.id))
            .expect("delete first workspace");
        futures::executor::block_on(cleanup_unused_workspace_push_target(&repo, &second))
            .expect("remove last remote");
        assert!(
            !run_git_output(dir.as_str(), &["remote", "get-url", "dcc-wharley"])
                .unwrap()
                .status
                .success()
        );
    }

    #[test]
    fn bundle_validation_accepts_distinct_registered_projects() {
        let repositories = vec![
            registered_repository("backend", "/tmp/backend"),
            registered_repository("frontend", "/tmp/frontend"),
        ];
        let input = CreateWorkspaceBundleForReposInput {
            name: "Checkout".to_string(),
            projects: vec![
                bundle_project("backend", "/tmp/backend"),
                bundle_project("frontend", "/tmp/frontend"),
            ],
        };

        validate_bundle_projects(&repositories, &input).expect("valid bundle projects");
    }

    #[test]
    fn bundle_validation_rejects_unregistered_or_duplicate_projects() {
        let repositories = vec![registered_repository("backend", "/tmp/backend")];
        let unregistered = CreateWorkspaceBundleForReposInput {
            name: "Checkout".to_string(),
            projects: vec![
                bundle_project("backend", "/tmp/backend"),
                bundle_project("frontend", "/tmp/frontend"),
            ],
        };
        assert!(validate_bundle_projects(&repositories, &unregistered)
            .expect_err("unregistered project must fail")
            .contains("must be opened"));

        let duplicate = CreateWorkspaceBundleForReposInput {
            name: "Checkout".to_string(),
            projects: vec![
                bundle_project("backend", "/tmp/backend"),
                bundle_project("backend", "/tmp/backend"),
            ],
        };
        assert!(validate_bundle_projects(&repositories, &duplicate)
            .expect_err("duplicate project must fail")
            .contains("duplicate"));
    }

    #[test]
    fn sync_base_fetch_refspec_materializes_remote_tracking_branch() {
        assert_eq!(
            remote_tracking_ref("origin", "main"),
            "refs/remotes/origin/main"
        );
        assert_eq!(
            remote_branch_fetch_refspec("origin", "release/2026"),
            "+refs/heads/release/2026:refs/remotes/origin/release/2026"
        );
    }

    #[test]
    fn unmerged_index_parser_groups_stages_and_classifies_conflicts() {
        let oid1 = "1".repeat(40);
        let oid2 = "2".repeat(40);
        let oid3 = "3".repeat(40);
        let mut raw = Vec::new();
        for (stage, oid) in [(1, &oid1), (2, &oid2), (3, &oid3)] {
            raw.extend_from_slice(format!("100644 {oid} {stage}\tsrc/with\ttab.ts").as_bytes());
            raw.push(0);
        }
        raw.extend_from_slice(format!("100644 {oid2} 2\tadded.ts").as_bytes());
        raw.push(0);
        raw.extend_from_slice(format!("100644 {oid3} 3\tadded.ts").as_bytes());
        raw.push(0);

        let entries = parse_unmerged_index_entries(&raw).expect("parse unmerged index");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "added.ts");
        assert_eq!(
            classify_unmerged_entry(&entries[0]),
            WorkspaceGitConflictKind::BothAdded
        );
        assert_eq!(entries[1].path, "src/with\ttab.ts");
        assert_eq!(
            classify_unmerged_entry(&entries[1]),
            WorkspaceGitConflictKind::BothModified
        );
    }

    #[test]
    fn conflict_content_distinguishes_text_binary_and_large_files() {
        let text = conflict_content(Some(b"hello\n".to_vec()), Some("100644".to_string()));
        assert_eq!(text.text.as_deref(), Some("hello\n"));
        assert!(!text.binary);
        assert!(!text.truncated);

        let binary = conflict_content(Some(vec![b'a', 0, b'b']), None);
        assert!(binary.binary);
        assert!(binary.text.is_none());

        let large = conflict_content(Some(vec![b'a'; MAX_CONFLICT_TEXT_BYTES + 1]), None);
        assert!(large.truncated);
        assert!(!large.binary);
        assert!(large.text.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn configured_merge_validations_capture_failures_and_passes() {
        let repo = TestDir::new("merge-validations");
        let git = |args: &[&str]| {
            let output = run_git_output(repo.as_str(), args).expect("run git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.name", "DCC Test"]);
        git(&["config", "user.email", "dcc@example.invalid"]);
        fs::write(repo.path.join("app.txt"), "stable\n").expect("write tracked file");
        git(&["add", "app.txt"]);
        git(&["commit", "-m", "base"]);

        fs::write(
            repo.path.join(".dcc.toml"),
            "[scripts]\nvalidate = \"printf 'import order error\\n'; exit 7\"\n",
        )
        .expect("write failing validation");
        let (failed, _) =
            workspace_git_run_validations_inner(repo.as_str()).expect("run validation");
        assert_eq!(failed.status, WorkspaceGitValidationStatus::Failed);
        assert_eq!(failed.steps[0].exit_code, Some(7));
        assert!(failed.steps[0].output.contains("import order error"));

        fs::write(
            repo.path.join(".dcc.toml"),
            "[scripts]\nvalidate = [\"printf 'lint ok\\n'\", \"printf 'types ok\\n'\"]\n",
        )
        .expect("write passing validations");
        let (passed, fingerprint) =
            workspace_git_run_validations_inner(repo.as_str()).expect("run validations");
        assert_eq!(passed.status, WorkspaceGitValidationStatus::Passed);
        assert_eq!(passed.steps.len(), 2);
        assert!(passed.steps.iter().all(|step| step.success));
        assert!(fingerprint.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn automation_validation_honors_task_working_directory() {
        let repo = TestDir::new("automation-validation-cwd");
        let git = |args: &[&str]| {
            let output = run_git_output(repo.as_str(), args).expect("run git");
            assert!(output.status.success(), "git {:?} failed", args);
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.name", "DCC Test"]);
        git(&["config", "user.email", "dcc@example.invalid"]);
        fs::write(repo.path.join("app.txt"), "stable\n").expect("write tracked file");
        fs::create_dir_all(repo.path.join("apps/web")).expect("create task cwd");
        fs::write(repo.path.join("apps/web/marker.txt"), "ok\n").expect("write marker");
        git(&["add", "."]);
        git(&["commit", "-m", "base"]);

        let (report, fingerprint) = workspace_git_run_automation_validations_inner(
            repo.as_str(),
            vec![RepoAutomationTask {
                id: "web_check".to_string(),
                label: Some("Web check".to_string()),
                command: "test -f marker.txt".to_string(),
                kind: RepoTaskKind::Check,
                cwd: Some("apps/web".to_string()),
                timeout_seconds: 30,
            }],
            Some(repo.path.join(".dcc.toml").to_string_lossy().to_string()),
        )
        .expect("run automation validation");

        assert_eq!(report.status, WorkspaceGitValidationStatus::Passed);
        assert_eq!(report.steps.len(), 1);
        assert!(fingerprint.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn configured_validation_that_changes_tracked_files_is_rejected() {
        let repo = TestDir::new("merge-validation-mutation");
        let git = |args: &[&str]| {
            let output = run_git_output(repo.as_str(), args).expect("run git");
            assert!(output.status.success(), "git {:?} failed", args);
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.name", "DCC Test"]);
        git(&["config", "user.email", "dcc@example.invalid"]);
        fs::write(repo.path.join("app.txt"), "before\n").expect("write tracked file");
        git(&["add", "app.txt"]);
        git(&["commit", "-m", "base"]);
        fs::write(
            repo.path.join(".dcc.toml"),
            "[scripts]\nvalidate = \"printf 'after\\n' > app.txt\"\n",
        )
        .expect("write mutating validation");

        let (report, _) =
            workspace_git_run_validations_inner(repo.as_str()).expect("run validation");
        assert_eq!(report.status, WorkspaceGitValidationStatus::Failed);
        assert!(report
            .steps
            .last()
            .is_some_and(|step| step.command.contains("consistency")));
    }

    #[test]
    fn conflict_state_reads_git_index_stages_and_worktree_result() {
        let repo = TestDir::new("merge-conflict-state");
        let git = |args: &[&str]| {
            let output = run_git_output(repo.as_str(), args).expect("run git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };

        git(&["init", "-b", "current"]);
        git(&["config", "user.name", "DCC Test"]);
        git(&["config", "user.email", "dcc@example.invalid"]);
        fs::write(repo.path.join("app.txt"), "shared\nbase\n").expect("write base");
        git(&["add", "app.txt"]);
        git(&["commit", "-m", "base"]);
        git(&["branch", "incoming"]);

        fs::write(repo.path.join("app.txt"), "shared\ncurrent\n").expect("write current");
        git(&["add", "app.txt"]);
        git(&["commit", "-m", "current"]);

        git(&["checkout", "incoming"]);
        fs::write(repo.path.join("app.txt"), "shared\nincoming\n").expect("write incoming");
        git(&["add", "app.txt"]);
        git(&["commit", "-m", "incoming"]);
        git(&["checkout", "current"]);

        let merge = run_git_output(repo.as_str(), &["merge", "incoming"]).expect("run merge");
        assert!(!merge.status.success(), "merge should conflict");

        let state = workspace_git_conflict_state_inner(repo.as_str()).expect("read conflict state");
        assert_eq!(state.operation, WorkspaceGitConflictOperation::Merge);
        assert_eq!(state.current_branch.as_deref(), Some("current"));
        assert_eq!(state.conflicts.len(), 1);
        let conflict = &state.conflicts[0];
        assert_eq!(conflict.path, "app.txt");
        assert_eq!(conflict.kind, WorkspaceGitConflictKind::BothModified);
        assert_eq!(conflict.base.text.as_deref(), Some("shared\nbase\n"));
        assert_eq!(conflict.current.text.as_deref(), Some("shared\ncurrent\n"));
        assert_eq!(
            conflict.incoming.text.as_deref(),
            Some("shared\nincoming\n")
        );
        assert!(conflict
            .result
            .text
            .as_deref()
            .is_some_and(|text| text.contains("<<<<<<< HEAD")));

        workspace_git_accept_conflict_inner(
            repo.as_str(),
            "app.txt",
            WorkspaceGitConflictSide::Incoming,
        )
        .expect("accept incoming file");
        assert_eq!(
            resolve_conflict_count(repo.as_str()).expect("conflict count"),
            0
        );
        assert_eq!(
            fs::read_to_string(repo.path.join("app.txt")).expect("read accepted file"),
            "shared\nincoming\n"
        );

        workspace_git_abort_merge_inner(repo.as_str()).expect("abort merge");
        assert_eq!(
            fs::read_to_string(repo.path.join("app.txt")).expect("read restored file"),
            "shared\ncurrent\n"
        );
        assert_eq!(
            resolve_conflict_operation(repo.as_str(), false),
            WorkspaceGitConflictOperation::None
        );

        let merge = run_git_output(repo.as_str(), &["merge", "incoming"]).expect("merge again");
        assert!(!merge.status.success(), "second merge should conflict");
        fs::write(repo.path.join("app.txt"), "shared\ncurrent\nincoming\n")
            .expect("write combined result");
        workspace_git_mark_conflict_resolved_inner(repo.as_str(), "app.txt", false)
            .expect("mark resolved");
        let ready = workspace_git_conflict_state_inner(repo.as_str())
            .expect("read merge-ready conflict state");
        assert_eq!(ready.operation, WorkspaceGitConflictOperation::Merge);
        assert!(ready.conflicts.is_empty());
        workspace_git_complete_merge_commit_inner(repo.as_str()).expect("complete merge commit");
        assert_eq!(
            resolve_conflict_operation(repo.as_str(), false),
            WorkspaceGitConflictOperation::None
        );
        let parents = run_git_output(repo.as_str(), &["rev-list", "--parents", "-n", "1", "HEAD"])
            .expect("read merge parents");
        assert_eq!(
            String::from_utf8_lossy(&parents.stdout)
                .split_whitespace()
                .count(),
            3,
            "merge commit should have two parents"
        );
    }

    #[test]
    fn accepting_deleted_side_resolves_modify_delete_conflict() {
        let repo = TestDir::new("merge-delete-conflict");
        let git = |args: &[&str]| {
            let output = run_git_output(repo.as_str(), args).expect("run git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };

        git(&["init", "-b", "current"]);
        git(&["config", "user.name", "DCC Test"]);
        git(&["config", "user.email", "dcc@example.invalid"]);
        fs::write(repo.path.join("removed.txt"), "base\n").expect("write base");
        git(&["add", "removed.txt"]);
        git(&["commit", "-m", "base"]);
        git(&["branch", "incoming"]);

        fs::write(repo.path.join("removed.txt"), "current changed\n").expect("modify current");
        git(&["add", "removed.txt"]);
        git(&["commit", "-m", "modify current"]);
        git(&["checkout", "incoming"]);
        git(&["rm", "removed.txt"]);
        git(&["commit", "-m", "delete incoming"]);
        git(&["checkout", "current"]);

        let merge = run_git_output(repo.as_str(), &["merge", "incoming"]).expect("merge");
        assert!(!merge.status.success(), "merge should conflict");
        let state = workspace_git_conflict_state_inner(repo.as_str()).expect("conflict state");
        assert_eq!(
            state.conflicts[0].kind,
            WorkspaceGitConflictKind::DeletedByIncoming
        );

        workspace_git_accept_conflict_inner(
            repo.as_str(),
            "removed.txt",
            WorkspaceGitConflictSide::Incoming,
        )
        .expect("accept incoming deletion");
        assert!(!repo.path.join("removed.txt").exists());
        assert_eq!(
            resolve_conflict_count(repo.as_str()).expect("conflict count"),
            0
        );

        workspace_git_abort_merge_inner(repo.as_str()).expect("abort merge");
        assert_eq!(
            fs::read_to_string(repo.path.join("removed.txt")).expect("restored current file"),
            "current changed\n"
        );
    }

    #[test]
    fn relative_path_validation_blocks_escapes_but_allows_dot_names() {
        assert_eq!(
            validate_git_relative_path("src/..hidden/file.rs").expect("valid dot-name"),
            "src/..hidden/file.rs"
        );
        assert_eq!(
            validate_git_relative_path("./src/main.rs").expect("valid curdir"),
            "./src/main.rs"
        );
        assert!(validate_git_relative_path("../secret").is_err());
        assert!(validate_git_relative_path("src/../secret").is_err());
        assert!(validate_git_relative_path("/tmp/secret").is_err());
    }

    #[test]
    fn delegation_worktree_paths_are_scoped_to_delegation_root() {
        let repo = TestDir::new("delegation-paths");
        let active_root = repo.path.join(".dcc-worktrees").join("main-123");
        let delegation_root = repo.path.join(".dcc-worktrees").join(".dcc-delegations");
        let delegation_path = delegation_root.join("dcc-delegation-abc");
        fs::create_dir_all(&delegation_path).expect("create delegation worktree");

        assert_eq!(delegation_worktrees_root(&active_root), delegation_root,);
        assert_eq!(
            validate_delegation_worktree_path(
                active_root.to_str().expect("utf-8 active root"),
                delegation_path.to_str().expect("utf-8 delegation path"),
            )
            .expect("delegation path is accepted"),
            delegation_path,
        );

        let outside = repo.path.join(".dcc-worktrees").join("main-456");
        fs::create_dir_all(&outside).expect("create outside worktree");
        assert!(validate_delegation_worktree_path(
            active_root.to_str().expect("utf-8 active root"),
            outside.to_str().expect("utf-8 outside path"),
        )
        .is_err());
    }

    #[test]
    fn copy_untracked_delegation_files_copies_nested_files() {
        let parent = TestDir::new("delegation-parent");
        let child = TestDir::new("delegation-child");
        let migration = "apps/api/prisma/migrations/20260703120000_add_flag/migration.sql";
        let source_path = child.path.join(migration);
        fs::create_dir_all(source_path.parent().expect("migration parent"))
            .expect("create migration dir");
        fs::write(&source_path, "alter table users add column flag boolean;\n")
            .expect("write migration");

        copy_untracked_delegation_files(&child.path, &parent.path, &[migration.to_string()])
            .expect("copy untracked migration");

        assert_eq!(
            fs::read_to_string(parent.path.join(migration)).expect("read copied migration"),
            "alter table users add column flag boolean;\n",
        );
    }

    #[test]
    fn copy_untracked_delegation_files_refuses_existing_destination() {
        let parent = TestDir::new("delegation-parent-existing");
        let child = TestDir::new("delegation-child-existing");
        let relative_path = "db/migrations/001.sql";
        let source_path = child.path.join(relative_path);
        let destination_path = parent.path.join(relative_path);
        fs::create_dir_all(source_path.parent().expect("source parent")).expect("source dir");
        fs::create_dir_all(destination_path.parent().expect("destination parent"))
            .expect("destination dir");
        fs::write(&source_path, "select 1;\n").expect("write source");
        fs::write(&destination_path, "select 0;\n").expect("write destination");

        assert!(copy_untracked_delegation_files(
            &child.path,
            &parent.path,
            &[relative_path.to_string()],
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(destination_path).expect("read destination"),
            "select 0;\n",
        );
    }

    #[test]
    fn delegation_key_suffix_sanitizes_user_controlled_keys() {
        assert_eq!(
            delegation_key_suffix(Some("turn-123_../../escape")),
            "turn123escap"
        );
        assert_eq!(
            delegation_key_suffix(Some("abcDEF123456789")),
            "abcDEF123456"
        );
        assert_eq!(delegation_key_suffix(Some("!!!")).len(), 12);
    }

    #[test]
    fn read_and_write_workspace_file_stay_inside_root() {
        let root = TestDir::new("workspace-files");
        fs::create_dir_all(root.path.join("src")).expect("create src");
        fs::write(root.path.join("src/main.rs"), "fn main() {}\n").expect("write file");

        let content = read_worktree_file_text(root.as_str(), "src/main.rs")
            .expect("read succeeds")
            .expect("file exists");
        assert_eq!(content, "fn main() {}\n");

        let path =
            resolve_worktree_write_path(root.as_str(), "src/main.rs").expect("resolve write path");
        fs::write(path, "fn main() { println!(\"ok\"); }\n").expect("write resolved file");
        let updated = read_worktree_file_text(root.as_str(), "src/main.rs")
            .expect("read updated")
            .expect("file exists");
        assert!(updated.contains("println!"));
    }

    #[cfg(unix)]
    #[test]
    fn workspace_file_helpers_reject_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = TestDir::new("workspace-symlink-root");
        let outside = TestDir::new("workspace-symlink-outside");
        let outside_file = outside.path.join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside file");
        symlink(&outside_file, root.path.join("linked-secret.txt")).expect("create symlink");

        let read_error = read_worktree_file_text(root.as_str(), "linked-secret.txt")
            .expect_err("read should reject symlink escape");
        assert!(read_error.contains("escapes workspace"));

        let write_error = resolve_worktree_write_path(root.as_str(), "linked-secret.txt")
            .expect_err("write should reject symlink escape");
        assert!(write_error.contains("escapes workspace"));
    }

    #[test]
    fn git_grep_parser_handles_colon_paths_and_truncation() {
        let mut input = Vec::new();
        input.extend_from_slice(b"src/with:colon.rs");
        input.push(0);
        input.extend_from_slice(b"12:let value = 1;\nsrc/lib.rs");
        input.push(0);
        input.extend_from_slice(b"3:needle\n");
        let output = parse_git_grep_z_output(&input, 1);

        assert!(output.truncated);
        assert_eq!(output.matches.len(), 1);
        assert_eq!(output.matches[0].path, "src/with:colon.rs");
        assert_eq!(output.matches[0].line, 12);
        assert_eq!(output.matches[0].text, "let value = 1;");
    }
}

/// Content search across tracked files via `git grep` (on demand, capped). Fixed
/// string, case-insensitive, repo-scoped — consistent with Quick Open's ls-files.
#[tauri::command]
pub async fn search_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: SearchWorkspaceInput,
) -> Result<SearchWorkspaceOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    let query = input.query.trim();
    if root.is_empty() || query.is_empty() {
        return Ok(SearchWorkspaceOutput {
            matches: Vec::new(),
            truncated: false,
        });
    }

    // `-z` makes the path delimiter NUL, so paths containing `:` parse cleanly.
    // git grep exits 1 with empty output when there are no matches (not an error).
    let output = run_git_output(
        root,
        &["grep", "-z", "-n", "-I", "-i", "-F", "-e", query, "--"],
    )?;

    Ok(parse_git_grep_z_output(
        &output.stdout,
        SEARCH_WORKSPACE_MAX_RESULTS,
    ))
}

/// Mission specs are intentionally scoped to DCC-owned worktree state.
#[tauri::command]
pub async fn list_mission_specs(
    state: State<'_, WorkspaceCommandState>,
    input: ListMissionSpecsInput,
) -> Result<ListMissionSpecsOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(ListMissionSpecsOutput { specs: Vec::new() });
    }

    let specs_dir = PathBuf::from(root).join(".devcommandcenter").join("specs");
    if !specs_dir.is_dir() {
        return Ok(ListMissionSpecsOutput { specs: Vec::new() });
    }

    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let specs_canonical = specs_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !specs_canonical.starts_with(&root_canonical) {
        return Err("mission specs directory must stay inside the workspace".to_string());
    }

    let mut specs = Vec::new();
    for entry in fs::read_dir(&specs_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".spec.md") {
            continue;
        }

        let file_canonical = path.canonicalize().map_err(|error| error.to_string())?;
        if !file_canonical.starts_with(&specs_canonical) {
            continue;
        }

        let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let validation_name = file_name.replace(".spec.md", ".validation.json");
        let validation_path = specs_canonical.join(&validation_name);
        let validation =
            read_mission_validation_entry(&validation_path, &validation_name, &specs_canonical)?;
        specs.push(MissionSpecEntry {
            relative_path: format!(".devcommandcenter/specs/{file_name}"),
            name: file_name.to_string(),
            content,
            validation,
        });
    }

    specs.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ListMissionSpecsOutput { specs })
}

fn read_mission_validation_entry(
    path: &Path,
    file_name: &str,
    specs_canonical: &Path,
) -> Result<Option<MissionValidationEntry>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let file_canonical = path.canonicalize().map_err(|error| error.to_string())?;
    if !file_canonical.starts_with(specs_canonical) {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let history_name = file_name.replace(".validation.json", ".validation.history.jsonl");
    let history_path = specs_canonical.join(&history_name);
    let history_relative_path = if history_path.is_file()
        && !fs::symlink_metadata(&history_path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    {
        let history_canonical = history_path
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if history_canonical.starts_with(specs_canonical) {
            Some(format!(".devcommandcenter/specs/{history_name}"))
        } else {
            None
        }
    } else {
        None
    };
    Ok(Some(MissionValidationEntry {
        relative_path: format!(".devcommandcenter/specs/{file_name}"),
        content,
        history_relative_path,
    }))
}

#[tauri::command]
pub async fn save_mission_validation(
    state: State<'_, WorkspaceCommandState>,
    input: SaveMissionValidationInput,
) -> Result<SaveMissionValidationOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let spec_relative_path = validate_mission_spec_relative_path(&input.spec_relative_path)?;
    let report: Value =
        serde_json::from_str(&input.report_json).map_err(|error| error.to_string())?;
    if report.get("dccMissionValidation").and_then(Value::as_bool) != Some(true) {
        return Err("validation report missing dccMissionValidation=true".to_string());
    }

    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let specs_dir = root_canonical.join(".devcommandcenter").join("specs");
    fs::create_dir_all(&specs_dir).map_err(|error| error.to_string())?;
    let specs_canonical = specs_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !specs_canonical.starts_with(&root_canonical) {
        return Err("mission specs directory must stay inside the workspace".to_string());
    }

    let spec_name = spec_relative_path
        .strip_prefix(".devcommandcenter/specs/")
        .ok_or_else(|| "invalid spec path".to_string())?;
    let validation_name = spec_name.replace(".spec.md", ".validation.json");
    let history_name = spec_name.replace(".spec.md", ".validation.history.jsonl");
    let target = specs_canonical.join(&validation_name);
    if fs::symlink_metadata(&target)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("validation target must not be a symlink".to_string());
    }
    let history_target = specs_canonical.join(&history_name);
    if fs::symlink_metadata(&history_target)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("validation history target must not be a symlink".to_string());
    }

    let pretty = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    fs::write(&target, format!("{pretty}\n")).map_err(|error| error.to_string())?;
    append_mission_validation_history_entry(
        &history_target,
        &spec_relative_path,
        &validation_name,
        &report,
    )?;

    Ok(SaveMissionValidationOutput {
        relative_path: format!(".devcommandcenter/specs/{validation_name}"),
        history_relative_path: format!(".devcommandcenter/specs/{history_name}"),
    })
}

fn append_mission_validation_history_entry(
    history_target: &Path,
    spec_relative_path: &str,
    validation_name: &str,
    report: &Value,
) -> Result<(), String> {
    let criteria = report
        .get("criteria")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut pass_count = 0usize;
    let mut fail_count = 0usize;
    let mut unknown_count = 0usize;
    for criterion in criteria {
        match criterion.get("status").and_then(Value::as_str) {
            Some("PASS") => pass_count += 1,
            Some("FAIL") => fail_count += 1,
            Some("UNKNOWN") => unknown_count += 1,
            _ => {}
        }
    }

    let saved_at = report
        .get("dccSavedAt")
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let history_entry = serde_json::json!({
        "dccMissionValidationHistory": true,
        "savedAt": saved_at,
        "persistenceMode": report
            .get("dccPersistenceMode")
            .and_then(Value::as_str)
            .unwrap_or("manual"),
        "specRelativePath": spec_relative_path,
        "specHash": report.get("specHash").and_then(Value::as_str),
        "validationRelativePath": format!(".devcommandcenter/specs/{validation_name}"),
        "summary": report.get("summary").and_then(Value::as_str),
        "passCount": pass_count,
        "failCount": fail_count,
        "unknownCount": unknown_count,
    });

    let serialized = serde_json::to_string(&history_entry).map_err(|error| error.to_string())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_target)
        .map_err(|error| error.to_string())?;
    use std::io::Write;
    writeln!(file, "{serialized}").map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn compile_mission_spec_context(
    state: State<'_, WorkspaceCommandState>,
    input: CompileMissionSpecContextInput,
) -> Result<CompileMissionSpecContextOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    compile_mission_spec_context_for_path(&input.workspace_root, &input.spec_relative_path)
}

fn compile_mission_spec_context_for_path(
    workspace_root: &str,
    spec_relative_path: &str,
) -> Result<CompileMissionSpecContextOutput, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let spec_relative_path = validate_mission_spec_relative_path(spec_relative_path)?;
    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let specs_dir = root_canonical.join(".devcommandcenter").join("specs");
    let specs_canonical = specs_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !specs_canonical.starts_with(&root_canonical) {
        return Err("mission specs directory must stay inside the workspace".to_string());
    }

    let spec_path = root_canonical.join(&spec_relative_path);
    if fs::symlink_metadata(&spec_path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("mission spec source must not be a symlink".to_string());
    }
    let spec_canonical = spec_path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !spec_canonical.starts_with(&specs_canonical) {
        return Err("mission spec source must stay inside .devcommandcenter/specs".to_string());
    }
    let spec_markdown = fs::read_to_string(&spec_path).map_err(|error| error.to_string())?;

    let mut files = Vec::new();
    for target in DCC_SPEC_CONTEXT_TARGETS {
        let target_path = root_canonical.join(target.relative_path);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if fs::symlink_metadata(&target_path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(format!("{} must not be a symlink", target.relative_path));
        }

        let created = !target_path.exists();
        let current = if created {
            String::new()
        } else {
            fs::read_to_string(&target_path).map_err(|error| error.to_string())?
        };
        let next = render_mission_spec_context_target_content(
            target,
            &spec_relative_path,
            &spec_markdown,
            &current,
        )?;
        let updated = next != current;
        if updated {
            fs::write(&target_path, next).map_err(|error| error.to_string())?;
        }
        files.push(CompiledMissionSpecContextFile {
            relative_path: target.relative_path.to_string(),
            created,
            updated,
        });
    }

    Ok(CompileMissionSpecContextOutput { files })
}

#[tauri::command]
pub async fn mission_spec_context_status(
    state: State<'_, WorkspaceCommandState>,
    input: MissionSpecContextStatusInput,
) -> Result<MissionSpecContextStatusOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let spec_relative_path = validate_mission_spec_relative_path(&input.spec_relative_path)?;
    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let specs_dir = root_canonical.join(".devcommandcenter").join("specs");
    let specs_canonical = specs_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !specs_canonical.starts_with(&root_canonical) {
        return Err("mission specs directory must stay inside the workspace".to_string());
    }

    let spec_path = root_canonical.join(&spec_relative_path);
    if fs::symlink_metadata(&spec_path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("mission spec source must not be a symlink".to_string());
    }
    let spec_canonical = spec_path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !spec_canonical.starts_with(&specs_canonical) {
        return Err("mission spec source must stay inside .devcommandcenter/specs".to_string());
    }
    let spec_markdown = fs::read_to_string(&spec_path).map_err(|error| error.to_string())?;

    let mut files = Vec::new();
    for target in DCC_SPEC_CONTEXT_TARGETS {
        let target_path = root_canonical.join(target.relative_path);
        let symlink = fs::symlink_metadata(&target_path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false);
        let (state, message) = if symlink {
            (
                MissionSpecContextFileState::Symlink,
                Some("context file target must not be a symlink".to_string()),
            )
        } else if !target_path.exists() {
            (
                MissionSpecContextFileState::Missing,
                Some("context file has not been generated yet".to_string()),
            )
        } else {
            let current = fs::read_to_string(&target_path).map_err(|error| error.to_string())?;
            classify_mission_spec_context_target(
                target,
                &spec_relative_path,
                &spec_markdown,
                &current,
            )?
        };
        files.push(MissionSpecContextFileStatus {
            relative_path: target.relative_path.to_string(),
            state,
            message,
        });
    }

    let current = files
        .iter()
        .all(|file| matches!(file.state, MissionSpecContextFileState::Current));

    Ok(MissionSpecContextStatusOutput { current, files })
}

fn render_mission_spec_context_target_content(
    target: MissionSpecContextTarget,
    spec_relative_path: &str,
    spec_markdown: &str,
    current: &str,
) -> Result<String, String> {
    match target.kind {
        MissionSpecContextTargetKind::MarkdownSection => {
            let generated_section =
                render_mission_spec_context_section(spec_relative_path, spec_markdown);
            upsert_generated_context_section(current, &generated_section)
        }
        MissionSpecContextTargetKind::ManifestJson => {
            render_mission_spec_context_manifest(spec_relative_path, spec_markdown)
        }
    }
}

fn render_mission_spec_context_section(spec_relative_path: &str, spec_markdown: &str) -> String {
    let normalized_spec = spec_markdown.trim();
    format!(
        "{DCC_SPEC_CONTEXT_START}\n\
## DCC Mission Spec Context\n\n\
Generated from `{spec_relative_path}` by DevCommandCenter.\n\
Do not edit inside the `dcc:spec` markers; update the source spec instead.\n\n\
### Active Mission Spec\n\n\
{normalized_spec}\n\
{DCC_SPEC_CONTEXT_END}\n"
    )
}

fn render_mission_spec_context_manifest(
    spec_relative_path: &str,
    spec_markdown: &str,
) -> Result<String, String> {
    let spec_hash = compute_mission_spec_hash(spec_markdown);
    let manifest = serde_json::json!({
        "dccContext": true,
        "version": 1,
        "kind": "mission_spec",
        "source": {
            "specRelativePath": spec_relative_path,
            "specHash": spec_hash,
        },
        "targets": DCC_SPEC_CONTEXT_TARGETS
            .iter()
            .filter(|target| target.kind == MissionSpecContextTargetKind::MarkdownSection)
            .map(|target| {
                serde_json::json!({
                    "provider": target.provider_id,
                    "relativePath": target.relative_path,
                    "format": "markdown",
                    "injection": "dcc:spec-section",
                    "startMarker": DCC_SPEC_CONTEXT_START,
                    "endMarker": DCC_SPEC_CONTEXT_END,
                })
            })
            .collect::<Vec<_>>(),
    });
    let pretty = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    Ok(format!("{pretty}\n"))
}

fn select_active_mission_spec_relative_path(
    workspace_root: &str,
    workspace_branch: &str,
) -> Result<Option<String>, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Ok(None);
    }

    let specs_dir = PathBuf::from(root).join(".devcommandcenter").join("specs");
    if !specs_dir.is_dir() {
        return Ok(None);
    }

    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let specs_canonical = specs_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !specs_canonical.starts_with(&root_canonical) {
        return Err("mission specs directory must stay inside the workspace".to_string());
    }

    let preferred = build_mission_spec_filename(workspace_branch);
    let mut spec_names = Vec::new();
    for entry in fs::read_dir(&specs_canonical).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if fs::symlink_metadata(&path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.ends_with(".spec.md") {
            spec_names.push(file_name.to_string());
        }
    }

    spec_names.sort();
    let selected = spec_names
        .iter()
        .find(|name| **name == preferred)
        .or_else(|| spec_names.first());

    Ok(selected.map(|name| format!(".devcommandcenter/specs/{name}")))
}

fn build_mission_spec_filename(workspace_branch: &str) -> String {
    let source = workspace_branch.trim();
    let mut slug = String::new();
    let mut last_was_dash = false;
    for character in source.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "mission.spec.md".to_string()
    } else {
        format!("{slug}.spec.md")
    }
}

fn classify_generated_context_section(
    current: &str,
    generated_section: &str,
) -> (MissionSpecContextFileState, Option<String>) {
    match (
        current.find(DCC_SPEC_CONTEXT_START),
        current.find(DCC_SPEC_CONTEXT_END),
    ) {
        (Some(start), Some(end)) if start < end => {
            let end_index = end + DCC_SPEC_CONTEXT_END.len();
            let existing_section = &current[start..end_index];
            if existing_section.trim_end_matches('\n') == generated_section.trim_end_matches('\n') {
                (MissionSpecContextFileState::Current, None)
            } else {
                (
                    MissionSpecContextFileState::Stale,
                    Some("compiled dcc:spec section differs from the active spec".to_string()),
                )
            }
        }
        (None, None) => (
            MissionSpecContextFileState::Missing,
            Some("context file exists without a dcc:spec section".to_string()),
        ),
        _ => (
            MissionSpecContextFileState::Invalid,
            Some("context file has incomplete dcc:spec markers".to_string()),
        ),
    }
}

fn classify_mission_spec_context_target(
    target: MissionSpecContextTarget,
    spec_relative_path: &str,
    spec_markdown: &str,
    current: &str,
) -> Result<(MissionSpecContextFileState, Option<String>), String> {
    match target.kind {
        MissionSpecContextTargetKind::MarkdownSection => Ok(classify_generated_context_section(
            current,
            &render_mission_spec_context_section(spec_relative_path, spec_markdown),
        )),
        MissionSpecContextTargetKind::ManifestJson => classify_generated_context_manifest(
            current,
            &render_mission_spec_context_manifest(spec_relative_path, spec_markdown)?,
        ),
    }
}

fn classify_generated_context_manifest(
    current: &str,
    generated_manifest: &str,
) -> Result<(MissionSpecContextFileState, Option<String>), String> {
    let current_json: Value = match serde_json::from_str(current) {
        Ok(value) => value,
        Err(_) => {
            return Ok((
                MissionSpecContextFileState::Invalid,
                Some("context manifest is not valid JSON".to_string()),
            ));
        }
    };
    let generated_json: Value =
        serde_json::from_str(generated_manifest).map_err(|error| error.to_string())?;
    if current_json == generated_json {
        Ok((MissionSpecContextFileState::Current, None))
    } else {
        Ok((
            MissionSpecContextFileState::Stale,
            Some("context manifest differs from the active spec and provider targets".to_string()),
        ))
    }
}

fn compute_mission_spec_hash(spec_markdown: &str) -> String {
    let mut hash: u32 = 0x811c9dc5;
    for byte in spec_markdown.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("fnv1a32:{hash:08x}")
}

fn upsert_generated_context_section(
    current: &str,
    generated_section: &str,
) -> Result<String, String> {
    match (
        current.find(DCC_SPEC_CONTEXT_START),
        current.find(DCC_SPEC_CONTEXT_END),
    ) {
        (Some(start), Some(end)) if start < end => {
            let end_index = end + DCC_SPEC_CONTEXT_END.len();
            let mut next = String::new();
            next.push_str(&current[..start]);
            next.push_str(generated_section);
            next.push_str(current[end_index..].trim_start_matches('\n'));
            Ok(next)
        }
        (None, None) => {
            if current.trim().is_empty() {
                return Ok(generated_section.to_string());
            }
            Ok(format!("{}\n\n{}", current.trim_end(), generated_section))
        }
        _ => Err("existing context file has incomplete dcc:spec markers".to_string()),
    }
}

fn validate_mission_spec_relative_path(path: &str) -> Result<String, String> {
    let normalized = normalize_git_relative_path(path);
    if !normalized.starts_with(".devcommandcenter/specs/") {
        return Err("mission spec path must be under .devcommandcenter/specs".to_string());
    }
    if normalized.contains("..") || normalized.contains("//") {
        return Err("invalid mission spec path".to_string());
    }
    let file_name = normalized
        .strip_prefix(".devcommandcenter/specs/")
        .unwrap_or_default();
    if file_name.is_empty() || file_name.contains('/') || !file_name.ends_with(".spec.md") {
        return Err("mission spec path must point to a .spec.md file".to_string());
    }
    Ok(normalized)
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitBranchDiffInput {
    pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitBranchDiffOutput {
    pub changes: Vec<WorkspaceGitChangeEntry>,
    pub base_branch: Option<String>,
}

/// Returns files changed on HEAD vs the upstream/base branch (`git diff <base>...HEAD`).
/// Falls back through: @{upstream} → origin/HEAD → origin/main → origin/master.
#[tauri::command]
pub async fn workspace_git_branch_diff(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitBranchDiffInput,
) -> Result<WorkspaceGitBranchDiffOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    let base = resolve_branch_diff_base(root, protected_branch.as_deref());

    let changes = match base {
        Some(ref b) => compute_branch_diff(root, b)?,
        None => vec![],
    };

    Ok(WorkspaceGitBranchDiffOutput {
        changes,
        base_branch: base,
    })
}

fn compute_branch_diff(root: &str, base: &str) -> Result<Vec<WorkspaceGitChangeEntry>, String> {
    let range = format!("{base}...HEAD");

    // name-status
    let ns_out = run_git_output_owned(
        root,
        vec![
            OsString::from("diff"),
            OsString::from("--name-status"),
            OsString::from("-z"),
            OsString::from(&range),
        ],
    )?;
    if !ns_out.status.success() {
        return Err(git_output_err("git diff --name-status", &ns_out.stderr));
    }

    // numstat
    let stat_map = {
        let stat_out = run_git_output_owned(
            root,
            vec![
                OsString::from("diff"),
                OsString::from("--numstat"),
                OsString::from("-z"),
                OsString::from(&range),
            ],
        )?;
        if stat_out.status.success() {
            parse_numstat_z(&stat_out.stdout)
        } else {
            HashMap::new()
        }
    };

    let mut entries = Vec::new();
    for entry in parse_name_status_z(&ns_out.stdout) {
        let path = entry.path;
        let status = entry.status;
        let (insertions, deletions) = stat_map.get(&path).copied().unwrap_or((0, 0));
        let name = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        entries.push(WorkspaceGitChangeEntry {
            path: normalize_git_relative_path(&path),
            name,
            absolute_path: join_workspace_path(root, &path),
            status,
            insertions,
            deletions,
        });
    }
    Ok(entries)
}

/// Immediate child directories of `path` (absolute paths), sorted.
#[tauri::command]
pub async fn list_child_directories(
    input: ListChildDirectoriesInput,
) -> Result<ListChildDirectoriesOutput, String> {
    let root = Path::new(input.path.trim());
    if !root.is_dir() {
        return Ok(ListChildDirectoriesOutput { paths: Vec::new() });
    }

    let mut paths: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            paths.push(p.to_string_lossy().to_string());
        }
    }
    paths.sort();
    Ok(ListChildDirectoriesOutput { paths })
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIdInput {
    pub workspace_id: String,
}

async fn ensure_workspace_is_not_a_bundle_member(
    repo: &SqliteWorkspaceRepo,
    workspace_id: &WorkspaceId,
    operation: &str,
) -> Result<(), String> {
    let Some(summary) = repo
        .get_workspace_bundle_for_workspace(workspace_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    Err(format!(
        "workspace {} belongs to multi-workspace '{}' ({}); {operation} the multi-workspace instead",
        workspace_id.0, summary.bundle.name, summary.bundle.id.0
    ))
}

#[tauri::command]
pub async fn archive_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceIdInput,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|e| e.to_string())?;
    let id = WorkspaceId(input.workspace_id);
    ensure_workspace_is_not_a_bundle_member(&repo, &id, "archive").await?;
    let mut workspace = repo
        .get_workspace(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", id.0))?;
    workspace.state = WorkspaceState::Archived;
    repo.save_workspace(&workspace)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceIdInput,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|e| e.to_string())?;
    let id = WorkspaceId(input.workspace_id);
    ensure_workspace_is_not_a_bundle_member(&repo, &id, "restore").await?;
    let mut workspace = repo
        .get_workspace(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", id.0))?;
    workspace.state = WorkspaceState::Ready;
    repo.save_workspace(&workspace)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceIdInput,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|e| e.to_string())?;
    let session_repo = SqliteSessionRepo::open(&state.db_path).map_err(|e| e.to_string())?;
    let id = WorkspaceId(input.workspace_id);
    ensure_workspace_is_not_a_bundle_member(&repo, &id, "delete").await?;
    let workspace = repo
        .get_workspace(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", id.0))?;
    cleanup_delegation_worktrees(&session_repo, &workspace).await?;
    cleanup_workspace_files(&workspace)?;
    cleanup_unused_workspace_push_target(&repo, &workspace).await?;
    repo.delete_workspace(&id)
        .await
        .map_err(|e| e.to_string())?;
    state.clear_delivery_failures(&workspace.root_path);
    if let Some(worktree_path) = workspace.worktree_path.as_deref() {
        state.clear_delivery_failures(worktree_path);
    }
    Ok(())
}

async fn cleanup_delegation_worktrees(
    session_repo: &SqliteSessionRepo,
    workspace: &Workspace,
) -> Result<(), String> {
    let delegations = DelegationRepo::list_delegations(session_repo, Some(&workspace.id), None)
        .await
        .map_err(|e| e.to_string())?;
    let workspace_root = Path::new(workspace.root_path.trim());
    let workspace_worktree = workspace
        .worktree_path
        .as_deref()
        .map(str::trim)
        .unwrap_or("");

    for delegation in delegations {
        let Some(child_session_id) = delegation.child_session_id.as_ref() else {
            continue;
        };
        let Some(child_session) = SessionRepo::get_session(session_repo, child_session_id)
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Some(worktree_root) = child_session
            .working_directory_override
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if worktree_root == workspace.root_path.trim() || worktree_root == workspace_worktree {
            continue;
        }

        let worktree_path = Path::new(worktree_root);
        if !worktree_path.exists() {
            continue;
        }

        if !workspace.root_path.trim().is_empty()
            && is_git_repo(workspace_root)
            && worktree_path.join(".git").exists()
        {
            match remove_worktree(workspace_root, worktree_path) {
                Ok(()) => continue,
                Err(error) if broken_worktree_reason(worktree_path).is_none() => {
                    return Err(error.to_string());
                }
                Err(_) => {
                    // Fall through to direct removal for already-broken worktree metadata.
                }
            }
        }

        fs::remove_dir_all(worktree_path).map_err(|error| {
            format!(
                "failed to remove delegation worktree {}: {}",
                worktree_path.display(),
                error
            )
        })?;
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_repository(
    state: State<'_, WorkspaceCommandState>,
    input: RepositoryIdInput,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|e| e.to_string())?;
    let id = RepositoryId(input.repository_id);
    repo.delete_repository(&id).await.map_err(|e| e.to_string())
}
