//! `workspace-recap.test.ts`, case by case.

use super::*;

fn input(mode: CommitMode) -> WorkspaceRecapInput {
    WorkspaceRecapInput::new(mode)
}

fn number(recap: &WorkspaceRecap, key: &str) -> Option<i64> {
    match recap.params.get(key) {
        Some(RecapParam::Number(value)) => Some(*value),
        _ => None,
    }
}

fn text<'a>(recap: &'a WorkspaceRecap, key: &str) -> Option<&'a str> {
    match recap.params.get(key) {
        Some(RecapParam::Text(value)) => Some(value),
        _ => None,
    }
}

fn signal(recap: &WorkspaceRecap, id: SignalId) -> &DeliverySignal {
    recap.signals.iter().find(|signal| signal.id == id).unwrap()
}

fn review(approvals_received: i64) -> RecapReviewState {
    RecapReviewState {
        review_state: Some("approved".into()),
        approvals_available: true,
        approvals_received,
        approvals_left: Some(0),
        has_conflicts: Some(false),
        behind_by: Some(0),
        discussions_resolved: Some(true),
        draft: Some(false),
    }
}

fn pipeline(status: &str, failed_jobs: i64) -> RecapPipeline {
    RecapPipeline {
        status: status.into(),
        failed_jobs,
    }
}

#[test]
fn reports_clean_when_there_is_nothing_to_act_on() {
    let recap = build_workspace_recap(&input(CommitMode::CreatePr));
    assert_eq!(recap.message_key, "clean");
    assert_eq!(recap.action, None);
}

#[test]
fn puts_conflicts_above_everything_else() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        conflict_count: 2,
        turn_running: true,
        pending_review_findings_count: 4,
        changed_files_count: 3,
        ..input(CommitMode::ResolveConflicts)
    });
    assert_eq!(recap.message_key, "conflicts");
    assert_eq!(recap.state, DeliveryState::Blocked);
    assert_eq!(recap.tone, RecapTone::Attention);
    assert_eq!(
        recap.action,
        Some(RecapAction::Git(CommitMode::ResolveConflicts))
    );
    assert_eq!(
        recap.action.unwrap().label_key(),
        "commit.modes.resolve-conflicts.idle"
    );
}

#[test]
fn surfaces_marker_free_conflict_edits_as_ready_for_explicit_completion() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        conflict_count: 3,
        ..input(CommitMode::CompleteMerge)
    });
    assert_eq!(recap.state, DeliveryState::ReadyToDeliver);
    assert_eq!(recap.message_key, "conflictResolutionReady");
    assert_eq!(
        recap.action.unwrap().label_key(),
        "commit.modes.complete-merge.idle"
    );
    let workspace = &recap.signals[0];
    assert_eq!(workspace.id, SignalId::Workspace);
    assert_eq!(workspace.state, SignalState::Pending);
    assert_eq!(workspace.message_key, "conflictResolutionReady");
}

#[test]
fn treats_raw_conflict_count_as_conflicts_even_without_the_commit_mode() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        conflict_count: 1,
        changed_files_count: 5,
        ..input(CommitMode::CommitAndPush)
    });
    assert_eq!(recap.message_key, "conflicts");
    assert_eq!(number(&recap, "count"), Some(1));
}

#[test]
fn reports_delivery_without_offering_a_second_action_after_a_merge() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(42),
        turn_running: true,
        ..input(CommitMode::Merged)
    });
    assert_eq!(recap.message_key, "merged");
    assert_eq!(recap.state, DeliveryState::Delivered);
    assert_eq!(text(&recap, "pr"), Some("PR #42"));
    assert_eq!(recap.tone, RecapTone::Done);
    assert_eq!(recap.action, None);
}

#[test]
fn reports_a_closed_pr_without_inventing_a_next_git_step() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_state: Some("closed".into()),
        pr_number: Some(7),
        ..input(CommitMode::OpenPr)
    });
    assert_eq!(recap.message_key, "closed");
    assert_eq!(recap.state, DeliveryState::NeedsAttention);
    assert_eq!(recap.action, Some(RecapAction::Git(CommitMode::OpenPr)));
}

