use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::{AppHandle, State};
use serde::{Deserialize, Serialize};
use specta::Type;

use dcc_core::{
	application::{
		create_workspace_for_repo as run_create_workspace_for_repo,
		create_workspace_from_url as run_create_workspace_from_url,
		CreateWorkspaceForRepoInput,
		CreateWorkspaceFromUrlInput,
	},
	domain::workspace::Workspace,
	ports::WorkspaceRepo,
};
use dcc_infra::{db::SqliteWorkspaceRepo, git::{list_local_branch_names, CommandGitOps}};

use crate::{
	events::TauriEventBus,
	state::WorkspaceCommandState,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceForRepoOutput {
	pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceFromUrlOutput {
	pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspacesOutput {
	pub workspaces: Vec<Workspace>,
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

fn normalize_git_relative_path(path: &str) -> String {
	path.trim().replace('\\', "/")
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

fn git_output_err(cmd: &str, stderr: &[u8]) -> String {
	let msg = String::from_utf8_lossy(stderr);
	format!("{cmd} failed: {}", msg.trim())
}

fn path_is_tracked(root: &str, rel: &str) -> bool {
	Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["ls-files", "--error-unmatch", "--", rel])
		.output()
		.map(|o| o.status.success())
		.unwrap_or(false)
}

/// `git add -- path` (Helmor `stage_workspace_file`).
#[tauri::command]
pub async fn workspace_git_stage_file(input: WorkspaceGitPathInput) -> Result<(), String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Err("workspace_root is empty".to_string());
	}
	let path = validate_git_relative_path(&input.relative_path)?;
	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["add", "--", &path])
		.output()
		.map_err(|e| e.to_string())?;
	if output.status.success() {
		return Ok(());
	}
	Err(git_output_err(
		"git add",
		&output.stderr,
	))
}

/// `git restore --staged` with `git reset HEAD --` fallback (Helmor `unstage_workspace_file`).
#[tauri::command]
pub async fn workspace_git_unstage_file(input: WorkspaceGitPathInput) -> Result<(), String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Err("workspace_root is empty".to_string());
	}
	let path = validate_git_relative_path(&input.relative_path)?;
	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["restore", "--staged", "--", &path])
		.output()
		.map_err(|e| e.to_string())?;
	if output.status.success() {
		return Ok(());
	}
	let fallback = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["reset", "HEAD", "--", &path])
		.output()
		.map_err(|e| e.to_string())?;
	if fallback.status.success() {
		return Ok(());
	}
	Err(git_output_err(
		"git reset",
		&fallback.stderr,
	))
}

/// Tracked: `git checkout HEAD -- path`; untracked file: remove (Helmor `discard_workspace_file`).
#[tauri::command]
pub async fn workspace_git_discard_file(input: WorkspaceGitPathInput) -> Result<(), String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Err("workspace_root is empty".to_string());
	}
	let path = validate_git_relative_path(&input.relative_path)?;
	let absolute = PathBuf::from(root).join(&path);

	if path_is_tracked(root, &path) {
		let output = Command::new("git")
			.arg("-C")
			.arg(root)
			.args(["checkout", "HEAD", "--", &path])
			.output()
			.map_err(|e| e.to_string())?;
		if output.status.success() {
			return Ok(());
		}
		return Err(git_output_err(
			"git checkout",
			&output.stderr,
		));
	}

	if absolute.is_file() {
		fs::remove_file(&absolute).map_err(|e| e.to_string())?;
		return Ok(());
	}

	Err("cannot discard: path is not a tracked file or a single untracked file".to_string())
}

fn parse_git_numstat_maps(root: &str, cached: bool) -> Result<HashMap<String, (u32, u32)>, String> {
	let mut cmd = Command::new("git");
	cmd.arg("-C").arg(root).arg("diff");
	if cached {
		cmd.arg("--cached");
	}
	let output = cmd
		.args(["--numstat"])
		.output()
		.map_err(|e| e.to_string())?;
	if !output.status.success() {
		return Ok(HashMap::new());
	}
	let stdout = String::from_utf8_lossy(&output.stdout);
	Ok(parse_numstat_tab_lines(&stdout))
}

