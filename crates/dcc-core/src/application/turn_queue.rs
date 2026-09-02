use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    application::SendTurnInput,
    domain::session::{QueuedTurn, SessionEventKind, SessionEventRecord, SessionId, TurnId},
    ports::{AppendEventOutcome, CoreEvent, EventBus, SessionEventRepo},
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QueueTurnInput {
    pub turn: SendTurnInput,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveQueuedTurnInput {
    pub session_id: SessionId,
    pub queued_turn_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReorderTurnQueueInput {
    pub session_id: SessionId,
    pub queued_turn_ids: Vec<String>,
}

pub fn project_turn_queue(history: &[SessionEventRecord]) -> Vec<QueuedTurn> {
    let mut queue = Vec::<QueuedTurn>::new();
    for event in history {
        match &event.kind {
            SessionEventKind::TurnQueued { queued_turn } => queue.push(queued_turn.clone()),
            SessionEventKind::QueuedTurnRemoved { queued_turn_id }
            | SessionEventKind::QueuedTurnDispatched { queued_turn_id, .. } => {
                queue.retain(|turn| turn.id != *queued_turn_id);
            }
            SessionEventKind::TurnQueueReordered { queued_turn_ids } => {
                let mut reordered = Vec::with_capacity(queue.len());
                for id in queued_turn_ids {
                    if let Some(turn) = queue.iter().find(|turn| &turn.id == id) {
                        reordered.push(turn.clone());
                    }
                }
                queue = reordered;
            }
            _ => {}
        }
    }
    queue
}

async fn append<E: SessionEventRepo + Sync>(
    events: &E,
    session_id: &SessionId,
    history: &[SessionEventRecord],
    kind: SessionEventKind,
) -> Result<(SessionEventRecord, bool)> {
    let event = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        sequence: history.last().map(|event| event.sequence + 1).unwrap_or(1),
        occurred_at: chrono::Utc::now().to_rfc3339(),
        kind,
    };
    let outcome = events.append_event(&event).await?;
    let inserted = matches!(&outcome, AppendEventOutcome::Inserted(_));
    let event = match outcome {
        AppendEventOutcome::Inserted(event) | AppendEventOutcome::Existing(event) => event,
    };
    Ok((event, inserted))
}

pub async fn queue_turn<E, B>(events: &E, bus: &B, input: QueueTurnInput) -> Result<QueuedTurn>
where
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    if input.turn.prompt.trim().is_empty() {
        return Err(crate::CoreError::InvalidInput(
            "queued prompt must not be empty".to_string(),
        ));
    }
    let history = events
        .list_events_by_session(&input.turn.session_id)
        .await?;
    if history.is_empty() {
        return Err(crate::CoreError::Repository(
            "session not found".to_string(),
        ));
    }
    let queued_turn = QueuedTurn {
        id: Uuid::new_v4().to_string(),
        session_id: input.turn.session_id.clone(),
        prompt: input.turn.prompt,
        tool_instructions: input.turn.tool_instructions,
        plan_mode: input.turn.plan_mode,
        effort: input.turn.effort,
        fast_mode: input.turn.fast_mode,
        approval_policy: input.turn.approval_policy,
        evidence: input.turn.evidence,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let (event, inserted) = append(
        events,
        &queued_turn.session_id,
        &history,
        SessionEventKind::TurnQueued {
            queued_turn: queued_turn.clone(),
        },
    )
    .await?;
    if inserted {
        bus.publish_durable_session(
            &event,
            CoreEvent::SessionTurnQueued {
                session_id: queued_turn.session_id.0.clone(),
                queued_turn: queued_turn.clone(),
            },
        )
        .await?;
    }
    Ok(queued_turn)
}

pub async fn list_turn_queue<E: SessionEventRepo + Sync>(
    events: &E,
    session_id: &SessionId,
) -> Result<Vec<QueuedTurn>> {
    let history = events.list_events_by_session(session_id).await?;
    Ok(project_turn_queue(&history))
}

pub async fn remove_queued_turn<E, B>(
    events: &E,
    bus: &B,
    input: RemoveQueuedTurnInput,
) -> Result<Vec<QueuedTurn>>
where
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let mut history = events.list_events_by_session(&input.session_id).await?;
    if !project_turn_queue(&history)
        .iter()
        .any(|turn| turn.id == input.queued_turn_id)
    {
        return Err(crate::CoreError::InvalidInput(
            "queued turn was not found".to_string(),
        ));
    }
    let (event, inserted) = append(
        events,
        &input.session_id,
        &history,
        SessionEventKind::QueuedTurnRemoved {
            queued_turn_id: input.queued_turn_id.clone(),
        },
    )
    .await?;
    if inserted {
        bus.publish_durable_session(
            &event,
            CoreEvent::SessionQueuedTurnRemoved {
                session_id: input.session_id.0.clone(),
                queued_turn_id: input.queued_turn_id,
            },
        )
        .await?;
    }
    history.push(event);
    Ok(project_turn_queue(&history))
}

