use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use dcc_core::{
    domain::{
        project::{Project, ProjectId},
        provider::{ProviderEvent, SessionHandle},
        session::{
            Session, SessionEventKind, SessionEventRecord, SessionId, SessionProjection, TurnId,
            WorkspaceSessionSummary,
        },
        thread::{Thread, ThreadId},
        workspace::{Workspace, WorkspaceId},
    },
    ports::{
        EventBus, Input, ProjectRepo, Provider, ProviderRuntimeConfig, SessionConfig,
        SessionEventRepo, SessionRepo, ThreadRepo, WorkspaceRepo,
    },
    Result,
};
use dcc_infra::db::SqliteWorkspaceRepo;

use crate::events::core_event_name;
use dcc_providers::provider_runtime;

#[derive(Clone, Debug)]
pub struct WorkspaceCommandState {
    pub db_path: PathBuf,
}

impl WorkspaceCommandState {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

#[derive(Clone, Debug)]
pub struct SessionCommandState {
    app: AppHandle,
    db_path: PathBuf,
    store: Arc<Mutex<SessionStore>>,
}

#[derive(Clone, Debug)]
struct ProviderSessionBinding {
    provider_id: String,
    handle: SessionHandle,
    current_turn_id: Arc<AsyncMutex<Option<String>>>,
}

#[derive(Default, Debug)]
struct SessionStore {
    sessions: HashMap<SessionId, Session>,
    threads: HashMap<ThreadId, Thread>,
    events: HashMap<SessionId, Vec<SessionEventRecord>>,
    provider_sessions: HashMap<SessionId, ProviderSessionBinding>,
}

impl SessionCommandState {
    pub fn new(app: AppHandle, db_path: PathBuf) -> Self {
        Self {
            app,
            db_path,
            store: Arc::new(Mutex::new(SessionStore::default())),
        }
    }

    fn lock_store(&self) -> Result<std::sync::MutexGuard<'_, SessionStore>> {
        self.store
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    fn provider_binding(&self, session_id: &SessionId) -> Result<Option<ProviderSessionBinding>> {
        let store = self.lock_store()?;
        Ok(store.provider_sessions.get(session_id).cloned())
    }

    fn provider_home_root(&self) -> PathBuf {
        self.app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("provider-homes")
    }

    fn default_provider_runtime_config(
        &self,
        provider_id: &str,
    ) -> Result<ProviderRuntimeConfig> {
        let home_path = self.provider_home_root().join(provider_id);
        std::fs::create_dir_all(&home_path)
            .map_err(|error| dcc_core::CoreError::Provider(error.to_string()))?;
        Ok(ProviderRuntimeConfig {
            home_path: Some(home_path.to_string_lossy().to_string()),
            shadow_home_path: None,
        })
    }

