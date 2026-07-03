use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    provider::ProviderId,
    session::{SessionId, TurnId},
    workspace::WorkspaceId,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct DelegationId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DelegationMode {
    Review,
    Implement,
    Explain,
    Test,
    Research,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DelegationStatus {
    Draft,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DelegationContextPolicy {
    Minimal,
    ReviewCurrentDiff,
    SpecPlan,
    SelectedFiles { paths: Vec<String> },
    FullReanchor,
}

impl Default for DelegationContextPolicy {
    fn default() -> Self {
        Self::Minimal
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DelegationBudget {
    pub turn_limit: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub allow_file_edits: bool,
}

impl Default for DelegationBudget {
    fn default() -> Self {
        Self {
            turn_limit: Some(1),
            timeout_seconds: Some(600),
            allow_file_edits: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Delegation {
    pub id: DelegationId,
    pub parent_session_id: SessionId,
    pub parent_turn_id: Option<TurnId>,
    pub child_session_id: Option<SessionId>,
    pub workspace_id: WorkspaceId,
    pub target_provider_id: ProviderId,
    pub mode: DelegationMode,
    pub status: DelegationStatus,
    pub prompt: String,
    pub context_policy: DelegationContextPolicy,
    pub budget: DelegationBudget,
    pub result_summary: Option<String>,
    #[serde(default)]
    pub touched_files: Vec<String>,
    pub diff_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
