//! `buildWorkspaceDeliverySignals`: the six checks behind the recap, each
//! marked required when the project's delivery policy asks for it.

use super::{
    DeliverySignal, RecapParam, RecapParams, SignalId as Id, SignalState as State, SourceStatus,
    WorkspaceRecapInput,
};
use crate::domain::commit_mode::CommitMode;

fn signal(
    id: Id,
    state: State,
    message_key: &'static str,
    params: RecapParams,
    required: bool,
) -> DeliverySignal {
    DeliverySignal {
        id,
        state,
        message_key,
        params,
        required,
    }
}

fn count(value: i64) -> RecapParams {
    RecapParams::from([("count", RecapParam::Number(value))])
}

pub(super) fn build(input: &WorkspaceRecapInput) -> Vec<DeliverySignal> {
    vec![
        workspace(input),
        recovery(input),
        agent_review(input),
        review(input),
        pipeline(input),
        checks(input),
    ]
}

fn workspace(input: &WorkspaceRecapInput) -> DeliverySignal {
    let (state, key, params) = if input.commit_mode == CommitMode::CompleteMerge {
        (
            State::Pending,
            "conflictResolutionReady",
            count(input.conflict_count),
        )
    } else if input.conflict_count > 0 || input.commit_mode == CommitMode::ResolveConflicts {
        (
            State::Blocked,
            "workspaceConflicts",
            count(input.conflict_count.max(1)),
        )
    } else if input.changed_files_count > 0 {
        (
            State::Pending,
            "workspaceChanges",
            count(input.changed_files_count),
        )
    } else if input.ahead_of_remote_count > 0 {
        (
            State::Pending,
            "workspaceAhead",
            count(input.ahead_of_remote_count),
        )
    } else {
        (State::Passed, "workspaceClean", RecapParams::new())
    };
    signal(Id::Workspace, state, key, params, false)
}

fn recovery(input: &WorkspaceRecapInput) -> DeliverySignal {
    match &input.delivery_failure {
        Some(failure) => {
            let state = if failure.is_soft() {
                State::Attention
            } else {
                State::Blocked
            };
            signal(
                Id::Recovery,
                state,
                "recoveryCaptured",
                RecapParams::new(),
                false,
            )
        }
        None => signal(
            Id::Recovery,
            State::Passed,
            "recoveryClear",
            RecapParams::new(),
            false,
        ),
    }
}

fn agent_review(input: &WorkspaceRecapInput) -> DeliverySignal {
    if input.pending_review_findings_count > 0 {
        signal(
            Id::AgentReview,
            State::Attention,
            "agentFindings",
            count(input.pending_review_findings_count),
            false,
        )
    } else if input.code_rabbit_review_available {
        signal(
            Id::AgentReview,
            State::Passed,
            "agentReviewClear",
            RecapParams::new(),
            false,
        )
    } else {
        signal(
            Id::AgentReview,
            State::Unavailable,
            "agentReviewUnavailable",
            RecapParams::new(),
            false,
        )
    }
}

fn review(input: &WorkspaceRecapInput) -> DeliverySignal {
    let policy = input.delivery_policy.unwrap_or_default();
    let required = policy.minimum_approvals > 0
        || policy.require_resolved_discussions
        || policy.require_current_base;
    let plain = |state, key| signal(Id::Review, state, key, RecapParams::new(), required);
    if input.pr_number.is_none() {
        return plain(State::Unavailable, "reviewNotStarted");
    }
    if input.review_status == Some(SourceStatus::Loading) {
        return plain(State::Pending, "reviewLoading");
    }
    let review = match &input.review_state {
        Some(review) if input.review_status != Some(SourceStatus::Error) => review,
        _ => return plain(State::Unavailable, "reviewUnavailable"),
    };
    let deficit = (policy.minimum_approvals - review.approvals_received).max(0);
    if review.has_conflicts == Some(true)
        || review.review_state.as_deref() == Some("changes_requested")
    {
        plain(State::Blocked, "reviewBlocked")
    } else if review.discussions_resolved == Some(false) || review.behind_by.unwrap_or(0) > 0 {
        plain(State::Attention, "reviewAttention")
    } else if !review.approvals_available && required {
        plain(State::Unavailable, "reviewUnavailable")
    } else if deficit > 0
        || review.draft == Some(true)
        || review.review_state.as_deref() == Some("pending")
    {
        signal(
            Id::Review,
            State::Pending,
            "reviewPending",
            count(deficit),
            required,
        )
    } else {
        signal(
            Id::Review,
            State::Passed,
            "reviewPassed",
            count(review.approvals_received),
            required,
        )
    }
}

fn pipeline(input: &WorkspaceRecapInput) -> DeliverySignal {
    let required = input
        .delivery_policy
        .is_some_and(|policy| policy.require_pipeline);
    let plain = |state, key| signal(Id::Pipeline, state, key, RecapParams::new(), required);
    match (input.pipeline_status, &input.pipeline) {
        (Some(SourceStatus::Loading), _) => plain(State::Pending, "pipelineLoading"),
        (Some(SourceStatus::Error), _) => plain(State::Unavailable, "pipelineUnavailable"),
        (Some(SourceStatus::Idle), None) => plain(State::Unavailable, "pipelineNotIntegrated"),
        (_, None) => plain(State::Unavailable, "pipelineMissing"),
        (_, Some(pipeline)) => match pipeline.status.as_str() {
            "success" => plain(State::Passed, "pipelinePassed"),
            "failed" => plain(State::Blocked, "pipelineFailed"),
            _ if pipeline.is_active() => plain(State::Pending, "pipelineRunning"),
            _ => plain(State::Attention, "pipelineAttention"),
        },
    }
}

fn checks(input: &WorkspaceRecapInput) -> DeliverySignal {
    let required = input
        .delivery_policy
        .is_some_and(|policy| policy.require_before_merge_checks);
    let configured = input.before_merge_checks_count;
    let (state, key) = if configured > 0 {
        (State::Passed, "checksConfigured")
    } else if required {
        (State::Blocked, "checksMissing")
    } else {
        (State::Unavailable, "checksOptional")
    };
    signal(Id::Checks, state, key, count(configured), required)
}