#[test]
fn surfaces_the_streaming_agent_above_findings_and_uncommitted_changes() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        turn_running: true,
        changed_files_count: 3,
        additions: 153,
        deletions: 88,
        pending_review_findings_count: 2,
        ..input(CommitMode::CommitAndPush)
    });
    assert_eq!(recap.message_key, "working");
    assert_eq!(recap.state, DeliveryState::InDevelopment);
    assert_eq!(number(&recap, "count"), Some(3));
    assert_eq!(number(&recap, "additions"), Some(153));
    assert_eq!(number(&recap, "deletions"), Some(88));
    assert_eq!(recap.tone, RecapTone::Working);
    assert_eq!(recap.action, Some(RecapAction::Activity));
}

#[test]
fn uses_the_no_changes_variant_while_the_agent_has_not_edited_anything() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        turn_running: true,
        ..input(CommitMode::CreatePr)
    });
    assert_eq!(recap.message_key, "workingClean");
}

#[test]
fn returns_to_the_git_action_after_the_turn_finishes() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        changed_files_count: 2,
        ..input(CommitMode::CommitAndPush)
    });
    assert_eq!(recap.message_key, "changes");
    assert!(matches!(recap.action, Some(RecapAction::Git(_))));
}

#[test]
fn puts_fresh_review_findings_ahead_of_committing() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        changed_files_count: 3,
        pending_review_findings_count: 4,
        ..input(CommitMode::CommitAndPush)
    });
    assert_eq!(recap.message_key, "findings");
    assert_eq!(number(&recap, "count"), Some(4));
    assert_eq!(recap.action, Some(RecapAction::Review));
}

#[test]
fn surfaces_delegations_waiting_for_review_before_committing() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        changed_files_count: 3,
        pending_delegation_results_count: 2,
        pending_review_findings_count: 4,
        ..input(CommitMode::CommitAndPush)
    });
    assert_eq!(recap.message_key, "delegationResults");
    assert_eq!(number(&recap, "count"), Some(2));
    assert_eq!(recap.tone, RecapTone::Attention);
    assert_eq!(recap.action, Some(RecapAction::Activity));
}

#[test]
fn maps_commit_and_push_to_the_changes_recap() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        changed_files_count: 2,
        additions: 10,
        deletions: 4,
        ..input(CommitMode::CommitAndPush)
    });
    assert_eq!(recap.message_key, "changes");
    assert_eq!(number(&recap, "additions"), Some(10));
    assert_eq!(
        recap.action.unwrap().label_key(),
        "commit.modes.commit-and-push.idle"
    );
}

#[test]
fn maps_push_to_the_ahead_recap() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        ahead_of_remote_count: 3,
        ..input(CommitMode::Push)
    });
    assert_eq!(recap.message_key, "ahead");
    assert_eq!(number(&recap, "count"), Some(3));
}

#[test]
fn maps_merge_to_a_ready_to_merge_recap_with_the_pr_reference() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(12),
        pr_state: Some("open".into()),
        ..input(CommitMode::Merge)
    });
    assert_eq!(recap.message_key, "mergeReady");
    assert_eq!(recap.state, DeliveryState::ReadyToDeliver);
    assert_eq!(text(&recap, "pr"), Some("PR #12"));
    assert_eq!(recap.tone, RecapTone::Ready);
    assert_eq!(recap.action, Some(RecapAction::Git(CommitMode::Merge)));
}

#[test]
fn maps_fix_to_failing_checks_with_attention_tone() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(9),
        pr_state: Some("open".into()),
        request_label: "MR",
        ..input(CommitMode::Fix)
    });
    assert_eq!(recap.message_key, "checksFailing");
    assert_eq!(text(&recap, "pr"), Some("MR #9"));
    assert_eq!(recap.tone, RecapTone::Attention);
}

#[test]
fn a_blocked_or_conflicting_pull_request_is_not_called_failing_checks() {
    let recap = |merge_state: &str| {
        build_workspace_recap(&WorkspaceRecapInput {
            pr_number: Some(9),
            pr_state: Some("open".into()),
            pr_merge_state: Some(merge_state.into()),
            request_label: "PR",
            ..input(CommitMode::Fix)
        })
        .message_key
    };
    assert_eq!(recap("UNSTABLE"), "checksFailing");
    assert_eq!(recap("BLOCKED"), "prBlocked");
    assert_eq!(recap("dirty"), "prConflicts");
}