pub async fn reorder_turn_queue<E, B>(
    events: &E,
    bus: &B,
    input: ReorderTurnQueueInput,
) -> Result<Vec<QueuedTurn>>
where
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let mut history = events.list_events_by_session(&input.session_id).await?;
    let mut current_ids = project_turn_queue(&history)
        .iter()
        .map(|turn| turn.id.clone())
        .collect::<Vec<_>>();
    let mut requested_ids = input.queued_turn_ids.clone();
    current_ids.sort();
    requested_ids.sort();
    requested_ids.dedup();
    if current_ids != requested_ids {
        return Err(crate::CoreError::InvalidInput(
            "queue order must contain every queued turn exactly once".to_string(),
        ));
    }
    let (event, inserted) = append(
        events,
        &input.session_id,
        &history,
        SessionEventKind::TurnQueueReordered {
            queued_turn_ids: input.queued_turn_ids.clone(),
        },
    )
    .await?;
    if inserted {
        bus.publish_durable_session(
            &event,
            CoreEvent::SessionTurnQueueReordered {
                session_id: input.session_id.0.clone(),
                queued_turn_ids: input.queued_turn_ids,
            },
        )
        .await?;
    }
    history.push(event);
    Ok(project_turn_queue(&history))
}

pub async fn mark_queued_turn_dispatched<E, B>(
    events: &E,
    bus: &B,
    session_id: &SessionId,
    queued_turn_id: String,
    turn_id: TurnId,
) -> Result<()>
where
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let history = events.list_events_by_session(session_id).await?;
    let (event, inserted) = append(
        events,
        session_id,
        &history,
        SessionEventKind::QueuedTurnDispatched {
            queued_turn_id: queued_turn_id.clone(),
            turn_id: turn_id.clone(),
        },
    )
    .await?;
    if inserted {
        bus.publish_durable_session(
            &event,
            CoreEvent::SessionQueuedTurnDispatched {
                session_id: session_id.0.clone(),
                queued_turn_id,
                turn_id: turn_id.0,
            },
        )
        .await
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{project::ProjectId, workspace::WorkspaceId};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct ExistingRepo {
        history: Vec<SessionEventRecord>,
        canonical: SessionEventRecord,
    }

    #[async_trait]
    impl SessionEventRepo for ExistingRepo {
        async fn append_event(&self, _: &SessionEventRecord) -> Result<AppendEventOutcome> {
            Ok(AppendEventOutcome::Existing(self.canonical.clone()))
        }

        async fn list_events_by_session(&self, _: &SessionId) -> Result<Vec<SessionEventRecord>> {
            Ok(self.history.clone())
        }

        async fn delete_events_by_session(&self, _: &SessionId) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingBus(Arc<Mutex<Vec<CoreEvent>>>);

    #[async_trait]
    impl EventBus for RecordingBus {
        async fn publish(&self, event: CoreEvent) -> Result<()> {
            self.0.lock().expect("bus lock").push(event);
            Ok(())
        }
    }

    fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: format!("event-{sequence}"),
            session_id: SessionId("session-1".to_string()),
            sequence,
            occurred_at: "2026-08-02T10:00:00Z".to_string(),
            kind,
        }
    }

    fn queued(id: &str) -> QueuedTurn {
        QueuedTurn {
            id: id.to_string(),
            session_id: SessionId("session-1".to_string()),
            prompt: format!("prompt {id}"),
            tool_instructions: None,
            plan_mode: None,
            effort: None,
            fast_mode: None,
            approval_policy: None,
            evidence: None,
            created_at: "2026-08-02T10:00:00Z".to_string(),
        }
    }

    #[test]
    fn projects_persistent_queue_lifecycle() {
        let history = vec![
            event(
                1,
                SessionEventKind::SessionStarted {
                    workspace_id: WorkspaceId("workspace-1".into()),
                    project_id: ProjectId("project-1".into()),
                    provider_id: "codex".into(),
                    model: None,
                },
            ),
            event(
                2,
                SessionEventKind::TurnQueued {
                    queued_turn: queued("a"),
                },
            ),
            event(
                3,
                SessionEventKind::TurnQueued {
                    queued_turn: queued("b"),
                },
            ),
            event(
                4,
                SessionEventKind::TurnQueueReordered {
                    queued_turn_ids: vec!["b".into(), "a".into()],
                },
            ),
            event(
                5,
                SessionEventKind::QueuedTurnDispatched {
                    queued_turn_id: "b".into(),
                    turn_id: TurnId("turn-2".into()),
                },
            ),
        ];
        let queue = project_turn_queue(&history);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, "a");
    }

    #[test]
    fn queue_existing_append_does_not_publish_duplicate() {
        let session_id = SessionId("session-1".to_owned());
        let history = vec![event(
            1,
            SessionEventKind::SessionStarted {
                workspace_id: WorkspaceId("workspace-1".into()),
                project_id: ProjectId("project-1".into()),
                provider_id: "codex".into(),
                model: None,
            },
        )];
        let canonical = event(
            2,
            SessionEventKind::TurnQueued {
                queued_turn: queued("canonical"),
            },
        );
        let repo = ExistingRepo { history, canonical };
        let bus = RecordingBus::default();
        let input = QueueTurnInput {
            turn: SendTurnInput {
                session_id,
                prompt: "retry".to_owned(),
                tool_instructions: None,
                provider_id: None,
                model: None,
                provider_runtime: None,
                plan_mode: None,
                effort: None,
                fast_mode: None,
                approval_policy: None,
                evidence: None,
                retry_of_turn_id: None,
            },
        };
        let _ = futures::executor::block_on(queue_turn(&repo, &bus, input))
            .expect("existing append remains successful");
        assert!(bus.0.lock().expect("bus lock").is_empty());
    }
}