fn parse_numstat_tab_lines(stdout: &str) -> HashMap<String, (u32, u32)> {
	let mut m = HashMap::new();
	for line in stdout.lines() {
		let line = line.trim_end();
		if line.is_empty() {
			continue;
		}
		let mut parts = line.splitn(3, '\t');
		let ins_s = parts.next().unwrap_or("");
		let del_s = parts.next().unwrap_or("");
		let path = parts.next().unwrap_or("").trim();
		if path.is_empty() {
			continue;
		}
		let insertions = if ins_s == "-" {
			0
		} else {
			ins_s.parse().unwrap_or(0)
		};
		let deletions = if del_s == "-" {
			0
		} else {
			del_s.parse().unwrap_or(0)
		};
		m.insert(path.to_string(), (insertions, deletions));
	}
	m
}

fn join_workspace_path(root: &str, rel: &str) -> String {
	PathBuf::from(root).join(rel).to_string_lossy().to_string()
}

/// `git diff --cached --quiet` → `true` if there is at least one staged change.
fn git_has_staged_changes(root: &str) -> Result<bool, String> {
	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["diff", "--cached", "--quiet"])
		.output()
		.map_err(|e| e.to_string())?;
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
pub async fn workspace_git_commit_push(input: WorkspaceGitCommitPushInput) -> Result<(), String> {
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

	let commit = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["commit", "-m", message])
		.output()
		.map_err(|e| e.to_string())?;
	if !commit.status.success() {
		return Err(git_output_err("git commit", &commit.stderr));
	}

	let push = Command::new("git")
		.arg("-C")
		.arg(root)
		.arg("push")
		.output()
		.map_err(|e| e.to_string())?;
	if !push.status.success() {
		return Err(git_output_err("git push", &push.stderr));
	}

	Ok(())
}

#[tauri::command]
pub async fn workspace_git_push(input: WorkspaceGitPushInput) -> Result<(), String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Err("workspace_root is empty".to_string());
	}

	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.arg("push")
		.output()
		.map_err(|e| e.to_string())?;
	if output.status.success() {
		return Ok(());
	}
	Err(git_output_err("git push", &output.stderr))
}

/// Opens the current PR in the browser (`gh pr view --web`), if GitHub CLI is available.
#[tauri::command]
pub async fn workspace_gh_pr_view_web(input: WorkspaceGitPushInput) -> Result<(), String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Err("workspace_root is empty".to_string());
	}

	let output = Command::new("gh")
		.arg("-C")
		.arg(root)
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

