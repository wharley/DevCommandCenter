use std::{
	path::{Path, PathBuf},
	process::Command,
};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use dcc_core::{
	ports::{GitOps, PreparedWorktree},
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

#[async_trait]
impl GitOps for CommandGitOps {
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
			std::fs::create_dir_all(parent)
				.map_err(|error| CoreError::Git(error.to_string()))?;
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