#[test]
fn marks_an_open_change_request_as_awaiting_review() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(31),
        pr_state: Some("open".into()),
        ..input(CommitMode::OpenPr)
    });
    assert_eq!(recap.message_key, "prOpen");
    assert_eq!(recap.state, DeliveryState::AwaitingReview);
}

#[test]
fn surfaces_a_captured_delivery_failure_without_hiding_an_active_agent_turn() {
    let failure = Some(RecapDeliveryFailure {
        operation: "push".into(),
        classification: "authentication".into(),
    });
    let failed = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(9),
        delivery_failure: failure.clone(),
        ..input(CommitMode::Merge)
    });
    let recovering = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(9),
        delivery_failure: failure,
        turn_running: true,
        ..input(CommitMode::Merge)
    });
    assert_eq!(failed.state, DeliveryState::NeedsAttention);
    assert_eq!(failed.message_key, "deliveryFailure.push");
    assert_eq!(failed.action, Some(RecapAction::Delivery));
    assert_eq!(recovering.state, DeliveryState::InDevelopment);
    assert_eq!(recovering.message_key, "workingClean");
}

#[test]
fn puts_review_conflicts_and_failed_pipelines_ahead_of_merge_readiness() {
    let review_blocked = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(12),
        review_state: Some(RecapReviewState {
            review_state: Some("pending".into()),
            approvals_left: Some(1),
            has_conflicts: Some(true),
            ..review(0)
        }),
        pipeline: Some(pipeline("failed", 2)),
        ..input(CommitMode::Merge)
    });
    let pipeline_blocked = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(12),
        pipeline: Some(pipeline("failed", 2)),
        ..input(CommitMode::Merge)
    });
    assert_eq!(review_blocked.message_key, "reviewConflicts");
    assert_eq!(review_blocked.state, DeliveryState::Blocked);
    assert_eq!(pipeline_blocked.message_key, "pipelineFailed");
    assert_eq!(pipeline_blocked.state, DeliveryState::Blocked);
    assert_eq!(pipeline_blocked.action, Some(RecapAction::Pipeline));
}

#[test]
fn never_treats_unavailable_approval_or_pipeline_information_as_approval() {
    let approvals_unavailable = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(18),
        review_state: Some(RecapReviewState {
            review_state: Some("unknown".into()),
            approvals_available: false,
            approvals_left: None,
            ..review(0)
        }),
        ..input(CommitMode::Merge)
    });
    let pipeline_unavailable = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(18),
        pipeline_status: Some(SourceStatus::Error),
        ..input(CommitMode::Merge)
    });
    assert_eq!(approvals_unavailable.state, DeliveryState::AwaitingReview);
    assert_eq!(approvals_unavailable.message_key, "approvalsUnavailable");
    assert_eq!(pipeline_unavailable.state, DeliveryState::NeedsAttention);
    assert_eq!(pipeline_unavailable.message_key, "pipelineUnavailable");
}

#[test]
fn keeps_ready_only_after_the_available_review_and_pipeline_signals_pass() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(24),
        review_status: Some(SourceStatus::Available),
        review_state: Some(review(1)),
        pipeline_status: Some(SourceStatus::Available),
        pipeline: Some(pipeline("success", 0)),
        ..input(CommitMode::Merge)
    });
    assert_eq!(recap.state, DeliveryState::ReadyToDeliver);
    assert_eq!(recap.message_key, "mergeReady");
}

