use tauri::State;

use dcc_tauri::{
    commands::forge_commands::{
        ForgeCliAccountsInput, ForgeCliAccountsOutput, ForgeCliHostsInput, ForgeCliHostsOutput,
        ForgeCliSelectLoginInput, ForgeCliStatusInput, ForgeCliStatusOutput, GithubCliStatusInput,
        GithubCliStatusOutput, WorkspaceForgeContextInput, WorkspaceForgeContextOutput,
        WorkspacePrStatusInput, WorkspacePrStatusOutput,
    },
    commands::workspace_commands::WorkspaceGitPushInput,
    state::WorkspaceCommandState,
};

#[tauri::command]
pub async fn workspace_github_cli_status(
    state: State<'_, WorkspaceCommandState>,
    input: GithubCliStatusInput,
) -> Result<GithubCliStatusOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_github_cli_status(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_status(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliStatusInput,
) -> Result<ForgeCliStatusOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_status(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_accounts(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliAccountsInput,
) -> Result<ForgeCliAccountsOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_accounts(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_hosts(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliHostsInput,
) -> Result<ForgeCliHostsOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_hosts(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_cli_select_login(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliSelectLoginInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_select_login(state, input).await
}

#[tauri::command]
pub async fn workspace_forge_context(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceForgeContextInput,
) -> Result<WorkspaceForgeContextOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_forge_context(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_change_request_view_web(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_create(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_change_request_create(state, input).await
}

#[tauri::command]
pub async fn workspace_change_request_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_change_request_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_view_web(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_gh_pr_view_web(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_create_fill(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_gh_pr_create_fill(state, input).await
}

#[tauri::command]
pub async fn workspace_gh_pr_merge(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceGitPushInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_gh_pr_merge(state, input).await
}

#[tauri::command]
pub async fn workspace_pr_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspacePrStatusInput,
) -> Result<WorkspacePrStatusOutput, String> {
    dcc_tauri::commands::forge_commands::workspace_pr_status(state, input).await
}
