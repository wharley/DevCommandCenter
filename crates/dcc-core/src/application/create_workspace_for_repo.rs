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
pub struct CreateWorkspaceForRepoInput {
	pub project_id: ProjectId,
	pub workspace_root: String,
	pub base_branch: String,
	pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PreparedWorkspace {
	pub workspace: Workspace,
	pub worktree_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct FinalizedWorkspace {
	pub workspace: Workspace,
}

fn now_iso() -> String {
	Utc::now().to_rfc3339()
}

pub async fn prepare_workspace_for_repo<G>(
	git: &G,
	input: CreateWorkspaceForRepoInput,
) -> Result<PreparedWorkspace>
where
	G: GitOps + Sync,
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
		root_path: input.workspace_root,
		base_branch: branch,
		worktree_path: Some(worktree_path.clone()),
		state: WorkspaceState::SetupPending,
		created_at: created_at,
		updated_at: now,
	};

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
	let prepared = prepare_workspace_for_repo(git, input).await?;
	finalize_workspace_for_repo(repo, events, prepared).await
}
