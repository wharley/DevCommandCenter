//! "Where is this task and what now?", one to one with the desktop's
//! `buildWorkspaceRecap` (`features/inspector/workspace-recap.ts`): the
//! delivery state, a one-sentence message with its action, and the delivery
//! signals behind it. The next git step is `resolve_commit_mode`; this only
//! layers what it cannot see (the agent's turn, a captured failure, waiting
//! delegations, review, pipeline and the project's delivery policy).

mod primary;
mod signals;

use std::collections::BTreeMap;

use super::commit_mode::{commit_translation_key, CommitButtonStatus, CommitMode};

/// `WorkspaceDeliveryState`: derived, never persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryState {
    InDevelopment,
    NeedsAttention,
    Blocked,
    AwaitingReview,
    ReadyToDeliver,
    Delivered,
}

impl DeliveryState {
    /// The key under `inspector.recap.state`.
    pub fn id(self) -> &'static str {
        match self {
            Self::InDevelopment => "in_development",
            Self::NeedsAttention => "needs_attention",
            Self::Blocked => "blocked",
            Self::AwaitingReview => "awaiting_review",
            Self::ReadyToDeliver => "ready_to_deliver",
            Self::Delivered => "delivered",
        }
    }
}

/// `WorkspaceRecapTone`: only `Attention` spends chroma.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecapTone {
    Neutral,
    Working,
    Attention,
    Ready,
    Done,
}

/// `WorkspaceRecapAction`: what the strip's button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecapAction {
    /// The git header's own step.
    Git(CommitMode),
    Activity,
    /// CodeRabbit findings.
    Review,
    ReviewState,
    Pipeline,
    Delivery,
    Sync,
    Automation,
}

impl RecapAction {
    /// The label key (namespace `common`); a git step's label is the commit
    /// button's, outside "Changes" only.
    pub fn label_key(self) -> String {
        match self {
            Self::Git(mode) => commit_translation_key(mode, CommitButtonStatus::Idle),
            Self::Activity => "inspector.recap.actions.activity".into(),
            Self::Review => "inspector.recap.actions.review".into(),
            Self::ReviewState => "inspector.recap.actions.reviewState".into(),
            Self::Pipeline => "inspector.recap.actions.pipeline".into(),
            Self::Delivery => "inspector.recap.actions.delivery".into(),
            Self::Sync => "inspector.recap.actions.synchronize".into(),
            Self::Automation => "inspector.recap.actions.policy".into(),
        }
    }
}

/// `workspaceRecapActionForMode`: inside "Changes" the git header already
/// offers its step, so the strip does not repeat it.
pub fn action_in_changes(action: Option<RecapAction>) -> Option<RecapAction> {
    action.filter(|action| !matches!(action, RecapAction::Git(_)))
}

/// A message parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecapParam {
    Number(i64),
    Text(String),
}

pub type RecapParams = BTreeMap<&'static str, RecapParam>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalState {
    Passed,
    Pending,
    Attention,
    Blocked,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalId {
    Workspace,
    Recovery,
    AgentReview,
    Review,
    Pipeline,
    Checks,
}

/// `WorkspaceDeliverySignal`; the message is under `inspector.recap.signals`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliverySignal {
    pub id: SignalId,
    pub state: SignalState,
    pub message_key: &'static str,
    pub params: RecapParams,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRecap {
    pub state: DeliveryState,
    /// Under `inspector.recap.messages`.
    pub message_key: String,
    pub params: RecapParams,
    pub tone: RecapTone,
    pub action: Option<RecapAction>,
    pub signals: Vec<DeliverySignal>,
}

/// Whether a forge read answered; `None` is a read nobody asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    Idle,
    Loading,
    Error,
    Available,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecapDeliveryFailure {
    /// `fetch`, `pull`, `push` or `pipeline`.
    pub operation: String,
    pub classification: String,
}

