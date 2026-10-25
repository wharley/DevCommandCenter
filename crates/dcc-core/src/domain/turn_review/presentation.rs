//! `turn-review.logic.ts`: what the review surface says about a turn and
//! its Guarded Undo, in safe categories (never the backend's own text).

use chrono::{DateTime, Utc};

use super::{GuardedUndoCaptureSummary, GuardedUndoOperationSummary};
use crate::domain::session::TurnReviewFile;

/// `hasTurnReviewLineStats`: a binary file, or one without a preview and
/// no counted lines, has no `+N −M` worth showing.
pub fn has_turn_review_line_stats(file: &TurnReviewFile) -> bool {
    !(file.binary || (file.preview_unavailable && file.insertions == 0 && file.deletions == 0))
}

/// `canPrepareGuardedUndo`
pub fn can_prepare_guarded_undo(
    capture: Option<&GuardedUndoCaptureSummary>,
    active: Option<&GuardedUndoOperationSummary>,
) -> bool {
    capture.is_some_and(|capture| capture.state == "eligible") && active.is_none()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Aborted,
}

impl TurnOutcome {
    /// The key under `turnReview.outcomes`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Aborted => "aborted",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbortReason {
    Interrupted,
    Timeout,
    Provider,
    Other,
}

/// `resolveTurnReviewOutcome`: `None` for anything but a finished turn.
pub fn resolve_turn_review_outcome(
    turn_outcome: Option<&str>,
    outcome_reason: Option<&str>,
) -> Option<(TurnOutcome, Option<AbortReason>)> {
    match turn_outcome? {
        "completed" => return Some((TurnOutcome::Completed, None)),
        "aborted" => {}
        _ => return None,
    }
    let reason = match outcome_reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
    {
        None => AbortReason::Other,
        Some(reason) => {
            let reason = reason.to_lowercase();
            let any = |words: &[&str]| words.iter().any(|word| reason.contains(word));
            if any(&["timeout", "timed out", "tempo limite", "expir"]) {
                AbortReason::Timeout
            } else if any(&["interrupt", "abort", "cancel", "cancelad", "interromp"]) {
                AbortReason::Interrupted
            } else if any(&[
                "provider", "model", "process", "exit", "failed", "error", "falh",
            ]) {
                AbortReason::Provider
            } else {
                AbortReason::Other
            }
        }
    };
    Some((TurnOutcome::Aborted, Some(reason)))
}

/// `GuardedUndoCapturePresentation`: the shield's state and why, as keys
/// under `turnReview.guardedUndo.states` / `.reasons`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturePresentation {
    pub state: &'static str,
    pub reason: &'static str,
}

const KNOWN_STATES: [&str; 6] = [
    "eligible",
    "ineligible",
    "failed",
    "collecting",
    "expired",
    "consumed",
];

const UNSUPPORTED: [&str; 8] = [
    "adapter_unsupported",
    "filesystem_unsupported",
    "extended_metadata_unsupported",
    "symlink_or_reparse_point",
    "hardlink_unsupported",
    "non_regular_file",
    "submodule",
    "sparse_or_skip_worktree",
];

/// `resolveGuardedUndoCapture`: fails closed on states it does not know.
pub fn resolve_guarded_undo_capture(
    capture: Option<&GuardedUndoCaptureSummary>,
) -> CapturePresentation {
    let Some(capture) = capture else {
        return CapturePresentation {
            state: "unavailable",
            reason: "capture_v2_missing",
        };
    };
    let Some(state) = KNOWN_STATES
        .iter()
        .copied()
        .find(|known| *known == capture.state)
    else {
        return CapturePresentation {
            state: "failed",
            reason: "ineligible",
        };
    };
    let reason = match capture.reason_code.as_deref() {
        None => match state {
            "eligible" => "protected",
            "collecting" => "collecting",
            "expired" => "expired",
            "consumed" => "consumed",
            _ => "ineligible",
        },
        Some(code) => capture_reason(code),
    };
    CapturePresentation { state, reason }
}

fn capture_reason(code: &str) -> &'static str {
    match code {
        "capture_interrupted"
        | "operation_interrupted"
        | "capture_timeout"
        | "capture_race"
        | "concurrent_workspace_mutation"
        | "mutation_in_progress" => "interrupted",
        "untracked_path" => "untracked_path",
        "index_changed" => "index_changed",
        "metadata_changed" => "metadata_changed",
        "detached_head" => "detached_head",
        "no_target_changes" => "no_target_changes",
        "head_changed" | "ref_changed" | "repository_identity_changed" => "git_changed",
        "unsupported_status" | "unmerged_path" | "tracked_manifest_changed" => "unsupported_change",
        "git_filter_present"
        | "working_tree_encoding_present"
        | "git_attributes_changed"
        | "assume_unchanged" => "unsupported_git",
        code if UNSUPPORTED.contains(&code) => "unsupported",
        "file_too_large"
        | "set_too_large"
        | "baseline_too_large"
        | "too_many_files"
        | "too_many_baseline_files"
        | "insufficient_disk_space"
        | "index_too_large" => "limit",
        "artifact_missing"
        | "artifact_corrupt"
        | "artifact_store_unsafe"
        | "app_instance_conflict"
        | "permission_denied"
        | "io_error" => "storage",
        _ => "ineligible",
    }
}

/// `resolveGuardedUndoFailureReason`: the key under
/// `turnReview.guardedUndo.failureReasons`.
pub fn resolve_guarded_undo_failure_reason(code: Option<&str>) -> &'static str {
    match code.unwrap_or_default() {
        "retention_expired" | "preview_expired" | "preview_consumed" => "expired",
        "head_changed"
        | "ref_changed"
        | "index_changed"
        | "repository_identity_changed"
        | "target_missing"
        | "target_result_mismatch"
        | "metadata_changed"
        | "git_attributes_changed"
        | "tracked_manifest_changed"
        | "preview_context_changed" => "changed",
        "artifact_missing"
        | "artifact_corrupt"
        | "displaced_file_missing"
        | "displaced_file_corrupt" => "corrupt",
        "displaced_target_mismatch"
        | "recovery_target_changed"
        | "exchange_rollback_failed"
        | "manual_recovery_required"
        | "operation_interrupted" => "recovery",
        "mutation_in_progress" | "concurrent_workspace_mutation" | "app_instance_conflict" => {
            "busy"
        }
        code if UNSUPPORTED.contains(&code) => "unsupported",
        _ => "unavailable",
    }
}

/// `isGuardedUndoPreviewExpired`: an unreadable time counts as expired.
pub fn is_guarded_undo_preview_expired(expires_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(expires_at).map_or(true, |at| at.with_timezone(&Utc) <= now)
}
