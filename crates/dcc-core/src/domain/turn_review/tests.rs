//! `turn-review.logic.test.ts` and `turn-undo-across-projects.logic.test.ts`.

use chrono::{DateTime, Utc};

use super::*;
use crate::domain::session::TurnReviewFile;

fn capture(state: &str, reason: Option<&str>) -> GuardedUndoCaptureSummary {
    GuardedUndoCaptureSummary {
        state: state.into(),
        reason_code: reason.map(str::to_string),
        file_count: 0,
        artifact_bytes: 0,
        completed_at: Some("t1".into()),
        expires_at: None,
    }
}

fn presented(state: &'static str, reason: &'static str) -> CapturePresentation {
    CapturePresentation { state, reason }
}

#[test]
fn maps_outcome_reasons_to_safe_categories_without_returning_backend_text() {
    assert_eq!(
        resolve_turn_review_outcome(Some("completed"), Some("ignored detail")),
        Some((TurnOutcome::Completed, None))
    );
    assert_eq!(
        resolve_turn_review_outcome(Some("aborted"), Some("Interrupted by user")),
        Some((TurnOutcome::Aborted, Some(AbortReason::Interrupted)))
    );
    assert_eq!(
        resolve_turn_review_outcome(Some("aborted"), Some("provider process exited")),
        Some((TurnOutcome::Aborted, Some(AbortReason::Provider)))
    );
    assert_eq!(
        resolve_turn_review_outcome(Some("aborted"), Some("secret/path/token")),
        Some((TurnOutcome::Aborted, Some(AbortReason::Other)))
    );
    assert_eq!(
        resolve_turn_review_outcome(Some("unexpected"), Some("anything")),
        None
    );
}

#[test]
fn shows_an_explicit_unavailable_status_when_capture_v2_is_absent() {
    assert_eq!(
        resolve_guarded_undo_capture(None),
        presented("unavailable", "capture_v2_missing")
    );
}

#[test]
fn maps_guarded_undo_state_and_reason_without_exposing_artifact_metadata() {
    assert_eq!(
        resolve_guarded_undo_capture(Some(&capture("ineligible", Some("hardlink_unsupported")))),
        presented("ineligible", "unsupported")
    );
}

#[test]
fn explains_ineligible_capture_reasons() {
    for (code, reason) in [
        ("untracked_path", "untracked_path"),
        ("index_changed", "index_changed"),
        ("metadata_changed", "metadata_changed"),
        ("detached_head", "detached_head"),
        ("no_target_changes", "no_target_changes"),
        ("head_changed", "git_changed"),
        ("unsupported_status", "unsupported_change"),
        ("git_filter_present", "unsupported_git"),
    ] {
        assert_eq!(
            resolve_guarded_undo_capture(Some(&capture("ineligible", Some(code)))),
            presented("ineligible", reason),
            "{code}"
        );
    }
}

#[test]
fn maps_a_null_reason_to_each_states_own_safe_presentation() {
    for (state, reason) in [
        ("eligible", "protected"),
        ("collecting", "collecting"),
        ("expired", "expired"),
        ("consumed", "consumed"),
    ] {
        assert_eq!(
            resolve_guarded_undo_capture(Some(&capture(state, None))),
            presented(state, reason)
        );
    }
}

#[test]
fn fails_closed_when_the_persisted_guarded_undo_state_is_unknown() {
    assert_eq!(
        resolve_guarded_undo_capture(Some(&capture("future_state", None))),
        presented("failed", "ineligible")
    );
}

#[test]
fn enables_prepare_only_for_an_eligible_capture_without_an_active_operation() {
    let eligible = capture("eligible", None);
    let active = GuardedUndoOperationSummary {
        status: "recovery_required".into(),
        operation_id: "operation-1".into(),
        reason_code: Some("manual_recovery_required".into()),
    };
    assert!(can_prepare_guarded_undo(Some(&eligible), None));
    assert!(!can_prepare_guarded_undo(Some(&eligible), Some(&active)));
    assert!(!can_prepare_guarded_undo(
        Some(&capture("consumed", None)),
        None
    ));
}

#[test]
fn maps_guarded_undo_failure_reasons() {
    for (code, expected) in [
        ("preview_expired", "expired"),
        ("head_changed", "changed"),
        ("artifact_corrupt", "corrupt"),
        ("manual_recovery_required", "recovery"),
        ("mutation_in_progress", "busy"),
        ("filesystem_unsupported", "unsupported"),
        ("future_reason", "unavailable"),
    ] {
        assert_eq!(resolve_guarded_undo_failure_reason(Some(code)), expected);
    }
    assert_eq!(resolve_guarded_undo_failure_reason(None), "unavailable");
}

