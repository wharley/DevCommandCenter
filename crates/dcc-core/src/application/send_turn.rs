use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::{
        model_registry,
        session::{
            Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection,
            SessionState, Turn, TurnId, TurnState,
        },
    },
    ports::{CoreEvent, EventBus, ProviderRuntimeConfig, SessionEventRepo, SessionRepo},
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SendTurnInput {
    pub session_id: SessionId,
    /// Raw composer text for timeline / Turn records; provider adapters decide how to transport it.
    pub prompt: String,
    /// Provider-facing tool/system instructions. Not persisted as user text.
    #[serde(default)]
    pub tool_instructions: Option<String>,
    /// When set, replaces the session provider before this turn (same workspace session).
    #[serde(default)]
    pub provider_id: Option<String>,
    /// When set, replaces the session model before this turn.
    #[serde(default)]
    pub model: Option<String>,
    /// When set, replaces the session runtime config for this provider selection.
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
    /// Plan-before-edit style directives.
    #[serde(default)]
    pub plan_mode: Option<bool>,
    /// `minimal` | `low` | `medium` | `high` | `xhigh` | `max`
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub fast_mode: Option<bool>,
}

/// Merge UI selection into session fields for per-turn model routing.
pub fn merge_send_turn_session_selection(
    session: &Session,
    input: &SendTurnInput,
) -> (String, Option<String>, Option<ProviderRuntimeConfig>) {
    let merged_provider = input
        .provider_id
        .as_ref()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| session.provider_id.clone());
    let merged_model = match input.model.as_ref() {
        None => session
            .model
            .as_deref()
            .map(|m| model_registry::resolve_alias(&merged_provider, m)),
        Some(m) => {
            let t = m.trim();
            if t.is_empty() {
                None
            } else {
                Some(model_registry::resolve_alias(&merged_provider, t))
            }
        }
    };
    let merged_provider_runtime = input
        .provider_runtime
        .clone()
        .or_else(|| session.provider_runtime.clone());
    (merged_provider, merged_model, merged_provider_runtime)
}

