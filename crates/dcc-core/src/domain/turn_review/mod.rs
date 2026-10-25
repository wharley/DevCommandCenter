//! What a turn changed and whether Guarded Undo can put it back: the
//! desktop's turn review contract (`last_turn_review`,
//! `turn_review_file_diff`, `prepare_guarded_undo`, `execute_guarded_undo`)
//! and how its review surfaces read it (`turn-review.logic.ts`,
//! `turn-undo-across-projects.logic.ts`).

mod presentation;
mod undo_plan;

pub use presentation::*;
pub use undo_plan::*;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::session::TurnReviewFile;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnReviewSummary {
    pub snapshot_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub workspace_id: String,
    pub state: String,
    pub compatibility: String,
    pub base_fingerprint: Option<String>,
    pub result_fingerprint: Option<String>,
    pub files: Vec<TurnReviewFile>,
    pub insertions: u32,
    pub deletions: u32,
    pub diff_truncated: bool,
    pub excluded_preexisting_untracked_count: usize,
    pub observed_validations: Vec<String>,
    pub turn_outcome: Option<String>,
    pub outcome_reason: Option<String>,
    pub error: Option<String>,
    pub completed_at: Option<String>,
    pub guarded_undo: Option<GuardedUndoCaptureSummary>,
    pub active_undo: Option<GuardedUndoOperationSummary>,
}

/// Content-free capture-v2 status associated with this review snapshot.
/// Artifact locations, hashes, physical identities, paths, and bytes never
/// cross the desktop contract.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuardedUndoCaptureSummary {
    pub state: String,
    pub reason_code: Option<String>,
    pub file_count: u32,
    pub artifact_bytes: u64,
    pub completed_at: Option<String>,
    pub expires_at: Option<String>,
}

/// Content-free status for a nonterminal Guarded Undo operation. The opaque
/// operation id is safe to display/use for diagnostics; recovery artifacts and
/// target identities remain backend-only.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuardedUndoOperationSummary {
    pub status: String,
    pub operation_id: String,
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuardedUndoPreviewFile {
    /// Display-only path. Execute accepts only the opaque preview token and
    /// never trusts paths, hashes, or replacement bytes sent by the UI.
    pub display_path: String,
    pub size: u64,
    pub binary: bool,
    pub preview: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PrepareGuardedUndoOutput {
    Ready {
        snapshot_id: String,
        preview_token: String,
        expires_at: String,
        file_count: u32,
        total_bytes: u64,
        files: Vec<GuardedUndoPreviewFile>,
        unrelated_paths_are_not_targets: bool,
    },
    Blocked {
        snapshot_id: String,
        reason_code: String,
        details_available: bool,
    },
    Unavailable {
        snapshot_id: String,
        reason_code: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ExecuteGuardedUndoOutput {
    Completed {
        operation_id: Option<String>,
        reason_code: Option<String>,
    },
    Blocked {
        operation_id: Option<String>,
        reason_code: Option<String>,
    },
    RolledBack {
        operation_id: Option<String>,
        reason_code: Option<String>,
    },
    RecoveryRequired {
        operation_id: Option<String>,
        reason_code: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnReviewFileDiffOutput {
    pub snapshot_id: String,
    pub path: String,
    pub diff: Option<String>,
    pub preview_unavailable: bool,
}

#[cfg(test)]
mod tests;
