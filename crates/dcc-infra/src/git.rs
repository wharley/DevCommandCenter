use std::{
    path::{Path, PathBuf},
    process::Command,
};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use dcc_core::{
    ports::{ClonedRepository, GitOps, PreparedWorktree},
    CoreError, Result,
};

#[derive(Clone, Debug, Default)]
pub struct CommandGitOps;

impl CommandGitOps {
    pub fn new() -> Self {
        Self
    }
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

fn build_worktree_path(workspace_root: &Path, base_branch: &str) -> PathBuf {
    let repo_parent = workspace_root.parent().unwrap_or(workspace_root);
    let worktrees_root = repo_parent.join(".dcc-worktrees");
    worktrees_root.join(format!(
        "{}-{}",
        sanitize_segment(base_branch),
        Uuid::new_v4().simple()
    ))
}

fn is_git_repo(workspace_root: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn parse_local_branch_names(output: &str) -> Vec<String> {
    let mut branches: Vec<String> = output
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    branches.sort();
    branches.dedup();
    branches
}

fn parse_default_branch_from_ls_remote_output(output: &str) -> Option<String> {
    for line in output.lines() {
        let Some((ref_part, remainder)) = line.split_once('\t') else {
            continue;
        };

        if remainder.trim() != "HEAD" {
            continue;
        }

        let Some(branch) = ref_part.strip_prefix("ref: refs/heads/") else {
            continue;
        };

        let branch = branch.trim();
        if !branch.is_empty() {
            return Some(branch.to_string());
        }
    }

    None
}

fn detect_default_branch(repository_url: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("ls-remote")
        .arg("--symref")
        .arg(repository_url)
        .arg("HEAD")
        .output()
        .map_err(|error| CoreError::Git(error.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CoreError::Git(if stderr.is_empty() {
            "failed to detect default branch".to_string()
        } else {
            stderr
        }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_default_branch_from_ls_remote_output(&stdout)
        .ok_or_else(|| CoreError::Git("failed to detect default branch from remote".to_string()))
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

    let output = Command::new("git")
        .arg("-C")
        .arg(project_path)
        .arg("for-each-ref")
        .arg("--format=%(refname:short)")
        .arg("refs/heads/")
        .output()
        .map_err(|error| CoreError::Git(error.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CoreError::Git(if stderr.is_empty() {
            "failed to list local branches".to_string()
        } else {
            stderr
        }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_local_branch_names(&stdout))
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

        let output = Command::new("git")
            .arg("clone")
            .arg("--branch")
            .arg(&resolved_branch)
            .arg("--single-branch")
            .arg(repository_url)
            .arg(destination_path)
            .output()
            .map_err(|error| CoreError::Git(error.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(CoreError::Git(if stderr.is_empty() {
                "git clone failed".to_string()
            } else {
                stderr
            }));
        }

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
            let output = Command::new("git")
                .arg("-C")
                .arg(workspace_root)
                .arg("worktree")
                .arg("add")
                .arg("--detach")
                .arg(&worktree_path)
                .arg(base_branch)
                .output()
                .map_err(|error| CoreError::Git(error.to_string()))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(CoreError::Git(if stderr.is_empty() {
                    "git worktree add failed".to_string()
                } else {
                    stderr
                }));
            }
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
    use super::{parse_default_branch_from_ls_remote_output, parse_local_branch_names};

    #[test]
    fn parse_default_branch_from_ls_remote_output_extracts_branch_name() {
        let output = "ref: refs/heads/main\tHEAD\nabc123\tHEAD\n";

        assert_eq!(
            parse_default_branch_from_ls_remote_output(output),
            Some("main".to_string()),
        );
    }

    #[test]
    fn parse_default_branch_from_ls_remote_output_returns_none_when_missing() {
        let output = "abc123\trefs/heads/main\n";

        assert_eq!(parse_default_branch_from_ls_remote_output(output), None);
    }

    #[test]
    fn parse_local_branch_names_sorts_and_deduplicates() {
        let output = "feature/a\nmain\nfeature/a\nrelease\n";

        assert_eq!(
            parse_local_branch_names(output),
            vec![
                "feature/a".to_string(),
                "main".to_string(),
                "release".to_string(),
            ]
        );
    }
}
