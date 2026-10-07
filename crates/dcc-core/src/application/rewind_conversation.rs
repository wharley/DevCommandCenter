//! "Edit from here": take the conversation back to just before one of the
//! person's own messages. This module only decides which turns leave and
//! records that durably; files and the provider's native memory are handled
//! by the runtime, which must finish both before the rewind is recorded.

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::session::{
        rewound_turn_ids, RewindProviderContext, SessionEventKind, SessionEventRecord, SessionId,
        TurnId,
    },
    ports::{AppendEventOutcome, CoreEvent, EventBus, SessionEventRepo},
    Result,
};

use super::turn_queue::project_turn_queue;

/// Which turns a rewind to `anchor_turn_id` takes out, oldest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRewindPlan {
    pub anchor_turn_id: TurnId,
    /// The anchor and every later turn still in the conversation.
    pub removed_turn_ids: Vec<TurnId>,
    /// The newest turn that stays, if any. The provider resumes after it.
    pub last_kept_turn_id: Option<TurnId>,
    /// How many turns stay in the conversation.
    pub kept_turn_count: u32,
    /// The anchor's prompt, attachments included, for the composer.
    pub anchor_prompt: String,
}

/// Why a rewind cannot be planned. Each one is checked before anything on
/// disk or in the provider changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRewindBlock {
    /// The message is not a turn of this conversation, or was already
    /// rewound away.
    AnchorNotFound,
    /// A turn is still running; stop it first.
    TurnRunning,
    /// Queued messages would be sent on top of the rewound conversation.
    QueuePending,
}

/// Decides the rewind from durable history alone.
pub fn plan_conversation_rewind(
    history: &[SessionEventRecord],
    anchor_turn_id: &TurnId,
) -> std::result::Result<ConversationRewindPlan, ConversationRewindBlock> {
    let rewound = rewound_turn_ids(history);
    let mut visible_turns: Vec<(&TurnId, &str)> = Vec::new();
    let mut open_turns: Vec<&TurnId> = Vec::new();
    for event in history {
        match &event.kind {
            SessionEventKind::TurnStarted {
                turn_id, prompt, ..
            } if !rewound.contains(turn_id) => {
                visible_turns.push((turn_id, prompt.as_str()));
                open_turns.push(turn_id);
            }
            SessionEventKind::TurnCompleted { turn_id }
            | SessionEventKind::TurnAborted { turn_id, .. } => {
                open_turns.retain(|open| *open != turn_id);
            }
            SessionEventKind::SessionCompleted | SessionEventKind::SessionAborted { .. } => {
                open_turns.clear();
            }
            _ => {}
        }
    }
    let anchor_index = visible_turns
        .iter()
        .position(|(turn_id, _)| *turn_id == anchor_turn_id)
        .ok_or(ConversationRewindBlock::AnchorNotFound)?;
    if !open_turns.is_empty() {
        return Err(ConversationRewindBlock::TurnRunning);
    }
    if !project_turn_queue(history).is_empty() {
        return Err(ConversationRewindBlock::QueuePending);
    }
    Ok(ConversationRewindPlan {
        anchor_turn_id: anchor_turn_id.clone(),
        removed_turn_ids: visible_turns[anchor_index..]
            .iter()
            .map(|(turn_id, _)| (*turn_id).clone())
            .collect(),
        last_kept_turn_id: anchor_index
            .checked_sub(1)
            .map(|index| visible_turns[index].0.clone()),
        kept_turn_count: anchor_index as u32,
        anchor_prompt: visible_turns[anchor_index].1.to_string(),
    })
}

