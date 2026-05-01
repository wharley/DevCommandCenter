use tauri::{AppHandle, State};

use dcc_core::application::{CreateWorkspaceForRepoInput, CreateWorkspaceFromUrlInput};
use dcc_tauri::{
	commands::workspace_commands::{
		CreateWorkspaceForRepoOutput,
		CreateWorkspaceFromUrlOutput,
		ListChildDirectoriesInput,
		ListChildDirectoriesOutput,
		ListGitTrackedFilesInput,
		ListGitTrackedFilesOutput,
		ListLocalBranchesInput,
		ListLocalBranchesOutput,
		ListWorkspacesOutput,
	},
	state::WorkspaceCommandState,
};

#[tauri::command]
pub async fn create_workspace_for_repo(
	state: State<'_, WorkspaceCommandState>,
	app: AppHandle,
	input: CreateWorkspaceForRepoInput,
) -> Result<CreateWorkspaceForRepoOutput, String> {
	dcc_tauri::commands::workspace_commands::create_workspace_for_repo(state, app, input).await
}

#[tauri::command]
pub async fn create_workspace_from_url(
	state: State<'_, WorkspaceCommandState>,
	app: AppHandle,
	input: CreateWorkspaceFromUrlInput,
) -> Result<CreateWorkspaceFromUrlOutput, String> {
	dcc_tauri::commands::workspace_commands::create_workspace_from_url(state, app, input).await
}

#[tauri::command]
pub async fn list_workspaces(
	state: State<'_, WorkspaceCommandState>,
) -> Result<ListWorkspacesOutput, String> {
	dcc_tauri::commands::workspace_commands::list_workspaces(state).await
}

#[tauri::command]
pub async fn list_local_branches(
	input: ListLocalBranchesInput,
) -> Result<ListLocalBranchesOutput, String> {
	dcc_tauri::commands::workspace_commands::list_local_branches(input).await
}

#[tauri::command]
pub async fn list_git_tracked_files(
	input: ListGitTrackedFilesInput,
) -> Result<ListGitTrackedFilesOutput, String> {
	dcc_tauri::commands::workspace_commands::list_git_tracked_files(input).await
}

#[tauri::command]
pub async fn list_child_directories(
	input: ListChildDirectoriesInput,
) -> Result<ListChildDirectoriesOutput, String> {
	dcc_tauri::commands::workspace_commands::list_child_directories(input).await
}
