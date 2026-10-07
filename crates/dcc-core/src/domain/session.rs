use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

use super::{
    project::ProjectId, provider::ProviderApprovalPolicy, thread::Thread, workspace::WorkspaceId,
};
use crate::ports::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};
use crate::ports::ProviderRuntimeConfig;

use super::delegation::DelegationId;
use super::provider::{NativeSubagentStatus, ToolCallDetail};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct SessionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct TurnId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct CheckpointId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Draft,
    Active,
    Completed,
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Running,
    Completed,
    Aborted,
}

/// Provider-neutral semantic role of an assistant message inside a turn.
/// Providers with a native distinction preserve it; adapters without one use
/// `Unknown` and let the timeline select the last completed message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AssistantMessagePhase {
    Commentary,
    FinalAnswer,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: TurnId,
    pub session_id: SessionId,
    pub role: String,
    pub content: String,
    pub state: TurnState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub session_id: SessionId,
    pub label: String,
    pub created_at: String,
}

pub const MAX_TURN_EVIDENCE_ITEMS: usize = 8;
pub const MAX_TURN_EVIDENCE_ITEM_CHARS: u32 = 32_000;

/// Debug stage the person selected for the evidence carried by a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnEvidenceStage {
    Observe,
    Reproduce,
    Investigate,
    Fix,
    Verify,
}

/// Closed vocabulary of explicit evidence sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnEvidenceSource {
    Browser,
    Terminal,
    Diff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnEvidenceTrust {
    RemoteUntrusted,
    LocalTerminal,
    LocalWorkspace,
}

/// One evidence item as metadata only: no body, URL, path, ref, note or text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnEvidenceItemSummary {
    pub source: TurnEvidenceSource,
    pub trust: TurnEvidenceTrust,
    pub chars: u32,
    pub truncated: bool,
}

/// Metadata-only linkage between a turn and the explicit evidence it carried.
/// The evidence bodies already live inside the prompt text; this record lets
/// the timeline explain what was attached and why without duplicating or
/// retaining any content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnEvidenceSummary {
    pub stage: TurnEvidenceStage,
    pub items: Vec<TurnEvidenceItemSummary>,
}

