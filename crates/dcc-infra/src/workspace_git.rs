//! A task checkout's git state, branch diff and pull request, read without
//! any app state: the desktop's Tauri commands and the native app share it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dcc_core::domain::workspace::{Workspace, WorkspaceSource};
use dcc_core::domain::workspace_git::{
    WorkspaceGitBranchDiffOutput, WorkspaceGitChangeEntry, WorkspaceGitStatusOutput,
    WorkspacePrStatusOutput,
};
use dcc_core::ports::WorkspaceRepo;
use sha2::{Digest, Sha256};

use crate::db::SqliteWorkspaceRepo;
use crate::git::{
    git_command_succeeds, git_output_err, parse_name_status_z, parse_numstat_z, run_git_output,
    run_git_output_owned, split_null_terminated_fields,
};

/// `git status --porcelain` → staged / unstaged rows.
pub fn read_workspace_git_status(workspace_root: &str) -> Result<WorkspaceGitStatusOutput, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Ok(WorkspaceGitStatusOutput {
            staged: vec![],
            unstaged: vec![],
            staged_fingerprint: String::new(),
            current_branch: None,
            ahead_of_remote_count: 0,
            behind_of_remote_count: 0,
            conflict_count: 0,
            merge_in_progress: false,
            has_remote: false,
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
    let has_remote = resolve_has_remote(root);

    Ok(WorkspaceGitStatusOutput {
        staged,
        unstaged,
        staged_fingerprint: staged_snapshot_fingerprint(root)?,
        current_branch,
        ahead_of_remote_count,
        behind_of_remote_count,
        conflict_count,
        merge_in_progress,
        has_remote,
    })
}

pub fn parse_git_numstat_maps(
    root: &str,
    cached: bool,
) -> Result<HashMap<String, (u32, u32)>, String> {
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

pub fn join_workspace_path(root: &str, rel: &str) -> String {
    PathBuf::from(root).join(rel).to_string_lossy().to_string()
}

pub fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

pub fn resolve_upstream_branch_counts(root: &str) -> Result<(u32, u32), String> {
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

/// Unknown counts as present: hiding push on a git hiccup would be worse than
/// offering it to a repository that turns out to have no remote.
pub fn resolve_has_remote(root: &str) -> bool {
    match run_git_output(root, &["remote"]) {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| !line.trim().is_empty()),
        _ => true,
    }
}

pub fn resolve_conflict_count(root: &str) -> Result<u32, String> {
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

pub fn git_internal_path_exists(root: &str, name: &str) -> bool {
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

pub struct StagedSnapshot {
    pub name_status: Vec<u8>,
    pub patch: Vec<u8>,
    pub fingerprint: String,
}

pub fn capture_staged_snapshot(root: &str) -> Result<StagedSnapshot, String> {
    let name_status = run_git_output(root, &["diff", "--cached", "--name-status", "-z"])?;
    if !name_status.status.success() {
        return Err(git_output_err(
            "git diff --cached --name-status",
            &name_status.stderr,
        ));
    }
    let patch = run_git_output(root, &["diff", "--cached", "--patch", "--no-ext-diff"])?;
    if !patch.status.success() {
        return Err(git_output_err("git diff --cached", &patch.stderr));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"dcc-staged-snapshot-v1\0");
    hasher.update(&name_status.stdout);
    hasher.update(b"\0");
    hasher.update(&patch.stdout);
    Ok(StagedSnapshot {
        name_status: name_status.stdout,
        patch: patch.stdout,
        fingerprint: format!("{:x}", hasher.finalize()),
    })
}

pub fn staged_snapshot_fingerprint(root: &str) -> Result<String, String> {
    Ok(capture_staged_snapshot(root)?.fingerprint)
}

pub fn compute_branch_diff(root: &str, base: &str) -> Result<Vec<WorkspaceGitChangeEntry>, String> {
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

pub fn normalize_git_relative_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

pub fn resolve_workspace_setup_root(workspace: &Workspace) -> &str {
    workspace
        .worktree_path
        .as_deref()
        .unwrap_or(workspace.root_path.as_str())
}

pub fn resolve_workspace_active_root(workspace: &Workspace) -> &str {
    resolve_workspace_setup_root(workspace)
}

pub async fn find_workspace_by_root(
    repo: &SqliteWorkspaceRepo,
    workspace_root: &str,
) -> Result<Option<Workspace>, String> {
    let workspace_root = workspace_root.trim();
    let workspaces = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?;
    // Local-direct and protected tasks can share a repository root. Prefer
    // the task executing at this path over another task's parent checkout.
    Ok(workspaces
        .iter()
        .find(|workspace| resolve_workspace_active_root(workspace) == workspace_root)
        .or_else(|| {
            workspaces
                .iter()
                .find(|workspace| workspace.root_path == workspace_root)
        })
        .cloned())
}

pub fn resolve_current_branch_name(root: &str) -> Result<String, String> {
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
    Ok(branch)
}

/// Whether `root` is a linked Git worktree rather than the repository's main
/// checkout. Answers `true` when Git cannot tell, so callers keep their
/// worktree behavior on failure.
pub fn is_linked_worktree(root: &str) -> bool {
    let resolve = |flag: &str| {
        run_git_output(root, &["rev-parse", "--path-format=absolute", flag])
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|path| !path.is_empty())
    };
    match (resolve("--git-dir"), resolve("--git-common-dir")) {
        (Some(git_dir), Some(common_dir)) => git_dir != common_dir,
        _ => true,
    }
}

pub fn resolve_current_commit_sha(root: &str) -> Result<Option<String>, String> {
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

pub fn workspace_branch_hints(root: &str, branch: Option<&str>) -> Vec<String> {
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

pub fn resolve_branch_diff_base(root: &str, target_branch: Option<&str>) -> Option<String> {
    let current_branch = resolve_current_branch_name(root).ok();

    if let Some(target_branch) = target_branch
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
    {
        let remote_target = format!("origin/{target_branch}");
        if git_command_succeeds(root, &["rev-parse", "--verify", &remote_target]) {
            return Some(remote_target);
        }
        if git_command_succeeds(root, &["rev-parse", "--verify", target_branch]) {
            return Some(target_branch.to_string());
        }
    }

    let upstream = run_git_output(root, &["rev-parse", "--abbrev-ref", "@{upstream}"])
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .filter(|upstream| {
            let short = upstream.rsplit('/').next().unwrap_or(upstream.as_str());
            current_branch
                .as_deref()
                .map(|branch| branch != short)
                .unwrap_or(true)
        });
    if upstream.is_some() {
        return upstream;
    }

    for fallback in [
        "origin/HEAD",
        "origin/main",
        "origin/master",
        "origin/develop",
    ] {
        if git_command_succeeds(root, &["rev-parse", "--verify", fallback]) {
            return Some(fallback.to_string());
        }
    }

    None
}

/// The base branch a task was created from: the task's own `base_branch`,
/// never the repository row's, which the newest task overwrites.
pub async fn workspace_base_branch(db_path: &Path, workspace_root: &str) -> Option<String> {
    let repo = SqliteWorkspaceRepo::open(db_path).ok()?;
    let workspace = find_workspace_by_root(&repo, workspace_root)
        .await
        .ok()
        .flatten()?;
    let branch = workspace.base_branch.trim();
    (!branch.is_empty()).then(|| branch.to_string())
}

/// Where an imported task came from (a pull request opened elsewhere).
pub async fn imported_workspace_source(
    db_path: &Path,
    root: &str,
) -> Result<Option<WorkspaceSource>, String> {
    let repo = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    Ok(find_workspace_by_root(&repo, root)
        .await?
        .and_then(|workspace| workspace.source))
}

/// Files committed on the task's branch against its base.
pub async fn workspace_git_branch_diff(
    db_path: &Path,
    workspace_root: &str,
) -> Result<WorkspaceGitBranchDiffOutput, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let protected_branch = workspace_base_branch(db_path, root).await;
    let base = resolve_branch_diff_base(root, protected_branch.as_deref());
    let changes = match base {
        Some(ref base) => compute_branch_diff(root, base)?,
        None => vec![],
    };
    Ok(WorkspaceGitBranchDiffOutput {
        changes,
        base_branch: base,
    })
}

fn empty_pr_status(
    provider: Option<String>,
    host: Option<String>,
    head_branch: Option<String>,
) -> WorkspacePrStatusOutput {
    WorkspacePrStatusOutput {
        provider,
        host,
        number: None,
        title: None,
        url: None,
        head_branch,
        base_branch: None,
        state: None,
        is_draft: false,
        mergeable: None,
        merge_state_status: None,
    }
}

/// The task's pull request on its forge (GitHub or GitLab CLI), or what is
/// known without one. Blocking: it runs the forge CLI.
pub async fn workspace_pr_status(
    db_path: &Path,
    workspace_root: &str,
    branch: Option<String>,
    forge_login: Option<&str>,
) -> Result<WorkspacePrStatusOutput, String> {
    use crate::forge::{context as forge_context, provider as forge_provider};
    use dcc_core::domain::forge::ForgeCliProvider;

    let root = workspace_root.trim();
    if root.is_empty() {
        return Ok(empty_pr_status(None, None, None));
    }
    let forge_context = forge_context::resolve_workspace_forge_context(db_path, root, forge_login)?;
    let effective_login = forge_context
        .as_ref()
        .and_then(|context| context.effective_login.as_deref());
    let head_sha = resolve_current_commit_sha(root).ok().flatten();
    let source = imported_workspace_source(db_path, root).await?;
    let branch = match branch
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(branch) => branch,
        None if source.is_some() => source
            .as_ref()
            .map(|source| source.head_branch.clone())
            .unwrap_or_default(),
        None => match resolve_current_branch_name(root) {
            Ok(branch) => branch,
            Err(_) => return Ok(empty_pr_status(None, None, None)),
        },
    };
    let branch_hints = workspace_branch_hints(root, Some(&branch));
    let resolved = forge_provider::resolve_workspace_change_request_status(
        root,
        &branch,
        &branch_hints,
        head_sha.as_deref(),
        effective_login,
    )?;
    // A task without a worktree runs on whatever branch the main checkout is on.
    // On a long-lived branch such as `develop`, the latest change request is an
    // old merged release that says nothing about the work in progress.
    let resolved = resolved
        .filter(|resolved| resolved.state.as_deref() != Some("merged") || is_linked_worktree(root));
    let Some(resolved) = resolved else {
        return Ok(empty_pr_status(
            forge_context
                .as_ref()
                .map(|context| match context.provider {
                    ForgeCliProvider::Github => "github".to_string(),
                    ForgeCliProvider::Gitlab => "gitlab".to_string(),
                }),
            forge_context.as_ref().map(|context| context.host.clone()),
            Some(branch),
        ));
    };
    Ok(WorkspacePrStatusOutput {
        provider: Some(resolved.provider),
        host: resolved.host,
        number: resolved.number,
        title: resolved.title,
        url: resolved.url,
        head_branch: resolved.head_branch,
        base_branch: resolved.base_branch,
        state: resolved.state,
        is_draft: resolved.is_draft,
        mergeable: resolved.mergeable,
        merge_state_status: resolved.merge_state_status,
    })
}

/// Most ignored files the tree lists; past this the ignore rules are matching
/// build output, not the few local files someone wants to open.
const GIT_IGNORED_FILES_LIMIT: usize = 2_000;

/// Ignored files outside wholly ignored folders: `--directory` reports
/// `node_modules/` as one entry instead of walking it, and those folder
/// entries are dropped. Sorted, repo-relative forward slashes. Empty when the
/// root is blank, not a git worktree, or git fails.
pub fn git_ignored_files(workspace_root: &str) -> Result<Vec<String>, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Ok(Vec::new());
    }

    let output = run_git_output(
        root,
        &[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
    )?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let root_path = std::path::Path::new(root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut paths: Vec<String> = stdout
        .split('\0')
        .filter(|line| !line.is_empty() && !line.ends_with('/'))
        .filter(|line| !line.ends_with(".DS_Store"))
        // A symlinked folder is reported without the trailing slash.
        .filter(|line| !root_path.join(line).is_dir())
        .map(|line| line.to_string())
        .take(GIT_IGNORED_FILES_LIMIT)
        .collect();
    paths.sort();
    Ok(paths)
}

/// The worktree's files as git sees them: tracked files still on disk plus new
/// files that are not ignored, so what an agent just created shows up before
/// anyone stages it. Sorted, repo-relative forward slashes. Empty when the
/// root is blank, not a git worktree, or git fails. Shared by the desktop's
/// file tree and `@` mention and the phone's file search.
pub fn git_tracked_files(workspace_root: &str) -> Result<Vec<String>, String> {
    let root = workspace_root.trim();
    if root.is_empty() {
        return Ok(Vec::new());
    }

    let output = run_git_output(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let root_path = std::path::Path::new(root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut paths: Vec<String> = stdout
        .split('\0')
        .filter(|line| !line.is_empty())
        // `--cached` still lists tracked files deleted from the worktree.
        .filter(|line| root_path.join(line).symlink_metadata().is_ok())
        .map(|line| line.to_string())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}