pub fn send_turn_selection_differs_from_session(session: &Session, input: &SendTurnInput) -> bool {
    let (p, m, r) = merge_send_turn_session_selection(session, input);
    p != session.provider_id || m != session.model || r != session.provider_runtime
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SendTurnOutput {
    pub session: Session,
    pub turn: Turn,
    pub projection: SessionProjection,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub async fn send_turn<S, E, B>(
    sessions: &S,
    session_events: &E,
    events: &B,
    input: SendTurnInput,
) -> Result<SendTurnOutput>
where
    S: SessionRepo + Sync,
    E: SessionEventRepo + Sync,
    B: EventBus + Sync,
{
    let mut session = sessions
        .get_session(&input.session_id)
        .await?
        .ok_or_else(|| crate::CoreError::Repository("session not found".to_string()))?;

    let (merged_provider, merged_model, merged_provider_runtime) =
        merge_send_turn_session_selection(&session, &input);
    if merged_provider != session.provider_id
        || merged_model != session.model
        || merged_provider_runtime != session.provider_runtime
    {
        session.provider_id = merged_provider;
        session.model = merged_model;
        session.provider_runtime = merged_provider_runtime;
        session.updated_at = now_iso();
        sessions.save_session(&session).await?;
    }

    if session.state != SessionState::Active {
        return Err(crate::CoreError::InvalidInput(
            "session must be active to send a turn".to_string(),
        ));
    }

    let history = session_events
        .list_events_by_session(&input.session_id)
        .await?;
    let projection = SessionProjection::fold(&history)
        .ok_or_else(|| crate::CoreError::Repository("session history is empty".to_string()))?;

    if projection.active_turn_id.is_some() {
        return Err(crate::CoreError::InvalidInput(
            "session already has an active turn".to_string(),
        ));
    }

    let now = now_iso();
    let turn_id = TurnId(Uuid::new_v4().to_string());
    let turn = Turn {
        id: turn_id.clone(),
        session_id: input.session_id.clone(),
        role: "user".to_string(),
        content: input.prompt.clone(),
        state: TurnState::Running,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let sequence = history.last().map(|event| event.sequence + 1).unwrap_or(1);
    let started = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        sequence,
        occurred_at: now.clone(),
        kind: SessionEventKind::TurnStarted {
            turn_id: turn_id.clone(),
            prompt: input.prompt.clone(),
            plan_mode: input.plan_mode,
        },
    };

    session_events.append_event(&started).await?;
    session.updated_at = now.clone();
    sessions.save_session(&session).await?;
    events
        .publish(CoreEvent::SessionTurnStarted {
            session_id: input.session_id.0.clone(),
            turn_id: turn_id.0.clone(),
            prompt: input.prompt,
            plan_mode: input.plan_mode,
        })
        .await?;

    let mut replay = history;
    replay.push(started);
    let projection = SessionProjection::fold(&replay).expect("session projection exists");

    Ok(SendTurnOutput {
        session,
        turn,
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
    struct FakeSessionEventRepo {
        events: Arc<Mutex<Vec<SessionEventRecord>>>,
    }

    #[async_trait]
    impl SessionEventRepo for FakeSessionEventRepo {
        async fn append_event(&self, event: &SessionEventRecord) -> Result<()> {
            self.events
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
                .events
                .lock()
                .expect("session events lock poisoned")
                .iter()
                .filter(|event| &event.session_id == session_id)
                .cloned()
                .collect();
            Ok(events)
        }

        async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
            self.events
                .lock()
                .expect("session events lock poisoned")
                .retain(|event| &event.session_id != session_id);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FakeEventBus {
        events: Arc<Mutex<Vec<CoreEvent>>>,
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

    #[test]
    fn send_turn_creates_running_turn_and_projection() {
        let sessions = FakeSessionRepo::default();
        let session_events = FakeSessionEventRepo::default();
        let events = FakeEventBus::default();
        let session_id = SessionId("session-1".to_string());

        sessions
            .sessions
            .lock()
            .expect("sessions lock poisoned")
            .push(Session {
                id: session_id.clone(),
                project_id: crate::domain::project::ProjectId("project-1".to_string()),
                workspace_id: crate::domain::workspace::WorkspaceId("workspace-1".to_string()),
                additional_workspace_ids: Vec::new(),
                provider_id: "codex".to_string(),
                model: Some("gpt-5-codex".to_string()),
                provider_runtime: None,
                working_directory_override: None,
                state: SessionState::Active,
                created_at: "2026-05-01T12:00:00Z".to_string(),
                updated_at: "2026-05-01T12:00:00Z".to_string(),
            });
        session_events
            .events
            .lock()
            .expect("session events lock poisoned")
            .push(SessionEventRecord {
                event_id: "evt-1".to_string(),
                session_id: session_id.clone(),
                sequence: 1,
                occurred_at: "2026-05-01T12:00:00Z".to_string(),
                kind: SessionEventKind::SessionStarted {
                    workspace_id: crate::domain::workspace::WorkspaceId("workspace-1".to_string()),
                    project_id: crate::domain::project::ProjectId("project-1".to_string()),
                    provider_id: "codex".to_string(),
                    model: Some("gpt-5-codex".to_string()),
                },
            });

        let output = futures::executor::block_on(send_turn(
            &sessions,
            &session_events,
            &events,
            SendTurnInput {
                session_id: session_id.clone(),
                prompt: "hello".to_string(),
                tool_instructions: None,
                provider_id: None,
                model: None,
                provider_runtime: None,
                plan_mode: None,
                effort: None,
                fast_mode: None,
            },
        ))
        .expect("send_turn should succeed");

        assert_eq!(output.session.id, session_id);
        assert_eq!(output.turn.state, TurnState::Running);
        assert_eq!(output.projection.turn_count, 0);

        let session_events = session_events
            .events
            .lock()
            .expect("session events lock poisoned");
        assert_eq!(session_events.len(), 2);
        assert!(matches!(
            session_events[1].kind,
            SessionEventKind::TurnStarted { .. }
        ));
    }

    #[test]
    fn send_turn_updates_provider_runtime_when_selection_changes() {
        let sessions = FakeSessionRepo::default();
        let session_events = FakeSessionEventRepo::default();
        let events = FakeEventBus::default();
        let session_id = SessionId("session-2".to_string());

        sessions
            .sessions
            .lock()
            .expect("sessions lock poisoned")
            .push(Session {
                id: session_id.clone(),
                project_id: crate::domain::project::ProjectId("project-1".to_string()),
                workspace_id: crate::domain::workspace::WorkspaceId("workspace-1".to_string()),
                additional_workspace_ids: Vec::new(),
                provider_id: "codex".to_string(),
                model: Some("gpt-5-codex".to_string()),
                provider_runtime: Some(ProviderRuntimeConfig {
                    home_path: Some("/tmp/codex-home".to_string()),
                    shadow_home_path: None,
                }),
                working_directory_override: None,
                state: SessionState::Active,
                created_at: "2026-05-01T12:00:00Z".to_string(),
                updated_at: "2026-05-01T12:00:00Z".to_string(),
            });
        session_events
            .events
            .lock()
            .expect("session events lock poisoned")
            .push(SessionEventRecord {
                event_id: "evt-1".to_string(),
                session_id: session_id.clone(),
                sequence: 1,
                occurred_at: "2026-05-01T12:00:00Z".to_string(),
                kind: SessionEventKind::SessionStarted {
                    workspace_id: crate::domain::workspace::WorkspaceId("workspace-1".to_string()),
                    project_id: crate::domain::project::ProjectId("project-1".to_string()),
                    provider_id: "codex".to_string(),
                    model: Some("gpt-5-codex".to_string()),
                },
            });

        let output = futures::executor::block_on(send_turn(
            &sessions,
            &session_events,
            &events,
            SendTurnInput {
                session_id,
                prompt: "hello".to_string(),
                tool_instructions: None,
                provider_id: Some("gemini".to_string()),
                model: Some("gemini-2.5-pro".to_string()),
                provider_runtime: Some(ProviderRuntimeConfig {
                    home_path: Some("/tmp/gemini-home".to_string()),
                    shadow_home_path: None,
                }),
                plan_mode: None,
                effort: None,
                fast_mode: None,
            },
        ))
        .expect("send_turn should succeed");

        assert_eq!(output.session.provider_id, "gemini");
        assert_eq!(output.session.model.as_deref(), Some("gemini-2.5-pro"));
        assert_eq!(
            output
                .session
                .provider_runtime
                .as_ref()
                .and_then(|runtime| runtime.home_path.as_deref()),
            Some("/tmp/gemini-home")
        );
    }
}