#[test]
fn applies_optional_project_approval_and_pipeline_rules_after_a_change_request_exists() {
    let policy = RecapDeliveryPolicy {
        minimum_approvals: 2,
        require_pipeline: true,
        ..RecapDeliveryPolicy::default()
    };
    let clean = build_workspace_recap(&WorkspaceRecapInput {
        delivery_policy: Some(policy),
        ..input(CommitMode::CreatePr)
    });
    let ready_for_request = build_workspace_recap(&WorkspaceRecapInput {
        committed_vs_base_count: 3,
        delivery_policy: Some(policy),
        ..input(CommitMode::CreatePr)
    });
    let approvals_pending = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(41),
        delivery_policy: Some(policy),
        review_state: Some(review(1)),
        pipeline_status: Some(SourceStatus::Available),
        pipeline: Some(pipeline("success", 0)),
        ..input(CommitMode::Merge)
    });
    let pipeline_missing = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(41),
        delivery_policy: Some(RecapDeliveryPolicy {
            minimum_approvals: 0,
            ..policy
        }),
        pipeline_status: Some(SourceStatus::Available),
        ..input(CommitMode::Merge)
    });
    assert_eq!(clean.message_key, "clean");
    assert_eq!(ready_for_request.message_key, "readyForPr");
    assert_eq!(approvals_pending.message_key, "policy.approvalsPending");
    assert_eq!(number(&approvals_pending, "count"), Some(1));
    assert_eq!(pipeline_missing.message_key, "policy.pipelineMissing");
    assert_eq!(approvals_pending.signals.len(), 6);
    assert!(signal(&approvals_pending, SignalId::Review).required);
    assert!(signal(&approvals_pending, SignalId::Pipeline).required);
    assert!(!signal(&approvals_pending, SignalId::Checks).required);
}

#[test]
fn keeps_missing_external_signals_unavailable_instead_of_passing_them() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(61),
        review_status: Some(SourceStatus::Error),
        pipeline_status: Some(SourceStatus::Idle),
        ..input(CommitMode::OpenPr)
    });
    assert_eq!(
        signal(&recap, SignalId::Review).state,
        SignalState::Unavailable
    );
    assert_eq!(
        signal(&recap, SignalId::Pipeline).state,
        SignalState::Unavailable
    );
    assert_eq!(
        signal(&recap, SignalId::Pipeline).message_key,
        "pipelineNotIntegrated"
    );
}

#[test]
fn routes_missing_required_checks_to_the_existing_project_automation_action() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(52),
        delivery_policy: Some(RecapDeliveryPolicy {
            require_before_merge_checks: true,
            ..RecapDeliveryPolicy::default()
        }),
        ..input(CommitMode::Merge)
    });
    assert_eq!(recap.state, DeliveryState::NeedsAttention);
    assert_eq!(recap.message_key, "policy.checksMissing");
    assert_eq!(recap.action, Some(RecapAction::Automation));
}

#[test]
fn falls_back_to_the_request_label_when_the_pr_number_is_unknown() {
    let recap = build_workspace_recap(&input(CommitMode::Fix));
    assert_eq!(text(&recap, "pr"), Some("PR"));
}

#[test]
fn suggests_creating_a_pr_once_commits_exist_against_the_base() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        committed_vs_base_count: 5,
        ..input(CommitMode::CreatePr)
    });
    assert_eq!(recap.message_key, "readyForPr");
    assert_eq!(recap.state, DeliveryState::InDevelopment);
    assert_eq!(number(&recap, "count"), Some(5));
    assert_eq!(text(&recap, "requestLabel"), Some("PR"));
    assert_eq!(
        recap.action.unwrap().label_key(),
        "commit.modes.create-pr.idle"
    );
}

#[test]
fn behind_the_base_offers_to_sync() {
    let recap = build_workspace_recap(&WorkspaceRecapInput {
        pr_number: Some(3),
        review_state: Some(RecapReviewState {
            behind_by: Some(4),
            ..review(1)
        }),
        ..input(CommitMode::Merge)
    });
    assert_eq!(recap.message_key, "behindBase");
    assert_eq!(number(&recap, "count"), Some(4));
    assert_eq!(recap.action, Some(RecapAction::Sync));
    assert_eq!(
        signal(&recap, SignalId::Review).state,
        SignalState::Attention
    );
}

#[test]
fn inside_changes_the_git_step_is_left_to_the_header() {
    assert_eq!(
        action_in_changes(Some(RecapAction::Git(CommitMode::CreatePr))),
        None
    );
    assert_eq!(
        action_in_changes(Some(RecapAction::Review)),
        Some(RecapAction::Review)
    );
}
