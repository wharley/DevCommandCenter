use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Result;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PreparedWorktree {
	pub path: String,
	pub branch: String,
	pub created_at: String,
}

#[async_trait]
pub trait GitOps: Send + Sync {
	async fn prepare_worktree(
		&self,
		workspace_root: &str,
		base_branch: &str,
	) -> Result<PreparedWorktree>;
}