#[test]
fn fails_closed_for_invalid_or_expired_preview_timestamps() {
    let now: DateTime<Utc> = "2026-08-28T12:00:00Z".parse().unwrap();
    assert!(!is_guarded_undo_preview_expired(
        "2026-08-28T12:00:01Z",
        now
    ));
    assert!(is_guarded_undo_preview_expired("2026-08-28T12:00:00Z", now));
    assert!(is_guarded_undo_preview_expired("invalid", now));
}

#[test]
fn line_stats_hide_for_binaries_and_files_without_a_preview() {
    let file = |binary, preview_unavailable, insertions| TurnReviewFile {
        path: "a.rs".into(),
        old_path: None,
        status: "M".into(),
        insertions,
        deletions: 0,
        untracked: false,
        binary,
        preview_unavailable,
    };
    assert!(has_turn_review_line_stats(&file(false, false, 2)));
    assert!(!has_turn_review_line_stats(&file(true, false, 2)));
    assert!(!has_turn_review_line_stats(&file(false, true, 0)));
    assert!(has_turn_review_line_stats(&file(false, true, 3)));
}

fn review(snapshot: &str, files: usize, undo: &str, active: bool) -> TurnReviewSummary {
    TurnReviewSummary {
        snapshot_id: snapshot.into(),
        session_id: "session".into(),
        turn_id: "turn".into(),
        workspace_id: snapshot.into(),
        state: "completed".into(),
        compatibility: "matches_result".into(),
        base_fingerprint: None,
        result_fingerprint: None,
        files: (0..files)
            .map(|index| TurnReviewFile {
                path: format!("file-{index}.ts"),
                old_path: None,
                status: "M".into(),
                insertions: 1,
                deletions: 0,
                untracked: false,
                binary: false,
                preview_unavailable: false,
            })
            .collect(),
        insertions: 1,
        deletions: 0,
        diff_truncated: false,
        excluded_preexisting_untracked_count: 0,
        observed_validations: Vec::new(),
        turn_outcome: Some("completed".into()),
        outcome_reason: None,
        error: None,
        completed_at: None,
        guarded_undo: Some(capture(undo, None)),
        active_undo: active.then(|| GuardedUndoOperationSummary {
            status: "running".into(),
            operation_id: "op".into(),
            reason_code: None,
        }),
    }
}

fn project(id: &str, name: &str, review: Option<TurnReviewSummary>) -> TurnUndoProject {
    TurnUndoProject {
        workspace_id: id.into(),
        name: name.into(),
        review,
    }
}

fn target(id: &str, name: &str) -> TurnUndoTarget {
    TurnUndoTarget {
        workspace_id: id.into(),
        name: name.into(),
        snapshot_id: id.into(),
    }
}

fn ready(snapshot: &str) -> PrepareGuardedUndoOutput {
    PrepareGuardedUndoOutput::Ready {
        snapshot_id: snapshot.into(),
        preview_token: format!("token-{snapshot}"),
        expires_at: "2026-10-13T12:00:00Z".into(),
        file_count: 1,
        total_bytes: 10,
        files: Vec::new(),
        unrelated_paths_are_not_targets: true,
    }
}

fn prepared(id: &str) -> PreparedTurnUndo {
    PreparedTurnUndo {
        target: target(id, &id.to_uppercase()),
        preview: ReadyPreview {
            preview_token: format!("token-{id}"),
            expires_at: "2026-10-13T12:00:00Z".into(),
            file_count: 1,
            total_bytes: 10,
            files: Vec::new(),
        },
    }
}

fn completed() -> ExecuteGuardedUndoOutput {
    ExecuteGuardedUndoOutput::Completed {
        operation_id: None,
        reason_code: None,
    }
}

#[test]
fn targets_every_changed_project_undo_can_restore_and_lists_the_rest() {
    let plan = plan_turn_undo(&[
        project("dcc", "DCC", Some(review("dcc", 1, "eligible", false))),
        project("lp", "LP", Some(review("lp", 1, "ineligible", false))),
        project("api", "API", Some(review("api", 0, "eligible", false))),
        project("web", "Web", None),
    ]);
    assert_eq!(plan.targets, vec![target("dcc", "DCC")]);
    assert_eq!(plan.uncovered, vec![("lp".to_string(), "LP".to_string())]);
}

