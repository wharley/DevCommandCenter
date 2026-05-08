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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSetupStatus {
    Skipped,
    Completed,
    Warning,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSetupStepReport {
    pub label: String,
    pub command: String,
    pub source_path: String,
    pub status: WorkspaceSetupStatus,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSetupReport {
    pub status: WorkspaceSetupStatus,
    pub steps: Vec<WorkspaceSetupStepReport>,
    pub message: Option<String>,
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
    pub setup_report: Option<WorkspaceSetupReport>,
    pub created_at: String,
    pub updated_at: String,
}
