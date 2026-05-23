use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::{AppHandle, State};

use dcc_core::{
    application::{
        create_workspace_for_repo as run_create_workspace_for_repo,
        create_workspace_from_url as run_create_workspace_from_url, CreateWorkspaceForRepoInput,
        CreateWorkspaceFromUrlInput,
    },
    domain::{
        repository::{Repository, RepositoryId},
        workspace::{
            Workspace, WorkspaceId, WorkspaceSetupReport, WorkspaceSetupStatus, WorkspaceState,
        },
    },
    ports::{RepositoryRepo, WorkspaceRepo},
};
use dcc_infra::{
    db::SqliteWorkspaceRepo,
    git::{detect_workspace_setup_suggestions, list_local_branch_names, CommandGitOps},
};

use crate::{
    commands::forge::remote::resolve_workspace_remote_info,
    commands::workspace_support::{
        broken_workspace_message, cleanup_workspace_files, ensure_pushable_branch,
        find_workspace_by_root, next_available_branch_name, preflight_workspace_root,
        purge_broken_workspace_by_root, push_branch_refspec, resolve_branch_diff_base,
        resolve_current_branch_name, resolve_workspace_active_root,
        resolve_workspace_broken_reason, resolve_workspace_setup_root,
        resolve_workspace_target_branch,
    },
    events::TauriEventBus,
    git::{
        git_command_succeeds, git_output_err, parse_name_status_z, parse_numstat_z,
        run_git_network_output, run_git_output, run_git_output_owned, split_null_terminated_fields,
    },
    state::WorkspaceCommandState,
    workspace_setup::run_detected_workspace_setup,
};

const DCC_SPEC_CONTEXT_START: &str = "<!-- dcc:spec:start -->";
const DCC_SPEC_CONTEXT_END: &str = "<!-- dcc:spec:end -->";
const DCC_SPEC_CONTEXT_TARGETS: [&str; 2] = ["AGENTS.md", "GEMINI.md"];

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

fn validate_git_relative_path(path: &str) -> Result<String, String> {
    let p = normalize_git_relative_path(path);
    if p.is_empty() {
        return Err("path is empty".to_string());
    }
    if p.contains("..") {
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

fn push_current_branch(
    db_path: &Path,
    root: &str,
    protected_branch: Option<&str>,
    forge_login: Option<&str>,
) -> Result<(), String> {
    let branch = ensure_pushable_branch(root, protected_branch)?;
    push_branch_refspec(db_path, root, &branch, forge_login)
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

fn read_worktree_file_text(root: &str, rel: &str) -> Result<Option<String>, String> {
    let path = PathBuf::from(root).join(rel);
    if !path.is_file() {
        return Ok(None);
    }

    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
}

/// `git add -- path` (Helmor `stage_workspace_file`).
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

/// `git restore --staged` with `git reset HEAD --` fallback (Helmor `unstage_workspace_file`).
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

/// Tracked: `git checkout HEAD -- path`; untracked file: remove (Helmor `discard_workspace_file`).
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
        return Err(git_output_err("git commit", &commit.stderr));
    }

    let protected_branch = resolve_workspace_target_branch(&state, root).await;
    push_current_branch(
        &state.db_path,
        root,
        protected_branch.as_deref(),
        input.forge_login.as_deref(),
    )
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
        &state.db_path,
        root,
        protected_branch.as_deref(),
        input.forge_login.as_deref(),
    )
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// `git status --porcelain` → staged / unstaged rows (Helmor-style split).
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

    Ok(WorkspaceGitStatusOutput {
        staged,
        unstaged,
        current_branch,
        ahead_of_remote_count,
        behind_of_remote_count,
        conflict_count,
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
    let git = CommandGitOps::new();
    let events = TauriEventBus::new(app);

    let finalized = run_create_workspace_for_repo(&repo, &git, &events, input)
        .await
        .map_err(|error| error.to_string())?;
    refresh_repository_forge_metadata(&repo, &finalized.workspace).await?;
    let setup_hints = collect_workspace_setup_hints(&finalized.workspace);
    let setup_report = execute_workspace_setup_report(&finalized.workspace).await;
    let mut workspace = finalized.workspace;
    persist_workspace_setup_outcome(&repo, &mut workspace, &setup_report).await?;

    Ok(CreateWorkspaceForRepoOutput {
        workspace,
        setup_hints,
        setup_report,
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
    Ok(Some(MissionValidationEntry {
        relative_path: format!(".devcommandcenter/specs/{file_name}"),
        content,
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
    let target = specs_canonical.join(&validation_name);
    if fs::symlink_metadata(&target)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("validation target must not be a symlink".to_string());
    }

    let pretty = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    fs::write(&target, format!("{pretty}\n")).map_err(|error| error.to_string())?;

    Ok(SaveMissionValidationOutput {
        relative_path: format!(".devcommandcenter/specs/{validation_name}"),
    })
}

#[tauri::command]
pub async fn compile_mission_spec_context(
    state: State<'_, WorkspaceCommandState>,
    input: CompileMissionSpecContextInput,
) -> Result<CompileMissionSpecContextOutput, String> {
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
    let generated_section =
        render_mission_spec_context_section(&spec_relative_path, &spec_markdown);

    let mut files = Vec::new();
    for target_name in DCC_SPEC_CONTEXT_TARGETS {
        let target = root_canonical.join(target_name);
        if fs::symlink_metadata(&target)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(format!("{target_name} must not be a symlink"));
        }

        let created = !target.exists();
        let current = if created {
            String::new()
        } else {
            fs::read_to_string(&target).map_err(|error| error.to_string())?
        };
        let next = upsert_generated_context_section(&current, &generated_section)?;
        let updated = next != current;
        if updated {
            fs::write(&target, next).map_err(|error| error.to_string())?;
        }
        files.push(CompiledMissionSpecContextFile {
            relative_path: target_name.to_string(),
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
    let generated_section =
        render_mission_spec_context_section(&spec_relative_path, &spec_markdown);

    let mut files = Vec::new();
    for target_name in DCC_SPEC_CONTEXT_TARGETS {
        let target = root_canonical.join(target_name);
        let symlink = fs::symlink_metadata(&target)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false);
        let (state, message) = if symlink {
            (
                MissionSpecContextFileState::Symlink,
                Some("context file target must not be a symlink".to_string()),
            )
        } else if !target.exists() {
            (
                MissionSpecContextFileState::Missing,
                Some("context file has not been generated yet".to_string()),
            )
        } else {
            let current = fs::read_to_string(&target).map_err(|error| error.to_string())?;
            classify_generated_context_section(&current, &generated_section)
        };
        files.push(MissionSpecContextFileStatus {
            relative_path: target_name.to_string(),
            state,
            message,
        });
    }

    let current = files
        .iter()
        .all(|file| matches!(file.state, MissionSpecContextFileState::Current));

    Ok(MissionSpecContextStatusOutput { current, files })
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

#[tauri::command]
pub async fn archive_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceIdInput,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|e| e.to_string())?;
    let id = WorkspaceId(input.workspace_id);
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
    let id = WorkspaceId(input.workspace_id);
    let workspace = repo
        .get_workspace(&id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", id.0))?;
    cleanup_workspace_files(&workspace)?;
    repo.delete_workspace(&id).await.map_err(|e| e.to_string())
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
