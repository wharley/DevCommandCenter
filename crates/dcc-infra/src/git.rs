use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use dcc_core::{
    ports::{ClonedRepository, GitOps, PreparedWorktree},
    CoreError, Result,
};

pub use crate::git_command::{
    configure_git_command, git_command_succeeds, git_output_detail, git_output_err,
    run_git_network_output, run_git_network_output_with_env, run_git_output, run_git_output_owned,
    run_git_output_with_timeout, GIT_CLONE_TIMEOUT, GIT_HARDENED_CONFIG_ARGS, GIT_LOCAL_TIMEOUT,
    GIT_NETWORK_TIMEOUT,
};
use crate::git_command::{run_git_output_with_timeout_in_dir, run_git_stdout_in_dir};
pub use crate::git_parsing::{
    parse_default_branch_from_ls_remote_output, parse_git_status_porcelain_z,
    parse_local_branch_names, parse_name_status_z, parse_numstat_z, split_null_terminated_fields,
    GitNameStatusEntry, GitStatusPorcelainEntry,
};
pub use crate::workspace_setup_plan::{
    detect_workspace_setup_suggestions, WorkspaceSetupSuggestion,
};
pub use crate::repo_config::{read_workspace_validation_config, RepoValidationConfig};

#[derive(Clone, Debug, Default)]
pub struct CommandGitOps;

impl CommandGitOps {
    pub fn new() -> Self {
        Self
    }
}

fn run_git_stdout<I>(
    args: I,
    current_dir: Option<&Path>,
    timeout: std::time::Duration,
    fallback: &str,
) -> Result<String>
where
    I: IntoIterator,
    I::Item: AsRef<std::ffi::OsStr>,
{
    run_git_stdout_in_dir(args, current_dir, timeout, fallback).map_err(CoreError::Git)
}

fn run_git_with_timeout<I>(
    args: I,
    current_dir: Option<&Path>,
    timeout: std::time::Duration,
) -> Result<std::process::Output>
where
    I: IntoIterator,
    I::Item: AsRef<std::ffi::OsStr>,
{
    run_git_output_with_timeout_in_dir(args, current_dir, timeout).map_err(CoreError::Git)
}

fn sanitize_segment(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
        } else {
            output.push('-');
        }
    }
    let trimmed = output.trim_matches('-').trim_matches('.');
    if trimmed.is_empty() {
        "workspace".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn build_worktree_path(workspace_root: &Path, base_branch: &str) -> PathBuf {
    let repo_parent = workspace_root.parent().unwrap_or(workspace_root);
    let worktrees_root = repo_parent.join(".dcc-worktrees");
    worktrees_root.join(format!(
        "{}-{}",
        sanitize_segment(base_branch),
        Uuid::new_v4().simple()
    ))
}

pub fn is_git_repo(workspace_root: &Path) -> bool {
    run_git_with_timeout(
        ["rev-parse", "--is-inside-work-tree"],
        Some(workspace_root),
        GIT_LOCAL_TIMEOUT,
    )
    .map(|output| output.status.success())
    .unwrap_or(false)
}

fn detect_default_branch(repository_url: &str) -> Result<String> {
    let stdout = run_git_stdout(
        ["ls-remote", "--symref", repository_url, "HEAD"],
        None,
        GIT_NETWORK_TIMEOUT,
        "failed to detect default branch",
    )?;
    parse_default_branch_from_ls_remote_output(&stdout)
        .ok_or_else(|| CoreError::Git("failed to detect default branch from remote".to_string()))
}

pub fn is_broken_worktree_error_text(message: &str) -> bool {
    message.contains("not a git repository")
        || message.contains("is not a working tree")
        || message.contains("cannot change to")
        || message.contains("No such file or directory")
}

pub fn broken_worktree_reason(workspace_root: &Path) -> Option<String> {
    if !workspace_root.exists() {
        return Some(format!(
            "workspace path no longer exists: {}",
            workspace_root.display()
        ));
    }

    match run_git_with_timeout(
        ["rev-parse", "--is-inside-work-tree"],
        Some(workspace_root),
        GIT_LOCAL_TIMEOUT,
    ) {
        Ok(output) if output.status.success() => None,
        Ok(output) => {
            let detail = git_output_detail(&output, "failed to inspect git worktree");
            if is_broken_worktree_error_text(&detail) {
                Some(detail)
            } else {
                None
            }
        }
        Err(CoreError::Git(message)) if is_broken_worktree_error_text(&message) => Some(message),
        Err(_) => None,
    }
}

pub fn list_local_branch_names(project_path: &str) -> Result<Vec<String>> {
    let project_path = Path::new(project_path);

    if !project_path.exists() {
        return Err(CoreError::Git(format!(
            "repository path does not exist: {}",
            project_path.display()
        )));
    }

    if !is_git_repo(project_path) {
        return Err(CoreError::Git(format!(
            "not a git repository: {}",
            project_path.display()
        )));
    }

    let stdout = run_git_stdout(
        [
            "for-each-ref",
            "--format=%(refname:short)%00",
            "refs/heads/",
        ],
        Some(project_path),
        GIT_LOCAL_TIMEOUT,
        "failed to list local branches",
    )?;
    Ok(parse_local_branch_names(&stdout))
}

pub fn create_worktree_branch_from_ref(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
    start_point: &str,
) -> Result<()> {
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CoreError::Git(error.to_string()))?;
    }

    run_git_stdout(
        ["worktree", "prune"],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git worktree prune failed",
    )?;

    let args = vec![
        OsString::from("worktree"),
        OsString::from("add"),
        OsString::from("-b"),
        OsString::from(branch),
        worktree_path.as_os_str().to_os_string(),
        OsString::from(start_point),
    ];
    run_git_stdout(
        args,
        Some(repo_root),
        GIT_NETWORK_TIMEOUT,
        "git worktree add failed",
    )?;
    Ok(())
}

