use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use dcc_core::{
    domain::{
        delegation::{Delegation, DelegationId, DelegationStatus},
        project::{Project, ProjectId},
        provider::{ProviderEvent, SessionHandle},
        repository::{Repository, RepositoryId},
        session::{
            Session, SessionEventKind, SessionEventRecord, SessionId, SessionSearchResult, TurnId,
            WorkspaceSessionSummary,
        },
        thread::{Thread, ThreadId},
        workspace::{Workspace, WorkspaceId},
    },
    ports::{
        DelegationRepo, EventBus, Input, ProjectRepo, Provider, ProviderRuntimeConfig,
        RepositoryRepo, SessionConfig, SessionEventRepo, SessionRepo, ThreadRepo, WorkspaceRepo,
    },
    Result,
};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};

use crate::events::TauriEventBus;
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

#[derive(Clone)]
pub struct SessionCommandState {
    app_data_dir: PathBuf,
    db_path: PathBuf,
    session_repo: SqliteSessionRepo,
    event_bus: Arc<dyn EventBus>,
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
    provider_sessions: HashMap<SessionId, ProviderSessionBinding>,
}

#[derive(Clone, Copy, Debug, Default)]
struct NoopEventBus;

#[async_trait]
impl EventBus for NoopEventBus {
    async fn publish(&self, _event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        Ok(())
    }
}

impl SessionCommandState {
    pub fn new(app: AppHandle, db_path: PathBuf) -> Self {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        Self::from_parts(db_path, app_data_dir, Arc::new(TauriEventBus::new(app)))
    }

    pub fn new_headless(db_path: PathBuf, app_data_dir: PathBuf) -> Self {
        Self::from_parts(db_path, app_data_dir, Arc::new(NoopEventBus))
    }

    pub fn new_with_event_bus(
        db_path: PathBuf,
        app_data_dir: PathBuf,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self::from_parts(db_path, app_data_dir, event_bus)
    }

