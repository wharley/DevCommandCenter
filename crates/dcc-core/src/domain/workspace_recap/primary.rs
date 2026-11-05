//! `buildWorkspaceRecapPrimary`: the first rule that holds, top to bottom.

use super::{
    DeliveryState as State, RecapAction as Action, RecapParam, RecapParams, RecapTone as Tone,
    WorkspaceRecapInput,
};
use crate::domain::commit_mode::CommitMode;

pub(super) struct Primary {
    pub state: State,
    pub message_key: String,
    pub params: RecapParams,
    pub tone: Tone,
    pub action: Option<Action>,
}

fn primary(
    state: State,
    message_key: &str,
    params: RecapParams,
    tone: Tone,
    action: Option<Action>,
) -> Primary {
    Primary {
        state,
        message_key: message_key.to_string(),
        params,
        tone,
        action,
    }
}

fn count(value: i64) -> RecapParams {
    RecapParams::from([("count", RecapParam::Number(value))])
}

fn pr(input: &WorkspaceRecapInput) -> RecapParams {
    RecapParams::from([("pr", RecapParam::Text(input.pr_ref()))])
}

fn diff(input: &WorkspaceRecapInput) -> RecapParams {
    RecapParams::from([
        ("count", RecapParam::Number(input.changed_files_count)),
        ("additions", RecapParam::Number(input.additions)),
        ("deletions", RecapParam::Number(input.deletions)),
    ])
}

pub(super) fn build(input: &WorkspaceRecapInput) -> Primary {
    if let Some(found) = git_and_turn(input) {
        return found;
    }
    if let Some(found) = review_and_pipeline(input) {
        return found;
    }
    if let Some(found) = policy(input) {
        return found;
    }
    last_git_step(input)
}

/// Conflicts, a finished PR, the agent's turn, a captured failure and what
/// waits on the person, then the local git steps.
fn git_and_turn(input: &WorkspaceRecapInput) -> Option<Primary> {
    let mode = input.commit_mode;
    if mode == CommitMode::CompleteMerge {
        return Some(primary(
            State::ReadyToDeliver,
            "conflictResolutionReady",
            count(input.conflict_count),
            Tone::Ready,
            Some(Action::Git(mode)),
        ));
    }
    if mode == CommitMode::ResolveConflicts || input.conflict_count > 0 {
        let mut params = pr(input);
        params.insert("count", RecapParam::Number(input.conflict_count.max(1)));
        return Some(primary(
            State::Blocked,
            "conflicts",
            params,
            Tone::Attention,
            Some(Action::Git(CommitMode::ResolveConflicts)),
        ));
    }
    if mode == CommitMode::Merged {
        return Some(primary(
            State::Delivered,
            "merged",
            pr(input),
            Tone::Done,
            None,
        ));
    }
    let closed_pr = input
        .pr_state
        .as_deref()
        .is_some_and(|state| state.eq_ignore_ascii_case("closed"));
    if mode == CommitMode::Closed || (mode == CommitMode::OpenPr && closed_pr) {
        return Some(primary(
            State::NeedsAttention,
            "closed",
            pr(input),
            Tone::Neutral,
            input.pr_number.map(|_| Action::Git(CommitMode::OpenPr)),
        ));
    }
    if input.turn_running {
        let key = if input.changed_files_count > 0 {
            "working"
        } else {
            "workingClean"
        };
        return Some(primary(
            State::InDevelopment,
            key,
            diff(input),
            Tone::Working,
            Some(Action::Activity),
        ));
    }
    if let Some(failure) = &input.delivery_failure {
        let state = if failure.is_soft() {
            State::NeedsAttention
        } else {
            State::Blocked
        };
        return Some(Primary {
            state,
            message_key: format!("deliveryFailure.{}", failure.operation),
            params: RecapParams::new(),
            tone: Tone::Attention,
            action: Some(Action::Delivery),
        });
    }
    if input.pending_delegation_results_count > 0 {
        // It waits on the person, like the sidebar's amber "needs you".
        return Some(primary(
            State::NeedsAttention,
            "delegationResults",
            count(input.pending_delegation_results_count),
            Tone::Attention,
            Some(Action::Activity),
        ));
    }
    if input.pending_review_findings_count > 0 {
        return Some(primary(
            State::NeedsAttention,
            "findings",
            count(input.pending_review_findings_count),
            Tone::Attention,
            Some(Action::Review),
        ));
    }
    match mode {
        CommitMode::CommitAndPush => Some(primary(
            State::InDevelopment,
            "changes",
            diff(input),
            Tone::Neutral,
            Some(Action::Git(mode)),
        )),
        CommitMode::Push => Some(primary(
            State::InDevelopment,
            "ahead",
            count(input.ahead_of_remote_count),
            Tone::Neutral,
            Some(Action::Git(mode)),
        )),
        _ => None,
    }
}

