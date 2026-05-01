use serde::{Deserialize, Serialize};
use specta::Type;

use super::project::ProjectId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct WorkspaceId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceState {
	Initializing,
	SetupPending,
	Ready,
	Archived,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
	pub id: WorkspaceId,
	pub project_id: ProjectId,
	pub name: Option<String>,
	pub root_path: String,
	pub base_branch: String,
	pub worktree_path: Option<String>,
	pub state: WorkspaceState,
	pub created_at: String,
	pub updated_at: String,
}
