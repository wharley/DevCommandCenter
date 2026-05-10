use tauri::{AppHandle, State};

use dcc_core::application::{CreateWorkspaceForRepoInput, CreateWorkspaceFromUrlInput};
use dcc_tauri::{
    commands::workspace_commands::{
        CreateWorkspaceForRepoOutput, CreateWorkspaceFromUrlOutput, GithubCliStatusInput,
        GithubCliStatusOutput, ForgeCliStatusInput, ForgeCliStatusOutput,
        ListChildDirectoriesInput, ListChildDirectoriesOutput, ListGitTrackedFilesInput,
        ListGitTrackedFilesOutput, ListLocalBranchesInput, ListLocalBranchesOutput,
        ListRepositoriesOutput, ListWorkspacesOutput, RepositoryIdInput,
        WorkspaceContinueFromBaseBranchInput, WorkspaceContinueFromBaseBranchOutput,
        WorkspaceGitBranchDiffInput, WorkspaceGitBranchDiffOutput, WorkspaceGitCommitPushInput,
        WorkspaceGitFilePreviewContentOutput, WorkspaceGitFilePreviewInput, WorkspaceGitPathInput,
        WorkspaceGitPushInput, WorkspaceGitStatusInput, WorkspaceGitStatusOutput, WorkspaceIdInput,
        WorkspacePrStatusInput, WorkspacePrStatusOutput, WorkspaceRunSetupInput,
        WorkspaceRunSetupOutput,
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
pub async fn workspace_run_setup(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceRunSetupInput,
) -> Result<WorkspaceRunSetupOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_run_setup(state, input).await
}

#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListWorkspacesOutput, String> {
    dcc_tauri::commands::workspace_commands::list_workspaces(state).await
}

#[tauri::command]
pub async fn list_repositories(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListRepositoriesOutput, String> {
    dcc_tauri::commands::workspace_commands::list_repositories(state).await
}

#[tauri::command]
pub async fn workspace_github_cli_status(
    input: GithubCliStatusInput,
) -> Result<GithubCliStatusOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_github_cli_status(input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_status(
    input: ForgeCliStatusInput,
) -> Result<ForgeCliStatusOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_forge_cli_status(input).await
}

#[tauri::command]
pub async fn list_local_branches(
    state: State<'_, WorkspaceCommandState>,
    input: ListLocalBranchesInput,
) -> Result<ListLocalBranchesOutput, String> {
    dcc_tauri::commands::workspace_commands::list_local_branches(state, input).await
}

#[tauri::command]
pub async fn list_git_tracked_files(
    state: State<'_, WorkspaceCommandState>,
    input: ListGitTrackedFilesInput,
) -> Result<ListGitTrackedFilesOutput, String> {
    dcc_tauri::commands::workspace_commands::list_git_tracked_files(state, input).await
}

#[tauri::command]
pub async fn list_child_directories(
    input: ListChildDirectoriesInput,
) -> Result<ListChildDirectoriesOutput, String> {
    dcc_tauri::commands::workspace_commands::list_child_directories(input).await
}

#[tauri::command]
pub async fn workspace_git_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitStatusInput,
) -> Result<WorkspaceGitStatusOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_git_status(state, input).await
}

#[tauri::command]
pub async fn workspace_git_stage_file(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_stage_file(state, input).await
}

#[tauri::command]
pub async fn workspace_git_stage_all(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_stage_all(state, input).await
}

#[tauri::command]
pub async fn workspace_git_unstage_file(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_unstage_file(state, input).await
}

#[tauri::command]
pub async fn workspace_git_discard_file(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPathInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_discard_file(state, input).await
}

#[tauri::command]
pub async fn workspace_git_commit_push(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitCommitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_commit_push(state, input).await
}

#[tauri::command]
pub async fn workspace_git_push(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_push(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_change_request_view_web(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_create(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_change_request_create(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_change_request_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_gh_pr_view_web(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_create_fill(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_gh_pr_create_fill(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_gh_pr_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_git_branch_diff(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitBranchDiffInput,
) -> Result<WorkspaceGitBranchDiffOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_git_branch_diff(state, input).await
}

#[tauri::command]
pub async fn workspace_git_file_preview(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitFilePreviewInput,
) -> Result<String, String> {
    dcc_tauri::commands::workspace_commands::workspace_git_file_preview(state, input).await
}

#[tauri::command]
pub async fn workspace_git_file_preview_content(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitFilePreviewInput,
) -> Result<WorkspaceGitFilePreviewContentOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_git_file_preview_content(state, input).await
}

#[tauri::command]
pub async fn workspace_pr_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrStatusInput,
) -> Result<WorkspacePrStatusOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_pr_status(state, input).await
}

#[tauri::command]
pub async fn workspace_continue_from_base_branch(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceContinueFromBaseBranchInput,
) -> Result<WorkspaceContinueFromBaseBranchOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_continue_from_base_branch(state, input).await
}

#[tauri::command]
pub async fn archive_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceIdInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::archive_workspace(state, input).await
}

#[tauri::command]
pub async fn restore_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceIdInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::restore_workspace(state, input).await
}

#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceIdInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::delete_workspace(state, input).await
}

#[tauri::command]
pub async fn delete_repository(
    state: State<'_, WorkspaceCommandState>,
    input: RepositoryIdInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::delete_repository(state, input).await
}
