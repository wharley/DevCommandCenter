use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};
use crate::domain::{mcp::McpRuntimeStatus, session::AssistantMessagePhase};
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
        model: Option<String>,
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
    /// Ephemeral runtime truth. This event is not appended to the durable
    /// session transcript.
    SessionMcpRuntimeStatusChanged {
        session_id: String,
        statuses: Vec<McpRuntimeStatus>,
    },
    SessionTurnStarted {
        session_id: String,
        turn_id: String,
        prompt: String,
        plan_mode: Option<bool>,
    },
    SessionTurnSteered {
        session_id: String,
        turn_id: String,
        prompt: String,
    },
    SessionTurnQueued {
        session_id: String,
        queued_turn: crate::domain::session::QueuedTurn,
    },
    SessionQueuedTurnRemoved {
        session_id: String,
        queued_turn_id: String,
    },
    SessionTurnQueueReordered {
        session_id: String,
        queued_turn_ids: Vec<String>,
    },
    SessionQueuedTurnDispatched {
        session_id: String,
        queued_turn_id: String,
        turn_id: String,
    },
    SessionTurnDelta {
        session_id: String,
        turn_id: String,
        content: String,
    },
    SessionTurnAssistantMessageStarted {
        session_id: String,
        turn_id: String,
        message_id: String,
        phase: AssistantMessagePhase,
    },
    SessionTurnAssistantMessageDelta {
        session_id: String,
        turn_id: String,
        message_id: String,
        content: String,
    },
    SessionTurnAssistantMessageCompleted {
        session_id: String,
        turn_id: String,
        message_id: String,
        phase: AssistantMessagePhase,
        content: Option<String>,
    },
    SessionTurnReasoningStarted {
        session_id: String,
        turn_id: String,
        reasoning_id: String,
        label: Option<String>,
    },
    SessionTurnReasoningDelta {
        session_id: String,
        turn_id: String,
        reasoning_id: String,
        content: String,
    },
    SessionTurnReasoningCompleted {
        session_id: String,
        turn_id: String,
        reasoning_id: String,
    },
    SessionTurnToolCallStarted {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
        action: String,
        command: Option<String>,
        file: Option<String>,
    },
    SessionTurnToolCallDelta {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
        content: String,
    },
    SessionTurnToolCallCompleted {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
    },
    SessionTurnToolCallFailed {
        session_id: String,
        turn_id: String,
        tool_call_id: String,
        reason: Option<String>,
    },
    SessionTurnUserInputRequested {
        session_id: String,
        turn_id: String,
        request_id: String,
        questions: Vec<ProviderUserInputQuestion>,
    },
    SessionTurnUserInputResolved {
        session_id: String,
        turn_id: String,
        request_id: String,
        answers: Vec<ProviderUserInputAnswer>,
    },
    SessionTurnPermissionRequested {
        session_id: String,
        turn_id: String,
        request_id: String,
        tool_name: String,
        title: Option<String>,
        description: Option<String>,
        command: Option<String>,
        file: Option<String>,
    },
    SessionTurnPermissionResolved {
        session_id: String,
        turn_id: String,
        request_id: String,
        behavior: String,
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
    SessionPlanApproved {
        session_id: String,
        plan_message_id: String,
        plan_version: u32,
        plan_hash: String,
    },
    SessionPlanHandedOff {
        session_id: String,
        plan_message_id: String,
        plan_version: u32,
        plan_hash: String,
        action: String,
        target_session_id: Option<String>,
    },
    SessionDelegationRequested {
        session_id: String,
        delegation_id: String,
    },
    SessionDelegationStarted {
        session_id: String,
        delegation_id: String,
        child_session_id: Option<String>,
    },
    SessionDelegationDelta {
        session_id: String,
        delegation_id: String,
        content: String,
    },
    SessionDelegationCompleted {
        session_id: String,
        delegation_id: String,
        summary: Option<String>,
    },
    SessionDelegationFailed {
        session_id: String,
        delegation_id: String,
        reason: Option<String>,
    },
    SessionDelegationCancelled {
        session_id: String,
        delegation_id: String,
        reason: Option<String>,
    },
}

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: CoreEvent) -> Result<()>;
}
