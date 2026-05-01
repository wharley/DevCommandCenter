use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{
	domain::{
		project::ProjectId,
		workspace::{Workspace, WorkspaceId, WorkspaceState},
	},
	ports::{CoreEvent, EventBus, GitOps, PreparedWorktree, WorkspaceRepo},
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
		state: WorkspaceState::SetupPending,
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
	R: WorkspaceRepo + Sync,
	G: GitOps + Sync,
	B: EventBus + Sync,
{
	let prepared = prepare_workspace_for_repo(git, events, input).await?;
	finalize_workspace_for_repo(repo, events, prepared).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use async_trait::async_trait;
	use std::sync::{Arc, Mutex};

	#[derive(Clone, Default)]
	struct FakeWorkspaceRepo {
		saved: Arc<Mutex<Vec<Workspace>>>,
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
	}

	#[derive(Clone)]
	struct FakeGitOps {
		worktree_path: String,
	}

	#[async_trait]
	impl GitOps for FakeGitOps {
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

		let finalized = futures::executor::block_on(create_workspace_for_repo(
			&repo,
			&git,
			&events,
			input,
		))
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

		let recorded_events = events.events.lock().expect("events lock poisoned");
		assert_eq!(recorded_events.len(), 2);
		assert!(matches!(
			recorded_events[0],
			CoreEvent::WorkspacePrepared { .. }
		));
		assert!(matches!(recorded_events[1], CoreEvent::WorkspaceReady { .. }));
	}
}
