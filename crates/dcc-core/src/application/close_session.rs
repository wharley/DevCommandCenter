use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    domain::session::SessionId,
    ports::{SessionEventRepo, SessionRepo, ThreadRepo},
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionInput {
    pub session_id: SessionId,
    pub delete_history: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionOutput {
    pub session_id: SessionId,
    pub deleted_history: bool,
    pub archived_at: Option<String>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn close_session<S, T, E>(
    sessions: &S,
    threads: &T,
    events: &E,
    input: CloseSessionInput,
) -> Result<CloseSessionOutput>
where
    S: SessionRepo + Sync,
    T: ThreadRepo + Sync,
    E: SessionEventRepo + Sync,
{
    sessions
        .get_session(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("session not found".to_string()))?;

    let mut thread = threads
        .find_thread_by_session_id(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("thread not found for session".to_string()))?;

    if input.delete_history {
        threads.delete_thread(&thread.id).await?;
        sessions.delete_session(&input.session_id).await?;
        events.delete_events_by_session(&input.session_id).await?;

        return Ok(CloseSessionOutput {
            session_id: input.session_id,
            deleted_history: true,
            archived_at: None,
        });
    }

    let archived_at = thread.archived_at.clone().unwrap_or_else(now_iso);
    thread.archived_at = Some(archived_at.clone());
    threads.save_thread(&thread).await?;

    Ok(CloseSessionOutput {
        session_id: input.session_id,
        deleted_history: false,
        archived_at: Some(archived_at),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    use crate::{
        domain::{
            project::ProjectId,
            session::{Session, SessionEventKind, SessionEventRecord, SessionState},
            thread::{Thread, ThreadId},
            workspace::WorkspaceId,
        },
        ports::{SessionEventRepo, SessionRepo, ThreadRepo},
    };

    #[derive(Clone, Default)]
    struct FakeSessionRepo {
        sessions: Arc<Mutex<Vec<Session>>>,
    }

    #[async_trait]
    impl SessionRepo for FakeSessionRepo {
        async fn save_session(&self, session: &Session) -> Result<()> {
            let mut sessions = self.sessions.lock().expect("sessions lock poisoned");
            if let Some(index) = sessions
                .iter()
                .position(|candidate| candidate.id == session.id)
            {
                sessions[index] = session.clone();
            } else {
                sessions.push(session.clone());
            }
            Ok(())
        }

        async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
            Ok(self
                .sessions
                .lock()
                .expect("sessions lock poisoned")
                .iter()
                .find(|session| &session.id == id)
                .cloned())
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
            let mut threads = self.threads.lock().expect("threads lock poisoned");
            if let Some(index) = threads
                .iter()
                .position(|candidate| candidate.id == thread.id)
            {
                threads[index] = thread.clone();
            } else {
                threads.push(thread.clone());
            }
            Ok(())
        }

        async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
            Ok(self
                .threads
                .lock()
                .expect("threads lock poisoned")
                .iter()
                .find(|thread| &thread.id == id)
                .cloned())
        }

        async fn find_thread_by_session_id(
            &self,
            session_id: &SessionId,
        ) -> Result<Option<Thread>> {
            Ok(self
                .threads
                .lock()
                .expect("threads lock poisoned")
                .iter()
                .find(|thread| thread.session_id.as_ref() == Some(session_id))
                .cloned())
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
    struct FakeSessionEventRepo {
        events: Arc<Mutex<Vec<SessionEventRecord>>>,
    }

    #[async_trait]
    impl SessionEventRepo for FakeSessionEventRepo {
        async fn append_event(
            &self,
            event: &SessionEventRecord,
        ) -> Result<crate::ports::AppendEventOutcome> {
            self.events
                .lock()
                .expect("events lock poisoned")
                .push(event.clone());
            Ok(crate::ports::AppendEventOutcome::Inserted(event.clone()))
        }

        async fn list_events_by_session(
            &self,
            session_id: &SessionId,
        ) -> Result<Vec<SessionEventRecord>> {
            Ok(self
                .events
                .lock()
                .expect("events lock poisoned")
                .iter()
                .filter(|event| &event.session_id == session_id)
                .cloned()
                .collect())
        }

        async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
            self.events
                .lock()
                .expect("events lock poisoned")
                .retain(|event| &event.session_id != session_id);
            Ok(())
        }
    }

    fn sample_session() -> Session {
        Session {
            id: SessionId("session-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            workspace_id: WorkspaceId("workspace-1".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: Some("gpt-5".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_thread() -> Thread {
        Thread {
            id: ThreadId("thread-1".to_string()),
            project_id: ProjectId("project-1".to_string()),
            session_id: Some(SessionId("session-1".to_string())),
            title: "Thread".to_string(),
            archived_at: None,
        }
    }

    #[test]
    fn close_session_archives_thread_when_history_is_kept() {
        let sessions = FakeSessionRepo::default();
        let threads = FakeThreadRepo::default();
        let events = FakeSessionEventRepo::default();
        futures::executor::block_on(sessions.save_session(&sample_session()))
            .expect("save session should succeed");
        futures::executor::block_on(threads.save_thread(&sample_thread()))
            .expect("save thread should succeed");

        let output = futures::executor::block_on(close_session(
            &sessions,
            &threads,
            &events,
            CloseSessionInput {
                session_id: SessionId("session-1".to_string()),
                delete_history: false,
            },
        ))
        .expect("close session should succeed");

        assert!(!output.deleted_history);
        assert!(output.archived_at.is_some());
        let thread = futures::executor::block_on(
            threads.find_thread_by_session_id(&SessionId("session-1".to_string())),
        )
        .expect("find thread should succeed")
        .expect("thread should exist");
        assert!(thread.archived_at.is_some());
    }

    #[test]
    fn close_session_deletes_session_thread_and_events() {
        let sessions = FakeSessionRepo::default();
        let threads = FakeThreadRepo::default();
        let events = FakeSessionEventRepo::default();
        futures::executor::block_on(sessions.save_session(&sample_session()))
            .expect("save session should succeed");
        futures::executor::block_on(threads.save_thread(&sample_thread()))
            .expect("save thread should succeed");
        futures::executor::block_on(events.append_event(&SessionEventRecord {
            event_id: "event-1".to_string(),
            session_id: SessionId("session-1".to_string()),
            sequence: 1,
            occurred_at: "2026-01-01T00:00:00Z".to_string(),
            kind: SessionEventKind::SessionResumed,
        }))
        .expect("append event should succeed");

        let output = futures::executor::block_on(close_session(
            &sessions,
            &threads,
            &events,
            CloseSessionInput {
                session_id: SessionId("session-1".to_string()),
                delete_history: true,
            },
        ))
        .expect("delete session should succeed");

        assert!(output.deleted_history);
        assert!(futures::executor::block_on(
            sessions.get_session(&SessionId("session-1".to_string()))
        )
        .expect("get session should succeed")
        .is_none());
        assert!(futures::executor::block_on(
            threads.find_thread_by_session_id(&SessionId("session-1".to_string()))
        )
        .expect("find thread should succeed")
        .is_none());
        assert!(futures::executor::block_on(
            events.list_events_by_session(&SessionId("session-1".to_string()))
        )
        .expect("list events should succeed")
        .is_empty());
    }
}