impl TurnEvidenceSummary {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.items.is_empty() {
            return Err("turn evidence summary has no items".to_string());
        }
        if self.items.len() > MAX_TURN_EVIDENCE_ITEMS {
            return Err(format!(
                "turn evidence summary exceeds {MAX_TURN_EVIDENCE_ITEMS} items"
            ));
        }
        for item in &self.items {
            if item.chars > MAX_TURN_EVIDENCE_ITEM_CHARS {
                return Err(format!(
                    "turn evidence item exceeds {MAX_TURN_EVIDENCE_ITEM_CHARS} chars"
                ));
            }
            let coherent = matches!(
                (item.source, item.trust),
                (
                    TurnEvidenceSource::Browser,
                    TurnEvidenceTrust::RemoteUntrusted
                ) | (
                    TurnEvidenceSource::Terminal,
                    TurnEvidenceTrust::LocalTerminal
                ) | (TurnEvidenceSource::Diff, TurnEvidenceTrust::LocalWorkspace)
            );
            if !coherent {
                return Err("turn evidence item has a mismatched source and trust".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QueuedTurn {
    pub id: String,
    pub session_id: SessionId,
    pub prompt: String,
    #[serde(default)]
    pub tool_instructions: Option<String>,
    #[serde(default)]
    pub plan_mode: Option<bool>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub fast_mode: Option<bool>,
    #[serde(default)]
    pub approval_policy: Option<ProviderApprovalPolicy>,
    #[serde(default)]
    pub evidence: Option<TurnEvidenceSummary>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: SessionId,
    pub project_id: ProjectId,
    pub workspace_id: WorkspaceId,
    /// Additional DCC-managed workspaces authorized for this session.
    /// The primary workspace remains in `workspace_id` for backwards compatibility.
    #[serde(default)]
    pub additional_workspace_ids: Vec<WorkspaceId>,
    pub provider_id: String,
    pub model: Option<String>,
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
    #[serde(default)]
    pub working_directory_override: Option<String>,
    pub state: SessionState,
    pub created_at: String,
    pub updated_at: String,
}

/// Where a forked thread came from. Metadata only: the fork snapshot itself
/// travels with the first turn and is never stored here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForkOrigin {
    pub session_id: SessionId,
    /// The user turn the fork was anchored on, when known.
    #[serde(default)]
    pub turn_id: Option<TurnId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventKind {
    SessionStarted {
        #[serde(rename = "workspaceId")]
        workspace_id: WorkspaceId,
        #[serde(rename = "projectId")]
        project_id: ProjectId,
        #[serde(rename = "providerId")]
        provider_id: String,
        model: Option<String>,
        /// Present only for threads created by fork-by-message.
        #[serde(rename = "forkedFrom", default)]
        forked_from: Option<ForkOrigin>,
    },
    TurnStarted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        prompt: String,
        #[serde(rename = "planMode", default)]
        plan_mode: Option<bool>,
        #[serde(default)]
        model: Option<String>,
        /// Metadata-only evidence linkage; absent for turns without evidence
        /// and for records persisted before this field existed.
        #[serde(default)]
        evidence: Option<TurnEvidenceSummary>,
        /// Explicit retry linkage: the aborted turn this one re-runs.
        #[serde(rename = "retryOfTurnId", default)]
        retry_of_turn_id: Option<TurnId>,
        /// The permission level the turn ran with; a delegated child is
        /// capped at its parent's latest one. Absent on older records.
        #[serde(rename = "approvalPolicy", default)]
        approval_policy: Option<ProviderApprovalPolicy>,
    },
    TurnSteered {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        prompt: String,
    },
    TurnQueued {
        #[serde(rename = "queuedTurn")]
        queued_turn: QueuedTurn,
    },
    QueuedTurnRemoved {
        #[serde(rename = "queuedTurnId")]
        queued_turn_id: String,
    },
    TurnQueueReordered {
        #[serde(rename = "queuedTurnIds")]
        queued_turn_ids: Vec<String>,
    },
    QueuedTurnDispatched {
        #[serde(rename = "queuedTurnId")]
        queued_turn_id: String,
        #[serde(rename = "turnId")]
        turn_id: TurnId,
    },
    TurnDelta {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        content: String,
    },
    TurnAssistantMessageStarted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "messageId")]
        message_id: String,
        phase: AssistantMessagePhase,
    },
    TurnAssistantMessageDelta {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "messageId")]
        message_id: String,
        content: String,
    },
    TurnAssistantMessageCompleted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "messageId")]
        message_id: String,
        phase: AssistantMessagePhase,
        /// Final provider snapshot. When present this replaces accumulated
        /// deltas and is authoritative for replay.
        content: Option<String>,
    },
    TurnReasoningStarted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "reasoningId")]
        reasoning_id: String,
        label: Option<String>,
    },
    TurnReasoningDelta {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "reasoningId")]
        reasoning_id: String,
        content: String,
    },
    TurnReasoningCompleted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "reasoningId")]
        reasoning_id: String,
    },
    TurnToolCallStarted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        action: String,
        command: Option<String>,
        file: Option<String>,
    },
    TurnToolCallDelta {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        content: String,
    },
    TurnToolCallUpdated {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        detail: ToolCallDetail,
    },
    TurnToolCallCompleted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(default)]
        detail: Option<ToolCallDetail>,
    },
    TurnToolCallFailed {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        reason: Option<String>,
        #[serde(default)]
        detail: Option<ToolCallDetail>,
    },
    TurnUserInputRequested {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "requestId")]
        request_id: String,
        questions: Vec<ProviderUserInputQuestion>,
    },
    TurnUserInputResolved {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "requestId")]
        request_id: String,
        answers: Vec<ProviderUserInputAnswer>,
    },
    TurnPermissionRequested {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        title: Option<String>,
        description: Option<String>,
        command: Option<String>,
        file: Option<String>,
    },
    TurnPermissionResolved {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "requestId")]
        request_id: String,
        behavior: String,
    },
    TurnNativeSubagentActivity {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        id: String,
        #[serde(rename = "agentId")]
        agent_id: Option<String>,
        #[serde(rename = "agentThreadId")]
        agent_thread_id: Option<String>,
        #[serde(default)]
        path: Option<String>,
        name: Option<String>,
        role: Option<String>,
        model: Option<String>,
        status: NativeSubagentStatus,
    },
    TurnNativeSubagentModelConfirmed {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "correlationId")]
        correlation_id: String,
        model: String,
    },
    TurnNativeSubagentModelRequested {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        #[serde(rename = "correlationId")]
        correlation_id: String,
        model: String,
    },
    TurnModelEffective {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        model: String,
    },
    TurnCompleted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
    },
    TurnAborted {
        #[serde(rename = "turnId")]
        turn_id: TurnId,
        reason: Option<String>,
    },
    CheckpointCreated {
        #[serde(rename = "checkpointId")]
        checkpoint_id: CheckpointId,
        label: String,
    },
    PlanApproved {
        #[serde(rename = "planMessageId")]
        plan_message_id: String,
        #[serde(rename = "planVersion")]
        plan_version: u32,
        #[serde(rename = "planHash")]
        plan_hash: String,
    },
    PlanHandedOff {
        #[serde(rename = "planMessageId")]
        plan_message_id: String,
        #[serde(rename = "planVersion")]
        plan_version: u32,
        #[serde(rename = "planHash")]
        plan_hash: String,
        action: String,
        #[serde(rename = "targetSessionId")]
        target_session_id: Option<SessionId>,
    },
    DelegationRequested {
        #[serde(rename = "delegationId")]
        delegation_id: DelegationId,
    },
    DelegationStarted {
        #[serde(rename = "delegationId")]
        delegation_id: DelegationId,
        #[serde(rename = "childSessionId")]
        child_session_id: Option<SessionId>,
    },
    DelegationDelta {
        #[serde(rename = "delegationId")]
        delegation_id: DelegationId,
        content: String,
    },
    DelegationCompleted {
        #[serde(rename = "delegationId")]
        delegation_id: DelegationId,
        summary: Option<String>,
    },
    DelegationFailed {
        #[serde(rename = "delegationId")]
        delegation_id: DelegationId,
        reason: Option<String>,
    },
    DelegationCancelled {
        #[serde(rename = "delegationId")]
        delegation_id: DelegationId,
        reason: Option<String>,
    },
    SessionCompleted,
    SessionAborted {
        reason: Option<String>,
    },
    SessionResumed,
    /// The durable objective paused itself (budget or failure limit). Metadata
    /// only, so the timeline can explain why automatic follow-ups stopped.
    ObjectivePaused {
        reason: super::objective::ObjectivePauseReason,
        #[serde(rename = "consecutiveFailures")]
        consecutive_failures: u32,
        #[serde(rename = "turnsUsed")]
        turns_used: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnReviewFile {
    pub path: String,
    #[serde(rename = "oldPath", default)]
    pub old_path: Option<String>,
    pub status: String,
    pub insertions: u32,
    pub deletions: u32,
    #[serde(default)]
    pub untracked: bool,
    #[serde(default)]
    pub binary: bool,
    #[serde(rename = "previewUnavailable", default)]
    pub preview_unavailable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnReviewUntrackedFingerprint {
    pub path: String,
    pub sha256: String,
}

/// Versioned persistence record kept outside the session transcript. The
/// transcript remains small; file diffs are loaded lazily by snapshot + path.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnChangeSet {
    pub snapshot_id: String,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub workspace_id: WorkspaceId,
    pub capture_version: u32,
    pub state: String,
    pub base_tree: Option<String>,
    pub result_tree: Option<String>,
    #[serde(default)]
    pub baseline_untracked: Vec<String>,
    #[serde(default)]
    pub result_untracked: Vec<TurnReviewUntrackedFingerprint>,
    #[serde(default)]
    pub files: Vec<TurnReviewFile>,
    #[serde(default)]
    pub file_diffs: BTreeMap<String, String>,
    #[serde(default)]
    pub observed_validations: Vec<String>,
    pub diff_truncated: bool,
    #[serde(default)]
    pub turn_outcome: Option<String>,
    #[serde(default)]
    pub outcome_reason: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionEventRecord {
    pub event_id: String,
    pub session_id: SessionId,
    pub sequence: u64,
    pub occurred_at: String,
    pub kind: SessionEventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionProjection {
    pub session_id: SessionId,
    pub project_id: ProjectId,
    pub workspace_id: WorkspaceId,
    pub provider_id: String,
    pub model: Option<String>,
    pub state: SessionState,
    pub active_turn_id: Option<TurnId>,
    pub turn_count: u32,
    pub checkpoint_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

impl SessionProjection {
    pub fn new(
        session_id: SessionId,
        project_id: ProjectId,
        workspace_id: WorkspaceId,
        provider_id: String,
        model: Option<String>,
        occurred_at: String,
    ) -> Self {
        Self {
            session_id,
            project_id,
            workspace_id,
            provider_id,
            model,
            state: SessionState::Draft,
            active_turn_id: None,
            turn_count: 0,
            checkpoint_count: 0,
            created_at: occurred_at.clone(),
            updated_at: occurred_at,
        }
    }

    pub fn apply_event(&mut self, event: &SessionEventRecord) {
        self.updated_at = event.occurred_at.clone();

        match &event.kind {
            SessionEventKind::SessionStarted {
                workspace_id,
                project_id,
                provider_id,
                model,
                ..
            } => {
                self.workspace_id = workspace_id.clone();
                self.project_id = project_id.clone();
                self.provider_id = provider_id.clone();
                self.model = model.clone();
                self.state = SessionState::Active;
            }
            SessionEventKind::TurnStarted { turn_id, .. } => {
                self.state = SessionState::Active;
                self.active_turn_id = Some(turn_id.clone());
            }
            SessionEventKind::TurnSteered { .. }
            | SessionEventKind::TurnQueued { .. }
            | SessionEventKind::QueuedTurnRemoved { .. }
            | SessionEventKind::TurnQueueReordered { .. }
            | SessionEventKind::QueuedTurnDispatched { .. }
            | SessionEventKind::TurnDelta { .. }
            | SessionEventKind::TurnAssistantMessageStarted { .. }
            | SessionEventKind::TurnAssistantMessageDelta { .. }
            | SessionEventKind::TurnAssistantMessageCompleted { .. }
            | SessionEventKind::TurnReasoningStarted { .. }
            | SessionEventKind::TurnReasoningDelta { .. }
            | SessionEventKind::TurnReasoningCompleted { .. }
            | SessionEventKind::TurnToolCallStarted { .. }
            | SessionEventKind::TurnToolCallDelta { .. }
            | SessionEventKind::TurnToolCallUpdated { .. }
            | SessionEventKind::TurnToolCallCompleted { .. }
            | SessionEventKind::TurnToolCallFailed { .. }
            | SessionEventKind::TurnUserInputRequested { .. }
            | SessionEventKind::TurnUserInputResolved { .. }
            | SessionEventKind::TurnPermissionRequested { .. }
            | SessionEventKind::TurnPermissionResolved { .. }
            | SessionEventKind::TurnNativeSubagentActivity { .. }
            | SessionEventKind::TurnNativeSubagentModelConfirmed { .. }
            | SessionEventKind::TurnNativeSubagentModelRequested { .. }
            | SessionEventKind::TurnModelEffective { .. }
            | SessionEventKind::PlanApproved { .. }
            | SessionEventKind::PlanHandedOff { .. }
            | SessionEventKind::DelegationRequested { .. }
            | SessionEventKind::DelegationStarted { .. }
            | SessionEventKind::DelegationDelta { .. }
            | SessionEventKind::DelegationCompleted { .. }
            | SessionEventKind::DelegationFailed { .. }
            | SessionEventKind::DelegationCancelled { .. } => {}
            SessionEventKind::TurnCompleted { turn_id } => {
                self.turn_count = self.turn_count.saturating_add(1);
                if self.active_turn_id.as_ref() == Some(turn_id) {
                    self.active_turn_id = None;
                }
            }
            SessionEventKind::TurnAborted { turn_id, .. } => {
                if self.active_turn_id.as_ref() == Some(turn_id) {
                    self.active_turn_id = None;
                }
            }
            SessionEventKind::CheckpointCreated { .. } => {
                self.checkpoint_count = self.checkpoint_count.saturating_add(1);
            }
            SessionEventKind::SessionCompleted => {
                self.state = SessionState::Completed;
                self.active_turn_id = None;
            }
            SessionEventKind::SessionAborted { .. } => {
                self.state = SessionState::Aborted;
                self.active_turn_id = None;
            }
            SessionEventKind::SessionResumed => {
                self.state = SessionState::Active;
            }
            SessionEventKind::ObjectivePaused { .. } => {}
        }
    }

    pub fn fold(events: &[SessionEventRecord]) -> Option<Self> {
        let first = events.first()?;
        let mut projection = match &first.kind {
            SessionEventKind::SessionStarted {
                workspace_id,
                project_id,
                provider_id,
                model,
                ..
            } => Self::new(
                first.session_id.clone(),
                project_id.clone(),
                workspace_id.clone(),
                provider_id.clone(),
                model.clone(),
                first.occurred_at.clone(),
            ),
            _ => Self::new(
                first.session_id.clone(),
                ProjectId(String::new()),
                WorkspaceId(String::new()),
                String::new(),
                None,
                first.occurred_at.clone(),
            ),
        };

        for event in events {
            projection.apply_event(event);
        }

        Some(projection)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSessionSummary {
    pub session: Session,
    pub thread: Thread,
    pub projection: SessionProjection,
    pub last_turn_prompt: Option<String>,
    pub last_turn_state: Option<String>,
    pub last_turn_started_at: Option<String>,
    pub last_turn_completed_at: Option<String>,
    /// `"permission"` or `"input"` while the latest turn is blocked on the
    /// user; a pending permission wins over a pending question.
    pub last_turn_awaiting_user: Option<String>,
    /// When the latest turn started waiting on the user: the oldest request
    /// still open. Orders the sidebar's "Needs you" by when tasks came back.
    pub last_turn_awaiting_since: Option<String>,
}

/// What the latest turn is waiting on the user for, if anything. Requests are
/// matched by id, and only requests of the turn that is still open count: a
/// finished or aborted turn cannot be waiting, even if the provider never
/// resolved its request.
pub fn last_turn_awaiting_user(events: &[SessionEventRecord]) -> Option<String> {
    let pending = open_turn_requests(events)?;
    if !pending.permissions.is_empty() {
        Some("permission".to_string())
    } else if !pending.inputs.is_empty() {
        Some("input".to_string())
    } else {
        None
    }
}

/// When the latest turn started waiting on the user: the time of its oldest
/// request still open, permission or question alike.
pub fn last_turn_awaiting_since(events: &[SessionEventRecord]) -> Option<String> {
    let pending = open_turn_requests(events)?;
    pending
        .permissions
        .iter()
        .chain(pending.inputs.iter())
        .map(|(_, occurred_at)| *occurred_at)
        .min_by_key(|occurred_at| {
            chrono::DateTime::parse_from_rfc3339(occurred_at)
                .map(|at| at.timestamp_millis())
                .unwrap_or(i64::MAX)
        })
        .map(str::to_string)
}

/// Open requests of the turn still running, as (request id, requested at).
struct OpenTurnRequests<'a> {
    permissions: Vec<(&'a str, &'a str)>,
    inputs: Vec<(&'a str, &'a str)>,
}

fn open_turn_requests(events: &[SessionEventRecord]) -> Option<OpenTurnRequests<'_>> {
    let mut open_turn: Option<&TurnId> = None;
    let mut permissions: Vec<(&str, &str)> = Vec::new();
    let mut inputs: Vec<(&str, &str)> = Vec::new();
    for event in events {
        match &event.kind {
            SessionEventKind::TurnStarted { turn_id, .. } => {
                open_turn = Some(turn_id);
                permissions.clear();
                inputs.clear();
            }
            SessionEventKind::TurnCompleted { turn_id }
            | SessionEventKind::TurnAborted { turn_id, .. }
                if open_turn == Some(turn_id) =>
            {
                open_turn = None;
            }
            SessionEventKind::TurnPermissionRequested {
                turn_id,
                request_id,
                ..
            } if open_turn == Some(turn_id) => {
                permissions.push((request_id, event.occurred_at.as_str()))
            }
            SessionEventKind::TurnPermissionResolved { request_id, .. } => {
                permissions.retain(|(pending, _)| *pending != request_id.as_str());
            }
            SessionEventKind::TurnUserInputRequested {
                turn_id,
                request_id,
                ..
            } if open_turn == Some(turn_id) => inputs.push((request_id, event.occurred_at.as_str())),
            SessionEventKind::TurnUserInputResolved { request_id, .. } => {
                inputs.retain(|(pending, _)| *pending != request_id.as_str());
            }
            SessionEventKind::SessionCompleted | SessionEventKind::SessionAborted { .. } => {
                open_turn = None;
            }
            _ => {}
        }
    }
    open_turn?;
    Some(OpenTurnRequests { permissions, inputs })
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchResult {
    pub session_id: SessionId,
    pub workspace_id: WorkspaceId,
    pub project_id: ProjectId,
    pub thread_title: String,
    pub workspace_name: Option<String>,
    pub workspace_branch: Option<String>,
    pub workspace_root_path: Option<String>,
    pub provider_id: String,
    pub model: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub snippet: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(sequence: u64, occurred_at: &str, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("evt-{sequence}"),
            session_id: SessionId("session-1".to_string()),
            sequence,
            occurred_at: occurred_at.to_string(),
            kind,
        }
    }

    fn turn_started(sequence: u64, turn: &str) -> SessionEventRecord {
        event(
            sequence,
            "2026-09-30T10:00:00Z",
            SessionEventKind::TurnStarted {
                turn_id: TurnId(turn.to_string()),
                prompt: "Fix it".to_string(),
                plan_mode: None,
                model: None,
                evidence: None,
                retry_of_turn_id: None,
                approval_policy: None,
            },
        )
    }

    fn permission_requested(sequence: u64, turn: &str, request: &str) -> SessionEventRecord {
        event(
            sequence,
            "2026-09-30T10:00:01Z",
            SessionEventKind::TurnPermissionRequested {
                turn_id: TurnId(turn.to_string()),
                request_id: request.to_string(),
                tool_name: "Bash".to_string(),
                title: None,
                description: None,
                command: Some("yarn test".to_string()),
                file: None,
            },
        )
    }

    fn permission_resolved(sequence: u64, turn: &str, request: &str) -> SessionEventRecord {
        event(
            sequence,
            "2026-09-30T10:00:02Z",
            SessionEventKind::TurnPermissionResolved {
                turn_id: TurnId(turn.to_string()),
                request_id: request.to_string(),
                behavior: "allow".to_string(),
            },
        )
    }

    fn input_requested(sequence: u64, turn: &str, request: &str) -> SessionEventRecord {
        event(
            sequence,
            "2026-09-30T10:00:03Z",
            SessionEventKind::TurnUserInputRequested {
                turn_id: TurnId(turn.to_string()),
                request_id: request.to_string(),
                questions: Vec::new(),
            },
        )
    }

    #[test]
    fn awaiting_user_follows_open_requests_of_the_running_turn() {
        let waiting = vec![turn_started(1, "t1"), permission_requested(2, "t1", "p1")];
        assert_eq!(
            last_turn_awaiting_user(&waiting).as_deref(),
            Some("permission")
        );

        let answered = vec![
            turn_started(1, "t1"),
            permission_requested(2, "t1", "p1"),
            permission_resolved(3, "t1", "p1"),
        ];
        assert_eq!(last_turn_awaiting_user(&answered), None);

        let asking = vec![turn_started(1, "t1"), input_requested(2, "t1", "q1")];
        assert_eq!(last_turn_awaiting_user(&asking).as_deref(), Some("input"));

        let both = vec![
            turn_started(1, "t1"),
            input_requested(2, "t1", "q1"),
            permission_requested(3, "t1", "p1"),
        ];
        assert_eq!(
            last_turn_awaiting_user(&both).as_deref(),
            Some("permission")
        );
    }

    #[test]
    fn awaiting_user_clears_when_the_turn_ends_or_a_new_one_starts() {
        let aborted = vec![
            turn_started(1, "t1"),
            permission_requested(2, "t1", "p1"),
            event(
                3,
                "2026-09-30T10:00:04Z",
                SessionEventKind::TurnAborted {
                    turn_id: TurnId("t1".to_string()),
                    reason: None,
                },
            ),
        ];
        assert_eq!(last_turn_awaiting_user(&aborted), None);

        let next_turn = vec![
            turn_started(1, "t1"),
            permission_requested(2, "t1", "p1"),
            turn_started(3, "t2"),
        ];
        assert_eq!(last_turn_awaiting_user(&next_turn), None);

        let stale_request = vec![turn_started(1, "t2"), permission_requested(2, "t1", "p1")];
        assert_eq!(last_turn_awaiting_user(&stale_request), None);
    }

    #[test]
    fn awaiting_since_is_the_oldest_request_still_open() {
        let question_then_permission = vec![
            turn_started(1, "t1"),
            input_requested(2, "t1", "q1"),
            event(
                3,
                "2026-09-30T10:05:00Z",
                SessionEventKind::TurnPermissionRequested {
                    turn_id: TurnId("t1".to_string()),
                    request_id: "p1".to_string(),
                    tool_name: "Bash".to_string(),
                    title: None,
                    description: None,
                    command: None,
                    file: None,
                },
            ),
        ];
        assert_eq!(
            last_turn_awaiting_since(&question_then_permission).as_deref(),
            Some("2026-09-30T10:00:03Z")
        );

        // A request answered mid-turn no longer counts; the turn came back to
        // the user again only when the next one arrived.
        let answered_then_asked = vec![
            turn_started(1, "t1"),
            permission_requested(2, "t1", "p1"),
            permission_resolved(3, "t1", "p1"),
            input_requested(4, "t1", "q1"),
        ];
        assert_eq!(
            last_turn_awaiting_since(&answered_then_asked).as_deref(),
            Some("2026-09-30T10:00:03Z")
        );

        let running = vec![turn_started(1, "t1")];
        assert_eq!(last_turn_awaiting_since(&running), None);
    }

    #[test]
    fn projection_tracks_turns_checkpoints_and_terminal_state() {
        let events = vec![
            event(
                1,
                "2026-05-01T10:00:00Z",
                SessionEventKind::SessionStarted {
                    workspace_id: WorkspaceId("workspace-1".to_string()),
                    project_id: ProjectId("project-1".to_string()),
                    provider_id: "codex".to_string(),
                    model: Some("gpt-5-codex".to_string()),
                    forked_from: None,
                },
            ),
            event(
                2,
                "2026-05-01T10:00:05Z",
                SessionEventKind::TurnStarted {
                    turn_id: TurnId("turn-1".to_string()),
                    prompt: "Create shell".to_string(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: None,
                },
            ),
            event(
                3,
                "2026-05-01T10:00:10Z",
                SessionEventKind::TurnCompleted {
                    turn_id: TurnId("turn-1".to_string()),
                },
            ),
            event(
                4,
                "2026-05-01T10:00:12Z",
                SessionEventKind::CheckpointCreated {
                    checkpoint_id: CheckpointId("checkpoint-1".to_string()),
                    label: "After turn".to_string(),
                },
            ),
            event(
                5,
                "2026-05-01T10:00:20Z",
                SessionEventKind::SessionAborted {
                    reason: Some("user stopped".to_string()),
                },
            ),
            event(6, "2026-05-01T10:00:25Z", SessionEventKind::SessionResumed),
        ];

        let projection = SessionProjection::fold(&events).expect("projection should exist");

        assert_eq!(projection.session_id.0, "session-1");
        assert_eq!(projection.project_id.0, "project-1");
        assert_eq!(projection.workspace_id.0, "workspace-1");
        assert_eq!(projection.provider_id, "codex");
        assert_eq!(projection.turn_count, 1);
        assert_eq!(projection.checkpoint_count, 1);
        assert_eq!(projection.active_turn_id, None);
        assert_eq!(projection.state, SessionState::Active);
        assert_eq!(projection.created_at, "2026-05-01T10:00:00Z");
        assert_eq!(projection.updated_at, "2026-05-01T10:00:25Z");
    }

    #[test]
    fn projection_resumes_after_aborted_state() {
        let events = vec![
            event(
                1,
                "2026-05-01T10:00:00Z",
                SessionEventKind::SessionStarted {
                    workspace_id: WorkspaceId("workspace-1".to_string()),
                    project_id: ProjectId("project-1".to_string()),
                    provider_id: "codex".to_string(),
                    model: Some("gpt-5-codex".to_string()),
                    forked_from: None,
                },
            ),
            event(
                2,
                "2026-05-01T10:00:05Z",
                SessionEventKind::SessionAborted {
                    reason: Some("Stopped".to_string()),
                },
            ),
            event(3, "2026-05-01T10:00:10Z", SessionEventKind::SessionResumed),
        ];

        let projection = SessionProjection::fold(&events).expect("projection should exist");

        assert_eq!(projection.state, SessionState::Active);
    }

    #[test]
    fn fold_returns_none_for_empty_logs() {
        assert!(SessionProjection::fold(&[]).is_none());
    }

    #[test]
    fn native_subagent_path_is_backward_compatible_with_existing_events() {
        let kind = serde_json::from_value::<SessionEventKind>(serde_json::json!({
            "type": "turn_native_subagent_activity",
            "turnId": "turn-1",
            "id": "agent-1",
            "agentId": null,
            "agentThreadId": "thread-1",
            "name": "Reviewer",
            "role": "reviewer",
            "model": null,
            "status": "running"
        }))
        .expect("pre-tree native subagent events remain readable");

        assert!(matches!(
            kind,
            SessionEventKind::TurnNativeSubagentActivity { path: None, .. }
        ));
    }
}

#[cfg(test)]
mod turn_evidence_tests {
    use super::{
        SessionEventKind, TurnEvidenceItemSummary, TurnEvidenceSource, TurnEvidenceStage,
        TurnEvidenceSummary, TurnEvidenceTrust, TurnId, MAX_TURN_EVIDENCE_ITEMS,
        MAX_TURN_EVIDENCE_ITEM_CHARS,
    };

    fn item(source: TurnEvidenceSource, trust: TurnEvidenceTrust) -> TurnEvidenceItemSummary {
        TurnEvidenceItemSummary {
            source,
            trust,
            chars: 120,
            truncated: false,
        }
    }

    #[test]
    fn evidence_summary_validation_is_bounded_and_source_trust_coherent() {
        let valid = TurnEvidenceSummary {
            stage: TurnEvidenceStage::Investigate,
            items: vec![
                item(
                    TurnEvidenceSource::Browser,
                    TurnEvidenceTrust::RemoteUntrusted,
                ),
                item(
                    TurnEvidenceSource::Terminal,
                    TurnEvidenceTrust::LocalTerminal,
                ),
                item(TurnEvidenceSource::Diff, TurnEvidenceTrust::LocalWorkspace),
            ],
        };
        valid.validate().expect("coherent summary");

        let empty = TurnEvidenceSummary {
            stage: TurnEvidenceStage::Observe,
            items: Vec::new(),
        };
        assert!(empty.validate().is_err());

        let too_many = TurnEvidenceSummary {
            stage: TurnEvidenceStage::Observe,
            items: vec![
                item(
                    TurnEvidenceSource::Terminal,
                    TurnEvidenceTrust::LocalTerminal
                );
                MAX_TURN_EVIDENCE_ITEMS + 1
            ],
        };
        assert!(too_many.validate().is_err());

        let mismatched = TurnEvidenceSummary {
            stage: TurnEvidenceStage::Fix,
            items: vec![item(
                TurnEvidenceSource::Browser,
                TurnEvidenceTrust::LocalTerminal,
            )],
        };
        assert!(mismatched.validate().is_err());

        let oversized = TurnEvidenceSummary {
            stage: TurnEvidenceStage::Verify,
            items: vec![TurnEvidenceItemSummary {
                chars: MAX_TURN_EVIDENCE_ITEM_CHARS + 1,
                ..item(TurnEvidenceSource::Diff, TurnEvidenceTrust::LocalWorkspace)
            }],
        };
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn turn_started_records_stay_compatible_without_evidence() {
        let legacy = serde_json::json!({
            "type": "turn_started",
            "turnId": "turn-1",
            "prompt": "hello",
            "planMode": true
        });
        let kind: SessionEventKind = serde_json::from_value(legacy).expect("legacy record");
        match &kind {
            SessionEventKind::TurnStarted { evidence, .. } => assert!(evidence.is_none()),
            _ => panic!("expected turn started"),
        }
        let serialized = serde_json::to_value(&kind).expect("serialize");
        assert!(serialized["evidence"].is_null());

        let with_evidence = SessionEventKind::TurnStarted {
            turn_id: TurnId("turn-2".to_string()),
            prompt: "why".to_string(),
            plan_mode: None,
            model: None,
            evidence: Some(TurnEvidenceSummary {
                stage: TurnEvidenceStage::Reproduce,
                items: vec![item(
                    TurnEvidenceSource::Browser,
                    TurnEvidenceTrust::RemoteUntrusted,
                )],
            }),
            retry_of_turn_id: None,
            approval_policy: None,
        };
        let value = serde_json::to_value(&with_evidence).expect("serialize evidence");
        assert_eq!(value["evidence"]["stage"], "reproduce");
        assert_eq!(value["evidence"]["items"][0]["source"], "browser");
        assert_eq!(value["evidence"]["items"][0]["trust"], "remote_untrusted");
        assert!(value["evidence"]["items"][0].get("body").is_none());
    }
}
