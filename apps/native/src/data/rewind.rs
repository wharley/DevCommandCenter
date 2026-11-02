//! "Editar a partir daqui": the backend plans the rewind before anything
//! changes (which turns leave, how the provider follows, whether their files
//! can be restored); this module asks for it and turns the plan and the
//! outcome into what the dialog offers and says.

use dcc_core::application::ConversationRewindBlock;
use dcc_core::domain::turn_review::{
    GuardedUndoCaptureSummary, resolve_guarded_undo_capture, resolve_guarded_undo_failure_reason,
};
use dcc_tauri::commands::session_commands::{
    ExecuteConversationRewindInput, ExecuteConversationRewindOutput,
    PrepareConversationRewindOutput,
};
use dcc_tauri::conversation_rewind::{
    RewindFilePreview, RewindFilesPlan, RewindFilesStage, RewindNewThreadReason, RewindProviderPlan,
};
use serde_json::json;

use super::desktop_api::{ApiError, DesktopApi};

/// A rewind that can go ahead, as prepared.
#[derive(Clone, Debug)]
pub struct Plan {
    pub session_id: String,
    pub anchor_turn_id: String,
    pub removed_turn_ids: Vec<String>,
    pub provider: RewindProviderPlan,
    pub files: RewindFilesPlan,
}

pub enum Prepared {
    Ready(Plan),
    /// Only a provider that cannot cut its own conversation: the app has no
    /// fork, so the message goes back to the composer (`newThreadReasons.*`).
    NewThread {
        reason: &'static str,
        prompt: String,
    },
    Blocked(ConversationRewindBlock),
}

impl DesktopApi {
    pub fn prepare_rewind(
        &self,
        session_id: &str,
        anchor_turn_id: &str,
    ) -> Result<Prepared, ApiError> {
        let output: PrepareConversationRewindOutput = Self::decoded(self.post(
            "/api/v1/conversation-rewind/prepare",
            &json!({ "sessionId": session_id, "anchorTurnId": anchor_turn_id }),
        )?)?;
        Ok(prepared(session_id, output))
    }

    pub fn execute_rewind(&self, plan: &Plan, restore_files: bool) -> Result<Outcome, ApiError> {
        let input = ExecuteConversationRewindInput {
            session_id: plan.session_id.clone(),
            anchor_turn_id: plan.anchor_turn_id.clone(),
            removed_turn_ids: plan.removed_turn_ids.clone(),
            restore_files: restore_files
                && matches!(plan.files, RewindFilesPlan::Restorable { .. }),
            continue_in_new_thread: false,
        };
        let value =
            serde_json::to_value(&input).map_err(|error| ApiError::Failed(error.to_string()))?;
        let output: ExecuteConversationRewindOutput =
            Self::decoded(self.post("/api/v1/conversation-rewind/execute", &value)?)?;
        Ok(outcome(output))
    }
}

fn prepared(session_id: &str, output: PrepareConversationRewindOutput) -> Prepared {
    match output {
        PrepareConversationRewindOutput::Blocked { reason } => Prepared::Blocked(reason),
        PrepareConversationRewindOutput::Ready {
            anchor_prompt,
            provider: RewindProviderPlan::NewThread { reason },
            ..
        } => Prepared::NewThread {
            reason: match reason {
                RewindNewThreadReason::ProviderWithoutRewind => "provider_without_rewind",
                RewindNewThreadReason::NoNativeConversation => "no_native_conversation",
                RewindNewThreadReason::CheckpointMissing => "checkpoint_missing",
            },
            prompt: anchor_prompt,
        },
        PrepareConversationRewindOutput::Ready {
            anchor_turn_id,
            removed_turn_ids,
            provider,
            files,
            ..
        } => Prepared::Ready(Plan {
            session_id: session_id.to_string(),
            anchor_turn_id,
            removed_turn_ids,
            provider,
            files,
        }),
    }
}

/// `blocked.*` under `conversation.editFromHere`.
pub fn block_key(block: ConversationRewindBlock) -> &'static str {
    match block {
        ConversationRewindBlock::AnchorNotFound => "anchor_not_found",
        ConversationRewindBlock::TurnRunning => "turn_running",
        ConversationRewindBlock::QueuePending => "queue_pending",
    }
}

/// What the files section says.
pub enum Files<'a> {
    None,
    Restorable {
        file_count: u32,
        files: &'a [RewindFilePreview],
    },
    /// `reason_key` reuses Guarded Undo's explanations; `project` names the
    /// project that blocks in a multi-project task.
    Blocked {
        reason_key: String,
        project: Option<&'a str>,
    },
}

pub fn files(plan: &Plan) -> Files<'_> {
    match &plan.files {
        RewindFilesPlan::NothingToRestore => Files::None,
        RewindFilesPlan::Restorable {
            file_count, files, ..
        } => Files::Restorable {
            file_count: *file_count,
            files,
        },
        RewindFilesPlan::NotRestorable {
            project,
            stage,
            reason_code,
            ..
        } => Files::Blocked {
            project: project.as_deref(),
            reason_key: match stage {
                RewindFilesStage::Capture => format!(
                    "turnReview.guardedUndo.reasons.{}",
                    resolve_guarded_undo_capture(Some(&GuardedUndoCaptureSummary {
                        state: "ineligible".into(),
                        reason_code: Some(reason_code.clone()),
                        file_count: 0,
                        artifact_bytes: 0,
                        completed_at: None,
                        expires_at: None,
                    }))
                    .reason
                ),
                RewindFilesStage::Prepare => format!(
                    "turnReview.guardedUndo.failureReasons.{}",
                    resolve_guarded_undo_failure_reason(Some(reason_code))
                ),
            },
        },
    }
}

