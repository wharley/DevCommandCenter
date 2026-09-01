use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::session::{
        Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection, SessionState,
    },
    ports::{AppendEventOutcome, CoreEvent, EventBus, SessionEventRepo, SessionRepo},
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AbortRunInput {
    pub session_id: SessionId,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AbortRunOutput {
    pub session: Session,
    pub projection: SessionProjection,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn session_abort_event_id(
    session_id: &SessionId,
    turn_id: &crate::domain::session::TurnId,
) -> String {
    let mut hasher = Sha256::new();
    for value in [session_id.0.as_bytes(), turn_id.0.as_bytes()] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    format!("session-aborted:v1:{:x}", hasher.finalize())
}

pub async fn abort_run<S, E, B>(
    sessions: &S,
    session_events: &E,
    events: &B,
    input: AbortRunInput,
) -> Result<AbortRunOutput>
where
    S: SessionRepo + Sync,
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let mut session = sessions
        .get_session(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("session not found".to_string()))?;

    let history = session_events
        .list_events_by_session(&input.session_id)
        .await?;
    let projection = SessionProjection::fold(&history)
        .ok_or_else(|| crate::CoreError::Repository("session history is empty".to_string()))?;

    if projection.state == SessionState::Aborted {
        // A retry can observe the complete durable terminal history after a
        // previous caller already committed it. Repair only a stale session
        // row; never append or publish from the loser's request.
        if session.state != SessionState::Aborted {
            session.state = SessionState::Aborted;
            if let Some(canonical_time) = history.iter().rev().find_map(|event| {
                matches!(event.kind, SessionEventKind::SessionAborted { .. })
                    .then(|| event.occurred_at.clone())
            }) {
                session.updated_at = canonical_time;
            }
            sessions.save_session(&session).await?;
        }
        return Ok(AbortRunOutput {
            session,
            projection,
        });
    }
    if projection.state != SessionState::Active {
        return Err(crate::CoreError::InvalidInput(
            "session must be active to abort a run".to_string(),
        ));
    }

    let active_turn_id = projection
        .active_turn_id
        .clone()
        .or_else(|| {
            // A process can crash after durably appending TurnAborted but before
            // appending SessionAborted. Treat that narrow gap as a retryable
            // abort, while never reviving a completed turn.
            match history.last().map(|event| &event.kind) {
                Some(SessionEventKind::TurnAborted { turn_id, .. }) => Some(turn_id.clone()),
                _ => None,
            }
        })
        .ok_or_else(|| {
            crate::CoreError::InvalidInput("session has no active turn to abort".to_string())
        })?;
    let now = now_iso();
    let sequence = history.last().map(|event| event.sequence + 1).unwrap_or(1);
    let turn_aborted = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence,
        occurred_at: now.clone(),
        kind: SessionEventKind::TurnAborted {
            turn_id: active_turn_id.clone(),
            reason: input.reason.clone(),
        },
    };
    let mut session_aborted = SessionEventRecord {
        // Stable across concurrent abort callers. This closes the small gap
        // between the turn-terminal uniqueness check and SessionAborted.
        event_id: session_abort_event_id(&input.session_id, &active_turn_id),
        session_id: input.session_id.clone(),
        sequence: sequence + 1,
        occurred_at: now.clone(),
        kind: SessionEventKind::SessionAborted {
            reason: input.reason.clone(),
        },
    };

    let turn_outcome = session_events.append_event(&turn_aborted).await?;
    let (canonical_turn, publish_turn) = match turn_outcome {
        AppendEventOutcome::Inserted(record) => (record, true),
        AppendEventOutcome::Existing(record) => {
            if matches!(record.kind, SessionEventKind::TurnCompleted { .. }) {
                return Err(crate::CoreError::InvalidInput(
                    "turn already completed; abort is no longer applicable".to_string(),
                ));
            }
            (record, false)
        }
    };
    let canonical_reason = match &canonical_turn.kind {
        SessionEventKind::TurnAborted { reason, .. } => reason.clone(),
        _ => {
            return Err(crate::CoreError::Repository(
                "terminal event is not an abort".to_string(),
            ))
        }
    };
    session_aborted.occurred_at = canonical_turn.occurred_at.clone();
    session_aborted.kind = SessionEventKind::SessionAborted {
        reason: canonical_reason.clone(),
    };
    let existing_session_abort = session_events
        .list_events_by_session(&input.session_id)
        .await?
        .into_iter()
        .find(|event| event.event_id == session_aborted.event_id);
    let session_outcome = if let Some(existing) = existing_session_abort {
        AppendEventOutcome::Existing(existing)
    } else {
        session_events.append_event(&session_aborted).await?
    };
    let (canonical_session, publish_session) = match session_outcome {
        AppendEventOutcome::Inserted(record) => (record, true),
        AppendEventOutcome::Existing(record) => (record, false),
    };
    session.state = SessionState::Aborted;
    session.updated_at = now.clone();
    sessions.save_session(&session).await?;
    if publish_turn {
        events
            .publish_durable_session(
                &canonical_turn,
                CoreEvent::SessionTurnAborted {
                    session_id: input.session_id.0.clone(),
                    turn_id: active_turn_id.0.clone(),
                    reason: canonical_reason.clone(),
                },
            )
            .await?;
    }
    if publish_session {
        events
            .publish_durable_session(
                &canonical_session,
                CoreEvent::SessionAborted {
                    session_id: input.session_id.0.clone(),
                    reason: canonical_reason,
                },
            )
            .await?;
    }

    let mut replay = history;
    replay.push(canonical_turn);
    replay.push(canonical_session);
    let projection = SessionProjection::fold(&replay).expect("session projection exists");

    Ok(AbortRunOutput {
        session,
        projection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    use crate::domain::{project::ProjectId, workspace::WorkspaceId};

    #[derive(Clone)]
    struct SharedRepo {
        session: Arc<Mutex<Session>>,
        events: Arc<Mutex<Vec<SessionEventRecord>>>,
    }

    #[async_trait]
    impl SessionRepo for SharedRepo {
        async fn save_session(&self, session: &Session) -> Result<()> {
            *self.session.lock().expect("session lock") = session.clone();
            Ok(())
        }

        async fn get_session(&self, _: &SessionId) -> Result<Option<Session>> {
            Ok(Some(self.session.lock().expect("session lock").clone()))
        }

        async fn delete_session(&self, _: &SessionId) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl SessionEventRepo for SharedRepo {
        async fn append_event(&self, event: &SessionEventRecord) -> Result<AppendEventOutcome> {
            let mut events = self.events.lock().expect("events lock");
            if let Some(existing) = events
                .iter()
                .find(|candidate| candidate.event_id == event.event_id)
            {
                let same_kind = serde_json::to_string(&existing.kind).expect("kind")
                    == serde_json::to_string(&event.kind).expect("kind");
                if existing.session_id != event.session_id
                    || existing.occurred_at != event.occurred_at
                    || !same_kind
                {
                    return Err(crate::CoreError::Repository(
                        "event identity conflicts with existing event".to_owned(),
                    ));
                }
                return Ok(AppendEventOutcome::Existing(existing.clone()));
            }
            let terminal = match &event.kind {
                SessionEventKind::TurnCompleted { turn_id }
                | SessionEventKind::TurnAborted { turn_id, .. } => Some(turn_id),
                _ => None,
            };
            if let Some(turn_id) = terminal {
                if let Some(existing) = events.iter().find(|candidate| {
                    candidate.session_id == event.session_id
                        && match &candidate.kind {
                            SessionEventKind::TurnCompleted { turn_id: candidate }
                            | SessionEventKind::TurnAborted {
                                turn_id: candidate, ..
                            } => candidate == turn_id,
                            _ => false,
                        }
                }) {
                    return Ok(AppendEventOutcome::Existing(existing.clone()));
                }
            }
            let mut canonical = event.clone();
            canonical.sequence = events
                .iter()
                .filter(|candidate| candidate.session_id == event.session_id)
                .map(|candidate| candidate.sequence)
                .max()
                .unwrap_or(0)
                + 1;
            events.push(canonical.clone());
            Ok(AppendEventOutcome::Inserted(canonical))
        }

        async fn list_events_by_session(
            &self,
            session_id: &SessionId,
        ) -> Result<Vec<SessionEventRecord>> {
            Ok(self
                .events
                .lock()
                .expect("events lock")
                .iter()
                .filter(|event| &event.session_id == session_id)
                .cloned()
                .collect())
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

    fn fixture() -> SharedRepo {
        let session_id = SessionId("session-1".to_owned());
        let workspace_id = WorkspaceId("workspace-1".to_owned());
        let project_id = ProjectId("project-1".to_owned());
        let session = Session {
            id: session_id.clone(),
            project_id: project_id.clone(),
            workspace_id: workspace_id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_owned(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "t0".to_owned(),
            updated_at: "t0".to_owned(),
        };
        let events = vec![
            SessionEventRecord {
                event_id: "started".to_owned(),
                session_id: session_id.clone(),
                sequence: 1,
                occurred_at: "t0".to_owned(),
                kind: SessionEventKind::SessionStarted {
                    workspace_id,
                    project_id,
                    provider_id: "codex".to_owned(),
                    model: None,
                },
            },
            SessionEventRecord {
                event_id: "turn-started".to_owned(),
                session_id: session_id.clone(),
                sequence: 2,
                occurred_at: "t1".to_owned(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: crate::domain::session::TurnId("turn-1".to_owned()),
                    prompt: "prompt".to_owned(),
                    plan_mode: None,
                    model: None,
                },
            },
        ];
        SharedRepo {
            session: Arc::new(Mutex::new(session)),
            events: Arc::new(Mutex::new(events)),
        }
    }

    #[test]
    fn session_abort_identity_is_opaque_domain_separated_and_deterministic() {
        let session = SessionId("session-1".to_owned());
        let turn = crate::domain::session::TurnId("turn-1".to_owned());
        let first = session_abort_event_id(&session, &turn);
        assert_eq!(first, session_abort_event_id(&session, &turn));
        assert!(first.starts_with("session-aborted:v1:"));
        assert_eq!(first.len(), "session-aborted:v1:".len() + 64);
        assert_ne!(
            first,
            session_abort_event_id(
                &session,
                &crate::domain::session::TurnId("turn-2".to_owned())
            )
        );
        assert_ne!(
            first,
            session_abort_event_id(&SessionId("session-2".to_owned()), &turn)
        );
        assert!(!first.contains("session-1"));
        assert!(!first.contains("turn-1"));
    }

    #[test]
    fn concurrent_abort_callers_share_canonical_reason_and_publish_once() {
        let repo = fixture();
        let bus = RecordingBus::default();
        let first_repo = repo.clone();
        let first_bus = bus.clone();
        let first = std::thread::spawn(move || {
            futures::executor::block_on(abort_run(
                &first_repo,
                &first_repo,
                &first_bus,
                AbortRunInput {
                    session_id: SessionId("session-1".to_owned()),
                    reason: Some("first reason".to_owned()),
                },
            ))
        });
        let second_repo = repo.clone();
        let second_bus = bus.clone();
        let second = std::thread::spawn(move || {
            futures::executor::block_on(abort_run(
                &second_repo,
                &second_repo,
                &second_bus,
                AbortRunInput {
                    session_id: SessionId("session-1".to_owned()),
                    reason: Some("second reason".to_owned()),
                },
            ))
        });
        let first = first.join().expect("first abort").expect("first result");
        let second = second.join().expect("second abort").expect("second result");
        assert_eq!(first.projection.state, SessionState::Aborted);
        assert_eq!(second.projection.state, SessionState::Aborted);
        let events = repo.events.lock().expect("events lock");
        let turn_reason = events.iter().find_map(|event| match &event.kind {
            SessionEventKind::TurnAborted { reason, .. } => Some(reason.clone()),
            _ => None,
        });
        let session_reason = events.iter().find_map(|event| match &event.kind {
            SessionEventKind::SessionAborted { reason } => Some(reason.clone()),
            _ => None,
        });
        assert_eq!(turn_reason, session_reason);
        assert_eq!(first.projection.active_turn_id, None);
        assert_eq!(second.projection.active_turn_id, None);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, SessionEventKind::TurnAborted { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, SessionEventKind::SessionAborted { .. }))
                .count(),
            1
        );
        let published = bus.0.lock().expect("bus lock");
        assert_eq!(
            published
                .iter()
                .filter(|event| matches!(event, CoreEvent::SessionTurnAborted { .. }))
                .count(),
            1
        );
        assert_eq!(
            published
                .iter()
                .filter(|event| matches!(event, CoreEvent::SessionAborted { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn abort_resume_new_turn_abort_creates_distinct_session_terminals() {
        let repo = fixture();
        let bus = RecordingBus::default();
        futures::executor::block_on(abort_run(
            &repo,
            &repo,
            &bus,
            AbortRunInput {
                session_id: SessionId("session-1".to_owned()),
                reason: Some("one".to_owned()),
            },
        ))
        .expect("first abort");
        futures::executor::block_on(repo.append_event(&SessionEventRecord {
            event_id: "resume".to_owned(),
            session_id: SessionId("session-1".to_owned()),
            sequence: 99,
            occurred_at: "t3".to_owned(),
            kind: SessionEventKind::SessionResumed,
        }))
        .expect("resume");
        let mut resumed_session = repo.session.lock().expect("session lock").clone();
        resumed_session.state = SessionState::Active;
        futures::executor::block_on(repo.save_session(&resumed_session)).expect("save active");
        futures::executor::block_on(repo.append_event(&SessionEventRecord {
            event_id: "turn-2".to_owned(),
            session_id: SessionId("session-1".to_owned()),
            sequence: 100,
            occurred_at: "t4".to_owned(),
            kind: SessionEventKind::TurnStarted {
                turn_id: crate::domain::session::TurnId("turn-2".to_owned()),
                prompt: "next".to_owned(),
                plan_mode: None,
                model: None,
            },
        }))
        .expect("new turn");
        futures::executor::block_on(abort_run(
            &repo,
            &repo,
            &bus,
            AbortRunInput {
                session_id: SessionId("session-1".to_owned()),
                reason: Some("two".to_owned()),
            },
        ))
        .expect("second abort");
        let events = repo.events.lock().expect("events lock");
        let ids = events
            .iter()
            .filter_map(|event| {
                matches!(event.kind, SessionEventKind::SessionAborted { .. })
                    .then(|| event.event_id.clone())
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn crash_after_turn_abort_repairs_session_without_republishing_turn() {
        let repo = fixture();
        let bus = RecordingBus::default();
        futures::executor::block_on(repo.append_event(&SessionEventRecord {
            event_id: "crashed-turn-abort".to_owned(),
            session_id: SessionId("session-1".to_owned()),
            sequence: 3,
            occurred_at: "t2".to_owned(),
            kind: SessionEventKind::TurnAborted {
                turn_id: crate::domain::session::TurnId("turn-1".to_owned()),
                reason: Some("crash reason".to_owned()),
            },
        }))
        .expect("durable turn abort");
        let output = futures::executor::block_on(abort_run(
            &repo,
            &repo,
            &bus,
            AbortRunInput {
                session_id: SessionId("session-1".to_owned()),
                reason: Some("loser request reason".to_owned()),
            },
        ))
        .expect("repair retry");
        assert_eq!(output.projection.state, SessionState::Aborted);
        assert_eq!(output.session.state, SessionState::Aborted);
        let events = repo.events.lock().expect("events lock");
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, SessionEventKind::TurnAborted { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, SessionEventKind::SessionAborted { .. }))
                .count(),
            1
        );
        let published = bus.0.lock().expect("bus lock");
        assert_eq!(
            published
                .iter()
                .filter(|event| matches!(event, CoreEvent::SessionTurnAborted { .. }))
                .count(),
            0
        );
        assert_eq!(
            published
                .iter()
                .filter(|event| matches!(event, CoreEvent::SessionAborted { .. }))
                .count(),
            1
        );
    }
}
