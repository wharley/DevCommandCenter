use std::{
	collections::HashMap,
	path::PathBuf,
	sync::{Arc, Mutex},
};

use async_trait::async_trait;
use futures::StreamExt;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

use dcc_core::{
	domain::{
		project::{Project, ProjectId},
		provider::{ProviderEvent, SessionHandle},
		session::{Session, SessionEventRecord, SessionId},
		thread::{Thread, ThreadId},
		workspace::{Workspace, WorkspaceId},
	},
	ports::{
		EventBus, Input, ProjectRepo, Provider, SessionConfig, SessionEventRepo, SessionRepo,
		ThreadRepo, WorkspaceRepo,
	},
	Result,
};

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
	pub fn new(app: AppHandle) -> Self {
		Self {
			app,
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

	pub(crate) async fn attach_provider_session(&self, session: &Session) -> Result<()> {
		if self.provider_binding(&session.id)?.is_some() {
			return Ok(());
		}

		let provider = provider_runtime(&session.provider_id).ok_or_else(|| {
			dcc_core::CoreError::Provider(format!(
				"unknown provider runtime: {}",
				session.provider_id
			))
		})?;

		let handle = provider
			.prepare_session(SessionConfig {
				workspace_id: session.workspace_id.clone(),
				session_id: session.id.clone(),
				model: None,
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
								.publish(dcc_core::ports::events::CoreEvent::SessionTurnDelta {
									session_id: session_id.0.clone(),
									turn_id,
									content,
								})
								.await;
						}
					}
					Ok(ProviderEvent::Completed { .. }) => {
						let turn_id = binding.current_turn_id.lock().await.take();
						if let Some(turn_id) = turn_id {
							let _ = state
								.publish(dcc_core::ports::events::CoreEvent::SessionTurnCompleted {
									session_id: session_id.0.clone(),
									turn_id,
								})
								.await;
						}
					}
					Ok(ProviderEvent::Failed { message, .. }) => {
						let turn_id = binding.current_turn_id.lock().await.take();
						if let Some(turn_id) = turn_id {
							let _ = state
								.publish(dcc_core::ports::events::CoreEvent::SessionTurnAborted {
									session_id: session_id.0.clone(),
									turn_id,
									reason: Some(message),
								})
								.await;
						}
					}
					Err(error) => {
						let turn_id = binding.current_turn_id.lock().await.take();
						if let Some(turn_id) = turn_id {
							let _ = state
								.publish(dcc_core::ports::events::CoreEvent::SessionTurnAborted {
									session_id: session_id.0.clone(),
									turn_id,
									reason: Some(error.to_string()),
								})
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

	pub(crate) async fn set_active_turn(&self, session_id: &SessionId, turn_id: Option<String>) -> Result<()> {
		let binding = self.provider_binding(session_id)?.ok_or_else(|| {
			dcc_core::CoreError::Provider(format!(
				"no provider binding for session {}",
				session_id.0
			))
		})?;
		*binding.current_turn_id.lock().await = turn_id;
		Ok(())
	}

	pub(crate) async fn send_provider_input(&self, session_id: &SessionId, input: String) -> Result<()> {
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
