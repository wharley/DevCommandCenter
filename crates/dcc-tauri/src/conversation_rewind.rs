//! "Edit from here": what a rewind does to the provider's native memory and
//! to the files the removed turns changed.
//!
//! Every decision below is made from durable facts before anything changes.
//! Files are restored only by Guarded Undo, newest turn first, and only after
//! the person chose it; the conversation is cut only after every requested
//! file is back. When the provider cannot cut its own conversation, the work
//! continues in a new thread re-anchored on what came before (fork) instead
//! of pretending the provider forgot.

use async_trait::async_trait;
use dcc_core::{
    application::ConversationRewindPlan, domain::session::TurnId, ports::NativeRewindCut,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::guarded_undo_runtime::{
    GuardedUndoChainPlanResult, GuardedUndoExecuteResult, GuardedUndoPrepareResult,
};

/// Where the work continues after the rewind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "mode",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RewindProviderPlan {
    /// This thread; the provider's native conversation is cut at the same
    /// point.
    Native,
    /// This thread; nothing is kept, so the provider starts over.
    Fresh,
    /// A new thread re-anchored on the messages before the anchor, because
    /// this provider's own conversation cannot be cut.
    NewThread { reason: RewindNewThreadReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RewindNewThreadReason {
    /// The provider has no way to cut its native conversation.
    ProviderWithoutRewind,
    /// DCC holds no native conversation to cut (it was lost or refused).
    NoNativeConversation,
    /// The kept point was never recorded for this conversation (turns made
    /// before DCC recorded it, or another native conversation).
    CheckpointMissing,
}

/// The native cut DCC stores for the next runtime of this session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeRewindTarget {
    pub native_id: String,
    pub checkpoint: String,
}

/// Decides how the provider follows the rewind. `checkpoint_for` returns the
/// (native conversation, checkpoint) the adapter recorded for a turn.
pub(crate) fn plan_provider_context(
    cut: Option<NativeRewindCut>,
    plan: &ConversationRewindPlan,
    stored_native_id: Option<&str>,
    checkpoint_for: impl Fn(&TurnId) -> Option<(String, String)>,
) -> (RewindProviderPlan, Option<NativeRewindTarget>) {
    let new_thread = |reason| (RewindProviderPlan::NewThread { reason }, None);
    let Some(cut) = cut else {
        return new_thread(RewindNewThreadReason::ProviderWithoutRewind);
    };
    if plan.kept_turn_count == 0 {
        return (RewindProviderPlan::Fresh, None);
    }
    let Some(native_id) = stored_native_id else {
        return new_thread(RewindNewThreadReason::NoNativeConversation);
    };
    let checkpoint_turn = match cut {
        NativeRewindCut::AfterLastKeptTurn => plan.last_kept_turn_id.as_ref(),
        NativeRewindCut::FromAnchorTurn => Some(&plan.anchor_turn_id),
    };
    match checkpoint_turn.and_then(checkpoint_for) {
        Some((checkpoint_native_id, checkpoint)) if checkpoint_native_id == native_id => (
            RewindProviderPlan::Native,
            Some(NativeRewindTarget {
                native_id: native_id.to_string(),
                checkpoint,
            }),
        ),
        _ => new_thread(RewindNewThreadReason::CheckpointMissing),
    }
}

/// What DCC knows about one removed turn's file changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TurnFileFacts {
    pub turn_id: TurnId,
    pub snapshot_id: Option<String>,
    /// Files the Last Turn review recorded; `None` without a review.
    pub changed_file_count: Option<usize>,
    /// Guarded Undo capture state and reason, when one exists.
    pub capture_state: Option<String>,
    pub capture_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RewindFilePreview {
    pub turn_id: String,
    pub display_path: String,
    pub binary: bool,
    pub preview: Option<String>,
}

/// Why the removed turns' files cannot be restored. `stage` says whether the
/// capture itself was not protected or the workspace no longer matches it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RewindFilesStage {
    Capture,
    Prepare,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RewindFilesPlan {
    /// No removed turn changed a file.
    NothingToRestore,
    /// Every changed file can be restored; the person decides whether to.
    Restorable {
        turn_count: u32,
        file_count: u32,
        files: Vec<RewindFilePreview>,
    },
    /// Files changed but cannot be restored safely; they stay as they are.
    NotRestorable {
        turn_id: String,
        stage: RewindFilesStage,
        reason_code: String,
    },
}

/// Guarded Undo, as the rewind uses it.
#[async_trait]
pub(crate) trait RewindFileRestorer: Send + Sync {
    /// Read-only: can these captures be restored in this order (newest
    /// first), each after the ones before it?
    async fn plan(&self, snapshot_ids_newest_first: Vec<String>) -> GuardedUndoChainPlanResult;
    /// Prepares one capture against the disk as it is now, accepting a file
    /// that a newer removed turn's completed restore just put back.
    async fn prepare(&self, snapshot_id: String) -> GuardedUndoPrepareResult;
    async fn execute(&self, preview_token: String) -> GuardedUndoExecuteResult;
}

enum TurnFiles {
    Skip,
    Restore(String),
    Blocked(RewindFilesPlan),
}

fn classify_turn_files(facts: &TurnFileFacts) -> TurnFiles {
    let blocked = |stage, reason_code: &str| {
        TurnFiles::Blocked(RewindFilesPlan::NotRestorable {
            turn_id: facts.turn_id.0.clone(),
            stage,
            reason_code: reason_code.to_string(),
        })
    };
    match facts.changed_file_count {
        Some(0) => return TurnFiles::Skip,
        // No review: DCC cannot say what the turn changed.
        None => return blocked(RewindFilesStage::Capture, "capture_v2_missing"),
        Some(_) => {}
    }
    match (facts.capture_state.as_deref(), facts.snapshot_id.as_ref()) {
        (Some("eligible"), Some(snapshot_id)) => TurnFiles::Restore(snapshot_id.clone()),
        // Already restored with "Undo last turn".
        (Some("consumed"), _) => TurnFiles::Skip,
        (Some("ineligible"), _) if facts.capture_reason.as_deref() == Some("no_target_changes") => {
            TurnFiles::Skip
        }
        (Some(state), _) => blocked(
            RewindFilesStage::Capture,
            facts.capture_reason.as_deref().unwrap_or(state),
        ),
        (None, _) => blocked(RewindFilesStage::Capture, "capture_v2_missing"),
    }
}

/// The removed turns' restorable captures, newest first, or why not.
fn restorable_snapshots(
    facts_oldest_first: &[TurnFileFacts],
) -> Result<Vec<(TurnId, String)>, RewindFilesPlan> {
    let mut snapshots = Vec::new();
    for facts in facts_oldest_first.iter().rev() {
        match classify_turn_files(facts) {
            TurnFiles::Skip => {}
            TurnFiles::Restore(snapshot_id) => snapshots.push((facts.turn_id.clone(), snapshot_id)),
            TurnFiles::Blocked(plan) => return Err(plan),
        }
    }
    Ok(snapshots)
}

/// Read-only check of every capture, newest first, before anything is
/// written. Guarded Undo checks each path against what the newer captures
/// will put back, or against the disk when no newer capture restores it, so
/// a file the person changed after a turn, or between two turns that both
/// changed it, blocks here. A file several removed turns changed is restored
/// turn by turn and listed once.
pub(crate) async fn plan_files(
    restorer: &dyn RewindFileRestorer,
    facts_oldest_first: &[TurnFileFacts],
) -> RewindFilesPlan {
    let snapshots = match restorable_snapshots(facts_oldest_first) {
        Ok(snapshots) => snapshots,
        Err(plan) => return plan,
    };
    let Some((first_turn, _)) = snapshots.first() else {
        return RewindFilesPlan::NothingToRestore;
    };
    let turn_for = |snapshot_id: &str| {
        snapshots
            .iter()
            .find(|(_, id)| id == snapshot_id)
            .map_or(first_turn, |(turn_id, _)| turn_id)
            .0
            .clone()
    };
    let not_restorable = |turn_id: String, reason_code: String| RewindFilesPlan::NotRestorable {
        turn_id,
        stage: RewindFilesStage::Prepare,
        reason_code,
    };
    let snapshot_ids = snapshots.iter().map(|(_, id)| id.clone()).collect();
    let planned = match restorer.plan(snapshot_ids).await {
        GuardedUndoChainPlanResult::Ready(planned) => planned,
        GuardedUndoChainPlanResult::Blocked {
            snapshot_id,
            reason_code,
        }
        | GuardedUndoChainPlanResult::Unavailable {
            snapshot_id,
            reason_code,
        } => return not_restorable(turn_for(&snapshot_id), reason_code),
    };
    if planned.len() != snapshots.len()
        || planned
            .iter()
            .zip(&snapshots)
            .any(|(restore, (_, snapshot_id))| restore.snapshot_id != *snapshot_id)
    {
        return not_restorable(first_turn.0.clone(), "invalid_persisted_record".to_string());
    }
    let mut files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    for (restore, (turn_id, _)) in planned.into_iter().zip(&snapshots) {
        for preview in restore.files {
            if seen_paths.insert(preview.display_path.clone()) {
                files.push(RewindFilePreview {
                    turn_id: turn_id.0.clone(),
                    display_path: preview.display_path,
                    binary: preview.binary,
                    preview: preview.preview,
                });
            }
        }
    }
    RewindFilesPlan::Restorable {
        turn_count: snapshots.len() as u32,
        file_count: files.len() as u32,
        files,
    }
}

/// How restoring stopped, when it did not finish.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RewindFilesStopped {
    /// Turns whose files were restored before the stop, newest first.
    pub restored_turn_ids: Vec<String>,
    pub turn_id: String,
    /// `blocked`, `rolled_back` or `recovery_required`, as Guarded Undo
    /// reported it for `turn_id`.
    pub outcome: String,
    pub reason_code: Option<String>,
}

