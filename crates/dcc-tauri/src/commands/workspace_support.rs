use std::path::Path;

use tauri::State;

use dcc_core::domain::workspace::Workspace;
use dcc_core::ports::WorkspaceRepo;
use dcc_infra::{
    db::SqliteWorkspaceRepo,
    git::{broken_worktree_reason, is_git_repo, remove_worktree},
};

use crate::{
    commands::forge::context as forge_context,
    git::{
        git_command_succeeds, git_output_err, run_git_network_output,
        run_git_network_output_with_env, run_git_output,
    },
    state::WorkspaceCommandState,
};

pub(crate) fn resolve_workspace_setup_root(workspace: &Workspace) -> &str {
    workspace
        .worktree_path
        .as_deref()
        .unwrap_or(workspace.root_path.as_str())
}

pub(crate) fn resolve_workspace_active_root(workspace: &Workspace) -> &str {
    resolve_workspace_setup_root(workspace)
}

pub(crate) fn resolve_workspace_broken_reason(workspace: &Workspace) -> Option<String> {
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

pub(crate) fn cleanup_workspace_files(workspace: &Workspace) -> Result<(), String> {
    let Some(worktree_root) = workspace.worktree_path.as_deref().map(str::trim) else {
        return Ok(());
    };
    if worktree_root.is_empty() {
        return Ok(());
    }

    let root_path = workspace.root_path.trim();
    if !root_path.is_empty() && Path::new(root_path) == Path::new(worktree_root) {
        return Ok(());
    }

    let worktree_path = Path::new(worktree_root);
    if !worktree_path.exists() {
        return Ok(());
    }

    let repo_root = Path::new(root_path);
    let should_use_git_worktree_removal =
        !root_path.is_empty() && is_git_repo(repo_root) && worktree_path.join(".git").exists();

    if should_use_git_worktree_removal {
        match remove_worktree(repo_root, worktree_path) {
            Ok(()) => return Ok(()),
            Err(error) if broken_worktree_reason(worktree_path).is_none() => {
                return Err(error.to_string());
            }
            Err(_) => {
                // Fall back to removing the directory directly when the worktree metadata
                // is already broken. A later `git worktree prune` clears stale metadata.
            }
        }
    }

    std::fs::remove_dir_all(worktree_path).map_err(|error| {
        format!(
            "failed to remove workspace path {}: {}",
            worktree_path.display(),
            error
        )
    })?;

    if !root_path.is_empty() && is_git_repo(repo_root) {
        let _ = run_git_output(root_path, &["worktree", "prune"]);
    }

    Ok(())
}

/// Returns the logical size of a directory without following symbolic links.
///
/// Worktrees may contain symlinks into large dependency caches. Following them
/// would both overstate the space reclaimed by deletion and risk walking a
/// cycle, so only regular files are counted.
pub(crate) fn directory_logical_size(root: &Path) -> Result<u64, String> {
    if !root.exists() {
        return Ok(0);
    }

    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            format!(
                "failed to inspect workspace path {}: {}",
                directory.display(),
                error
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to inspect workspace path {}: {}",
                    directory.display(),
                    error
                )
            })?;
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(|error| {
                format!(
                    "failed to inspect workspace entry {}: {}",
                    entry.path().display(),
                    error
                )
            })?;
            let file_type = metadata.file_type();
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