    fn from_parts(db_path: PathBuf, app_data_dir: PathBuf, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            app_data_dir,
            session_repo: SqliteSessionRepo::open(&db_path)
                .expect("failed to open sqlite session repo"),
            db_path,
            event_bus,
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

    fn provider_runtime_config(
        &self,
        provider_id: &str,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<ProviderRuntimeConfig> {
        let runtime = runtime.cloned().unwrap_or_default();
        if self.is_legacy_managed_provider_home(provider_id, &runtime) {
            return Ok(ProviderRuntimeConfig::default());
        }
        Ok(runtime)
    }

    fn provider_home_root(&self) -> PathBuf {
        self.app_data_dir.join("provider-homes")
    }

    fn is_legacy_managed_provider_home(
        &self,
        provider_id: &str,
        runtime: &ProviderRuntimeConfig,
    ) -> bool {
        if !matches!(provider_id, "claude_code" | "gemini" | "grok") {
            return false;
        }

        if runtime.shadow_home_path.is_some() {
            return false;
        }

        let Some(home_path) = runtime.home_path.as_deref() else {
            return false;
        };

        PathBuf::from(home_path) == self.provider_home_root().join(provider_id)
    }

    pub async fn peek_session(&self, session_id: &SessionId) -> Result<Option<Session>> {
        SessionRepo::get_session(&self.session_repo, session_id).await
    }

    pub fn list_workspace_sessions(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<WorkspaceSessionSummary>> {
        self.session_repo.list_workspace_sessions(workspace_id)
    }

    pub fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSearchResult>> {
        self.session_repo.search_sessions(query, limit)
    }

    async fn append_session_event(
        &self,
        session_id: &SessionId,
        kind: SessionEventKind,
    ) -> Result<SessionEventRecord> {
        let events =
            SessionEventRepo::list_events_by_session(&self.session_repo, session_id).await?;
        let sequence = events.last().map(|event| event.sequence + 1).unwrap_or(1);
        let record = SessionEventRecord {
            event_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            sequence,
            occurred_at: Utc::now().to_rfc3339(),
            kind,
        };
        SessionEventRepo::append_event(&self.session_repo, &record).await?;
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

    pub async fn emit_turn_aborted(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        reason: Option<String>,
    ) -> Result<()> {
        self.append_and_publish_session_event(
            session_id,
            SessionEventKind::TurnAborted {
                turn_id: turn_id.clone(),
                reason: reason.clone(),
            },
            dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                session_id: session_id.0.clone(),
                turn_id: turn_id.0.clone(),
                reason,
            },
        )
        .await
    }

    pub async fn attach_provider_session(&self, session: &Session) -> Result<()> {
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
        let working_directory = session
            .working_directory_override
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .or_else(|| {
                workspace
                    .worktree_path
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
            })
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
                    Ok(ProviderEvent::UserInputRequested { id, questions, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnUserInputRequested {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        questions: questions.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnUserInputRequested {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        questions,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::UserInputResolved { id, answers, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnUserInputResolved {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        answers: answers.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnUserInputResolved {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        answers,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::PermissionRequested { request, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnPermissionRequested {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: request.request_id.clone(),
                                        tool_name: request.tool_name.clone(),
                                        title: request.title.clone(),
                                        description: request.description.clone(),
                                        command: request.command.clone(),
                                        file: request.file.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnPermissionRequested {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: request.request_id,
                                        tool_name: request.tool_name,
                                        title: request.title,
                                        description: request.description,
                                        command: request.command,
                                        file: request.file,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::PermissionResolved { id, behavior, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnPermissionResolved {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        behavior: behavior.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnPermissionResolved {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        behavior,
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

    pub async fn set_active_turn(
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

    pub async fn send_provider_input(&self, session_id: &SessionId, input: Input) -> Result<()> {
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
        provider.send_input(&binding.handle, input).await
    }

    pub async fn cancel_provider_session(&self, session_id: &SessionId) -> Result<()> {
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
impl RepositoryRepo for SessionCommandState {
    async fn save_repository(&self, _repository: &Repository) -> Result<()> {
        Ok(())
    }

    async fn get_repository(&self, _id: &RepositoryId) -> Result<Option<Repository>> {
        Ok(None)
    }

    async fn list_repositories(&self) -> Result<Vec<Repository>> {
        Ok(Vec::new())
    }

    async fn delete_repository(&self, _id: &RepositoryId) -> Result<()> {
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
        SessionRepo::save_session(&self.session_repo, session).await
    }

    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
        SessionRepo::get_session(&self.session_repo, id).await
    }

    async fn delete_session(&self, id: &SessionId) -> Result<()> {
        let result = SessionRepo::delete_session(&self.session_repo, id).await;
        if result.is_ok() {
            let mut store = self.lock_store()?;
            store.provider_sessions.remove(id);
        }
        result
    }
}

#[async_trait]
impl ThreadRepo for SessionCommandState {
    async fn save_thread(&self, thread: &Thread) -> Result<()> {
        ThreadRepo::save_thread(&self.session_repo, thread).await
    }

    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        ThreadRepo::get_thread(&self.session_repo, id).await
    }

    async fn find_thread_by_session_id(&self, session_id: &SessionId) -> Result<Option<Thread>> {
        ThreadRepo::find_thread_by_session_id(&self.session_repo, session_id).await
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        ThreadRepo::delete_thread(&self.session_repo, id).await
    }
}

#[async_trait]
impl SessionEventRepo for SessionCommandState {
    async fn append_event(&self, event: &SessionEventRecord) -> Result<()> {
        SessionEventRepo::append_event(&self.session_repo, event).await
    }

    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        SessionEventRepo::list_events_by_session(&self.session_repo, session_id).await
    }

    async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
        SessionEventRepo::delete_events_by_session(&self.session_repo, session_id).await
    }
}

#[async_trait]
impl DelegationRepo for SessionCommandState {
    async fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        DelegationRepo::save_delegation(&self.session_repo, delegation).await
    }

    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        DelegationRepo::get_delegation(&self.session_repo, id).await
    }

    async fn list_delegations(
        &self,
        workspace_id: Option<&WorkspaceId>,
        parent_session_id: Option<&SessionId>,
    ) -> Result<Vec<Delegation>> {
        DelegationRepo::list_delegations(&self.session_repo, workspace_id, parent_session_id).await
    }

    async fn update_delegation_status(
        &self,
        id: &DelegationId,
        status: DelegationStatus,
        updated_at: String,
    ) -> Result<Option<Delegation>> {
        DelegationRepo::update_delegation_status(&self.session_repo, id, status, updated_at).await
    }
}

#[async_trait]
impl EventBus for SessionCommandState {
    async fn publish(&self, event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        self.event_bus.publish(event).await
    }
}
