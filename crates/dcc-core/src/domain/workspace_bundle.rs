use serde::{Deserialize, Serialize};
use specta::Type;

use super::workspace::{WorkspaceId, WorkspaceState};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct WorkspaceBundleId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBundleState {
    Ready,
    Archived,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBundle {
    pub id: WorkspaceBundleId,
    pub name: String,
    pub primary_workspace_id: WorkspaceId,
    pub state: WorkspaceBundleState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBundleMember {
    pub bundle_id: WorkspaceBundleId,
    pub workspace_id: WorkspaceId,
    pub created_for_bundle: bool,
    pub position: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBundleSummary {
    pub bundle: WorkspaceBundle,
    pub members: Vec<WorkspaceBundleMember>,
}

impl WorkspaceBundleState {
    pub fn from_workspace_state(state: &WorkspaceState) -> Self {
        match state {
            WorkspaceState::Archived => Self::Archived,
            WorkspaceState::Initializing | WorkspaceState::SetupPending | WorkspaceState::Ready => {
                Self::Ready
            }
        }
    }
}