pub(crate) async fn find_workspace_by_root(
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

pub(crate) async fn purge_broken_workspace_by_root(
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

pub(crate) fn broken_workspace_message(reason: &str) -> String {
    format!("workspace became unavailable and was removed from DCC: {reason}")
}

pub(crate) async fn preflight_workspace_root(
    state: &State<'_, WorkspaceCommandState>,
    workspace_root: &str,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    if let Some(reason) = purge_broken_workspace_by_root(&repo, workspace_root).await? {
        return Err(broken_workspace_message(&reason));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{cleanup_workspace_files, directory_logical_size};
    use dcc_core::domain::{
        project::ProjectId,
        workspace::{Workspace, WorkspaceId, WorkspaceState},
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("dcc-{name}-{unique}"))
    }

    fn test_workspace(root_path: &Path, worktree_path: Option<&Path>) -> Workspace {
        Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            name: Some("Workspace".to_string()),
            root_path: root_path.to_string_lossy().to_string(),
            base_branch: "main".to_string(),
            worktree_path: worktree_path.map(|path| path.to_string_lossy().to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn directory_size_counts_nested_files() {
        let root = temp_path("directory-size");
        fs::create_dir_all(root.join("nested")).expect("create nested directory");
        fs::write(root.join("one.txt"), b"1234").expect("write first file");
        fs::write(root.join("nested/two.txt"), b"123456").expect("write nested file");

        assert_eq!(
            directory_logical_size(&root).expect("measure directory"),
            10
        );

        fs::remove_dir_all(root).expect("remove temp directory");
    }

    #[test]
    fn cleanup_workspace_files_removes_plain_worktree_directory() {
        let root_path = temp_path("workspace-root");
        let worktree_path = root_path.join(".dcc-worktrees").join("main-123");

        fs::create_dir_all(&worktree_path).expect("create worktree dir");
        fs::write(worktree_path.join("package.json"), "{}").expect("create worktree file");

        let workspace = test_workspace(&root_path, Some(&worktree_path));
        cleanup_workspace_files(&workspace).expect("cleanup should succeed");

        assert!(!worktree_path.exists(), "worktree path should be removed");
    }

    #[test]
    fn cleanup_workspace_files_never_removes_root_path() {
        let root_path = temp_path("workspace-root-guard");

        fs::create_dir_all(&root_path).expect("create root dir");
        fs::write(root_path.join("README.md"), "root").expect("create root file");

        let workspace = test_workspace(&root_path, Some(&root_path));
        cleanup_workspace_files(&workspace).expect("cleanup should skip root path");

        assert!(root_path.exists(), "root path must be preserved");
        fs::remove_dir_all(&root_path).expect("remove root dir");
    }
}

pub(crate) fn resolve_current_branch_name(root: &str) -> Result<String, String> {
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

/// Resolves the base branch a workspace was created from.
///
/// Uses the per-workspace `base_branch` field. The repository row's
/// `base_branch` is shared across every workspace of the same repo and gets
/// overwritten by the most recently created workspace, so it must NOT be used
/// here — doing so makes the diff/PR base of one workspace leak into another.
pub(crate) async fn resolve_workspace_target_branch(
    state: &WorkspaceCommandState,
    workspace_root: &str,
) -> Option<String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).ok()?;
    let workspace = find_workspace_by_root(&repo, workspace_root)
        .await
        .ok()
        .flatten()?;
    let branch = workspace.base_branch.trim();
    if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    }
}

pub(crate) fn resolve_current_commit_sha(root: &str) -> Result<Option<String>, String> {
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

pub(crate) fn workspace_branch_hints(root: &str, branch: Option<&str>) -> Vec<String> {
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

pub(crate) fn resolve_default_remote_name(root: &str) -> Result<String, String> {
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

fn base64_encode(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;
    while index < bytes.len() {
        let b0 = bytes[index];
        let b1 = bytes.get(index + 1).copied().unwrap_or(0);
        let b2 = bytes.get(index + 2).copied().unwrap_or(0);
        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if index + 1 < bytes.len() {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if index + 2 < bytes.len() {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
        index += 3;
    }
    out
}

pub(crate) fn push_branch_refspec(
    db_path: &Path,
    root: &str,
    branch: &str,
    forge_login: Option<&str>,
) -> Result<(), String> {
    let remote = resolve_default_remote_name(root)?;
    push_branch_refspec_to_remote(db_path, root, &remote, branch, forge_login)
}

pub(crate) fn push_branch_refspec_to_remote(
    db_path: &Path,
    root: &str,
    remote: &str,
    branch: &str,
    forge_login: Option<&str>,
) -> Result<(), String> {
    let branch = branch.trim();
    if branch.is_empty() || branch == "HEAD" {
        return Err(
            "cannot push from detached HEAD because no branch name could be resolved".to_string(),
        );
    }

    let remote = remote.trim();
    if remote.is_empty() {
        return Err("cannot push because the source remote is empty".to_string());
    }
    let remote_ref = format!("HEAD:refs/heads/{branch}");
    let output = run_git_network_output_with_workspace_auth(
        db_path,
        root,
        &["push", "-u", remote, &remote_ref],
        forge_login,
    )?;
    if output.status.success() {
        return Ok(());
    }

    Err(git_output_err("git push -u", &output.stderr))
}

pub(crate) fn run_git_network_output_with_workspace_auth(
    db_path: &Path,
    root: &str,
    args: &[&str],
    forge_login: Option<&str>,
) -> Result<std::process::Output, String> {
    let auth = forge_context::resolve_workspace_git_auth(db_path, root, forge_login)?;
    if let Some(auth) = auth {
        let extraheader_key = format!("http.https://{}/.extraheader", auth.host);
        let extraheader_value = format!(
            "AUTHORIZATION: Basic {}",
            base64_encode(&auth.git_http_authorization)
        );
        let config_arg = format!("{extraheader_key}={extraheader_value}");
        let mut authed_args = Vec::with_capacity(args.len() + 2);
        authed_args.push("-c");
        authed_args.push(config_arg.as_str());
        authed_args.extend_from_slice(args);
        run_git_network_output_with_env(root, &authed_args, &auth.envs)
    } else {
        run_git_network_output(root, args)
    }
}

fn branch_name_from_worktree_path(root: &str) -> String {
    let dir = Path::new(root)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("dcc-branch");
    format!("dcc/{dir}")
}

fn materialize_workspace_branch(root: &str) -> Result<String, String> {
    let preferred = branch_name_from_worktree_path(root);
    let branch = next_available_branch_name(root, &preferred);
    let checkout = run_git_output(root, &["switch", "-c", &branch])?;
    if !checkout.status.success() {
        return Err(git_output_err("git switch -c", &checkout.stderr));
    }
    Ok(branch)
}

pub(crate) fn ensure_pushable_branch(
    root: &str,
    protected_branch: Option<&str>,
) -> Result<String, String> {
    let raw_branch = resolve_current_branch_name(root)?;
    let protected_branch = protected_branch
        .map(str::trim)
        .filter(|branch| !branch.is_empty());
    if raw_branch != "HEAD" && protected_branch != Some(raw_branch.as_str()) {
        return Ok(raw_branch);
    }

    materialize_workspace_branch(root)
}

pub(crate) fn resolve_branch_diff_base(root: &str, target_branch: Option<&str>) -> Option<String> {
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

pub(crate) fn next_available_branch_name(root: &str, preferred: &str) -> String {
    if !git_command_succeeds(
        root,
        &["show-ref", "--verify", &format!("refs/heads/{preferred}")],
    ) {
        return preferred.to_string();
    }

    for attempt in 2..=9999 {
        let candidate = format!("{preferred}-{attempt}");
        if !git_command_succeeds(
            root,
            &["show-ref", "--verify", &format!("refs/heads/{candidate}")],
        ) {
            return candidate;
        }
    }

    format!("{preferred}-{}", uuid::Uuid::new_v4().simple())
}
