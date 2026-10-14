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
    ReviewPending,
    Completed,
    Failed,
    Cancelled,
}

/// Who asked for the delegation. An agent-initiated delegation (the
/// `dcc_delegate_task` tool) hands its result back to the parent agent as a
/// turn; a person-initiated one only reports it in the thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DelegationOrigin {
    #[default]
    Person,
    Agent,
}

/// What the record is. A delegation is a bounded helper run whose result goes
/// back to the parent (`dcc_delegate_task`); a task is a conversation of its
/// own that the parent proposed (`dcc_propose_task`) and the person started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DelegationKind {
    #[default]
    Delegation,
    Task,
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
    /// Permission level the child actually runs with, never above the
    /// parent's. `None` when the target provider takes no policy at all (the
    /// read-only scope is then an instruction, not a guarantee).
    #[serde(default)]
    pub approval_policy: Option<super::provider::ProviderApprovalPolicy>,
    /// A task whose edits land straight in the parent's checkout: no
    /// worktree, nothing to review or apply. Only for tasks, by the person's
    /// choice (it follows a parent that runs without a worktree).
    #[serde(default)]
    pub shared_checkout: bool,
}

impl Default for DelegationBudget {
    fn default() -> Self {
        Self {
            turn_limit: Some(1),
            timeout_seconds: Some(600),
            allow_file_edits: false,
            approval_policy: None,
            shared_checkout: false,
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
    #[serde(default)]
    pub target_model_id: Option<String>,
    pub mode: DelegationMode,
    pub status: DelegationStatus,
    pub prompt: String,
    pub context_policy: DelegationContextPolicy,
    pub budget: DelegationBudget,
    pub result_summary: Option<String>,
    #[serde(default)]
    pub touched_files: Vec<String>,
    pub diff_summary: Option<String>,
    pub validation_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub origin: DelegationOrigin,
    /// The task as the requester wrote it, without the context DCC wraps
    /// around it in `prompt`.
    #[serde(default)]
    pub instruction: Option<String>,
    /// First transition to `Running`; the timeout budget counts from here.
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub kind: DelegationKind,
    /// The short name the requester gave the work; a task's proposal card
    /// shows it before any child conversation exists.
    #[serde(default)]
    pub title: Option<String>,
    /// A task's child turn already handed back to the parent, so a turn that
    /// ended is reported once however many signals announce it.
    #[serde(default)]
    pub reported_turn_id: Option<TurnId>,
}