/// What the forge says about the open PR and its pipeline.
fn review_and_pipeline(input: &WorkspaceRecapInput) -> Option<Primary> {
    let review = input.review_state.as_ref();
    let pipeline = input.pipeline.as_ref();
    let review_section = |state, key, tone| {
        Some(primary(
            state,
            key,
            pr(input),
            tone,
            Some(Action::ReviewState),
        ))
    };
    let pipeline_section = |state, key, params| {
        Some(primary(
            state,
            key,
            params,
            Tone::Attention,
            Some(Action::Pipeline),
        ))
    };
    if review.and_then(|review| review.has_conflicts) == Some(true) {
        return review_section(State::Blocked, "reviewConflicts", Tone::Attention);
    }
    if review.and_then(|review| review.review_state.as_deref()) == Some("changes_requested") {
        return review_section(State::Blocked, "changesRequested", Tone::Attention);
    }
    if let Some(pipeline) = pipeline.filter(|pipeline| pipeline.status == "failed") {
        return pipeline_section(
            State::Blocked,
            "pipelineFailed",
            count(pipeline.failed_jobs.max(1)),
        );
    }
    if review.and_then(|review| review.discussions_resolved) == Some(false) {
        return review_section(State::NeedsAttention, "discussionsOpen", Tone::Attention);
    }
    let behind = review.and_then(|review| review.behind_by).unwrap_or(0);
    if behind > 0 {
        return Some(primary(
            State::NeedsAttention,
            "behindBase",
            count(behind),
            Tone::Attention,
            Some(Action::Sync),
        ));
    }
    match pipeline.map(|pipeline| pipeline.status.as_str()) {
        Some("canceled") => {
            return pipeline_section(
                State::NeedsAttention,
                "pipelineCanceled",
                RecapParams::new(),
            );
        }
        Some("manual") => {
            return pipeline_section(State::NeedsAttention, "pipelineManual", RecapParams::new());
        }
        _ => {}
    }
    use super::SourceStatus::{Error, Loading};
    if input.pr_number.is_some() && input.review_status == Some(Loading) {
        return Some(primary(
            State::AwaitingReview,
            "reviewLoading",
            pr(input),
            Tone::Working,
            None,
        ));
    }
    if input.pipeline_status == Some(Loading) {
        return Some(primary(
            State::AwaitingReview,
            "pipelineLoading",
            RecapParams::new(),
            Tone::Working,
            None,
        ));
    }
    if input.review_status == Some(Error) {
        return review_section(State::NeedsAttention, "reviewUnavailable", Tone::Attention);
    }
    if input.pipeline_status == Some(Error) {
        return pipeline_section(
            State::NeedsAttention,
            "pipelineUnavailable",
            RecapParams::new(),
        );
    }
    None
}

