use async_trait::async_trait;

use crate::{
    domain::{
        delegation::{Delegation, DelegationId, DelegationStatus},
        project::{Project, ProjectId},
        repository::{Repository, RepositoryId},
        session::{Session, SessionEventRecord, SessionId},
        thread::{Thread, ThreadId},
        workspace::{Workspace, WorkspaceId},
    },
    Result,
};

#[async_trait]
pub trait WorkspaceRepo: Send + Sync {
    async fn save_workspace(&self, workspace: &Workspace) -> Result<()>;
    async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>>;
    async fn list_workspaces(&self) -> Result<Vec<Workspace>>;
    async fn delete_workspace(&self, id: &WorkspaceId) -> Result<()>;
}

#[async_trait]
pub trait RepositoryRepo: Send + Sync {
    async fn save_repository(&self, repository: &Repository) -> Result<()>;
    async fn get_repository(&self, id: &RepositoryId) -> Result<Option<Repository>>;
    async fn list_repositories(&self) -> Result<Vec<Repository>>;
    async fn delete_repository(&self, id: &RepositoryId) -> Result<()>;
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
    async fn delete_session(&self, id: &SessionId) -> Result<()>;
}

#[async_trait]
pub trait SessionEventRepo: Send + Sync {
    async fn append_event(&self, event: &SessionEventRecord) -> Result<()>;
    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>>;
    async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()>;
}

#[async_trait]
pub trait DelegationRepo: Send + Sync {
    async fn save_delegation(&self, delegation: &Delegation) -> Result<()>;
    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>>;
    async fn list_delegations(
        &self,
        workspace_id: Option<&WorkspaceId>,
        parent_session_id: Option<&SessionId>,
    ) -> Result<Vec<Delegation>>;
    async fn update_delegation_status(
        &self,
        id: &DelegationId,
        status: DelegationStatus,
        updated_at: String,
    ) -> Result<Option<Delegation>>;
}

#[async_trait]
pub trait ThreadRepo: Send + Sync {
    async fn save_thread(&self, thread: &Thread) -> Result<()>;
    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>>;
    async fn find_thread_by_session_id(&self, session_id: &SessionId) -> Result<Option<Thread>>;
    async fn delete_thread(&self, id: &ThreadId) -> Result<()>;
}
