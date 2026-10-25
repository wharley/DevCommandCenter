//! `turn-undo-across-projects.logic.ts`: Undo for a turn that changed
//! several projects of one task. Every project is prepared before anything
//! is restored, and the restores run one after the other, stopping at the
//! first that does not complete (there is no transaction across projects).

use super::{
    can_prepare_guarded_undo, ExecuteGuardedUndoOutput, GuardedUndoPreviewFile,
    PrepareGuardedUndoOutput, TurnReviewSummary,
};

/// One project of a multi-project turn and its review of that turn.
#[derive(Clone, Debug)]
pub struct TurnUndoProject {
    pub workspace_id: String,
    pub name: String,
    pub review: Option<TurnReviewSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnUndoTarget {
    pub workspace_id: String,
    pub name: String,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnUndoPlan {
    /// Projects whose changes in this turn Undo can restore.
    pub targets: Vec<TurnUndoTarget>,
    /// Projects the turn changed but Undo cannot restore: (id, name).
    pub uncovered: Vec<(String, String)>,
}

/// `planTurnUndo`: a project the turn did not change is in neither list.
pub fn plan_turn_undo(projects: &[TurnUndoProject]) -> TurnUndoPlan {
    let mut plan = TurnUndoPlan::default();
    for project in projects {
        let Some(review) = project
            .review
            .as_ref()
            .filter(|review| !review.files.is_empty())
        else {
            continue;
        };
        if can_prepare_guarded_undo(review.guarded_undo.as_ref(), review.active_undo.as_ref()) {
            plan.targets.push(TurnUndoTarget {
                workspace_id: project.workspace_id.clone(),
                name: project.name.clone(),
                snapshot_id: review.snapshot_id.clone(),
            });
        } else {
            plan.uncovered
                .push((project.workspace_id.clone(), project.name.clone()));
        }
    }
    plan
}

/// A project's ready preview.
#[derive(Clone, Debug)]
pub struct PreparedTurnUndo {
    pub target: TurnUndoTarget,
    pub preview: ReadyPreview,
}

#[derive(Clone, Debug)]
pub enum TurnUndoPreparation {
    Ready(Vec<PreparedTurnUndo>),
    /// The first project that cannot be undone right now.
    Blocked {
        target: TurnUndoTarget,
        /// `blocked` or `unavailable`.
        result: &'static str,
        reason_code: String,
    },
}

/// A preview Undo can run.
#[derive(Clone, Debug)]
pub struct ReadyPreview {
    pub preview_token: String,
    pub expires_at: String,
    pub file_count: u32,
    pub total_bytes: u64,
    pub files: Vec<GuardedUndoPreviewFile>,
}

/// What one preparation means for a single snapshot: a ready preview that
/// may touch paths the turn did not change is not ready (`blocked` or
/// `unavailable`, and why, otherwise).
pub fn ready_preview(
    output: PrepareGuardedUndoOutput,
) -> Result<ReadyPreview, (&'static str, String)> {
    match output {
        PrepareGuardedUndoOutput::Ready {
            preview_token,
            expires_at,
            file_count,
            total_bytes,
            files,
            unrelated_paths_are_not_targets: true,
            ..
        } => Ok(ReadyPreview {
            preview_token,
            expires_at,
            file_count,
            total_bytes,
            files,
        }),
        PrepareGuardedUndoOutput::Ready { .. } => {
            Err(("unavailable", "invalid_persisted_record".to_string()))
        }
        PrepareGuardedUndoOutput::Blocked { reason_code, .. } => Err(("blocked", reason_code)),
        PrepareGuardedUndoOutput::Unavailable { reason_code, .. } => {
            Err(("unavailable", reason_code))
        }
    }
}

/// `prepareTurnUndo`: preparing is read-only, so stopping at the first
/// project that is not free leaves every project as it is.
pub fn prepare_turn_undo<E>(
    targets: &[TurnUndoTarget],
    mut prepare: impl FnMut(&str) -> Result<PrepareGuardedUndoOutput, E>,
) -> Result<TurnUndoPreparation, E> {
    let mut prepared = Vec::new();
    for target in targets {
        match ready_preview(prepare(&target.snapshot_id)?) {
            Ok(preview) => prepared.push(PreparedTurnUndo {
                target: target.clone(),
                preview,
            }),
            Err((result, reason_code)) => {
                return Ok(TurnUndoPreparation::Blocked {
                    target: target.clone(),
                    result,
                    reason_code,
                });
            }
        }
    }
    Ok(TurnUndoPreparation::Ready(prepared))
}

/// A project's restore: the execute status, or `failed` / `not_attempted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnUndoStepResult {
    pub workspace_id: String,
    pub name: String,
    pub status: &'static str,
    pub reason_code: Option<String>,
}

impl ExecuteGuardedUndoOutput {
    /// The status as the contract names it, and its reason.
    pub fn status(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Completed { reason_code, .. } => ("completed", reason_code.as_deref()),
            Self::Blocked { reason_code, .. } => ("blocked", reason_code.as_deref()),
            Self::RolledBack { reason_code, .. } => ("rolled_back", reason_code.as_deref()),
            Self::RecoveryRequired { reason_code, .. } => {
                ("recovery_required", reason_code.as_deref())
            }
        }
    }
}

/// `executeTurnUndo`: each project restores whole or not at all; the first
/// that does not complete stops the rest, and every project says what
/// happened to it.
pub fn execute_turn_undo<E>(
    prepared: &[PreparedTurnUndo],
    mut execute: impl FnMut(&str) -> Result<ExecuteGuardedUndoOutput, E>,
) -> Vec<TurnUndoStepResult> {
    let mut results = Vec::new();
    let mut stopped = false;
    for project in prepared {
        let step = |status, reason_code| TurnUndoStepResult {
            workspace_id: project.target.workspace_id.clone(),
            name: project.target.name.clone(),
            status,
            reason_code,
        };
        if stopped {
            results.push(step("not_attempted", None));
            continue;
        }
        match execute(&project.preview.preview_token) {
            Ok(outcome) => {
                let (status, reason) = outcome.status();
                stopped = status != "completed";
                results.push(step(status, reason.map(str::to_string)));
            }
            Err(_) => {
                stopped = true;
                results.push(step("failed", None));
            }
        }
    }
    results
}
