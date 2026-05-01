use std::{
	collections::HashMap,
	path::PathBuf,
	sync::{Arc, Mutex},
};

use async_trait::async_trait;
use tauri::{AppHandle, Emitter};

use dcc_core::{
	domain::{
		project::{Project, ProjectId},
		session::{Session, SessionEventRecord, SessionId},
		thread::{Thread, ThreadId},
		workspace::{Workspace, WorkspaceId},
	},
	ports::{EventBus, ProjectRepo, SessionEventRepo, SessionRepo, ThreadRepo, WorkspaceRepo},
	Result,
};

use crate::events::core_event_name;

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

#[derive(Default, Debug)]
struct SessionStore {
	sessions: HashMap<SessionId, Session>,
	threads: HashMap<ThreadId, Thread>,
	events: HashMap<SessionId, Vec<SessionEventRecord>>,
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