/// The project's delivery policy and the review still to come.
fn policy(input: &WorkspaceRecapInput) -> Option<Primary> {
    let policy = input.delivery_policy.unwrap_or_default();
    let review = input.review_state.as_ref();
    let has_pr = input.pr_number.is_some();
    let review_section = |state, key: &str, params, tone| {
        Some(primary(state, key, params, tone, Some(Action::ReviewState)))
    };
    if policy.require_resolved_discussions
        && has_pr
        && review
            .and_then(|review| review.discussions_resolved)
            .is_none()
    {
        return review_section(
            State::NeedsAttention,
            "policy.discussionsUnavailable",
            pr(input),
            Tone::Attention,
        );
    }
    if policy.require_current_base && has_pr && review.and_then(|review| review.behind_by).is_none()
    {
        return review_section(
            State::NeedsAttention,
            "policy.baseUnavailable",
            pr(input),
            Tone::Attention,
        );
    }
    if review.and_then(|review| review.draft) == Some(true) {
        return review_section(State::AwaitingReview, "draft", pr(input), Tone::Neutral);
    }
    if has_pr && review.is_some_and(|review| !review.approvals_available) {
        return review_section(
            State::AwaitingReview,
            "approvalsUnavailable",
            pr(input),
            Tone::Neutral,
        );
    }
    let approvals_left = review.and_then(|review| review.approvals_left).unwrap_or(0);
    if approvals_left > 0 {
        return review_section(
            State::AwaitingReview,
            "approvalsPending",
            count(approvals_left),
            Tone::Neutral,
        );
    }
    let received = review.map_or(0, |review| review.approvals_received);
    if has_pr && policy.minimum_approvals > received {
        let mut params = count(policy.minimum_approvals - received);
        params.insert("required", RecapParam::Number(policy.minimum_approvals));
        return review_section(
            State::AwaitingReview,
            "policy.approvalsPending",
            params,
            Tone::Neutral,
        );
    }
    if matches!(
        review.and_then(|review| review.review_state.as_deref()),
        Some("pending" | "unknown")
    ) {
        return review_section(
            State::AwaitingReview,
            "reviewPending",
            pr(input),
            Tone::Neutral,
        );
    }
    if input
        .pipeline
        .as_ref()
        .is_some_and(|pipeline| pipeline.is_active())
    {
        return Some(primary(
            State::AwaitingReview,
            "pipelineRunning",
            RecapParams::new(),
            Tone::Working,
            Some(Action::Pipeline),
        ));
    }
    if has_pr && policy.require_pipeline {
        match &input.pipeline {
            None => {
                let action = if input.pipeline_status == Some(super::SourceStatus::Available) {
                    Action::Pipeline
                } else {
                    Action::Automation
                };
                return Some(primary(
                    State::NeedsAttention,
                    "policy.pipelineMissing",
                    RecapParams::new(),
                    Tone::Attention,
                    Some(action),
                ));
            }
            Some(pipeline) if pipeline.status != "success" => {
                return Some(primary(
                    State::NeedsAttention,
                    "policy.pipelineNotSuccessful",
                    RecapParams::new(),
                    Tone::Attention,
                    Some(Action::Pipeline),
                ));
            }
            Some(_) => {}
        }
    }
    if has_pr && policy.require_before_merge_checks && input.before_merge_checks_count == 0 {
        return Some(primary(
            State::NeedsAttention,
            "policy.checksMissing",
            RecapParams::new(),
            Tone::Attention,
            Some(Action::Automation),
        ));
    }
    None
}

/// What blocks the pull request: failing checks, a required review or
/// rule, or a conflict with its base.
fn fix_reason(merge_state: Option<&str>) -> &'static str {
    match merge_state.map(str::to_ascii_uppercase).as_deref() {
        Some("BLOCKED") => "prBlocked",
        Some("DIRTY") => "prConflicts",
        _ => "checksFailing",
    }
}

fn last_git_step(input: &WorkspaceRecapInput) -> Primary {
    let mode = input.commit_mode;
    match mode {
        CommitMode::Fix => primary(
            State::Blocked,
            fix_reason(input.pr_merge_state.as_deref()),
            pr(input),
            Tone::Attention,
            Some(Action::Git(mode)),
        ),
        CommitMode::Merge => primary(
            State::ReadyToDeliver,
            "mergeReady",
            pr(input),
            Tone::Ready,
            Some(Action::Git(mode)),
        ),
        CommitMode::OpenPr => primary(
            State::AwaitingReview,
            "prOpen",
            pr(input),
            Tone::Neutral,
            Some(Action::Git(mode)),
        ),
        CommitMode::CreatePr if input.committed_vs_base_count > 0 => {
            let mut params = count(input.committed_vs_base_count);
            params.insert(
                "requestLabel",
                RecapParam::Text(input.request_label.to_string()),
            );
            primary(
                State::InDevelopment,
                "readyForPr",
                params,
                Tone::Ready,
                Some(Action::Git(mode)),
            )
        }
        _ => primary(
            State::InDevelopment,
            "clean",
            RecapParams::new(),
            Tone::Neutral,
            None,
        ),
    }
}
