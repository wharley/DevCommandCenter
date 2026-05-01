use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum CoreEvent {
	WorkspacePrepared {
		workspace_id: String,
		project_id: String,
		worktree_path: String,
	},
	WorkspaceReady {
		workspace_id: String,
		project_id: String,
		worktree_path: String,
	},
}

#[async_trait]
pub trait EventBus: Send + Sync {
	async fn publish(&self, event: CoreEvent) -> Result<()>;
}
