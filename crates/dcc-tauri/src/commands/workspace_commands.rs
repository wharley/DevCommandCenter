use tauri::{AppHandle, State};
use serde::{Deserialize, Serialize};
use specta::Type;

use dcc_core::{
	application::{
		create_workspace_for_repo as run_create_workspace_for_repo,
		create_workspace_from_url as run_create_workspace_from_url,
		CreateWorkspaceForRepoInput,
		CreateWorkspaceFromUrlInput,
	},
	domain::workspace::Workspace,
	ports::WorkspaceRepo,
};
use dcc_infra::{db::SqliteWorkspaceRepo, git::{list_local_branch_names, CommandGitOps}};

use crate::{
	events::TauriEventBus,
	state::WorkspaceCommandState,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceForRepoOutput {
	pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceFromUrlOutput {
	pub workspace: Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspacesOutput {
	pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalBranchesInput {
	pub workspace_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListLocalBranchesOutput {
	pub branches: Vec<String>,
}

#[tauri::command]
pub async fn create_workspace_for_repo(
	state: State<'_, WorkspaceCommandState>,
	app: AppHandle,
	input: CreateWorkspaceForRepoInput,
) -> Result<CreateWorkspaceForRepoOutput, String> {
	let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
	let git = CommandGitOps::new();
	let events = TauriEventBus::new(app);

	let finalized = run_create_workspace_for_repo(&repo, &git, &events, input)
		.await
		.map_err(|error| error.to_string())?;

	Ok(CreateWorkspaceForRepoOutput {
		workspace: finalized.workspace,
	})
}

#[tauri::command]
pub async fn create_workspace_from_url(
	state: State<'_, WorkspaceCommandState>,
	app: AppHandle,
	input: CreateWorkspaceFromUrlInput,
) -> Result<CreateWorkspaceFromUrlOutput, String> {
	let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
	let git = CommandGitOps::new();
	let events = TauriEventBus::new(app);

	let finalized = run_create_workspace_from_url(&repo, &git, &events, input)
		.await
		.map_err(|error| error.to_string())?;

	Ok(CreateWorkspaceFromUrlOutput {
		workspace: finalized.workspace,
	})
}

#[tauri::command]
pub async fn list_workspaces(state: State<'_, WorkspaceCommandState>) -> Result<ListWorkspacesOutput, String> {
	let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
	let workspaces = repo.list_workspaces().await.map_err(|error| error.to_string())?;

	Ok(ListWorkspacesOutput { workspaces })
}

#[tauri::command]
pub async fn list_local_branches(
	input: ListLocalBranchesInput,
) -> Result<ListLocalBranchesOutput, String> {
	let branches = list_local_branch_names(&input.workspace_root).map_err(|error| error.to_string())?;
	Ok(ListLocalBranchesOutput { branches })
}