/// `contextNative` or `contextFresh`.
pub fn context_key(plan: &Plan) -> &'static str {
    match plan.provider {
        RewindProviderPlan::Fresh => "conversation.editFromHere.contextFresh",
        _ => "conversation.editFromHere.contextNative",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Rewound {
        prompt: String,
        restored_turns: usize,
    },
    /// Restoring stopped; the conversation did not move.
    FilesStopped(Stopped),
    /// The conversation changed while the person confirmed.
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stopped {
    pub restored_turns: usize,
    pub result_key: String,
    pub reason_key: Option<String>,
    pub project: Option<String>,
    pub recovery_required: bool,
}

fn outcome(output: ExecuteConversationRewindOutput) -> Outcome {
    match output {
        ExecuteConversationRewindOutput::Rewound {
            restored_turn_ids,
            anchor_prompt,
            ..
        } => Outcome::Rewound {
            prompt: anchor_prompt,
            restored_turns: restored_turn_ids.len(),
        },
        // Only asked for with `continue_in_new_thread`, which the app never sends.
        ExecuteConversationRewindOutput::ContinueInNewThread { .. } => Outcome::Refused,
        ExecuteConversationRewindOutput::FilesStopped { stopped } => {
            let result = match stopped.outcome.as_str() {
                known @ ("blocked" | "rolled_back" | "recovery_required") => known,
                _ => "blocked",
            };
            Outcome::FilesStopped(Stopped {
                restored_turns: stopped.restored_turn_ids.len(),
                result_key: format!("turnReview.guardedUndo.results.{result}"),
                reason_key: stopped.reason_code.as_deref().map(|code| {
                    format!(
                        "turnReview.guardedUndo.failureReasons.{}",
                        resolve_guarded_undo_failure_reason(Some(code))
                    )
                }),
                project: stopped.project,
                recovery_required: result == "recovery_required",
            })
        }
        ExecuteConversationRewindOutput::Refused { .. } => Outcome::Refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ready(provider: Value, files: Value) -> PrepareConversationRewindOutput {
        serde_json::from_value(json!({
            "status": "ready",
            "anchorTurnId": "t2",
            "removedTurnIds": ["t2", "t3"],
            "keptTurnCount": 1,
            "anchorPrompt": "Refaça @/tmp/a.png",
            "provider": provider,
            "files": files,
        }))
        .unwrap()
    }

    use serde_json::Value;

    #[test]
    fn a_provider_that_cannot_cut_sends_the_message_back() {
        let output = ready(
            json!({ "mode": "new_thread", "reason": "provider_without_rewind" }),
            json!({ "status": "nothing_to_restore" }),
        );
        let Prepared::NewThread { reason, prompt } = prepared("s1", output) else {
            panic!("new thread");
        };
        assert_eq!(reason, "provider_without_rewind");
        assert_eq!(prompt, "Refaça @/tmp/a.png");
    }

    #[test]
    fn files_read_as_restorable_or_with_guarded_undos_reason() {
        let output = ready(
            json!({ "mode": "native" }),
            json!({ "status": "restorable", "turnCount": 2, "fileCount": 3, "files": [] }),
        );
        let Prepared::Ready(plan) = prepared("s1", output) else {
            panic!("ready");
        };
        assert_eq!(plan.removed_turn_ids, ["t2", "t3"]);
        assert!(matches!(
            files(&plan),
            Files::Restorable { file_count: 3, .. }
        ));
        assert_eq!(
            context_key(&plan),
            "conversation.editFromHere.contextNative"
        );

        let output = ready(
            json!({ "mode": "fresh" }),
            json!({ "status": "not_restorable", "turnId": "t3", "project": "api", "stage": "prepare", "reasonCode": "head_changed" }),
        );
        let Prepared::Ready(plan) = prepared("s1", output) else {
            panic!("ready");
        };
        let Files::Blocked {
            reason_key,
            project,
        } = files(&plan)
        else {
            panic!("blocked");
        };
        assert_eq!(reason_key, "turnReview.guardedUndo.failureReasons.changed");
        assert_eq!(project, Some("api"));
        assert_eq!(context_key(&plan), "conversation.editFromHere.contextFresh");
    }

    #[test]
    fn a_stop_names_guarded_undos_result() {
        let output: ExecuteConversationRewindOutput = serde_json::from_value(json!({
            "status": "files_stopped",
            "stopped": { "restoredTurnIds": ["t3"], "turnId": "t2", "project": null, "outcome": "odd", "reasonCode": "preview_expired" },
        }))
        .unwrap();
        let Outcome::FilesStopped(stopped) = outcome(output) else {
            panic!("stopped");
        };
        assert_eq!(stopped.result_key, "turnReview.guardedUndo.results.blocked");
        assert_eq!(
            stopped.reason_key.as_deref(),
            Some("turnReview.guardedUndo.failureReasons.expired")
        );
        assert_eq!(stopped.restored_turns, 1);
        assert!(!stopped.recovery_required);
    }
}
