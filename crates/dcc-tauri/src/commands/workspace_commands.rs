use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    git::{
        broken_worktree_reason, detect_workspace_setup_suggestions, is_git_repo,
        list_local_branch_names, CommandGitOps,
    },
};

use crate::{
    events::TauriEventBus,
    git::{
        git_command_succeeds, git_output_err, parse_name_status_z, parse_numstat_z,
        run_git_network_output, run_git_output, run_git_output_owned, split_null_terminated_fields,
    },
    state::WorkspaceCommandState,
    workspace_setup::run_detected_workspace_setup,
};

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
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitPushInput {
    pub workspace_root: String,
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
pub struct GithubCliStatusInput {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum GithubCliStatusState {
    Ready,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GithubCliStatusOutput {
    pub cli_name: String,
    pub hostname: String,
    pub status: GithubCliStatusState,
    pub login: Option<String>,
    pub message: String,
    pub login_command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrStatusInput {
    pub workspace_root: String,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrStatusOutput {
    pub provider: Option<String>,
    pub number: Option<u32>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub head_branch: Option<String>,
    pub base_branch: Option<String>,
    pub state: Option<String>,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
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

fn resolve_workspace_setup_root(workspace: &Workspace) -> &str {
    workspace
        .worktree_path
        .as_deref()
        .unwrap_or(workspace.root_path.as_str())
}

fn resolve_workspace_active_root(workspace: &Workspace) -> &str {
    resolve_workspace_setup_root(workspace)
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

fn resolve_workspace_broken_reason(workspace: &Workspace) -> Option<String> {
    let active_root = Path::new(resolve_workspace_active_root(workspace));
    if !active_root.exists() {
        return Some(format!(
            "workspace path no longer exists: {}",
            active_root.display()
        ));
    }

    let repo_root = Path::new(workspace.root_path.as_str());
    let looks_git_backed = is_git_repo(repo_root) || active_root.join(".git").exists();
    if !looks_git_backed {
        return None;
    }

    broken_worktree_reason(active_root)
}

async fn find_workspace_by_root(
    repo: &SqliteWorkspaceRepo,
    workspace_root: &str,
) -> Result<Option<Workspace>, String> {
    let workspace_root = workspace_root.trim();
    let workspaces = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?;
    Ok(workspaces.into_iter().find(|workspace| {
        workspace.root_path == workspace_root
            || workspace.worktree_path.as_deref() == Some(workspace_root)
    }))
}

async fn purge_broken_workspace_by_root(
    repo: &SqliteWorkspaceRepo,
    workspace_root: &str,
) -> Result<Option<String>, String> {
    let Some(workspace) = find_workspace_by_root(repo, workspace_root).await? else {
        return Ok(None);
    };

    let Some(reason) = resolve_workspace_broken_reason(&workspace) else {
        return Ok(None);
    };

    repo.delete_workspace(&workspace.id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(reason))
}

fn broken_workspace_message(reason: &str) -> String {
    format!("workspace became unavailable and was removed from DCC: {reason}")
}

async fn preflight_workspace_root(
    state: &State<'_, WorkspaceCommandState>,
    workspace_root: &str,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    if let Some(reason) = purge_broken_workspace_by_root(&repo, workspace_root).await? {
        return Err(broken_workspace_message(&reason));
    }
    Ok(())
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

fn resolve_current_branch_name(root: &str) -> Result<String, String> {
    let output = run_git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !output.status.success() {
        return Err(git_output_err(
            "git rev-parse --abbrev-ref HEAD",
            &output.stderr,
        ));
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        return Err("current branch is empty".to_string());
    }
    if branch == "HEAD" {
        let remotes_output = run_git_output(root, &["remote"])?;
        let remotes: Vec<String> = if remotes_output.status.success() {
            String::from_utf8_lossy(&remotes_output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| line.to_string())
                .collect()
        } else {
            vec!["origin".to_string()]
        };

        let fallback = run_git_output(
            root,
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                "--contains",
                "HEAD",
                "refs/heads/",
                "refs/remotes/",
            ],
        )?;
        if fallback.status.success() {
            let mut candidates: Vec<String> = String::from_utf8_lossy(&fallback.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter(|line| !line.ends_with("/HEAD"))
                .map(|line| {
                    let branch = line.to_string();
                    for remote in &remotes {
                        let prefix = format!("{remote}/");
                        if branch.starts_with(&prefix) {
                            return branch[prefix.len()..].to_string();
                        }
                    }
                    branch
                })
                .filter(|line| {
                    let lower = line.to_lowercase();
                    lower != "main" && lower != "master" && lower != "develop"
                })
                .collect();
            candidates.sort();
            candidates.dedup();
            if let Some(candidate) = candidates.first() {
                return Ok(candidate.clone());
            }
        }
    }
    Ok(branch)
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

fn resolve_current_commit_sha(root: &str) -> Result<Option<String>, String> {
    let output = run_git_output(root, &["rev-parse", "HEAD"])?;
    if !output.status.success() {
        return Ok(None);
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Ok(None);
    }

    Ok(Some(sha))
}

fn workspace_branch_hints(root: &str, branch: Option<&str>) -> Vec<String> {
    let mut hints = Vec::new();

    if let Some(branch) = branch.map(str::trim).filter(|value| !value.is_empty()) {
        hints.push(branch.to_string());
    }

    if let Some(name) = Path::new(root)
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        hints.push(name.to_string());
        if !name.starts_with("dcc/") {
            hints.push(format!("dcc/{name}"));
        }
    }

    hints.sort();
    hints.dedup();
    hints
}

fn resolve_workspace_pr_status_json(
    root: &str,
    branch_hints: &[String],
    head_sha: Option<&str>,
) -> Result<Option<Value>, String> {
    let gh = match resolve_gh_binary() {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };

    let json_fields = "number,title,url,state,mergeable,mergeStateStatus,isDraft,headRefName,headRefOid,baseRefName";

    // Primary: `gh pr view` with no argument resolves the PR for the current branch.
    // This works even in worktrees and is more targeted than listing all PRs.
    let view_output = Command::new(&gh)
        .current_dir(root)
        .args(["pr", "view", "--json", json_fields])
        .output();
    if let Ok(output) = view_output {
        if output.status.success() {
            if let Ok(pr) = serde_json::from_slice::<Value>(&output.stdout) {
                if pr.is_object() && !pr.as_object().map_or(true, |o| o.is_empty()) {
                    return Ok(Some(pr));
                }
            }
        }
    }

    // Secondary: `gh pr view <hint>` for each branch hint (covers detached HEAD + renamed branches).
    for hint in branch_hints {
        let output = Command::new(&gh)
            .current_dir(root)
            .args(["pr", "view", hint, "--json", json_fields])
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(pr) = serde_json::from_slice::<Value>(&output.stdout) {
                    if pr.is_object() && !pr.as_object().map_or(true, |o| o.is_empty()) {
                        return Ok(Some(pr));
                    }
                }
            }
        }
    }

    // Fallback: `gh pr list --state all` filtered by branch name or commit SHA.
    let output = Command::new(&gh)
        .current_dir(root)
        .args(["pr", "list", "--state", "all", "--json", json_fields])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Ok(None);
    }

    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let Some(items) = parsed.as_array() else {
        return Ok(None);
    };
    let pr = items.iter().find(|item| {
        let matches_branch = item
            .get("headRefName")
            .and_then(|value| value.as_str())
            .map(|value| branch_hints.iter().any(|hint| hint == value))
            .unwrap_or(false);
        let matches_sha = head_sha
            .and_then(|sha| {
                item.get("headRefOid")
                    .and_then(|value| value.as_str())
                    .map(|value| value == sha)
            })
            .unwrap_or(false);
        matches_branch || matches_sha
    });
    let Some(pr) = pr else {
        return Ok(None);
    };
    Ok(Some(pr.clone()))
}

fn resolve_default_remote_name(root: &str) -> Result<String, String> {
    let output = run_git_output(root, &["remote"])?;
    if !output.status.success() {
        return Err(git_output_err("git remote", &output.stderr));
    }

    let remotes: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();
    if remotes.is_empty() {
        return Ok("origin".to_string());
    }
    if remotes.iter().any(|remote| remote == "origin") {
        return Ok("origin".to_string());
    }

    Ok(remotes[0].clone())
}

fn resolve_gh_binary() -> Result<PathBuf, String> {
    let candidates = [
        PathBuf::from("gh"),
        PathBuf::from("/opt/homebrew/bin/gh"),
        PathBuf::from("/usr/local/bin/gh"),
    ];

    for candidate in candidates {
        if candidate.as_os_str() == "gh" {
            if Command::new(&candidate).arg("--version").output().is_ok() {
                return Ok(candidate);
            }
            continue;
        }

        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(
		"GitHub CLI (`gh`) is not available to the app. Install it or launch DCC from a shell where `gh` is on PATH."
			.to_string(),
	)
}

fn parse_gh_active_login(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((_, login)) = trimmed.split_once(" as ") {
            let login = login
                .trim()
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(|ch: char| ch == '(' || ch == ')' || ch == ',' || ch == '.');
            if !login.is_empty() {
                return Some(login.to_string());
            }
        }

        if let Some((_, login)) = trimmed.split_once(" account ") {
            let login = login
                .trim()
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(|ch: char| ch == '(' || ch == ')' || ch == ',' || ch == '.');
            if !login.is_empty() {
                return Some(login.to_string());
            }
        }
    }

    None
}

fn github_cli_status_error(hostname: &str, message: String) -> GithubCliStatusOutput {
    GithubCliStatusOutput {
        cli_name: "gh".to_string(),
        hostname: hostname.to_string(),
        status: GithubCliStatusState::Error,
        login: None,
        message,
        login_command: "gh auth login".to_string(),
    }
}

#[tauri::command]
pub async fn workspace_github_cli_status(
    _input: GithubCliStatusInput,
) -> Result<GithubCliStatusOutput, String> {
    let hostname = "github.com";
    let gh = match resolve_gh_binary() {
        Ok(path) => path,
        Err(message) => return Ok(github_cli_status_error(hostname, message)),
    };

    let output = Command::new(gh)
        .args(["auth", "status", "--active", "--hostname", hostname])
        .output()
        .map_err(|error| error.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    if output.status.success() {
        let login = parse_gh_active_login(&combined);
        let message = login
            .as_ref()
            .map(|value| format!("Logged in as {value}"))
            .unwrap_or_else(|| "GitHub CLI is connected.".to_string());
        return Ok(GithubCliStatusOutput {
            cli_name: "gh".to_string(),
            hostname: hostname.to_string(),
            status: GithubCliStatusState::Ready,
            login,
            message,
            login_command: "gh auth login".to_string(),
        });
    }

    let lower = combined.to_lowercase();
    let message = if lower.contains("not logged in")
        || lower.contains("no active account")
        || lower.contains("gh auth login")
        || lower.contains("authenticate")
    {
        "Run `gh auth login` to connect GitHub locally.".to_string()
    } else {
        let trimmed = combined.trim();
        if trimmed.is_empty() {
            "GitHub CLI authentication failed.".to_string()
        } else {
            trimmed.to_string()
        }
    };

    Ok(github_cli_status_error(hostname, message))
}

#[cfg(test)]
mod github_cli_status_tests {
    use super::parse_gh_active_login;

    #[test]
    fn parses_logged_in_line_with_as() {
        let output = "github.com\n  ✓ Logged in to github.com as demo-user (keyring)";
        assert_eq!(parse_gh_active_login(output), Some("demo-user".to_string()));
    }

    #[test]
    fn parses_logged_in_line_with_account() {
        let output = "github.com\n  ✓ Logged in to github.com account demo-user";
        assert_eq!(parse_gh_active_login(output), Some("demo-user".to_string()));
    }
}

fn push_current_branch(root: &str) -> Result<(), String> {
    let branch = ensure_pushable_branch(root)?;
    push_branch_refspec(root, &branch)
}

fn push_branch_refspec(root: &str, branch: &str) -> Result<(), String> {
    let branch = branch.trim();
    if branch.is_empty() || branch == "HEAD" {
        return Err(
            "cannot push from detached HEAD because no branch name could be resolved".to_string(),
        );
    }

    let remote = resolve_default_remote_name(root)?;
    let remote_ref = format!("HEAD:refs/heads/{branch}");
    let output = run_git_network_output(root, &["push", "-u", &remote, &remote_ref])?;
    if output.status.success() {
        return Ok(());
    }

    Err(git_output_err("git push -u", &output.stderr))
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

    push_current_branch(root)
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

    push_current_branch(root)
}

/// Opens the current PR in the browser (`gh pr view --web`), if GitHub CLI is available.
#[tauri::command]
pub async fn workspace_gh_pr_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let gh = resolve_gh_binary()?;

    let output = Command::new(gh)
        .current_dir(root)
        .args(["pr", "view", "--web"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "gh pr view failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Merge do PR atual via GitHub CLI (`gh pr merge --merge`).
#[tauri::command]
pub async fn workspace_gh_pr_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let gh = resolve_gh_binary()?;

    let output = Command::new(gh)
        .current_dir(root)
        .args(["pr", "merge", "--merge"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "gh pr merge failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn push_named_branch(root: &str, branch: &str) -> Result<(), String> {
    push_branch_refspec(root, branch)
}

fn branch_name_from_worktree_path(root: &str) -> String {
    let dir = std::path::Path::new(root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("dcc-branch");
    format!("dcc/{dir}")
}

fn ensure_pushable_branch(root: &str) -> Result<String, String> {
    let raw_branch = resolve_current_branch_name(root)?;
    if raw_branch != "HEAD" {
        return Ok(raw_branch);
    }

    let branch = branch_name_from_worktree_path(root);
    let branch_ref = format!("refs/heads/{branch}");
    let already_exists = git_command_succeeds(root, &["rev-parse", "--verify", &branch_ref]);
    let checkout_args: &[&str] = if already_exists {
        &["checkout", &branch]
    } else {
        &["checkout", "-b", &branch]
    };
    let checkout = run_git_output(root, checkout_args)?;
    if !checkout.status.success() {
        return Err(git_output_err("git checkout", &checkout.stderr));
    }

    Ok(branch)
}

/// Non-interactive PR creation (`gh pr create --fill`).
#[tauri::command]
pub async fn workspace_gh_pr_create_fill(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;
    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }

    let raw_branch = ensure_pushable_branch(root)?;
    let base_ref = resolve_branch_diff_base(root).unwrap_or_else(|| "main".to_string());
    // gh pr create --base expects a branch name, not a remote tracking ref like origin/main
    let base_stripped = base_ref
        .split_once('/')
        .map(|(_, b)| b)
        .unwrap_or(&base_ref);
    let base_branch = if base_stripped == "HEAD" {
        "main"
    } else {
        base_stripped
    };
    let gh = resolve_gh_binary()?;

    let head_branch = raw_branch;

    // Branch must exist on the remote before gh can create a PR
    push_named_branch(root, &head_branch).map_err(|e| format!("git push failed: {e}"))?;

    let output = Command::new(gh)
        .current_dir(root)
        .args([
            "pr",
            "create",
            "--fill",
            "--base",
            base_branch,
            "--head",
            &head_branch,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "gh pr create failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
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
pub async fn workspace_pr_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrStatusInput,
) -> Result<WorkspacePrStatusOutput, String> {
    preflight_workspace_root(&state, &input.workspace_root).await?;

    let root = input.workspace_root.trim();
    if root.is_empty() {
        return Ok(WorkspacePrStatusOutput {
            provider: None,
            number: None,
            title: None,
            url: None,
            head_branch: None,
            base_branch: None,
            state: None,
            mergeable: None,
            merge_state_status: None,
        });
    }

    let head_sha = resolve_current_commit_sha(root).ok().flatten();
    let branch = match input
        .branch
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(branch) => branch,
        None => match resolve_current_branch_name(root) {
            Ok(branch) => branch,
            Err(_) => {
                return Ok(WorkspacePrStatusOutput {
                    provider: None,
                    number: None,
                    title: None,
                    url: None,
                    head_branch: None,
                    base_branch: None,
                    state: None,
                    mergeable: None,
                    merge_state_status: None,
                });
            }
        },
    };
    let branch_hints = workspace_branch_hints(root, Some(&branch));

    let Some(raw_pr) = resolve_workspace_pr_status_json(root, &branch_hints, head_sha.as_deref())?
    else {
        return Ok(WorkspacePrStatusOutput {
            provider: None,
            number: None,
            title: None,
            url: None,
            head_branch: Some(branch),
            base_branch: None,
            state: None,
            mergeable: None,
            merge_state_status: None,
        });
    };

    let state = raw_pr
        .get("state")
        .and_then(|value| value.as_str())
        .map(|value| value.to_lowercase());
    let state = match state.as_deref() {
        Some("open") | Some("opened") => Some("open".to_string()),
        Some("closed") => Some("closed".to_string()),
        Some("merged") => Some("merged".to_string()),
        _ => None,
    };

    let number = raw_pr
        .get("number")
        .and_then(|value| value.as_u64())
        .map(|value| value as u32);
    let title = raw_pr
        .get("title")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let url = raw_pr
        .get("url")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let head_branch = raw_pr
        .get("headRefName")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let base_branch = raw_pr
        .get("baseRefName")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let mergeable = raw_pr
        .get("mergeable")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let merge_state_status = raw_pr
        .get("mergeStateStatus")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    Ok(WorkspacePrStatusOutput {
        provider: Some("github".to_string()),
        number,
        title,
        url,
        head_branch,
        base_branch,
        state,
        mergeable,
        merge_state_status,
    })
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

    workspace.base_branch = new_branch.clone();
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

fn next_available_branch_name(root: &str, base: &str) -> String {
    let branch_exists = |name: &str| -> bool {
        let branch_ref = format!("refs/heads/{name}");
        git_command_succeeds(root, &["rev-parse", "--verify", &branch_ref])
    };
    if !branch_exists(base) {
        return base.to_string();
    }
    for i in 2u32..=50 {
        let candidate = format!("{base}-{i}");
        if !branch_exists(&candidate) {
            return candidate;
        }
    }
    format!("{base}-next")
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

    // Resolve base ref: try upstream, then common origin branches.
    let base = resolve_branch_diff_base(root);

    let changes = match base {
        Some(ref b) => compute_branch_diff(root, b)?,
        None => vec![],
    };

    Ok(WorkspaceGitBranchDiffOutput {
        changes,
        base_branch: base,
    })
}

fn resolve_branch_diff_base(root: &str) -> Option<String> {
    // 1. Try @{upstream}
    let upstream = run_git_output(root, &["rev-parse", "--abbrev-ref", "@{upstream}"])
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        });
    if upstream.is_some() {
        return upstream;
    }

    // 2. Try common origin branches
    for candidate in &[
        "origin/HEAD",
        "origin/main",
        "origin/master",
        "origin/develop",
    ] {
        let ok = git_command_succeeds(root, &["rev-parse", "--verify", candidate]);
        if ok {
            return Some(candidate.to_string());
        }
    }
    None
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
