use tauri::{AppHandle, State};

use dcc_core::application::{CreateWorkspaceForRepoInput, CreateWorkspaceFromUrlInput};
use dcc_tauri::{
    commands::workspace_commands::{
        CompileMissionSpecContextInput, CompileMissionSpecContextOutput,
        CreateWorkspaceForRepoOutput, CreateWorkspaceFromUrlOutput, ListChildDirectoriesInput,
        ListChildDirectoriesOutput, ListGitTrackedFilesInput, ListGitTrackedFilesOutput,
        ReadWorkspaceFileInput, ReadWorkspaceFileOutput, WriteWorkspaceFileInput,
        WriteWorkspaceFileOutput, SearchWorkspaceInput, SearchWorkspaceOutput,
        ListLocalBranchesInput, ListLocalBranchesOutput, ListMissionSpecsInput,
        ListMissionSpecsOutput, ListRepositoriesOutput, ListWorkspacesOutput,
        MissionSpecContextStatusInput, MissionSpecContextStatusOutput, RepositoryIdInput,
        SaveMissionValidationInput, SaveMissionValidationOutput,
        WorkspaceContinueFromBaseBranchInput, WorkspaceContinueFromBaseBranchOutput,
        WorkspaceApplyDelegationWorktreeInput, WorkspaceApplyDelegationWorktreeOutput,
        WorkspaceGitAcceptConflictInput, WorkspaceGitBranchDiffInput, WorkspaceGitBranchDiffOutput,
        WorkspaceGitCommitPushInput, WorkspaceGitConflictStateInput, WorkspaceGitConflictStateOutput,
        WorkspaceGitFilePreviewContentOutput, WorkspaceGitFilePreviewInput, WorkspaceGitPathInput,
        WorkspaceGitMarkConflictResolvedInput, WorkspaceGitPushInput, WorkspaceGitStatusInput,
        WorkspaceGitStatusOutput,
        WorkspaceGitSyncBaseInput, WorkspaceGitSyncBaseOutput, WorkspaceIdInput,
        WorkspacePrepareDelegationWorktreeInput, WorkspacePrepareDelegationWorktreeOutput,
        WorkspaceRemoveDelegationWorktreeInput, WorkspaceRunSetupInput, WorkspaceRunSetupOutput,
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
pub async fn read_workspace_file(
    state: State<'_, WorkspaceCommandState>,
    input: ReadWorkspaceFileInput,
) -> Result<ReadWorkspaceFileOutput, String> {
    dcc_tauri::commands::workspace_commands::read_workspace_file(state, input).await
}

#[tauri::command]
pub async fn write_workspace_file(
    state: State<'_, WorkspaceCommandState>,
    input: WriteWorkspaceFileInput,
) -> Result<WriteWorkspaceFileOutput, String> {
    dcc_tauri::commands::workspace_commands::write_workspace_file(state, input).await
}

#[tauri::command]
pub async fn search_workspace(
    state: State<'_, WorkspaceCommandState>,
    input: SearchWorkspaceInput,
) -> Result<SearchWorkspaceOutput, String> {
    dcc_tauri::commands::workspace_commands::search_workspace(state, input).await
}

#[tauri::command]
pub async fn list_mission_specs(
    state: State<'_, WorkspaceCommandState>,
    input: ListMissionSpecsInput,
) -> Result<ListMissionSpecsOutput, String> {
    dcc_tauri::commands::workspace_commands::list_mission_specs(state, input).await
}

#[tauri::command]
pub async fn save_mission_validation(
    state: State<'_, WorkspaceCommandState>,
    input: SaveMissionValidationInput,
) -> Result<SaveMissionValidationOutput, String> {
    dcc_tauri::commands::workspace_commands::save_mission_validation(state, input).await
}

#[tauri::command]
pub async fn compile_mission_spec_context(
    state: State<'_, WorkspaceCommandState>,
    input: CompileMissionSpecContextInput,
) -> Result<CompileMissionSpecContextOutput, String> {
    dcc_tauri::commands::workspace_commands::compile_mission_spec_context(state, input).await
}

#[tauri::command]
pub async fn mission_spec_context_status(
    state: State<'_, WorkspaceCommandState>,
    input: MissionSpecContextStatusInput,
) -> Result<MissionSpecContextStatusOutput, String> {
    dcc_tauri::commands::workspace_commands::mission_spec_context_status(state, input).await
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
pub async fn workspace_git_conflict_state(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitConflictStateInput,
) -> Result<WorkspaceGitConflictStateOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_git_conflict_state(state, input).await
}

#[tauri::command]
pub async fn workspace_git_accept_conflict(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitAcceptConflictInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_accept_conflict(state, input).await
}

#[tauri::command]
pub async fn workspace_git_mark_conflict_resolved(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitMarkConflictResolvedInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_mark_conflict_resolved(state, input)
        .await
}

#[tauri::command]
pub async fn workspace_git_abort_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitConflictStateInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_abort_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_git_complete_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_git_complete_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_prepare_delegation_worktree(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrepareDelegationWorktreeInput,
) -> Result<WorkspacePrepareDelegationWorktreeOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_prepare_delegation_worktree(state, input)
        .await
}

#[tauri::command]
pub async fn workspace_remove_delegation_worktree(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceRemoveDelegationWorktreeInput,
) -> Result<(), String> {
    dcc_tauri::commands::workspace_commands::workspace_remove_delegation_worktree(state, input)
        .await
}

#[tauri::command]
pub async fn workspace_apply_delegation_worktree(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceApplyDelegationWorktreeInput,
) -> Result<WorkspaceApplyDelegationWorktreeOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_apply_delegation_worktree(state, input)
        .await
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
pub async fn workspace_git_sync_base(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitSyncBaseInput,
) -> Result<WorkspaceGitSyncBaseOutput, String> {
    dcc_tauri::commands::workspace_commands::workspace_git_sync_base(state, input).await
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
