use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::session::{SessionEventKind, SessionEventRecord, SessionId},
    ports::{AppendEventOutcome, CoreEvent, EventBus, SessionEventRepo},
    CoreError, Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApprovePlanInput {
    pub session_id: SessionId,
    pub plan_message_id: String,
    pub plan_version: u32,
    pub plan_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApprovePlanOutput {
    pub event: SessionEventRecord,
    pub created: bool,
}

pub async fn approve_plan<E, B>(
    session_events: &E,
    events: &B,
    input: ApprovePlanInput,
) -> Result<ApprovePlanOutput>
where
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let plan_message_id = input.plan_message_id.trim();
    let plan_hash = input.plan_hash.trim();
    if plan_message_id.is_empty() {
        return Err(CoreError::InvalidInput(
            "plan_message_id is required".to_string(),
        ));
    }
    if input.plan_version == 0 {
        return Err(CoreError::InvalidInput(
            "plan_version must be greater than zero".to_string(),
        ));
    }
    if plan_hash.is_empty() {
        return Err(CoreError::InvalidInput("plan_hash is required".to_string()));
    }

    let history = session_events
        .list_events_by_session(&input.session_id)
        .await?;
    if history.is_empty() {
        return Err(CoreError::Repository(
            "session history is empty".to_string(),
        ));
    }

    if let Some(existing) = history.iter().find(|event| {
        matches!(
            &event.kind,
            SessionEventKind::PlanApproved {
                plan_message_id: existing_message_id,
                plan_version: existing_version,
                plan_hash: existing_hash,
            } if existing_message_id == plan_message_id && existing_hash == plan_hash
                && *existing_version == input.plan_version
        )
    }) {
        return Ok(ApprovePlanOutput {
            event: existing.clone(),
            created: false,
        });
    }

    let event = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence: history.last().map(|event| event.sequence + 1).unwrap_or(1),
        occurred_at: chrono::Utc::now().to_rfc3339(),
        kind: SessionEventKind::PlanApproved {
            plan_message_id: plan_message_id.to_string(),
            plan_version: input.plan_version,
            plan_hash: plan_hash.to_string(),
        },
    };
    let outcome = session_events.append_event(&event).await?;
    let created = matches!(&outcome, AppendEventOutcome::Inserted(_));
    let event = match outcome {
        AppendEventOutcome::Inserted(event) | AppendEventOutcome::Existing(event) => event,
    };
    if created {
        events
            .publish_durable_session(
                &event,
                CoreEvent::SessionPlanApproved {
                    session_id: input.session_id.0,
                    plan_message_id: plan_message_id.to_string(),
                    plan_version: input.plan_version,
                    plan_hash: plan_hash.to_string(),
                },
            )
            .await?;
    }

    Ok(ApprovePlanOutput { event, created })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeStore {
        records: Arc<Mutex<Vec<SessionEventRecord>>>,
        published: Arc<Mutex<Vec<CoreEvent>>>,
        durable: Arc<Mutex<Vec<(String, u64)>>>,
    }

    #[async_trait]
    impl SessionEventRepo for FakeStore {
        async fn append_event(
            &self,
            event: &SessionEventRecord,
        ) -> Result<crate::ports::AppendEventOutcome> {
            self.records
                .lock()
                .expect("records lock poisoned")
                .push(event.clone());
            Ok(crate::ports::AppendEventOutcome::Inserted(event.clone()))
        }

        async fn list_events_by_session(
            &self,
            session_id: &SessionId,
        ) -> Result<Vec<SessionEventRecord>> {
            Ok(self
                .records
                .lock()
                .expect("records lock poisoned")
                .iter()
                .filter(|event| &event.session_id == session_id)
                .cloned()
                .collect())
        }

        async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
            self.records
                .lock()
                .expect("records lock poisoned")
                .retain(|event| &event.session_id != session_id);
            Ok(())
        }
    }

    #[async_trait]
    impl EventBus for FakeStore {
        async fn publish(&self, event: CoreEvent) -> Result<()> {
            self.published
                .lock()
                .expect("published lock poisoned")
                .push(event);
            Ok(())
        }

        async fn publish_durable_session(
            &self,
            record: &SessionEventRecord,
            event: CoreEvent,
        ) -> Result<()> {
            self.durable
                .lock()
                .expect("durable lock poisoned")
                .push((record.event_id.clone(), record.sequence));
            self.publish(event).await
        }
    }

    fn started_event() -> SessionEventRecord {
        SessionEventRecord {
            event_id: "event-started".to_string(),
            session_id: SessionId("session-1".to_string()),
            sequence: 1,
            occurred_at: "2026-07-23T10:00:00Z".to_string(),
            kind: SessionEventKind::SessionResumed,
        }
    }

    fn input() -> ApprovePlanInput {
        ApprovePlanInput {
            session_id: SessionId("session-1".to_string()),
            plan_message_id: "assistant-session-1-turn-1".to_string(),
            plan_version: 1,
            plan_hash: "fnv1a32:12345678".to_string(),
        }
    }

    #[test]
    fn persists_and_publishes_plan_approval() {
        let store = FakeStore::default();
        store
            .records
            .lock()
            .expect("records lock poisoned")
            .push(started_event());

        let output = futures::executor::block_on(approve_plan(&store, &store, input()))
            .expect("approval should succeed");

        assert!(output.created);
        assert_eq!(output.event.sequence, 2);
        assert!(matches!(
            output.event.kind,
            SessionEventKind::PlanApproved {
                plan_version: 1,
                ..
            }
        ));
        assert_eq!(
            store.published.lock().expect("published lock poisoned")[0],
            CoreEvent::SessionPlanApproved {
                session_id: "session-1".to_string(),
                plan_message_id: "assistant-session-1-turn-1".to_string(),
                plan_version: 1,
                plan_hash: "fnv1a32:12345678".to_string(),
            }
        );
        assert_eq!(
            store
                .durable
                .lock()
                .expect("durable lock poisoned")
                .as_slice(),
            [(output.event.event_id.clone(), output.event.sequence)]
        );
    }

    #[test]
    fn repeated_approval_is_idempotent() {
        let store = FakeStore::default();
        store
            .records
            .lock()
            .expect("records lock poisoned")
            .push(started_event());

        let first = futures::executor::block_on(approve_plan(&store, &store, input()))
            .expect("first approval should succeed");
        let second = futures::executor::block_on(approve_plan(&store, &store, input()))
            .expect("second approval should succeed");

        assert!(first.created);
        assert!(!second.created);
        assert_eq!(
            store.records.lock().expect("records lock poisoned").len(),
            2
        );
        assert_eq!(
            store
                .published
                .lock()
                .expect("published lock poisoned")
                .len(),
            1
        );
        assert_eq!(
            store.durable.lock().expect("durable lock poisoned").len(),
            1
        );
    }

    #[test]
    fn a_different_version_creates_a_distinct_approval() {
        let store = FakeStore::default();
        store
            .records
            .lock()
            .expect("records lock poisoned")
            .push(started_event());

        futures::executor::block_on(approve_plan(&store, &store, input()))
            .expect("first approval should succeed");
        let mut revised = input();
        revised.plan_version = 2;
        let second = futures::executor::block_on(approve_plan(&store, &store, revised))
            .expect("revised approval should succeed");

        assert!(second.created);
        assert_eq!(
            store.records.lock().expect("records lock poisoned").len(),
            3
        );
    }
}
