use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::session::{SessionEventKind, SessionEventRecord, SessionId},
    ports::{CoreEvent, EventBus, SessionEventRepo},
    CoreError, Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordPlanHandoffInput {
    pub session_id: SessionId,
    pub plan_message_id: String,
    pub plan_version: u32,
    pub plan_hash: String,
    pub action: String,
    pub target_session_id: Option<SessionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordPlanHandoffOutput {
    pub event: SessionEventRecord,
    pub created: bool,
}

pub async fn record_plan_handoff<E, B>(
    session_events: &E,
    events: &B,
    input: RecordPlanHandoffInput,
) -> Result<RecordPlanHandoffOutput>
where
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let plan_message_id = input.plan_message_id.trim();
    let plan_hash = input.plan_hash.trim();
    let action = input.action.trim();
    if plan_message_id.is_empty() || plan_hash.is_empty() {
        return Err(CoreError::InvalidInput(
            "plan_message_id and plan_hash are required".to_string(),
        ));
    }
    if input.plan_version == 0 {
        return Err(CoreError::InvalidInput(
            "plan_version must be greater than zero".to_string(),
        ));
    }
    if !matches!(action, "delegation" | "new_thread") {
        return Err(CoreError::InvalidInput(
            "action must be delegation or new_thread".to_string(),
        ));
    }

    let history = session_events
        .list_events_by_session(&input.session_id)
        .await?;
    if history.is_empty() {
        return Err(CoreError::Repository(
            "session history is empty".to_string(),
        ));
    }
    if !history.iter().any(|event| {
        matches!(
            &event.kind,
            SessionEventKind::PlanApproved {
                plan_message_id: approved_message_id,
                plan_version: approved_version,
                plan_hash: approved_hash,
            } if approved_message_id == plan_message_id
                && *approved_version == input.plan_version
                && approved_hash == plan_hash
        )
    }) {
        return Err(CoreError::InvalidInput(
            "the exact plan version must be approved before handoff".to_string(),
        ));
    }

    if let Some(existing) = history.iter().find(|event| {
        matches!(
            &event.kind,
            SessionEventKind::PlanHandedOff {
                plan_message_id: existing_message_id,
                plan_version: existing_version,
                plan_hash: existing_hash,
                ..
            } if existing_message_id == plan_message_id
                && *existing_version == input.plan_version
                && existing_hash == plan_hash
        )
    }) {
        return Ok(RecordPlanHandoffOutput {
            event: existing.clone(),
            created: false,
        });
    }

    let target_session_id = input.target_session_id.clone();
    let event = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence: history.last().map(|event| event.sequence + 1).unwrap_or(1),
        occurred_at: chrono::Utc::now().to_rfc3339(),
        kind: SessionEventKind::PlanHandedOff {
            plan_message_id: plan_message_id.to_string(),
            plan_version: input.plan_version,
            plan_hash: plan_hash.to_string(),
            action: action.to_string(),
            target_session_id: target_session_id.clone(),
        },
    };
    session_events.append_event(&event).await?;
    events
        .publish(CoreEvent::SessionPlanHandedOff {
            session_id: input.session_id.0,
            plan_message_id: plan_message_id.to_string(),
            plan_version: input.plan_version,
            plan_hash: plan_hash.to_string(),
            action: action.to_string(),
            target_session_id: target_session_id.map(|session_id| session_id.0),
        })
        .await?;

    Ok(RecordPlanHandoffOutput {
        event,
        created: true,
    })
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
    }

    #[async_trait]
    impl SessionEventRepo for FakeStore {
        async fn append_event(&self, event: &SessionEventRecord) -> Result<()> {
            self.records.lock().unwrap().push(event.clone());
            Ok(())
        }

        async fn list_events_by_session(
            &self,
            session_id: &SessionId,
        ) -> Result<Vec<SessionEventRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .iter()
                .filter(|event| &event.session_id == session_id)
                .cloned()
                .collect())
        }

        async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
            self.records
                .lock()
                .unwrap()
                .retain(|event| &event.session_id != session_id);
            Ok(())
        }
    }

    #[async_trait]
    impl EventBus for FakeStore {
        async fn publish(&self, event: CoreEvent) -> Result<()> {
            self.published.lock().unwrap().push(event);
            Ok(())
        }
    }

    fn approved_event() -> SessionEventRecord {
        SessionEventRecord {
            event_id: "approved".to_string(),
            session_id: SessionId("session-1".to_string()),
            sequence: 1,
            occurred_at: "2026-07-23T10:00:00Z".to_string(),
            kind: SessionEventKind::PlanApproved {
                plan_message_id: "assistant-session-1-turn-1".to_string(),
                plan_version: 1,
                plan_hash: "fnv1a32:12345678".to_string(),
            },
        }
    }

    fn input() -> RecordPlanHandoffInput {
        RecordPlanHandoffInput {
            session_id: SessionId("session-1".to_string()),
            plan_message_id: "assistant-session-1-turn-1".to_string(),
            plan_version: 1,
            plan_hash: "fnv1a32:12345678".to_string(),
            action: "delegation".to_string(),
            target_session_id: None,
        }
    }

    #[test]
    fn records_an_approved_plan_handoff_once() {
        let store = FakeStore::default();
        store.records.lock().unwrap().push(approved_event());

        let first = futures::executor::block_on(record_plan_handoff(&store, &store, input()))
            .expect("first handoff should succeed");
        let second = futures::executor::block_on(record_plan_handoff(&store, &store, input()))
            .expect("repeated handoff should be idempotent");

        assert!(first.created);
        assert!(!second.created);
        assert_eq!(store.records.lock().unwrap().len(), 2);
        assert_eq!(store.published.lock().unwrap().len(), 1);
    }

    #[test]
    fn rejects_a_handoff_for_an_unapproved_plan_version() {
        let store = FakeStore::default();
        store.records.lock().unwrap().push(approved_event());
        let mut mismatched = input();
        mismatched.plan_version = 2;

        let result = futures::executor::block_on(record_plan_handoff(&store, &store, mismatched));

        assert!(matches!(result, Err(CoreError::InvalidInput(_))));
        assert_eq!(store.records.lock().unwrap().len(), 1);
    }
}