/// Records a rewind the runtime already carried out. The plan is re-derived
/// from the latest history so a turn that slipped in meanwhile refuses it.
pub async fn record_conversation_rewound<E, B>(
    session_events: &E,
    events: &B,
    session_id: &SessionId,
    expected: &ConversationRewindPlan,
    provider_context: RewindProviderContext,
    restored_turn_ids: Vec<TurnId>,
) -> Result<SessionEventRecord>
where
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let history = session_events.list_events_by_session(session_id).await?;
    let current =
        plan_conversation_rewind(&history, &expected.anchor_turn_id).map_err(|block| {
            crate::CoreError::InvalidInput(format!("conversation rewind refused: {block:?}"))
        })?;
    if current.removed_turn_ids != expected.removed_turn_ids {
        return Err(crate::CoreError::InvalidInput(
            "conversation changed while it was being rewound".to_string(),
        ));
    }
    let record = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        sequence: history.last().map(|event| event.sequence + 1).unwrap_or(1),
        occurred_at: chrono::Utc::now().to_rfc3339(),
        kind: SessionEventKind::ConversationRewound {
            anchor_turn_id: current.anchor_turn_id.clone(),
            removed_turn_ids: current.removed_turn_ids.clone(),
            provider_context,
            restored_turn_ids: restored_turn_ids.clone(),
        },
    };
    let outcome = session_events.append_event(&record).await?;
    let inserted = matches!(&outcome, AppendEventOutcome::Inserted(_));
    let canonical = match outcome {
        AppendEventOutcome::Inserted(event) | AppendEventOutcome::Existing(event) => event,
    };
    if inserted {
        events
            .publish_durable_session(
                &canonical,
                CoreEvent::SessionConversationRewound {
                    session_id: session_id.0.clone(),
                    anchor_turn_id: current.anchor_turn_id.0,
                    removed_turn_ids: current
                        .removed_turn_ids
                        .into_iter()
                        .map(|id| id.0)
                        .collect(),
                    provider_context,
                    restored_turn_ids: restored_turn_ids.into_iter().map(|id| id.0).collect(),
                },
            )
            .await?;
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::{visible_session_events, QueuedTurn};

    fn record(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("evt-{sequence}"),
            session_id: SessionId("session-1".to_string()),
            sequence,
            occurred_at: format!("2026-10-07T10:00:{sequence:02}Z"),
            kind,
        }
    }

    fn started(sequence: u64, turn: &str, prompt: &str) -> SessionEventRecord {
        record(
            sequence,
            SessionEventKind::TurnStarted {
                turn_id: TurnId(turn.to_string()),
                prompt: prompt.to_string(),
                plan_mode: None,
                model: None,
                evidence: None,
                retry_of_turn_id: None,
                approval_policy: None,
            },
        )
    }

    fn completed(sequence: u64, turn: &str) -> SessionEventRecord {
        record(
            sequence,
            SessionEventKind::TurnCompleted {
                turn_id: TurnId(turn.to_string()),
            },
        )
    }

    fn answer(sequence: u64, turn: &str, text: &str) -> SessionEventRecord {
        record(
            sequence,
            SessionEventKind::TurnAssistantMessageCompleted {
                turn_id: TurnId(turn.to_string()),
                message_id: format!("msg-{turn}"),
                phase: crate::domain::session::AssistantMessagePhase::FinalAnswer,
                content: Some(text.to_string()),
            },
        )
    }

    fn three_turns() -> Vec<SessionEventRecord> {
        vec![
            started(1, "t1", "first"),
            answer(2, "t1", "one"),
            completed(3, "t1"),
            started(4, "t2", "second @/tmp/shot.png"),
            answer(5, "t2", "two"),
            completed(6, "t2"),
            started(7, "t3", "third"),
            answer(8, "t3", "three"),
            completed(9, "t3"),
        ]
    }

    fn ids(values: &[&str]) -> Vec<TurnId> {
        values
            .iter()
            .map(|value| TurnId(value.to_string()))
            .collect()
    }

    #[test]
    fn last_turn_rewind_removes_only_that_turn() {
        let plan = plan_conversation_rewind(&three_turns(), &TurnId("t3".into())).unwrap();
        assert_eq!(plan.removed_turn_ids, ids(&["t3"]));
        assert_eq!(plan.last_kept_turn_id, Some(TurnId("t2".into())));
        assert_eq!(plan.kept_turn_count, 2);
        assert_eq!(plan.anchor_prompt, "third");
    }

    #[test]
    fn middle_turn_rewind_removes_it_and_everything_after() {
        let plan = plan_conversation_rewind(&three_turns(), &TurnId("t2".into())).unwrap();
        assert_eq!(plan.removed_turn_ids, ids(&["t2", "t3"]));
        assert_eq!(plan.last_kept_turn_id, Some(TurnId("t1".into())));
        assert_eq!(plan.anchor_prompt, "second @/tmp/shot.png");
    }

    #[test]
    fn first_turn_rewind_keeps_nothing() {
        let plan = plan_conversation_rewind(&three_turns(), &TurnId("t1".into())).unwrap();
        assert_eq!(plan.removed_turn_ids, ids(&["t1", "t2", "t3"]));
        assert_eq!(plan.last_kept_turn_id, None);
        assert_eq!(plan.kept_turn_count, 0);
    }

    #[test]
    fn running_turn_and_pending_queue_refuse_the_rewind() {
        let mut running = three_turns();
        running.push(started(10, "t4", "fourth"));
        assert_eq!(
            plan_conversation_rewind(&running, &TurnId("t2".into())),
            Err(ConversationRewindBlock::TurnRunning)
        );

        let mut queued = three_turns();
        queued.push(record(
            10,
            SessionEventKind::TurnQueued {
                queued_turn: QueuedTurn {
                    id: "q1".to_string(),
                    session_id: SessionId("session-1".to_string()),
                    prompt: "later".to_string(),
                    tool_instructions: None,
                    plan_mode: None,
                    effort: None,
                    fast_mode: None,
                    approval_policy: None,
                    evidence: None,
                    created_at: "2026-10-07T10:00:10Z".to_string(),
                },
            },
        ));
        assert_eq!(
            plan_conversation_rewind(&queued, &TurnId("t2".into())),
            Err(ConversationRewindBlock::QueuePending)
        );
    }

    #[test]
    fn rewound_turns_leave_the_visible_history_and_cannot_be_anchors_again() {
        let mut history = three_turns();
        history.push(record(
            10,
            SessionEventKind::ConversationRewound {
                anchor_turn_id: TurnId("t2".into()),
                removed_turn_ids: ids(&["t2", "t3"]),
                provider_context: RewindProviderContext::Native,
                restored_turn_ids: Vec::new(),
            },
        ));
        history.push(started(11, "t4", "second, edited"));
        history.push(completed(12, "t4"));

        let visible = visible_session_events(&history);
        let prompts: Vec<&str> = visible
            .iter()
            .filter_map(|event| match &event.kind {
                SessionEventKind::TurnStarted { prompt, .. } => Some(prompt.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(prompts, vec!["first", "second, edited"]);
        assert!(visible
            .iter()
            .all(|event| event.kind.turn_id() != Some(&TurnId("t3".into()))));

        assert_eq!(
            plan_conversation_rewind(&history, &TurnId("t3".into())),
            Err(ConversationRewindBlock::AnchorNotFound)
        );
        let plan = plan_conversation_rewind(&history, &TurnId("t4".into())).unwrap();
        assert_eq!(plan.removed_turn_ids, ids(&["t4"]));
        assert_eq!(plan.last_kept_turn_id, Some(TurnId("t1".into())));
    }
}
