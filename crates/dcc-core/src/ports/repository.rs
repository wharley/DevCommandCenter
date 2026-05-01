use async_trait::async_trait;

use crate::{
	domain::{
		project::{Project, ProjectId},
		session::{Session, SessionId},
		thread::{Thread, ThreadId},
		workspace::{Workspace, WorkspaceId},
	},
	Result,
};

#[async_trait]
pub trait WorkspaceRepo: Send + Sync {
	async fn save_workspace(&self, workspace: &Workspace) -> Result<()>;
	async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>>;
}

#[async_trait]
pub trait ProjectRepo: Send + Sync {
	async fn save_project(&self, project: &Project) -> Result<()>;
	async fn get_project(&self, id: &ProjectId) -> Result<Option<Project>>;
}

#[async_trait]
pub trait SessionRepo: Send + Sync {
	async fn save_session(&self, session: &Session) -> Result<()>;
	async fn get_session(&self, id: &SessionId) -> Result<Option<Session>>;
}

#[async_trait]
pub trait ThreadRepo: Send + Sync {
	async fn save_thread(&self, thread: &Thread) -> Result<()>;
	async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>>;
}
