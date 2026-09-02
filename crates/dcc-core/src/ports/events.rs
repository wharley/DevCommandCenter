use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};
use crate::domain::{
    mcp::McpRuntimeStatus,
    provider::NativeSubagentStatus,
    session::{AssistantMessagePhase, SessionEventKind, SessionEventRecord},
};
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
        model: Option<String>,
        #[serde(default)]
        evidence: Option<crate::domain::session::TurnEvidenceSummary>,
        #[serde(default)]
        retry_of_turn_id: Option<String>,
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
    SessionTurnNativeSubagentActivity {
        session_id: String,
        turn_id: String,
        id: String,
        agent_id: Option<String>,
        agent_thread_id: Option<String>,
        path: Option<String>,
        name: Option<String>,
        role: Option<String>,
        model: Option<String>,
        status: NativeSubagentStatus,
    },
    SessionTurnNativeSubagentModelConfirmed {
        session_id: String,
        turn_id: String,
        correlation_id: String,
        model: String,
    },
    SessionTurnNativeSubagentModelRequested {
        session_id: String,
        turn_id: String,
        correlation_id: String,
        model: String,
    },
    SessionTurnModelEffective {
        session_id: String,
        turn_id: String,
        model: String,
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

impl CoreEvent {
    /// Returns the session that owns this event. Workspace events deliberately
    /// return `None`: they stay on the legacy event transport until they have
    /// their own live-envelope contract.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::WorkspacePrepared { .. } | Self::WorkspaceReady { .. } => None,
            Self::SessionStarted { session_id, .. }
            | Self::SessionCompleted { session_id }
            | Self::SessionAborted { session_id, .. }
            | Self::SessionResumed { session_id }
            | Self::SessionMcpRuntimeStatusChanged { session_id, .. }
            | Self::SessionTurnStarted { session_id, .. }
            | Self::SessionTurnSteered { session_id, .. }
            | Self::SessionTurnQueued { session_id, .. }
            | Self::SessionQueuedTurnRemoved { session_id, .. }
            | Self::SessionTurnQueueReordered { session_id, .. }
            | Self::SessionQueuedTurnDispatched { session_id, .. }
            | Self::SessionTurnDelta { session_id, .. }
            | Self::SessionTurnAssistantMessageStarted { session_id, .. }
            | Self::SessionTurnAssistantMessageDelta { session_id, .. }
            | Self::SessionTurnAssistantMessageCompleted { session_id, .. }
            | Self::SessionTurnReasoningStarted { session_id, .. }
            | Self::SessionTurnReasoningDelta { session_id, .. }
            | Self::SessionTurnReasoningCompleted { session_id, .. }
            | Self::SessionTurnToolCallStarted { session_id, .. }
            | Self::SessionTurnToolCallDelta { session_id, .. }
            | Self::SessionTurnToolCallCompleted { session_id, .. }
            | Self::SessionTurnToolCallFailed { session_id, .. }
            | Self::SessionTurnUserInputRequested { session_id, .. }
            | Self::SessionTurnUserInputResolved { session_id, .. }
            | Self::SessionTurnPermissionRequested { session_id, .. }
            | Self::SessionTurnPermissionResolved { session_id, .. }
            | Self::SessionTurnNativeSubagentActivity { session_id, .. }
            | Self::SessionTurnNativeSubagentModelConfirmed { session_id, .. }
            | Self::SessionTurnNativeSubagentModelRequested { session_id, .. }
            | Self::SessionTurnModelEffective { session_id, .. }
            | Self::SessionTurnCompleted { session_id, .. }
            | Self::SessionTurnAborted { session_id, .. }
            | Self::SessionCheckpointCreated { session_id, .. }
            | Self::SessionPlanApproved { session_id, .. }
            | Self::SessionPlanHandedOff { session_id, .. }
            | Self::SessionDelegationRequested { session_id, .. }
            | Self::SessionDelegationStarted { session_id, .. }
            | Self::SessionDelegationDelta { session_id, .. }
            | Self::SessionDelegationCompleted { session_id, .. }
            | Self::SessionDelegationFailed { session_id, .. }
            | Self::SessionDelegationCancelled { session_id, .. } => Some(session_id),
        }
    }

    /// Returns true only when this legacy event is the exact public projection
    /// of the canonical SQLite record. Durable fanout must never invent an
    /// identity from an input event, so a mismatch is rejected before either
    /// legacy or live transports observe it.
    pub fn matches_session_record(&self, record: &SessionEventRecord) -> bool {
        if self.session_id() != Some(record.session_id.0.as_str()) {
            return false;
        }

        match (&record.kind, self) {
            (
                SessionEventKind::SessionStarted {
                    workspace_id,
                    project_id,
                    provider_id,
                    model,
                },
                Self::SessionStarted {
                    workspace_id: actual_workspace_id,
                    project_id: actual_project_id,
                    provider_id: actual_provider_id,
                    model: actual_model,
                    ..
                },
            ) => {
                workspace_id.0 == *actual_workspace_id
                    && project_id.0 == *actual_project_id
                    && provider_id == actual_provider_id
                    && model == actual_model
            }
            (
                SessionEventKind::TurnStarted {
                    turn_id,
                    prompt,
                    plan_mode,
                    model,
                    evidence,
                    retry_of_turn_id,
                },
                Self::SessionTurnStarted {
                    turn_id: actual_turn_id,
                    prompt: actual_prompt,
                    plan_mode: actual_plan_mode,
                    model: actual_model,
                    evidence: actual_evidence,
                    retry_of_turn_id: actual_retry_of_turn_id,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && prompt == actual_prompt
                    && plan_mode == actual_plan_mode
                    && model == actual_model
                    && evidence == actual_evidence
                    && retry_of_turn_id.as_ref().map(|id| id.0.as_str())
                        == actual_retry_of_turn_id.as_deref()
            }
            (
                SessionEventKind::TurnSteered { turn_id, prompt },
                Self::SessionTurnSteered {
                    turn_id: actual_turn_id,
                    prompt: actual_prompt,
                    ..
                },
            ) => turn_id.0 == *actual_turn_id && prompt == actual_prompt,
            (
                SessionEventKind::TurnQueued { queued_turn },
                Self::SessionTurnQueued {
                    queued_turn: actual_queued_turn,
                    ..
                },
            ) => queued_turn == actual_queued_turn,
            (
                SessionEventKind::QueuedTurnRemoved { queued_turn_id },
                Self::SessionQueuedTurnRemoved {
                    queued_turn_id: actual_queued_turn_id,
                    ..
                },
            ) => queued_turn_id == actual_queued_turn_id,
            (
                SessionEventKind::TurnQueueReordered { queued_turn_ids },
                Self::SessionTurnQueueReordered {
                    queued_turn_ids: actual_queued_turn_ids,
                    ..
                },
            ) => queued_turn_ids == actual_queued_turn_ids,
            (
                SessionEventKind::QueuedTurnDispatched {
                    queued_turn_id,
                    turn_id,
                },
                Self::SessionQueuedTurnDispatched {
                    queued_turn_id: actual_queued_turn_id,
                    turn_id: actual_turn_id,
                    ..
                },
            ) => queued_turn_id == actual_queued_turn_id && turn_id.0 == *actual_turn_id,
            (
                SessionEventKind::TurnDelta { turn_id, content },
                Self::SessionTurnDelta {
                    turn_id: actual_turn_id,
                    content: actual_content,
                    ..
                },
            ) => turn_id.0 == *actual_turn_id && content == actual_content,
            (
                SessionEventKind::TurnAssistantMessageStarted {
                    turn_id,
                    message_id,
                    phase,
                },
                Self::SessionTurnAssistantMessageStarted {
                    turn_id: actual_turn_id,
                    message_id: actual_message_id,
                    phase: actual_phase,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && message_id == actual_message_id
                    && phase == actual_phase
            }
            (
                SessionEventKind::TurnAssistantMessageDelta {
                    turn_id,
                    message_id,
                    content,
                },
                Self::SessionTurnAssistantMessageDelta {
                    turn_id: actual_turn_id,
                    message_id: actual_message_id,
                    content: actual_content,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && message_id == actual_message_id
                    && content == actual_content
            }
            (
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id,
                    message_id,
                    phase,
                    content,
                },
                Self::SessionTurnAssistantMessageCompleted {
                    turn_id: actual_turn_id,
                    message_id: actual_message_id,
                    phase: actual_phase,
                    content: actual_content,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && message_id == actual_message_id
                    && phase == actual_phase
                    && content == actual_content
            }
            (
                SessionEventKind::TurnReasoningStarted {
                    turn_id,
                    reasoning_id,
                    label,
                },
                Self::SessionTurnReasoningStarted {
                    turn_id: actual_turn_id,
                    reasoning_id: actual_reasoning_id,
                    label: actual_label,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && reasoning_id == actual_reasoning_id
                    && label == actual_label
            }
            (
                SessionEventKind::TurnReasoningDelta {
                    turn_id,
                    reasoning_id,
                    content,
                },
                Self::SessionTurnReasoningDelta {
                    turn_id: actual_turn_id,
                    reasoning_id: actual_reasoning_id,
                    content: actual_content,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && reasoning_id == actual_reasoning_id
                    && content == actual_content
            }
            (
                SessionEventKind::TurnReasoningCompleted {
                    turn_id,
                    reasoning_id,
                },
                Self::SessionTurnReasoningCompleted {
                    turn_id: actual_turn_id,
                    reasoning_id: actual_reasoning_id,
                    ..
                },
            ) => turn_id.0 == *actual_turn_id && reasoning_id == actual_reasoning_id,
            (
                SessionEventKind::TurnToolCallStarted {
                    turn_id,
                    tool_call_id,
                    action,
                    command,
                    file,
                },
                Self::SessionTurnToolCallStarted {
                    turn_id: actual_turn_id,
                    tool_call_id: actual_tool_call_id,
                    action: actual_action,
                    command: actual_command,
                    file: actual_file,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && tool_call_id == actual_tool_call_id
                    && action == actual_action
                    && command == actual_command
                    && file == actual_file
            }
            (
                SessionEventKind::TurnToolCallDelta {
                    turn_id,
                    tool_call_id,
                    content,
                },
                Self::SessionTurnToolCallDelta {
                    turn_id: actual_turn_id,
                    tool_call_id: actual_tool_call_id,
                    content: actual_content,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && tool_call_id == actual_tool_call_id
                    && content == actual_content
            }
            (
                SessionEventKind::TurnToolCallCompleted {
                    turn_id,
                    tool_call_id,
                },
                Self::SessionTurnToolCallCompleted {
                    turn_id: actual_turn_id,
                    tool_call_id: actual_tool_call_id,
                    ..
                },
            ) => turn_id.0 == *actual_turn_id && tool_call_id == actual_tool_call_id,
            (
                SessionEventKind::TurnToolCallFailed {
                    turn_id,
                    tool_call_id,
                    reason,
                },
                Self::SessionTurnToolCallFailed {
                    turn_id: actual_turn_id,
                    tool_call_id: actual_tool_call_id,
                    reason: actual_reason,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && tool_call_id == actual_tool_call_id
                    && reason == actual_reason
            }
            (
                SessionEventKind::TurnUserInputRequested {
                    turn_id,
                    request_id,
                    questions,
                },
                Self::SessionTurnUserInputRequested {
                    turn_id: actual_turn_id,
                    request_id: actual_request_id,
                    questions: actual_questions,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && request_id == actual_request_id
                    && questions == actual_questions
            }
            (
                SessionEventKind::TurnUserInputResolved {
                    turn_id,
                    request_id,
                    answers,
                },
                Self::SessionTurnUserInputResolved {
                    turn_id: actual_turn_id,
                    request_id: actual_request_id,
                    answers: actual_answers,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && request_id == actual_request_id
                    && answers == actual_answers
            }
            (
                SessionEventKind::TurnPermissionRequested {
                    turn_id,
                    request_id,
                    tool_name,
                    title,
                    description,
                    command,
                    file,
                },
                Self::SessionTurnPermissionRequested {
                    turn_id: actual_turn_id,
                    request_id: actual_request_id,
                    tool_name: actual_tool_name,
                    title: actual_title,
                    description: actual_description,
                    command: actual_command,
                    file: actual_file,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && request_id == actual_request_id
                    && tool_name == actual_tool_name
                    && title == actual_title
                    && description == actual_description
                    && command == actual_command
                    && file == actual_file
            }
            (
                SessionEventKind::TurnPermissionResolved {
                    turn_id,
                    request_id,
                    behavior,
                },
                Self::SessionTurnPermissionResolved {
                    turn_id: actual_turn_id,
                    request_id: actual_request_id,
                    behavior: actual_behavior,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && request_id == actual_request_id
                    && behavior == actual_behavior
            }
            (
                SessionEventKind::TurnNativeSubagentActivity {
                    turn_id,
                    id,
                    agent_id,
                    agent_thread_id,
                    path,
                    name,
                    role,
                    model,
                    status,
                },
                Self::SessionTurnNativeSubagentActivity {
                    turn_id: actual_turn_id,
                    id: actual_id,
                    agent_id: actual_agent_id,
                    agent_thread_id: actual_agent_thread_id,
                    path: actual_path,
                    name: actual_name,
                    role: actual_role,
                    model: actual_model,
                    status: actual_status,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && id == actual_id
                    && agent_id == actual_agent_id
                    && agent_thread_id == actual_agent_thread_id
                    && path == actual_path
                    && name == actual_name
                    && role == actual_role
                    && model == actual_model
                    && status == actual_status
            }
            (
                SessionEventKind::TurnNativeSubagentModelConfirmed {
                    turn_id,
                    correlation_id,
                    model,
                },
                Self::SessionTurnNativeSubagentModelConfirmed {
                    turn_id: actual_turn_id,
                    correlation_id: actual_correlation_id,
                    model: actual_model,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && correlation_id == actual_correlation_id
                    && model == actual_model
            }
            (
                SessionEventKind::TurnNativeSubagentModelRequested {
                    turn_id,
                    correlation_id,
                    model,
                },
                Self::SessionTurnNativeSubagentModelRequested {
                    turn_id: actual_turn_id,
                    correlation_id: actual_correlation_id,
                    model: actual_model,
                    ..
                },
            ) => {
                turn_id.0 == *actual_turn_id
                    && correlation_id == actual_correlation_id
                    && model == actual_model
            }
            (
                SessionEventKind::TurnModelEffective { turn_id, model },
                Self::SessionTurnModelEffective {
                    turn_id: actual_turn_id,
                    model: actual_model,
                    ..
                },
            ) => turn_id.0 == *actual_turn_id && model == actual_model,
            (
                SessionEventKind::TurnCompleted { turn_id },
                Self::SessionTurnCompleted {
                    turn_id: actual_turn_id,
                    ..
                },
            ) => turn_id.0 == *actual_turn_id,
            (
                SessionEventKind::TurnAborted { turn_id, reason },
                Self::SessionTurnAborted {
                    turn_id: actual_turn_id,
                    reason: actual_reason,
                    ..
                },
            ) => turn_id.0 == *actual_turn_id && reason == actual_reason,
            (
                SessionEventKind::CheckpointCreated {
                    checkpoint_id,
                    label,
                },
                Self::SessionCheckpointCreated {
                    checkpoint_id: actual_checkpoint_id,
                    label: actual_label,
                    ..
                },
            ) => checkpoint_id.0 == *actual_checkpoint_id && label == actual_label,
            (
                SessionEventKind::PlanApproved {
                    plan_message_id,
                    plan_version,
                    plan_hash,
                },
                Self::SessionPlanApproved {
                    plan_message_id: actual_plan_message_id,
                    plan_version: actual_plan_version,
                    plan_hash: actual_plan_hash,
                    ..
                },
            ) => {
                plan_message_id == actual_plan_message_id
                    && plan_version == actual_plan_version
                    && plan_hash == actual_plan_hash
            }
            (
                SessionEventKind::PlanHandedOff {
                    plan_message_id,
                    plan_version,
                    plan_hash,
                    action,
                    target_session_id,
                },
                Self::SessionPlanHandedOff {
                    plan_message_id: actual_plan_message_id,
                    plan_version: actual_plan_version,
                    plan_hash: actual_plan_hash,
                    action: actual_action,
                    target_session_id: actual_target_session_id,
                    ..
                },
            ) => {
                plan_message_id == actual_plan_message_id
                    && plan_version == actual_plan_version
                    && plan_hash == actual_plan_hash
                    && action == actual_action
                    && target_session_id.as_ref().map(|id| &id.0)
                        == actual_target_session_id.as_ref()
            }
            (
                SessionEventKind::DelegationRequested { delegation_id },
                Self::SessionDelegationRequested {
                    delegation_id: actual_delegation_id,
                    ..
                },
            ) => delegation_id.0 == *actual_delegation_id,
            (
                SessionEventKind::DelegationStarted {
                    delegation_id,
                    child_session_id,
                },
                Self::SessionDelegationStarted {
                    delegation_id: actual_delegation_id,
                    child_session_id: actual_child_session_id,
                    ..
                },
            ) => {
                delegation_id.0 == *actual_delegation_id
                    && child_session_id.as_ref().map(|id| &id.0) == actual_child_session_id.as_ref()
            }
            (
                SessionEventKind::DelegationDelta {
                    delegation_id,
                    content,
                },
                Self::SessionDelegationDelta {
                    delegation_id: actual_delegation_id,
                    content: actual_content,
                    ..
                },
            ) => delegation_id.0 == *actual_delegation_id && content == actual_content,
            (
                SessionEventKind::DelegationCompleted {
                    delegation_id,
                    summary,
                },
                Self::SessionDelegationCompleted {
                    delegation_id: actual_delegation_id,
                    summary: actual_summary,
                    ..
                },
            ) => delegation_id.0 == *actual_delegation_id && summary == actual_summary,
            (
                SessionEventKind::DelegationFailed {
                    delegation_id,
                    reason,
                },
                Self::SessionDelegationFailed {
                    delegation_id: actual_delegation_id,
                    reason: actual_reason,
                    ..
                },
            ) => delegation_id.0 == *actual_delegation_id && reason == actual_reason,
            (
                SessionEventKind::DelegationCancelled {
                    delegation_id,
                    reason,
                },
                Self::SessionDelegationCancelled {
                    delegation_id: actual_delegation_id,
                    reason: actual_reason,
                    ..
                },
            ) => delegation_id.0 == *actual_delegation_id && reason == actual_reason,
            (SessionEventKind::SessionCompleted, Self::SessionCompleted { .. }) => true,
            (
                SessionEventKind::SessionAborted { reason },
                Self::SessionAborted {
                    reason: actual_reason,
                    ..
                },
            ) => reason == actual_reason,
            (SessionEventKind::SessionResumed, Self::SessionResumed { .. }) => true,
            _ => false,
        }
    }
}

/// Canonical durable identity, supplied only after SQLite has accepted the
/// session event. It is intentionally absent for runtime-only session events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionLiveDurableIdentity {
    pub session_id: String,
    pub event_id: String,
    pub sequence: u64,
}

/// Additive live transport envelope for session events.
///
/// `runtime_generation` and `runtime_sequence` are process-local identity
/// hints, not durable ordering. Consumers must use `durable` to reconcile a
/// hydrated session transcript and must not infer a global session order from
/// `runtime_sequence`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionLiveEventEnvelope {
    pub runtime_generation: String,
    pub runtime_sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable: Option<SessionLiveDurableIdentity>,
    pub event: CoreEvent,
}

/// The largest integer represented exactly by JavaScript's `Number`.
/// Runtime live sequencing fails closed after this value; legacy fanout stays
/// available so a pathological long-lived process does not silently duplicate
/// or reorder sequence values in the renderer.
pub const MAX_SESSION_LIVE_RUNTIME_SEQUENCE: u64 = 9_007_199_254_740_991;

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: CoreEvent) -> Result<()>;

    /// Publishes a durable session event together with the canonical record
    /// returned by its repository. The default keeps existing adapters and
    /// mocks source-compatible while runtimes that support live envelopes can
    /// preserve durable identity.
    async fn publish_durable_session(
        &self,
        _record: &SessionEventRecord,
        event: CoreEvent,
    ) -> Result<()> {
        self.publish(event).await
    }

    /// Optional additive session-live transport. Existing provider adapters
    /// and test buses retain their legacy `CoreEvent` contract unchanged.
    async fn publish_session_live(&self, _event: SessionLiveEventEnvelope) -> Result<()> {
        Ok(())
    }
}