#[test]
fn leaves_a_project_with_an_undo_already_running_out_of_the_targets() {
    let plan = plan_turn_undo(&[project(
        "dcc",
        "DCC",
        Some(review("dcc", 1, "eligible", true)),
    )]);
    assert!(plan.targets.is_empty());
    assert_eq!(plan.uncovered, vec![("dcc".to_string(), "DCC".to_string())]);
}

#[test]
fn is_ready_only_when_every_project_is_ready() {
    let targets = [target("dcc", "DCC"), target("lp", "LP")];
    let result = prepare_turn_undo::<()>(&targets, |id| Ok(ready(id))).unwrap();
    let TurnUndoPreparation::Ready(prepared) = result else {
        panic!("ready");
    };
    let ids: Vec<&str> = prepared
        .iter()
        .map(|project| project.target.workspace_id.as_str())
        .collect();
    assert_eq!(ids, vec!["dcc", "lp"]);
}

#[test]
fn stops_at_the_first_project_that_is_not_free() {
    let targets = [target("dcc", "DCC"), target("lp", "LP")];
    let mut calls = 0;
    let result = prepare_turn_undo::<()>(&targets, |id| {
        calls += 1;
        Ok(if id == "dcc" {
            PrepareGuardedUndoOutput::Blocked {
                snapshot_id: id.into(),
                reason_code: "head_changed".into(),
                details_available: false,
            }
        } else {
            ready(id)
        })
    })
    .unwrap();
    let TurnUndoPreparation::Blocked {
        target: blocked,
        result,
        reason_code,
    } = result
    else {
        panic!("blocked");
    };
    assert_eq!(blocked, targets[0]);
    assert_eq!((result, reason_code.as_str()), ("blocked", "head_changed"));
    assert_eq!(calls, 1);
}

#[test]
fn treats_a_preview_that_may_touch_unrelated_paths_as_unavailable() {
    let result = prepare_turn_undo::<()>(&[target("dcc", "DCC")], |id| {
        let PrepareGuardedUndoOutput::Ready {
            snapshot_id,
            preview_token,
            expires_at,
            file_count,
            total_bytes,
            files,
            ..
        } = ready(id)
        else {
            unreachable!()
        };
        Ok(PrepareGuardedUndoOutput::Ready {
            snapshot_id,
            preview_token,
            expires_at,
            file_count,
            total_bytes,
            files,
            unrelated_paths_are_not_targets: false,
        })
    })
    .unwrap();
    assert!(matches!(
        result,
        TurnUndoPreparation::Blocked {
            result: "unavailable",
            ..
        }
    ));
}

#[test]
fn restores_every_project_in_order() {
    let mut tokens = Vec::new();
    let results = execute_turn_undo::<()>(&[prepared("dcc"), prepared("lp")], |token| {
        tokens.push(token.to_string());
        Ok(completed())
    });
    assert_eq!(tokens, vec!["token-dcc", "token-lp"]);
    let statuses: Vec<&str> = results.iter().map(|result| result.status).collect();
    assert_eq!(statuses, vec!["completed", "completed"]);
}

#[test]
fn stops_after_a_project_that_does_not_complete_and_reports_the_rest() {
    let mut calls = 0;
    let results = execute_turn_undo::<()>(
        &[prepared("dcc"), prepared("lp"), prepared("api")],
        |token| {
            calls += 1;
            Ok(if token == "token-lp" {
                ExecuteGuardedUndoOutput::RolledBack {
                    operation_id: None,
                    reason_code: Some("file_changed".into()),
                }
            } else {
                completed()
            })
        },
    );
    let step = |id: &str, name: &str, status, reason: Option<&str>| TurnUndoStepResult {
        workspace_id: id.into(),
        name: name.into(),
        status,
        reason_code: reason.map(str::to_string),
    };
    assert_eq!(
        results,
        vec![
            step("dcc", "DCC", "completed", None),
            step("lp", "LP", "rolled_back", Some("file_changed")),
            step("api", "API", "not_attempted", None),
        ]
    );
    assert_eq!(calls, 2);
}

#[test]
fn reports_a_request_that_fails_as_failed_and_stops() {
    let results = execute_turn_undo(&[prepared("dcc"), prepared("lp")], |_| Err("offline"));
    let statuses: Vec<&str> = results.iter().map(|result| result.status).collect();
    assert_eq!(statuses, vec!["failed", "not_attempted"]);
}