impl RecapDeliveryFailure {
    /// Retrying or signing in may fix it; anything else blocks.
    fn is_soft(&self) -> bool {
        matches!(
            self.classification.as_str(),
            "authentication" | "transport" | "unknown"
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecapReviewState {
    pub review_state: Option<String>,
    pub approvals_available: bool,
    pub approvals_received: i64,
    pub approvals_left: Option<i64>,
    pub has_conflicts: Option<bool>,
    pub behind_by: Option<i64>,
    pub discussions_resolved: Option<bool>,
    pub draft: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecapPipeline {
    pub status: String,
    /// Failed jobs that are not allowed to fail.
    pub failed_jobs: i64,
}

impl RecapPipeline {
    fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "created" | "waiting_for_resource" | "preparing" | "pending" | "running" | "scheduled"
        )
    }
}

/// The `[delivery]` table of `.dcc.toml`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecapDeliveryPolicy {
    pub minimum_approvals: i64,
    pub require_pipeline: bool,
    pub require_resolved_discussions: bool,
    pub require_current_base: bool,
    pub require_before_merge_checks: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRecapInput {
    pub commit_mode: CommitMode,
    /// A turn in flight (a session stays active between turns).
    pub turn_running: bool,
    pub changed_files_count: i64,
    pub additions: i64,
    pub deletions: i64,
    pub ahead_of_remote_count: i64,
    pub conflict_count: i64,
    /// Files committed on this branch against the base.
    pub committed_vs_base_count: i64,
    pub pr_number: Option<u32>,
    pub pr_state: Option<String>,
    /// The forge's merge state (`UNSTABLE`, `BLOCKED`, `DIRTY`…): what keeps
    /// an open pull request from merging.
    pub pr_merge_state: Option<String>,
    /// "PR" or "MR".
    pub request_label: &'static str,
    /// CodeRabbit findings that still match the diff.
    pub pending_review_findings_count: i64,
    pub code_rabbit_review_available: bool,
    /// Delegations whose edits wait on the person (`review_pending`).
    pub pending_delegation_results_count: i64,
    pub delivery_failure: Option<RecapDeliveryFailure>,
    pub review_state: Option<RecapReviewState>,
    pub review_status: Option<SourceStatus>,
    pub pipeline: Option<RecapPipeline>,
    pub pipeline_status: Option<SourceStatus>,
    pub delivery_policy: Option<RecapDeliveryPolicy>,
    pub before_merge_checks_count: i64,
}

impl WorkspaceRecapInput {
    /// Only the git step: nothing waiting, no forge reads.
    pub fn new(commit_mode: CommitMode) -> Self {
        Self {
            commit_mode,
            turn_running: false,
            changed_files_count: 0,
            additions: 0,
            deletions: 0,
            ahead_of_remote_count: 0,
            conflict_count: 0,
            committed_vs_base_count: 0,
            pr_number: None,
            pr_state: None,
            pr_merge_state: None,
            request_label: "PR",
            pending_review_findings_count: 0,
            code_rabbit_review_available: false,
            pending_delegation_results_count: 0,
            delivery_failure: None,
            review_state: None,
            review_status: None,
            pipeline: None,
            pipeline_status: None,
            delivery_policy: None,
            before_merge_checks_count: 0,
        }
    }

    /// `prRef`: "PR #12", or the bare label without a number.
    fn pr_ref(&self) -> String {
        match self.pr_number {
            Some(number) => format!("{} #{number}", self.request_label),
            None => self.request_label.to_string(),
        }
    }
}

/// `buildWorkspaceRecap`
pub fn build_workspace_recap(input: &WorkspaceRecapInput) -> WorkspaceRecap {
    let primary = primary::build(input);
    WorkspaceRecap {
        state: primary.state,
        message_key: primary.message_key,
        params: primary.params,
        tone: primary.tone,
        action: primary.action,
        signals: signals::build(input),
    }
}

#[cfg(test)]
mod tests;
