use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::{
        project::ProjectId,
        session::{
            Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection,
            SessionState,
        },
        thread::{Thread, ThreadId},
        workspace::WorkspaceId,
    },
    ports::{
        CoreEvent, EventBus, ProviderRuntimeConfig, SessionEventRepo, SessionRepo, ThreadRepo,
    },
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartThreadInput {
    pub workspace_id: WorkspaceId,
    pub project_id: ProjectId,
    pub provider_id: String,
    pub model: Option<String>,
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
    #[serde(default)]
    pub working_directory_override: Option<String>,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartThreadOutput {
    pub session: Session,
    pub thread: Thread,
    pub projection: SessionProjection,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn start_thread<S, T, B>(
    sessions: &S,
    threads: &T,
    session_events: &B,
    events: &B,
    input: StartThreadInput,
) -> Result<StartThreadOutput>
where
    S: SessionRepo + Sync,
    T: ThreadRepo + Sync,
    B: EventBus + SessionEventRepo + Sync,
{
    if input.provider_id.trim().is_empty() {
        return Err(crate::CoreError::InvalidInput(
            "provider_id cannot be empty".to_string(),
        ));
    }

    let session_id = SessionId(Uuid::new_v4().to_string());
    let thread_id = ThreadId(Uuid::new_v4().to_string());
    let now = now_iso();
    let title = input
        .title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "New session".to_string());

    let session = Session {
        id: session_id.clone(),
        project_id: input.project_id.clone(),
        workspace_id: input.workspace_id.clone(),
        provider_id: input.provider_id.clone(),
        model: input.model.clone(),
        provider_runtime: input.provider_runtime.clone(),
        working_directory_override: input.working_directory_override.clone(),
        state: SessionState::Active,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    let thread = Thread {
        id: thread_id,
        project_id: input.project_id.clone(),
        session_id: Some(session_id.clone()),
        title,
        archived_at: None,
    };

    sessions.save_session(&session).await?;
    threads.save_thread(&thread).await?;

    let started_event = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        sequence: 1,
        occurred_at: now.clone(),
        kind: SessionEventKind::SessionStarted {
            workspace_id: input.workspace_id.clone(),
            project_id: input.project_id.clone(),
            provider_id: input.provider_id.clone(),
            model: input.model.clone(),
        },
    };
    session_events.append_event(&started_event).await?;
    events
        .publish(CoreEvent::SessionStarted {
            session_id: session_id.0.clone(),
            workspace_id: input.workspace_id.0.clone(),
            project_id: input.project_id.0.clone(),
            provider_id: input.provider_id.clone(),
            model: input.model,
        })
        .await?;

    let projection = SessionProjection::fold(&[started_event]).expect("session projection exists");

    Ok(StartThreadOutput {
        session,
        thread,
        projection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeSessionRepo {
        sessions: Arc<Mutex<Vec<Session>>>,
    }

    #[async_trait]
    impl SessionRepo for FakeSessionRepo {
        async fn save_session(&self, session: &Session) -> Result<()> {
            self.sessions
                .lock()
                .expect("sessions lock poisoned")
                .push(session.clone());
            Ok(())
        }

        async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
            let found = self
                .sessions
                .lock()
                .expect("sessions lock poisoned")
                .iter()
                .find(|session| &session.id == id)
                .cloned();
            Ok(found)
        }

        async fn delete_session(&self, id: &SessionId) -> Result<()> {
            self.sessions
                .lock()
                .expect("sessions lock poisoned")
                .retain(|session| &session.id != id);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FakeThreadRepo {
        threads: Arc<Mutex<Vec<Thread>>>,
    }

    #[async_trait]
    impl ThreadRepo for FakeThreadRepo {
        async fn save_thread(&self, thread: &Thread) -> Result<()> {
            self.threads
                .lock()
                .expect("threads lock poisoned")
                .push(thread.clone());
            Ok(())
        }

        async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
            let found = self
                .threads
                .lock()
                .expect("threads lock poisoned")
                .iter()
                .find(|thread| &thread.id == id)
                .cloned();
            Ok(found)
        }

        async fn find_thread_by_session_id(
            &self,
            session_id: &SessionId,
        ) -> Result<Option<Thread>> {
            let found = self
                .threads
                .lock()
                .expect("threads lock poisoned")
                .iter()
                .find(|thread| thread.session_id.as_ref() == Some(session_id))
                .cloned();
            Ok(found)
        }

        async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
            self.threads
                .lock()
                .expect("threads lock poisoned")
                .retain(|thread| &thread.id != id);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FakeEventBus {
        events: Arc<Mutex<Vec<CoreEvent>>>,
        session_events: Arc<Mutex<Vec<SessionEventRecord>>>,
    }

    #[async_trait]
    impl EventBus for FakeEventBus {
        async fn publish(&self, event: CoreEvent) -> Result<()> {
            self.events
                .lock()
                .expect("events lock poisoned")
                .push(event);
            Ok(())
        }
    }

    #[async_trait]
    impl SessionEventRepo for FakeEventBus {
        async fn append_event(&self, event: &SessionEventRecord) -> Result<()> {
            self.session_events
                .lock()
                .expect("session events lock poisoned")
                .push(event.clone());
            Ok(())
        }

        async fn list_events_by_session(
            &self,
            session_id: &SessionId,
        ) -> Result<Vec<SessionEventRecord>> {
            let events = self
                .session_events
                .lock()
                .expect("session events lock poisoned")
                .iter()
                .filter(|event| &event.session_id == session_id)
                .cloned()
                .collect();
            Ok(events)
        }

        async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
            self.session_events
                .lock()
                .expect("session events lock poisoned")
                .retain(|event| &event.session_id != session_id);
            Ok(())
        }
    }

    #[test]
    fn start_thread_creates_session_thread_and_projection() {
        let sessions = FakeSessionRepo::default();
        let threads = FakeThreadRepo::default();
        let events = FakeEventBus::default();

        let output = futures::executor::block_on(start_thread(
            &sessions,
            &threads,
            &events,
            &events,
            StartThreadInput {
                workspace_id: WorkspaceId("workspace-1".to_string()),
                project_id: ProjectId("project-1".to_string()),
                provider_id: "codex".to_string(),
                model: Some("gpt-5-codex".to_string()),
                provider_runtime: None,
                working_directory_override: None,
                title: Some("Launch session".to_string()),
            },
        ))
        .expect("start_thread should succeed");

        assert_eq!(output.session.state, SessionState::Active);
        assert_eq!(output.session.workspace_id.0, "workspace-1");
        assert_eq!(output.session.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(
            output.thread.session_id.as_ref().map(|id| id.0.as_str()),
            Some(output.session.id.0.as_str())
        );
        assert_eq!(output.thread.title, "Launch session");
        assert_eq!(output.projection.state, SessionState::Active);
        assert_eq!(output.projection.turn_count, 0);

        let saved_sessions = sessions.sessions.lock().expect("sessions lock poisoned");
        assert_eq!(saved_sessions.len(), 1);
        let saved_threads = threads.threads.lock().expect("threads lock poisoned");
        assert_eq!(saved_threads.len(), 1);
        let session_events = events
            .session_events
            .lock()
            .expect("session events lock poisoned");
        assert_eq!(session_events.len(), 1);
        assert!(matches!(
            session_events[0].kind,
            SessionEventKind::SessionStarted { .. }
        ));
        let published_events = events.events.lock().expect("events lock poisoned");
        assert_eq!(published_events.len(), 1);
        assert!(matches!(
            published_events[0],
            CoreEvent::SessionStarted { .. }
        ));
    }
}
