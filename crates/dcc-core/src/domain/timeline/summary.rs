//! The label and one-line text of session-level rows (`eventLabel` and
//! `eventSummary`), word for word as the desktop writes them.

use crate::domain::objective::ObjectivePauseReason;
use crate::domain::session::{RewindProviderContext, SessionEventKind};

/// A note the conversation shows for a session-level event, if it shows one.
pub(super) fn system_note(kind: &SessionEventKind) -> Option<(&'static str, String)> {
    let note = match kind {
        SessionEventKind::SessionCompleted => ("session.completed", String::new()),
        SessionEventKind::SessionAborted { reason } => (
            "session.aborted",
            reason
                .clone()
                .unwrap_or_else(|| "Session aborted".to_string()),
        ),
        SessionEventKind::SessionResumed => ("session.resumed", String::new()),
        SessionEventKind::ObjectivePaused {
            reason,
            consecutive_failures,
            turns_used,
        } => {
            let why = match reason {
                ObjectivePauseReason::ConsecutiveFailures => {
                    format!("consecutive failures reached the limit ({consecutive_failures})")
                }
                ObjectivePauseReason::TurnBudget => format!("turn budget reached ({turns_used})"),
                ObjectivePauseReason::Manual => "paused".to_string(),
            };
            (
                "session.objective.paused",
                format!(
                    "Objective paused automatically: {why}. Queued follow-ups stop until you resume it."
                ),
            )
        }
        SessionEventKind::ConversationRewound {
            removed_turn_ids,
            provider_context,
            restored_turn_ids,
            ..
        } => {
            let turns = removed_turn_ids.len();
            let context = match provider_context {
                RewindProviderContext::Fresh => "the agent starts a new conversation",
                RewindProviderContext::Native => {
                    "the agent's own conversation was cut at the same point"
                }
            };
            let restored = restored_turn_ids.len();
            let files = if restored > 0 {
                format!(
                    "files restored for {restored} turn{}",
                    if restored == 1 { "" } else { "s" }
                )
            } else {
                "files kept as they were".to_string()
            };
            (
                "session.conversation.rewound",
                format!(
                    "Edited from an earlier message: {turns} turn{} left the conversation; {context}; {files}.",
                    if turns == 1 { "" } else { "s" }
                ),
            )
        }
        SessionEventKind::CheckpointCreated { label, .. } => {
            ("session.checkpoint.created", label.clone())
        }
        _ => return None,
    };
    Some(note)
}

/// The session id the desktop shows for completed/resumed (the event's own).
pub(super) fn with_session_id(label: &str, content: String, session_id: &str) -> String {
    if content.is_empty() && matches!(label, "session.completed" | "session.resumed") {
        session_id.to_string()
    } else {
        content
    }
}

/// The text of a delegation card at each step.
pub(super) fn delegation_text(kind: &SessionEventKind) -> Option<String> {
    Some(match kind {
        SessionEventKind::DelegationRequested { delegation_id } => {
            format!("Delegation requested · {}", delegation_id.0)
        }
        SessionEventKind::DelegationStarted {
            child_session_id, ..
        } => match child_session_id {
            Some(child) => format!("Child session {} is running.", child.0),
            None => "Delegation is running.".to_string(),
        },
        SessionEventKind::DelegationDelta { content, .. } => content.clone(),
        SessionEventKind::DelegationCompleted { summary, .. } => summary
            .clone()
            .unwrap_or_else(|| "Delegation completed.".to_string()),
        SessionEventKind::DelegationFailed { reason, .. } => reason
            .clone()
            .unwrap_or_else(|| "Delegation failed.".to_string()),
        SessionEventKind::DelegationCancelled { reason, .. } => reason
            .clone()
            .unwrap_or_else(|| "Delegation cancelled.".to_string()),
        _ => return None,
    })
}