    fn provider_runtime_config(
        &self,
        provider_id: &str,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<ProviderRuntimeConfig> {
        if let Some(runtime) = runtime {
            return Ok(runtime.clone());
        }
        self.default_provider_runtime_config(provider_id)
    }

    pub(crate) fn peek_session(&self, session_id: &SessionId) -> Result<Option<Session>> {
        let store = self.lock_store()?;
        Ok(store.sessions.get(session_id).cloned())
    }

    pub(crate) fn list_workspace_sessions(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<WorkspaceSessionSummary>> {
        let store = self.lock_store()?;
        let mut summaries = store
            .sessions
            .values()
            .filter(|session| &session.workspace_id == workspace_id)
            .filter_map(|session| {
                let thread = store
                    .threads
                    .values()
                    .find(|thread| thread.session_id.as_ref() == Some(&session.id))?
                    .clone();
                let mut events = store.events.get(&session.id).cloned().unwrap_or_default();
                events.sort_by_key(|event| event.sequence);
                let mut last_turn_prompt = None;
                let mut last_turn_state = None;
                for event in &events {
                    match &event.kind {
                        SessionEventKind::TurnStarted { prompt, .. } => {
                            last_turn_prompt = Some(prompt.clone());
                            last_turn_state = Some("running".to_string());
                        }
                        SessionEventKind::TurnCompleted { .. } => {
                            last_turn_state = Some("completed".to_string());
                        }
                        SessionEventKind::TurnAborted { .. } => {
                            last_turn_state = Some("aborted".to_string());
                        }
                        SessionEventKind::SessionCompleted => {
                            if last_turn_state.is_none() {
                                last_turn_state = Some("completed".to_string());
                            }
                        }
                        SessionEventKind::SessionAborted { .. } => {
                            if last_turn_state.is_none() {
                                last_turn_state = Some("aborted".to_string());
                            }
                        }
                        _ => {}
                    }
                }
                let projection = SessionProjection::fold(&events).unwrap_or_else(|| {
                    SessionProjection::new(
                        session.id.clone(),
                        session.project_id.clone(),
                        session.workspace_id.clone(),
                        session.provider_id.clone(),
                        session.model.clone(),
                        session.created_at.clone(),
                    )
                });
                Some(WorkspaceSessionSummary {
                    session: session.clone(),
                    thread,
                    projection,
                    last_turn_prompt,
                    last_turn_state,
                })
            })
            .collect::<Vec<_>>();

        summaries.sort_by(|left, right| {
            right
                .session
                .created_at
                .cmp(&left.session.created_at)
                .then_with(|| right.thread.title.cmp(&left.thread.title))
        });

        Ok(summaries)
    }

    async fn append_session_event(
        &self,
        session_id: &SessionId,
        kind: SessionEventKind,
    ) -> Result<SessionEventRecord> {
        let mut store = self.lock_store()?;
        let events = store.events.entry(session_id.clone()).or_default();
        let sequence = events.last().map(|event| event.sequence + 1).unwrap_or(1);
        let record = SessionEventRecord {
            event_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            sequence,
            occurred_at: Utc::now().to_rfc3339(),
            kind,
        };
        events.push(record.clone());
        Ok(record)
    }

    async fn append_and_publish_session_event(
        &self,
        session_id: &SessionId,
        kind: SessionEventKind,
        core_event: dcc_core::ports::events::CoreEvent,
    ) -> Result<()> {
        self.append_session_event(session_id, kind).await?;
        self.publish(core_event).await?;
        Ok(())
    }

    pub(crate) async fn attach_provider_session(&self, session: &Session) -> Result<()> {
        if self.provider_binding(&session.id)?.is_some() {
            return Ok(());
        }

        let workspace_repo = SqliteWorkspaceRepo::open(&self.db_path)?;
        let workspace = workspace_repo
            .get_workspace(&session.workspace_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository(format!(
                    "workspace not found for session {}",
                    session.id.0
                ))
            })?;
        let working_directory = workspace
            .worktree_path
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| workspace.root_path.clone());

        let provider = provider_runtime(&session.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                session.provider_id
            ))
        })?;
        let provider_runtime =
            self.provider_runtime_config(&session.provider_id, session.provider_runtime.as_ref())?;

        let handle = provider
            .prepare_session(SessionConfig {
                workspace_id: session.workspace_id.clone(),
                session_id: session.id.clone(),
                model: session.model.clone(),
                working_directory: Some(working_directory),
                provider_runtime: Some(provider_runtime),
            })
            .await?;

        let binding = ProviderSessionBinding {
            provider_id: session.provider_id.clone(),
            handle: handle.clone(),
            current_turn_id: Arc::new(AsyncMutex::new(None)),
        };

        {
            let mut store = self.lock_store()?;
            store
                .provider_sessions
                .insert(session.id.clone(), binding.clone());
        }

        self.spawn_provider_bridge(session.id.clone(), binding, provider)
            .await;
        Ok(())
    }

    async fn spawn_provider_bridge(
        &self,
        session_id: SessionId,
        binding: ProviderSessionBinding,
        provider: Arc<dyn Provider>,
    ) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut events = provider.stream_events(&binding.handle);

            while let Some(event) = events.next().await {
                match event {
                    Ok(ProviderEvent::Started { .. }) => {}
                    Ok(ProviderEvent::TextDelta { content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningStarted { id, label, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnReasoningStarted {
										turn_id: TurnId(turn_id.clone()),
										reasoning_id: id.clone(),
										label: label.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnReasoningStarted {
										session_id: session_id.0.clone(),
										turn_id,
										reasoning_id: id,
										label,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningDelta { id, content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnReasoningDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        reasoning_id: id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnReasoningDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        reasoning_id: id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningCompleted { id, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnReasoningCompleted {
										turn_id: TurnId(turn_id.clone()),
										reasoning_id: id.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnReasoningCompleted {
										session_id: session_id.0.clone(),
										turn_id,
										reasoning_id: id,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallStarted {
                        id,
                        action,
                        command,
                        file,
                        ..
                    }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnToolCallStarted {
										turn_id: TurnId(turn_id.clone()),
										tool_call_id: id.clone(),
										action: action.clone(),
										command: command.clone(),
										file: file.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnToolCallStarted {
										session_id: session_id.0.clone(),
										turn_id,
										tool_call_id: id,
										action,
										command,
										file,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallDelta { id, content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnToolCallDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        tool_call_id: id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnToolCallDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        tool_call_id: id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallCompleted { id, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnToolCallCompleted {
										turn_id: TurnId(turn_id.clone()),
										tool_call_id: id.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnToolCallCompleted {
										session_id: session_id.0.clone(),
										turn_id,
										tool_call_id: id,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallFailed { id, reason, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnToolCallFailed {
                                        turn_id: TurnId(turn_id.clone()),
                                        tool_call_id: id.clone(),
                                        reason: reason.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnToolCallFailed {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        tool_call_id: id,
                                        reason,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::Completed { .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.take();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnCompleted {
                                        turn_id: TurnId(turn_id.clone()),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnCompleted {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::Failed { message, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.take();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAborted {
                                        turn_id: TurnId(turn_id.clone()),
                                        reason: Some(message.clone()),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        reason: Some(message),
                                    },
                                )
                                .await;
                        }
                    }
                    Err(error) => {
                        let turn_id = binding.current_turn_id.lock().await.take();
                        if let Some(turn_id) = turn_id {
                            let reason = error.to_string();
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAborted {
                                        turn_id: TurnId(turn_id.clone()),
                                        reason: Some(reason.clone()),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        reason: Some(reason),
                                    },
                                )
                                .await;
                        }
                    }
                }
            }

            if let Ok(mut store) = state.store.lock() {
                store.provider_sessions.remove(&session_id);
            }
        });
    }

    pub(crate) async fn set_active_turn(
        &self,
        session_id: &SessionId,
        turn_id: Option<String>,
    ) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        *binding.current_turn_id.lock().await = turn_id;
        Ok(())
    }

    pub(crate) async fn send_provider_input(
        &self,
        session_id: &SessionId,
        input: String,
    ) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        provider
            .send_input(&binding.handle, Input::Text(input))
            .await
    }

    pub(crate) async fn cancel_provider_session(&self, session_id: &SessionId) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        *binding.current_turn_id.lock().await = None;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        let result = provider.cancel(&binding.handle).await;
        if let Ok(mut store) = self.store.lock() {
            store.provider_sessions.remove(session_id);
        }
        result
    }
}

#[async_trait]
impl WorkspaceRepo for SessionCommandState {
    async fn save_workspace(&self, _workspace: &Workspace) -> Result<()> {
        Ok(())
    }

    async fn get_workspace(&self, _id: &WorkspaceId) -> Result<Option<Workspace>> {
        Ok(None)
    }

    async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        Ok(Vec::new())
    }

    async fn delete_workspace(&self, _id: &WorkspaceId) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProjectRepo for SessionCommandState {
    async fn save_project(&self, _project: &Project) -> Result<()> {
        Ok(())
    }

    async fn get_project(&self, _id: &ProjectId) -> Result<Option<Project>> {
        Ok(None)
    }
}

#[async_trait]
impl SessionRepo for SessionCommandState {
    async fn save_session(&self, session: &Session) -> Result<()> {
        let mut store = self.lock_store()?;
        store.sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
        let store = self.lock_store()?;
        Ok(store.sessions.get(id).cloned())
    }
}

#[async_trait]
impl ThreadRepo for SessionCommandState {
    async fn save_thread(&self, thread: &Thread) -> Result<()> {
        let mut store = self.lock_store()?;
        store.threads.insert(thread.id.clone(), thread.clone());
        Ok(())
    }

    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        let store = self.lock_store()?;
        Ok(store.threads.get(id).cloned())
    }
}

#[async_trait]
impl SessionEventRepo for SessionCommandState {
    async fn append_event(&self, event: &SessionEventRecord) -> Result<()> {
        let mut store = self.lock_store()?;
        store
            .events
            .entry(event.session_id.clone())
            .or_default()
            .push(event.clone());
        Ok(())
    }

    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        let store = self.lock_store()?;
        let mut events = store.events.get(session_id).cloned().unwrap_or_default();
        events.sort_by_key(|event| event.sequence);
        Ok(events)
    }
}

#[async_trait]
impl EventBus for SessionCommandState {
    async fn publish(&self, event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        let event_name = core_event_name(&event);
        self.app
            .emit(&event_name, &event)
            .map_err(|error| dcc_core::CoreError::EventBus(error.to_string()))?;
        self.app
            .emit("dcc:core-event", &event)
            .map_err(|error| dcc_core::CoreError::EventBus(error.to_string()))?;
        Ok(())
    }
}
