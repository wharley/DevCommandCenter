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
		WorkspaceGitBranchDiffInput,
		WorkspaceGitBranchDiffOutput,
		WorkspaceGitCommitPushInput,
		WorkspaceGitPathInput,
		WorkspaceGitPushInput,
		WorkspaceGitStatusInput,
		WorkspaceGitStatusOutput,
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

#[tauri::command]
pub async fn workspace_git_status(
	input: WorkspaceGitStatusInput,
) -> Result<WorkspaceGitStatusOutput, String> {
	dcc_tauri::commands::workspace_commands::workspace_git_status(input).await
}

#[tauri::command]
pub async fn workspace_git_stage_file(input: WorkspaceGitPathInput) -> Result<(), String> {
	dcc_tauri::commands::workspace_commands::workspace_git_stage_file(input).await
}

#[tauri::command]
pub async fn workspace_git_unstage_file(input: WorkspaceGitPathInput) -> Result<(), String> {
	dcc_tauri::commands::workspace_commands::workspace_git_unstage_file(input).await
}

#[tauri::command]
pub async fn workspace_git_discard_file(input: WorkspaceGitPathInput) -> Result<(), String> {
	dcc_tauri::commands::workspace_commands::workspace_git_discard_file(input).await
}

#[tauri::command]
pub async fn workspace_git_commit_push(
	input: WorkspaceGitCommitPushInput,
) -> Result<(), String> {
	dcc_tauri::commands::workspace_commands::workspace_git_commit_push(input).await
}

#[tauri::command]
pub async fn workspace_git_push(input: WorkspaceGitPushInput) -> Result<(), String> {
	dcc_tauri::commands::workspace_commands::workspace_git_push(input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_view_web(input: WorkspaceGitPushInput) -> Result<(), String> {
	dcc_tauri::commands::workspace_commands::workspace_gh_pr_view_web(input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_create_fill(input: WorkspaceGitPushInput) -> Result<(), String> {
	dcc_tauri::commands::workspace_commands::workspace_gh_pr_create_fill(input).await
}

#[tauri::command]
pub async fn workspace_git_branch_diff(
	input: WorkspaceGitBranchDiffInput,
) -> Result<WorkspaceGitBranchDiffOutput, String> {
	dcc_tauri::commands::workspace_commands::workspace_git_branch_diff(input).await
}