pub fn remove_worktree(repo_root: &Path, worktree_path: &Path) -> Result<()> {
    let args = vec![
        OsString::from("worktree"),
        OsString::from("remove"),
        OsString::from("--force"),
        worktree_path.as_os_str().to_os_string(),
    ];
    run_git_stdout(
        args,
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git worktree remove failed",
    )?;
    Ok(())
}

pub fn remove_branch(repo_root: &Path, branch: &str) -> Result<()> {
    run_git_stdout(
        ["branch", "-D", branch],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git branch -D failed",
    )?;
    Ok(())
}

pub fn stash_create(repo_root: &Path) -> Result<Option<String>> {
    let output = run_git_stdout(
        ["stash", "create"],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git stash create failed",
    )?;
    let sha = output.trim();
    if sha.is_empty() {
        Ok(None)
    } else {
        Ok(Some(sha.to_string()))
    }
}

pub fn stash_apply_sha(workspace_dir: &Path, stash_sha: &str) -> Result<()> {
    run_git_stdout(
        ["stash", "apply", stash_sha],
        Some(workspace_dir),
        GIT_LOCAL_TIMEOUT,
        "git stash apply failed",
    )?;
    Ok(())
}

pub fn list_untracked_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = run_git_with_timeout(
        ["ls-files", "--others", "--exclude-standard", "-z"],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(CoreError::Git(git_output_detail(
            &output,
            "git ls-files failed",
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut paths = stdout
        .split('\0')
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

#[async_trait]
impl GitOps for CommandGitOps {
    async fn clone_repository(
        &self,
        repository_url: &str,
        destination_path: &str,
        base_branch: &str,
    ) -> Result<ClonedRepository> {
        let repository_url = repository_url.trim();
        let base_branch = base_branch.trim();
        let destination_path = Path::new(destination_path);

        if repository_url.is_empty() {
            return Err(CoreError::Git("repository URL cannot be empty".to_string()));
        }

        let resolved_branch = if base_branch.is_empty() {
            detect_default_branch(repository_url)?
        } else {
            base_branch.to_string()
        };

        if let Some(parent) = destination_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| CoreError::Git(error.to_string()))?;
        }

        let clone_args = vec![
            OsString::from("clone"),
            OsString::from("--branch"),
            OsString::from(&resolved_branch),
            OsString::from("--single-branch"),
            OsString::from(repository_url),
            destination_path.as_os_str().to_os_string(),
        ];

        run_git_stdout(clone_args, None, GIT_CLONE_TIMEOUT, "git clone failed")?;

        Ok(ClonedRepository {
            path: destination_path.to_string_lossy().to_string(),
            branch: resolved_branch,
            created_at: Utc::now().to_rfc3339(),
        })
    }

    async fn prepare_worktree(
        &self,
        workspace_root: &str,
        base_branch: &str,
    ) -> Result<PreparedWorktree> {
        let workspace_root = Path::new(workspace_root);
        if !workspace_root.exists() {
            return Err(CoreError::Git(format!(
                "workspace root does not exist: {}",
                workspace_root.display()
            )));
        }

        let worktree_path = build_worktree_path(workspace_root, base_branch);
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| CoreError::Git(error.to_string()))?;
        }

        if is_git_repo(workspace_root) {
            run_git_stdout(
                ["worktree", "prune"],
                Some(workspace_root),
                GIT_LOCAL_TIMEOUT,
                "git worktree prune failed",
            )?;
            let worktree_add_args = vec![
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("--detach"),
                worktree_path.as_os_str().to_os_string(),
                OsString::from(base_branch),
            ];
            run_git_stdout(
                worktree_add_args,
                Some(workspace_root),
                GIT_NETWORK_TIMEOUT,
                "git worktree add failed",
            )?;
        } else {
            std::fs::create_dir_all(&worktree_path)
                .map_err(|error| CoreError::Git(error.to_string()))?;
        }

        Ok(PreparedWorktree {
            path: worktree_path.to_string_lossy().to_string(),
            branch: base_branch.to_string(),
            created_at: Utc::now().to_rfc3339(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::is_broken_worktree_error_text;

    #[test]
    fn broken_worktree_detection_matches_known_git_failures() {
        assert!(is_broken_worktree_error_text(
            "fatal: not a git repository: /tmp/repo/.git/worktrees/demo"
        ));
        assert!(is_broken_worktree_error_text(
            "fatal: cannot change to '/tmp/missing': No such file or directory"
        ));
        assert!(!is_broken_worktree_error_text(
            "fatal: not a valid object name HEAD~99"
        ));
    }
}