/// Restores the removed turns' files newest first, each through a fresh
/// Guarded Undo prepare and execute against the disk as the newer restores
/// left it. Stops at the first turn that does not complete; earlier
/// restores stay done and are reported.
pub(crate) async fn restore_files(
    restorer: &dyn RewindFileRestorer,
    facts_oldest_first: &[TurnFileFacts],
) -> Result<Vec<TurnId>, RewindFilesStopped> {
    let snapshots = restorable_snapshots(facts_oldest_first).map_err(|plan| match plan {
        RewindFilesPlan::NotRestorable {
            turn_id,
            reason_code,
            ..
        } => RewindFilesStopped {
            restored_turn_ids: Vec::new(),
            turn_id,
            outcome: "blocked".to_string(),
            reason_code: Some(reason_code),
        },
        _ => unreachable!("only a blocked turn stops the plan"),
    })?;
    let mut restored: Vec<TurnId> = Vec::new();
    let stopped =
        |restored: &[TurnId], turn_id: &TurnId, outcome: &str, reason_code| RewindFilesStopped {
            restored_turn_ids: restored.iter().map(|turn_id| turn_id.0.clone()).collect(),
            turn_id: turn_id.0.clone(),
            outcome: outcome.to_string(),
            reason_code,
        };
    for (turn_id, snapshot_id) in snapshots {
        let preview_token = match restorer.prepare(snapshot_id).await {
            GuardedUndoPrepareResult::Ready {
                preview_token,
                unrelated_paths_are_not_targets: true,
                ..
            } => preview_token,
            GuardedUndoPrepareResult::Ready { .. } => {
                return Err(stopped(
                    &restored,
                    &turn_id,
                    "blocked",
                    Some("invalid_persisted_record".to_string()),
                ))
            }
            GuardedUndoPrepareResult::Blocked { reason_code, .. }
            | GuardedUndoPrepareResult::Unavailable { reason_code, .. } => {
                return Err(stopped(&restored, &turn_id, "blocked", Some(reason_code)))
            }
        };
        match restorer.execute(preview_token).await {
            GuardedUndoExecuteResult::Completed { .. } => restored.push(turn_id),
            GuardedUndoExecuteResult::Blocked { reason_code } => {
                return Err(stopped(&restored, &turn_id, "blocked", Some(reason_code)))
            }
            GuardedUndoExecuteResult::RolledBack { .. } => {
                return Err(stopped(&restored, &turn_id, "rolled_back", None))
            }
            GuardedUndoExecuteResult::RecoveryRequired { reason_code, .. } => {
                return Err(stopped(
                    &restored,
                    &turn_id,
                    "recovery_required",
                    Some(reason_code),
                ))
            }
        }
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guarded_undo_runtime::{GuardedUndoPlannedRestore, GuardedUndoPreview};
    use std::{
        collections::{HashMap, HashSet},
        sync::Mutex,
    };

    fn plan(anchor: &str, removed: &[&str], last_kept: Option<&str>) -> ConversationRewindPlan {
        ConversationRewindPlan {
            anchor_turn_id: TurnId(anchor.to_string()),
            removed_turn_ids: removed.iter().map(|id| TurnId(id.to_string())).collect(),
            last_kept_turn_id: last_kept.map(|id| TurnId(id.to_string())),
            kept_turn_count: u32::from(last_kept.is_some()) * 2,
            anchor_prompt: "edit me".to_string(),
        }
    }

    fn checkpoints(entries: &[(&str, &str, &str)]) -> impl Fn(&TurnId) -> Option<(String, String)> {
        let map: HashMap<String, (String, String)> = entries
            .iter()
            .map(|(turn, native, checkpoint)| {
                (
                    turn.to_string(),
                    (native.to_string(), checkpoint.to_string()),
                )
            })
            .collect();
        move |turn_id: &TurnId| map.get(&turn_id.0).cloned()
    }

    #[test]
    fn claude_resumes_at_the_last_kept_turn_and_codex_drops_from_the_anchor() {
        let middle = plan("t2", &["t2", "t3"], Some("t1"));
        let recorded = checkpoints(&[("t1", "native", "uuid-1"), ("t2", "native", "codex-turn-2")]);
        assert_eq!(
            plan_provider_context(
                Some(NativeRewindCut::AfterLastKeptTurn),
                &middle,
                Some("native"),
                &recorded
            ),
            (
                RewindProviderPlan::Native,
                Some(NativeRewindTarget {
                    native_id: "native".to_string(),
                    checkpoint: "uuid-1".to_string()
                })
            )
        );
        assert_eq!(
            plan_provider_context(
                Some(NativeRewindCut::FromAnchorTurn),
                &middle,
                Some("native"),
                &recorded
            )
            .1
            .map(|target| target.checkpoint),
            Some("codex-turn-2".to_string())
        );
    }

    #[test]
    fn provider_without_rewind_or_without_a_trustworthy_checkpoint_continues_in_a_new_thread() {
        let last = plan("t3", &["t3"], Some("t2"));
        let none = checkpoints(&[]);
        assert_eq!(
            plan_provider_context(None, &last, Some("native"), &none).0,
            RewindProviderPlan::NewThread {
                reason: RewindNewThreadReason::ProviderWithoutRewind
            }
        );
        assert_eq!(
            plan_provider_context(Some(NativeRewindCut::AfterLastKeptTurn), &last, None, &none).0,
            RewindProviderPlan::NewThread {
                reason: RewindNewThreadReason::NoNativeConversation
            }
        );
        assert_eq!(
            plan_provider_context(
                Some(NativeRewindCut::AfterLastKeptTurn),
                &last,
                Some("native"),
                &none
            )
            .0,
            RewindProviderPlan::NewThread {
                reason: RewindNewThreadReason::CheckpointMissing
            }
        );
        // A checkpoint of another native conversation (a refused resume
        // started over) would cut the wrong transcript.
        assert_eq!(
            plan_provider_context(
                Some(NativeRewindCut::AfterLastKeptTurn),
                &last,
                Some("native-2"),
                &checkpoints(&[("t2", "native-1", "uuid-2")])
            )
            .0,
            RewindProviderPlan::NewThread {
                reason: RewindNewThreadReason::CheckpointMissing
            }
        );
        // Nothing kept: start over, no checkpoint needed.
        assert_eq!(
            plan_provider_context(
                Some(NativeRewindCut::FromAnchorTurn),
                &plan("t1", &["t1"], None),
                None,
                &none
            ),
            (RewindProviderPlan::Fresh, None)
        );
    }

    /// Guarded Undo over an in-memory workspace: each capture restores its
    /// files from the content it left behind, and refuses when that content
    /// changed (another turn or the person edited the file afterwards).
    struct FakeWorkspace {
        files: Mutex<HashMap<String, String>>,
        /// snapshot → [(path, before, after)]
        captures: HashMap<String, Vec<(String, String, String)>>,
        consumed: Mutex<HashSet<String>>,
        tokens: Mutex<HashMap<String, String>>,
        prepares: Mutex<Vec<String>>,
    }

    impl FakeWorkspace {
        fn new(files: &[(&str, &str)], captures: &[(&str, &[(&str, &str, &str)])]) -> Self {
            Self {
                files: Mutex::new(
                    files
                        .iter()
                        .map(|(path, content)| (path.to_string(), content.to_string()))
                        .collect(),
                ),
                captures: captures
                    .iter()
                    .map(|(snapshot, entries)| {
                        (
                            snapshot.to_string(),
                            entries
                                .iter()
                                .map(|(path, before, after)| {
                                    (path.to_string(), before.to_string(), after.to_string())
                                })
                                .collect(),
                        )
                    })
                    .collect(),
                consumed: Mutex::new(HashSet::new()),
                tokens: Mutex::new(HashMap::new()),
                prepares: Mutex::new(Vec::new()),
            }
        }

        fn file(&self, path: &str) -> String {
            self.files.lock().unwrap()[path].clone()
        }
    }

    #[async_trait]
    impl RewindFileRestorer for FakeWorkspace {
        async fn plan(&self, snapshot_ids: Vec<String>) -> GuardedUndoChainPlanResult {
            // What each path will hold when an older capture's turn comes.
            let mut planned_files = self.files.lock().unwrap().clone();
            let mut planned = Vec::new();
            for snapshot_id in snapshot_ids {
                let entries = &self.captures[&snapshot_id];
                if entries
                    .iter()
                    .any(|(path, _, after)| planned_files[path] != *after)
                {
                    return GuardedUndoChainPlanResult::Blocked {
                        snapshot_id,
                        reason_code: "target_result_mismatch".to_string(),
                    };
                }
                for (path, before, _) in entries {
                    planned_files.insert(path.clone(), before.clone());
                }
                planned.push(GuardedUndoPlannedRestore {
                    snapshot_id,
                    files: entries
                        .iter()
                        .map(|(path, before, _)| GuardedUndoPreview {
                            display_path: path.clone(),
                            size: before.len() as u64,
                            binary: false,
                            preview: Some(format!("-> {before}")),
                        })
                        .collect(),
                });
            }
            GuardedUndoChainPlanResult::Ready(planned)
        }

        async fn prepare(&self, snapshot_id: String) -> GuardedUndoPrepareResult {
            self.prepares.lock().unwrap().push(snapshot_id.clone());
            if self.consumed.lock().unwrap().contains(&snapshot_id) {
                return GuardedUndoPrepareResult::Unavailable {
                    snapshot_id,
                    reason_code: "preview_consumed".to_string(),
                };
            }
            let entries = &self.captures[&snapshot_id];
            let files = self.files.lock().unwrap();
            if entries.iter().any(|(path, _, after)| files[path] != *after) {
                return GuardedUndoPrepareResult::Blocked {
                    snapshot_id,
                    reason_code: "target_result_mismatch".to_string(),
                };
            }
            let token = format!("token-{snapshot_id}");
            self.tokens
                .lock()
                .unwrap()
                .insert(token.clone(), snapshot_id.clone());
            GuardedUndoPrepareResult::Ready {
                snapshot_id,
                preview_token: token,
                expires_at: "2026-10-07T10:02:00Z".to_string(),
                file_count: entries.len() as u32,
                total_bytes: 1,
                files: entries
                    .iter()
                    .map(|(path, before, _)| GuardedUndoPreview {
                        display_path: path.clone(),
                        size: before.len() as u64,
                        binary: false,
                        preview: Some(format!("-> {before}")),
                    })
                    .collect(),
                unrelated_paths_are_not_targets: true,
            }
        }

        async fn execute(&self, preview_token: String) -> GuardedUndoExecuteResult {
            let Some(snapshot_id) = self.tokens.lock().unwrap().remove(&preview_token) else {
                return GuardedUndoExecuteResult::Blocked {
                    reason_code: "preview_consumed".to_string(),
                };
            };
            let mut files = self.files.lock().unwrap();
            for (path, before, after) in &self.captures[&snapshot_id] {
                if files[path] != *after {
                    return GuardedUndoExecuteResult::Blocked {
                        reason_code: "target_result_mismatch".to_string(),
                    };
                }
                files.insert(path.clone(), before.clone());
            }
            self.consumed.lock().unwrap().insert(snapshot_id);
            GuardedUndoExecuteResult::Completed {
                operation_id: "op".to_string(),
            }
        }
    }

    fn facts(turn: &str, files: usize, state: &str) -> TurnFileFacts {
        TurnFileFacts {
            turn_id: TurnId(turn.to_string()),
            snapshot_id: Some(format!("snap-{turn}")),
            changed_file_count: Some(files),
            capture_state: Some(state.to_string()),
            capture_reason: None,
        }
    }

    #[tokio::test]
    async fn last_turn_files_are_previewed_then_restored() {
        let workspace = FakeWorkspace::new(
            &[("a.rs", "a2"), ("b.rs", "b1")],
            &[("snap-t2", &[("a.rs", "a1", "a2")])],
        );
        let removed = [facts("t2", 1, "eligible")];
        let plan = plan_files(&workspace, &removed).await;
        let RewindFilesPlan::Restorable {
            turn_count,
            file_count,
            files,
        } = plan
        else {
            panic!("expected restorable, got {plan:?}");
        };
        assert_eq!((turn_count, file_count), (1, 1));
        assert_eq!(files[0].display_path, "a.rs");
        assert_eq!(workspace.file("a.rs"), "a2", "planning never writes");

        let restored = restore_files(&workspace, &removed).await.unwrap();
        assert_eq!(restored, vec![TurnId("t2".to_string())]);
        assert_eq!(workspace.file("a.rs"), "a1");
        assert_eq!(
            workspace.file("b.rs"),
            "b1",
            "unrelated files are not targets"
        );
    }

    #[tokio::test]
    async fn middle_turn_restores_every_later_turn_newest_first_skipping_clean_ones() {
        let workspace = FakeWorkspace::new(
            &[("a.rs", "a2"), ("b.rs", "b3")],
            &[
                ("snap-t2", &[("a.rs", "a1", "a2")]),
                ("snap-t4", &[("b.rs", "b1", "b3")]),
            ],
        );
        let removed = [
            facts("t2", 1, "eligible"),
            facts("t3", 0, "ineligible"),
            TurnFileFacts {
                capture_reason: Some("no_target_changes".to_string()),
                ..facts("t3b", 2, "ineligible")
            },
            facts("t4", 1, "eligible"),
        ];
        assert!(matches!(
            plan_files(&workspace, &removed).await,
            RewindFilesPlan::Restorable {
                turn_count: 2,
                file_count: 2,
                ..
            }
        ));
        workspace.prepares.lock().unwrap().clear();
        let restored = restore_files(&workspace, &removed).await.unwrap();
        assert_eq!(
            restored,
            vec![TurnId("t4".to_string()), TurnId("t2".to_string())]
        );
        assert_eq!(
            *workspace.prepares.lock().unwrap(),
            vec!["snap-t4".to_string(), "snap-t2".to_string()],
            "newest turn first, each with its own fresh prepare"
        );
        assert_eq!(
            (workspace.file("a.rs"), workspace.file("b.rs")),
            ("a1".to_string(), "b1".to_string())
        );
    }

    #[tokio::test]
    async fn a_file_changed_after_the_turn_blocks_before_anything_is_written() {
        // The person edited a.rs after turn t2 (a2 -> a2-mine).
        let workspace = FakeWorkspace::new(
            &[("a.rs", "a2-mine"), ("b.rs", "b2")],
            &[
                ("snap-t2", &[("a.rs", "a1", "a2")]),
                ("snap-t3", &[("b.rs", "b1", "b2")]),
            ],
        );
        let removed = [facts("t2", 1, "eligible"), facts("t3", 1, "eligible")];
        assert_eq!(
            plan_files(&workspace, &removed).await,
            RewindFilesPlan::NotRestorable {
                turn_id: "t2".to_string(),
                stage: RewindFilesStage::Prepare,
                reason_code: "target_result_mismatch".to_string(),
            }
        );
        assert_eq!(workspace.file("b.rs"), "b2");
        assert_eq!(
            workspace.file("a.rs"),
            "a2-mine",
            "the person's change is kept"
        );
    }

    #[tokio::test]
    async fn a_file_two_removed_turns_changed_is_restored_through_both_turns() {
        let workspace = FakeWorkspace::new(
            &[("a.rs", "a3"), ("b.rs", "b2")],
            &[
                ("snap-t2", &[("a.rs", "a1", "a2"), ("b.rs", "b1", "b2")]),
                ("snap-t3", &[("a.rs", "a2", "a3")]),
            ],
        );
        let removed = [facts("t2", 2, "eligible"), facts("t3", 1, "eligible")];
        let plan = plan_files(&workspace, &removed).await;
        let RewindFilesPlan::Restorable {
            turn_count,
            file_count,
            files,
        } = plan
        else {
            panic!("expected restorable, got {plan:?}");
        };
        assert_eq!((turn_count, file_count), (2, 2), "a.rs is listed once");
        assert_eq!(
            files
                .iter()
                .map(|file| (file.turn_id.as_str(), file.display_path.as_str()))
                .collect::<Vec<_>>(),
            vec![("t3", "a.rs"), ("t2", "b.rs")]
        );
        assert_eq!(workspace.file("a.rs"), "a3", "planning never writes");
        assert!(workspace.prepares.lock().unwrap().is_empty());

        let restored = restore_files(&workspace, &removed).await.unwrap();
        assert_eq!(
            restored,
            vec![TurnId("t3".to_string()), TurnId("t2".to_string())]
        );
        assert_eq!(
            *workspace.prepares.lock().unwrap(),
            vec!["snap-t3".to_string(), "snap-t2".to_string()],
            "the older turn is prepared only after the newer one is restored"
        );
        assert_eq!(
            (workspace.file("a.rs"), workspace.file("b.rs")),
            ("a1".to_string(), "b1".to_string())
        );
    }

    #[tokio::test]
    async fn a_file_changed_between_two_removed_turns_blocks_the_chain() {
        // The person edited a.rs between t2 and t3 (a2 -> a2-mine).
        let workspace = FakeWorkspace::new(
            &[("a.rs", "a3")],
            &[
                ("snap-t2", &[("a.rs", "a1", "a2")]),
                ("snap-t3", &[("a.rs", "a2-mine", "a3")]),
            ],
        );
        let removed = [facts("t2", 1, "eligible"), facts("t3", 1, "eligible")];
        assert_eq!(
            plan_files(&workspace, &removed).await,
            RewindFilesPlan::NotRestorable {
                turn_id: "t2".to_string(),
                stage: RewindFilesStage::Prepare,
                reason_code: "target_result_mismatch".to_string(),
            }
        );
        assert_eq!(workspace.file("a.rs"), "a3");
    }

    #[tokio::test]
    async fn unprotected_turns_cannot_be_restored_and_say_why() {
        let workspace = FakeWorkspace::new(&[], &[]);
        let untracked = TurnFileFacts {
            capture_reason: Some("untracked_path".to_string()),
            ..facts("t2", 1, "ineligible")
        };
        assert_eq!(
            plan_files(&workspace, &[untracked]).await,
            RewindFilesPlan::NotRestorable {
                turn_id: "t2".to_string(),
                stage: RewindFilesStage::Capture,
                reason_code: "untracked_path".to_string(),
            }
        );
        let no_review = TurnFileFacts {
            changed_file_count: None,
            ..facts("t3", 0, "eligible")
        };
        assert!(matches!(
            plan_files(&workspace, &[no_review]).await,
            RewindFilesPlan::NotRestorable {
                stage: RewindFilesStage::Capture,
                ..
            }
        ));
        assert_eq!(
            plan_files(
                &workspace,
                &[facts("t4", 0, "ineligible"), facts("t5", 3, "consumed")]
            )
            .await,
            RewindFilesPlan::NothingToRestore
        );
        assert!(workspace.prepares.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_change_between_planning_and_restoring_stops_and_reports_what_was_restored() {
        let workspace = FakeWorkspace::new(
            &[("a.rs", "a2"), ("b.rs", "b2")],
            &[
                ("snap-t2", &[("a.rs", "a1", "a2")]),
                ("snap-t3", &[("b.rs", "b1", "b2")]),
            ],
        );
        let removed = [facts("t2", 1, "eligible"), facts("t3", 1, "eligible")];
        assert!(matches!(
            plan_files(&workspace, &removed).await,
            RewindFilesPlan::Restorable { .. }
        ));
        // Someone edits a.rs after the person confirmed.
        workspace
            .files
            .lock()
            .unwrap()
            .insert("a.rs".to_string(), "a2-race".to_string());
        let stopped = restore_files(&workspace, &removed).await.unwrap_err();
        assert_eq!(
            stopped,
            RewindFilesStopped {
                restored_turn_ids: vec!["t3".to_string()],
                turn_id: "t2".to_string(),
                outcome: "blocked".to_string(),
                reason_code: Some("target_result_mismatch".to_string()),
            }
        );
        assert_eq!(workspace.file("a.rs"), "a2-race");
        assert_eq!(workspace.file("b.rs"), "b1");
    }
}
