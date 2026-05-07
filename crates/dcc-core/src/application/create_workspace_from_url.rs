use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    ports::{EventBus, GitOps, RepositoryRepo, WorkspaceRepo},
    CoreError, Result,
};

use super::create_workspace_for_repo::{
    create_workspace_for_repo as create_workspace_for_repo_impl, CreateWorkspaceForRepoInput,
    FinalizedWorkspace,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceFromUrlInput {
    pub project_id: crate::domain::project::ProjectId,
    pub repository_url: String,
    pub workspace_root: String,
    pub base_branch: String,
    pub name: Option<String>,
}

pub async fn create_workspace_from_url<R, G, B>(
    repo: &R,
    git: &G,
    events: &B,
    input: CreateWorkspaceFromUrlInput,
) -> Result<FinalizedWorkspace>
where
    R: WorkspaceRepo + RepositoryRepo + Sync,
    G: GitOps + Sync,
    B: EventBus + Sync,
{
    if input.repository_url.trim().is_empty() {
        return Err(CoreError::InvalidInput(
            "repository_url cannot be empty".to_string(),
        ));
    }

    let cloned = git
        .clone_repository(
            &input.repository_url,
            &input.workspace_root,
            &input.base_branch,
        )
        .await?;

    let workspace_input = CreateWorkspaceForRepoInput {
        project_id: input.project_id,
        workspace_root: cloned.path,
        base_branch: cloned.branch,
        name: input.name,
    };

    create_workspace_for_repo_impl(repo, git, events, workspace_input).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    use crate::{
        domain::{
            project::ProjectId,
            repository::{Repository, RepositoryId},
            workspace::{Workspace, WorkspaceId},
        },
        ports::CoreEvent,
        ports::{
            ClonedRepository, EventBus, GitOps, PreparedWorktree, RepositoryRepo, WorkspaceRepo,
        },
    };

    #[derive(Clone, Default)]
    struct FakeWorkspaceRepo {
        saved: Arc<Mutex<Vec<Workspace>>>,
        repositories: Arc<Mutex<Vec<Repository>>>,
    }

    #[async_trait]
    impl WorkspaceRepo for FakeWorkspaceRepo {
        async fn save_workspace(&self, workspace: &Workspace) -> Result<()> {
            self.saved
                .lock()
                .expect("saved workspaces lock poisoned")
                .push(workspace.clone());
            Ok(())
        }

        async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
            let found = self
                .saved
                .lock()
                .expect("saved workspaces lock poisoned")
                .iter()
                .find(|workspace| &workspace.id == id)
                .cloned();
            Ok(found)
        }

        async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
            Ok(self
                .saved
                .lock()
                .expect("saved workspaces lock poisoned")
                .clone())
        }

        async fn delete_workspace(&self, id: &WorkspaceId) -> Result<()> {
            self.saved
                .lock()
                .expect("saved workspaces lock poisoned")
                .retain(|workspace| &workspace.id != id);
            Ok(())
        }
    }

    #[async_trait]
    impl RepositoryRepo for FakeWorkspaceRepo {
        async fn save_repository(&self, repository: &Repository) -> Result<()> {
            let mut repositories = self
                .repositories
                .lock()
                .expect("saved repositories lock poisoned");
            repositories.retain(|current| current.id != repository.id);
            repositories.push(repository.clone());
            Ok(())
        }

        async fn get_repository(&self, id: &RepositoryId) -> Result<Option<Repository>> {
            let found = self
                .repositories
                .lock()
                .expect("saved repositories lock poisoned")
                .iter()
                .find(|repository| &repository.id == id)
                .cloned();
            Ok(found)
        }

        async fn list_repositories(&self) -> Result<Vec<Repository>> {
            Ok(self
                .repositories
                .lock()
                .expect("saved repositories lock poisoned")
                .clone())
        }

        async fn delete_repository(&self, id: &RepositoryId) -> Result<()> {
            self.repositories
                .lock()
                .expect("saved repositories lock poisoned")
                .retain(|repository| &repository.id != id);
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeGitOps {
        cloned_path: String,
        worktree_path: String,
    }

    #[async_trait]
    impl GitOps for FakeGitOps {
        async fn clone_repository(
            &self,
            _repository_url: &str,
            _destination_path: &str,
            base_branch: &str,
        ) -> Result<ClonedRepository> {
            Ok(ClonedRepository {
                path: self.cloned_path.clone(),
                branch: base_branch.to_string(),
                created_at: "2026-05-01T12:00:00Z".to_string(),
            })
        }

        async fn prepare_worktree(
            &self,
            _workspace_root: &str,
            base_branch: &str,
        ) -> Result<PreparedWorktree> {
            Ok(PreparedWorktree {
                path: self.worktree_path.clone(),
                branch: base_branch.to_string(),
                created_at: "2026-05-01T12:00:01Z".to_string(),
            })
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
    fn create_workspace_from_url_clones_then_prepares_then_finalizes() {
        let repo = FakeWorkspaceRepo::default();
        let git = FakeGitOps {
            cloned_path: "/tmp/dcc-projects/demo".to_string(),
            worktree_path: "/tmp/dcc-worktrees/main-123".to_string(),
        };
        let events = FakeEventBus::default();

        let result = futures::executor::block_on(create_workspace_from_url(
            &repo,
            &git,
            &events,
            CreateWorkspaceFromUrlInput {
                project_id: ProjectId("project-1".to_string()),
                repository_url: "https://github.com/acme/demo.git".to_string(),
                workspace_root: "/tmp/dcc-projects/demo".to_string(),
                base_branch: "main".to_string(),
                name: Some("Demo".to_string()),
            },
        ))
        .expect("create workspace from url");

        assert_eq!(result.workspace.project_id.0, "project-1");
        assert_eq!(result.workspace.root_path, "/tmp/dcc-projects/demo");
        assert_eq!(
            result.workspace.worktree_path.as_deref(),
            Some("/tmp/dcc-worktrees/main-123")
        );
        assert_eq!(result.workspace.base_branch, "main");
        let repositories = repo
            .repositories
            .lock()
            .expect("saved repositories lock poisoned");
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].root_path, "/tmp/dcc-projects/demo");
    }
}
