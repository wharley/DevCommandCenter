//! A session's conversation as the person reads it: the durable events folded
//! into rows — the person's prompts, one assistant row per turn with its
//! activity (commentary, reasoning, tool calls, questions, approvals, native
//! subagents), delegation cards and system notes. The same fold as the
//! desktop's `session-thread-history.logic.ts` (`projectWorkspaceMessages`),
//! so every client shows the same conversation.

pub mod activity;
mod events;
mod fold;
mod hand_back;
mod pending;
mod project;
mod summary;
#[cfg(test)]
mod tests;

use crate::domain::provider::{NativeSubagentStatus, ToolCallDetail};
use crate::domain::session::{AssistantMessagePhase, SessionEventRecord, TurnEvidenceSummary};
use crate::ports::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};

pub use hand_back::{parse_delegation_hand_back, DelegationHandBack, HandBackOutcome};
pub use pending::pending_permission_requests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A turn that stopped before it finished.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IncompleteStatus {
    pub reason: Option<String>,
    /// The plan's usage limit stopped it: a wait, not an error.
    pub usage_limited: bool,
    /// After that limit the task moved to this account (empty: the system
    /// login) and continued there.
    pub continued_on_account: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub action: String,
    pub command: Option<String>,
    pub file: Option<String>,
    /// Streamed output (stdout/progress). Never raw tool input.
    pub content: String,
    /// Provider-reported facts: final input, result output, exit code, diff.
    pub detail: Option<ToolCallDetail>,
    pub streaming: bool,
    pub created_at: Option<String>,
    /// `Some` when it failed, with the reason.
    pub failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSubagent {
    pub id: String,
    pub agent_id: Option<String>,
    pub agent_thread_id: Option<String>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub model: Option<String>,
    pub requested_model: Option<String>,
    pub status: NativeSubagentStatus,
    pub streaming: bool,
    pub created_at: Option<String>,
}

/// What happened inside an assistant turn, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Annotation {
    Commentary {
        id: String,
        content: String,
        streaming: bool,
        created_at: Option<String>,
    },
    Reasoning {
        id: String,
        label: Option<String>,
        content: String,
        streaming: bool,
        created_at: Option<String>,
    },
    ToolCall(ToolCall),
    UserInput {
        id: String,
        questions: Vec<ProviderUserInputQuestion>,
        answers: Vec<ProviderUserInputAnswer>,
        streaming: bool,
        created_at: Option<String>,
    },
    Approval {
        id: String,
        tool_name: String,
        title: Option<String>,
        description: Option<String>,
        command: Option<String>,
        file: Option<String>,
        behavior: Option<String>,
        streaming: bool,
        created_at: Option<String>,
    },
    NativeSubagent(NativeSubagent),
}

impl Annotation {
    pub fn id(&self) -> &str {
        match self {
            Self::Commentary { id, .. }
            | Self::Reasoning { id, .. }
            | Self::UserInput { id, .. }
            | Self::Approval { id, .. } => id,
            Self::ToolCall(call) => &call.id,
            Self::NativeSubagent(agent) => &agent.id,
        }
    }

    fn kind(&self) -> u8 {
        match self {
            Self::Commentary { .. } => 0,
            Self::Reasoning { .. } => 1,
            Self::ToolCall(_) => 2,
            Self::UserInput { .. } => 3,
            Self::Approval { .. } => 4,
            Self::NativeSubagent(_) => 5,
        }
    }

    fn set_streaming(&mut self, value: bool) {
        match self {
            Self::Commentary { streaming, .. }
            | Self::Reasoning { streaming, .. }
            | Self::UserInput { streaming, .. }
            | Self::Approval { streaming, .. } => *streaming = value,
            Self::ToolCall(call) => call.streaming = value,
            Self::NativeSubagent(agent) => agent.streaming = value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelegationPhase {
    Requested,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineDelegation {
    pub id: String,
    pub phase: DelegationPhase,
    pub child_session_id: Option<String>,
    pub summary: Option<String>,
    pub reason: Option<String>,
}

/// A row's button that opens another conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenSession {
    pub session_id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineMessage {
    pub id: String,
    pub role: Role,
    pub turn_id: Option<String>,
    pub turn_settled: bool,
    /// Durable start/end of the turn an assistant row belongs to.
    pub turn_started_at: Option<String>,
    pub turn_ended_at: Option<String>,
    pub assistant_phase: Option<AssistantMessagePhase>,
    pub content: String,
    pub label: String,
    pub model: Option<String>,
    pub streaming: bool,
    pub created_at: Option<String>,
    pub status: Option<IncompleteStatus>,
    pub annotations: Vec<Annotation>,
    pub plan_mode: bool,
    /// An assistant row answering a plan-mode prompt (its text may be a plan).
    pub answers_plan_mode: bool,
    /// Metadata-only evidence linkage recorded with the turn.
    pub evidence: Option<TurnEvidenceSummary>,
    /// The aborted turn this user turn re-ran.
    pub retry_of_turn_id: Option<String>,
    pub action: Option<OpenSession>,
    pub delegation: Option<TimelineDelegation>,
    /// The turn a delegation card belongs after: the one that requested it.
    pub delegation_turn_id: Option<String>,
    /// A `[DCC]` turn the backend wrote for the agent, not the person.
    pub hand_back: Option<DelegationHandBack>,
}

impl TimelineMessage {
    fn new(id: String, role: Role, label: &str, content: String) -> Self {
        Self {
            id,
            role,
            turn_id: None,
            turn_settled: false,
            turn_started_at: None,
            turn_ended_at: None,
            assistant_phase: None,
            content,
            label: label.to_string(),
            model: None,
            streaming: false,
            created_at: None,
            status: None,
            annotations: Vec::new(),
            plan_mode: false,
            answers_plan_mode: false,
            evidence: None,
            retry_of_turn_id: None,
            action: None,
            delegation: None,
            delegation_turn_id: None,
            hand_back: None,
        }
    }
}

/// The conversation of `session_id` from its durable events, in order.
pub fn project_session(events: &[SessionEventRecord], session_id: &str) -> Vec<TimelineMessage> {
    let timeline = events::timeline(events, session_id);
    project::project(&timeline, session_id)
}
