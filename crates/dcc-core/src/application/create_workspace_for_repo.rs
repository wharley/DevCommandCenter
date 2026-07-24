use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
    domain::{
        project::ProjectId,
        repository::{Repository, RepositoryId},
        workspace::{Workspace, WorkspaceId, WorkspaceState},
    },
    ports::{CoreEvent, EventBus, GitOps, PreparedWorktree, RepositoryRepo, WorkspaceRepo},
    CoreError, Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceForRepoInput {
    pub project_id: ProjectId,
    pub workspace_root: String,
    pub base_branch: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreparedWorkspace {
    pub workspace: Workspace,
    pub worktree_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedWorkspace {
    pub workspace: Workspace,
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn repository_name_from_root_path(root_path: &str) -> String {
    let normalized = root_path
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    normalized
        .rsplit('/')
        .find(|segment| !segment.trim().is_empty())
        .map(|segment| segment.trim().to_string())
        .unwrap_or_else(|| "Repository".to_string())
}

fn repository_from_workspace(workspace: &Workspace) -> Repository {
    let root_path = workspace.root_path.trim().to_string();
    Repository {
        id: RepositoryId(root_path.clone()),
        project_id: workspace.project_id.clone(),
        name: repository_name_from_root_path(&root_path),
        root_path,
        base_branch: workspace.base_branch.clone(),
        remote: None,
        remote_url: None,
        forge_provider: None,
        forge_login: None,
        created_at: workspace.created_at.clone(),
        updated_at: workspace.updated_at.clone(),
    }
}

pub async fn prepare_workspace_for_repo<G, B>(
    git: &G,
    events: &B,
    input: CreateWorkspaceForRepoInput,
) -> Result<PreparedWorkspace>
where
    G: GitOps + Sync,
    B: EventBus + Sync,
{
    if input.workspace_root.trim().is_empty() {
        return Err(CoreError::InvalidInput(
            "workspace_root cannot be empty".to_string(),
        ));
    }

    if input.base_branch.trim().is_empty() {
        return Err(CoreError::InvalidInput(
            "base_branch cannot be empty".to_string(),
        ));
    }

    let PreparedWorktree {
        path: worktree_path,
        branch,
        created_at,
    } = git
        .prepare_worktree(&input.workspace_root, &input.base_branch)
        .await?;
    let now = now_iso();
    let workspace_id = WorkspaceId(Uuid::new_v4().to_string());

    let workspace = Workspace {
        id: workspace_id,
        project_id: input.project_id,
        name: input.name.clone(),
        root_path: input.workspace_root,
        base_branch: branch,
        worktree_path: Some(worktree_path.clone()),
        source: None,
        state: WorkspaceState::SetupPending,
        setup_report: None,
        created_at: created_at,
        updated_at: now,
    };

    events
        .publish(CoreEvent::WorkspacePrepared {
            workspace_id: workspace.id.0.clone(),
            project_id: workspace.project_id.0.clone(),
            worktree_path: worktree_path.clone(),
        })
        .await?;

    Ok(PreparedWorkspace {
        workspace,
        worktree_path,
    })
}

pub async fn finalize_workspace_for_repo<R, B>(
    repo: &R,
    events: &B,
    prepared: PreparedWorkspace,
) -> Result<FinalizedWorkspace>
where
    R: WorkspaceRepo + Sync,
    B: EventBus + Sync,
{
    let mut workspace = prepared.workspace;
    workspace.state = WorkspaceState::Ready;
    workspace.updated_at = now_iso();

    repo.save_workspace(&workspace).await?;
    events
        .publish(CoreEvent::WorkspaceReady {
            workspace_id: workspace.id.0.clone(),
            project_id: workspace.project_id.0.clone(),
            worktree_path: prepared.worktree_path,
        })
        .await?;

    Ok(FinalizedWorkspace { workspace })
}

pub async fn create_workspace_for_repo<R, G, B>(
    repo: &R,
    git: &G,
    events: &B,
    input: CreateWorkspaceForRepoInput,
) -> Result<FinalizedWorkspace>
where
    R: WorkspaceRepo + RepositoryRepo + Sync,
    G: GitOps + Sync,
    B: EventBus + Sync,
{
    let prepared = prepare_workspace_for_repo(git, events, input).await?;
    let finalized = finalize_workspace_for_repo(repo, events, prepared).await?;
    repo.save_repository(&repository_from_workspace(&finalized.workspace))
        .await?;
    Ok(finalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    use crate::ports::ClonedRepository;

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
                path: "/tmp/dcc-clone".to_string(),
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
                created_at: "2026-05-01T12:00:00Z".to_string(),
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
    fn create_workspace_for_repo_runs_prepare_finalize_and_emits_events() {
        let repo = FakeWorkspaceRepo::default();
        let git = FakeGitOps {
            worktree_path: "/tmp/dcc-worktrees/main-123".to_string(),
        };
        let events = FakeEventBus::default();

        let input = CreateWorkspaceForRepoInput {
            project_id: ProjectId("project-1".to_string()),
            workspace_root: "/tmp/repo".to_string(),
            base_branch: "main".to_string(),
            name: Some("Feature shell".to_string()),
        };

        let finalized =
            futures::executor::block_on(create_workspace_for_repo(&repo, &git, &events, input))
                .expect("workspace creation should succeed");

        assert_eq!(finalized.workspace.project_id.0, "project-1");
        assert_eq!(finalized.workspace.name.as_deref(), Some("Feature shell"));
        assert_eq!(finalized.workspace.root_path, "/tmp/repo");
        assert_eq!(finalized.workspace.base_branch, "main");
        assert_eq!(
            finalized.workspace.worktree_path.as_deref(),
            Some("/tmp/dcc-worktrees/main-123")
        );
        assert_eq!(finalized.workspace.state, WorkspaceState::Ready);

        let saved = repo.saved.lock().expect("saved workspaces lock poisoned");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, finalized.workspace.id);
        assert_eq!(saved[0].state, WorkspaceState::Ready);

        let repositories = repo
            .repositories
            .lock()
            .expect("saved repositories lock poisoned");
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].root_path, "/tmp/repo");
        assert_eq!(repositories[0].base_branch, "main");

        let recorded_events = events.events.lock().expect("events lock poisoned");
        assert_eq!(recorded_events.len(), 2);
        assert!(matches!(
            recorded_events[0],
            CoreEvent::WorkspacePrepared { .. }
        ));
        assert!(matches!(
            recorded_events[1],
            CoreEvent::WorkspaceReady { .. }
        ));
    }
}
