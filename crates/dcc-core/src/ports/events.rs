use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
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
	SessionStarted {
		session_id: String,
		workspace_id: String,
		project_id: String,
		provider_id: String,
	},
	SessionCompleted {
		session_id: String,
	},
	SessionAborted {
		session_id: String,
		reason: Option<String>,
	},
	SessionResumed {
		session_id: String,
	},
	SessionTurnStarted {
		session_id: String,
		turn_id: String,
		prompt: String,
	},
	SessionTurnDelta {
		session_id: String,
		turn_id: String,
		content: String,
	},
	SessionTurnCompleted {
		session_id: String,
		turn_id: String,
	},
	SessionTurnAborted {
		session_id: String,
		turn_id: String,
		reason: Option<String>,
	},
	SessionCheckpointCreated {
		session_id: String,
		checkpoint_id: String,
		label: String,
	},
}

#[async_trait]
pub trait EventBus: Send + Sync {
	async fn publish(&self, event: CoreEvent) -> Result<()>;
}