/// Non-interactive PR creation (`gh pr create --fill`).
#[tauri::command]
pub async fn workspace_gh_pr_create_fill(input: WorkspaceGitPushInput) -> Result<(), String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Err("workspace_root is empty".to_string());
	}

	let output = Command::new("gh")
		.arg("-C")
		.arg(root)
		.args(["pr", "create", "--fill"])
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
		});
	}

	let cached_stats = parse_git_numstat_maps(root, true)?;
	let unstaged_stats = parse_git_numstat_maps(root, false)?;

	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(["status", "--porcelain"])
		.output()
		.map_err(|e| e.to_string())?;
	if !output.status.success() {
		return Err(format!(
			"git status failed: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	let raw = String::from_utf8_lossy(&output.stdout);
	let mut staged: Vec<WorkspaceGitChangeEntry> = Vec::new();
	let mut unstaged: Vec<WorkspaceGitChangeEntry> = Vec::new();

	for line in raw.lines() {
		let line = line.trim_end();
		if line.is_empty() {
			continue;
		}

		if line.starts_with("?? ") {
			let path = line[3..].trim();
			if path.is_empty() {
				continue;
			}
			let (ins, del) = unstaged_stats.get(path).copied().unwrap_or((0, 0));
			unstaged.push(WorkspaceGitChangeEntry {
				path: path.to_string(),
				name: file_name_from_path(path),
				absolute_path: join_workspace_path(root, path),
				status: "?".to_string(),
				insertions: ins,
				deletions: del,
			});
			continue;
		}

		if line.len() < 4 {
			continue;
		}
		let idx = line.chars().next().unwrap_or(' ');
		let wt = line.chars().nth(1).unwrap_or(' ');
		let path = line[3..].trim();
		if path.is_empty() {
			continue;
		}

		let idx_active = idx != ' ' && idx != '?';
		let wt_active = wt != ' ' && wt != '?';

		if idx_active {
			let st = idx.to_string();
			let (ins, del) = cached_stats.get(path).copied().unwrap_or((0, 0));
			staged.push(WorkspaceGitChangeEntry {
				path: path.to_string(),
				name: file_name_from_path(path),
				absolute_path: join_workspace_path(root, path),
				status: st,
				insertions: ins,
				deletions: del,
			});
		}
		if wt_active {
			let st = wt.to_string();
			let (ins, del) = unstaged_stats.get(path).copied().unwrap_or((0, 0));
			unstaged.push(WorkspaceGitChangeEntry {
				path: path.to_string(),
				name: file_name_from_path(path),
				absolute_path: join_workspace_path(root, path),
				status: st,
				insertions: ins,
				deletions: del,
			});
		}
	}

	staged.sort_by(|a, b| a.path.cmp(&b.path));
	unstaged.sort_by(|a, b| a.path.cmp(&b.path));

	Ok(WorkspaceGitStatusOutput { staged, unstaged })
}

#[tauri::command]
pub async fn workspace_git_status(
	input: WorkspaceGitStatusInput,
) -> Result<WorkspaceGitStatusOutput, String> {
	workspace_git_status_inner(&input.workspace_root)
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

	Ok(CreateWorkspaceForRepoOutput {
		workspace: finalized.workspace,
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

	Ok(CreateWorkspaceFromUrlOutput {
		workspace: finalized.workspace,
	})
}

#[tauri::command]
pub async fn list_workspaces(state: State<'_, WorkspaceCommandState>) -> Result<ListWorkspacesOutput, String> {
	let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
	let workspaces = repo.list_workspaces().await.map_err(|error| error.to_string())?;

	Ok(ListWorkspacesOutput { workspaces })
}

#[tauri::command]
pub async fn list_local_branches(
	input: ListLocalBranchesInput,
) -> Result<ListLocalBranchesOutput, String> {
	let branches = list_local_branch_names(&input.workspace_root).map_err(|error| error.to_string())?;
	Ok(ListLocalBranchesOutput { branches })
}

/// Paths tracked by git (`git ls-files`), repo-relative forward slashes.
/// Empty vec if not a git worktree or git fails.
#[tauri::command]
pub async fn list_git_tracked_files(
	input: ListGitTrackedFilesInput,
) -> Result<ListGitTrackedFilesOutput, String> {
	let root = input.workspace_root.trim();
	if root.is_empty() {
		return Ok(ListGitTrackedFilesOutput { paths: Vec::new() });
	}

	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.arg("ls-files")
		.output()
		.map_err(|error| error.to_string())?;

	if !output.status.success() {
		return Ok(ListGitTrackedFilesOutput { paths: Vec::new() });
	}

	let stdout = String::from_utf8_lossy(&output.stdout);
	let mut paths: Vec<String> = stdout
		.lines()
		.map(|line| line.trim().to_string())
		.filter(|line| !line.is_empty())
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
    input: WorkspaceGitBranchDiffInput,
) -> Result<WorkspaceGitBranchDiffOutput, String> {
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
    let upstream = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        });
    if upstream.is_some() {
        return upstream;
    }

    // 2. Try common origin branches
    for candidate in &["origin/HEAD", "origin/main", "origin/master", "origin/develop"] {
        let ok = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--verify", candidate])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Some(candidate.to_string());
        }
    }
    None
}

fn compute_branch_diff(
    root: &str,
    base: &str,
) -> Result<Vec<WorkspaceGitChangeEntry>, String> {
    let range = format!("{base}...HEAD");

    // name-status
    let ns_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["diff", "--name-status", &range])
        .output()
        .map_err(|e| e.to_string())?;
    if !ns_out.status.success() {
        return Err(git_output_err("git diff --name-status", &ns_out.stderr));
    }
    let ns_text = String::from_utf8_lossy(&ns_out.stdout);

    // numstat
    let stat_map = {
        let stat_out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["diff", "--numstat", &range])
            .output()
            .map_err(|e| e.to_string())?;
        if stat_out.status.success() {
            parse_numstat_tab_lines(&String::from_utf8_lossy(&stat_out.stdout))
        } else {
            HashMap::new()
        }
    };

    let mut entries = Vec::new();
    for line in ns_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let status_raw = parts.next().unwrap_or("").trim();
        let path_part = parts.next().unwrap_or("").trim();
        if path_part.is_empty() {
            continue;
        }
        // Handle rename: "R100\told_path\tnew_path"
        let path = if status_raw.starts_with('R') || status_raw.starts_with('C') {
            path_part.split('\t').last().unwrap_or(path_part).trim().to_string()
        } else {
            path_part.to_string()
        };
        let status = status_raw.chars().next().unwrap_or('M').to_string();
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
