use tauri::State;

use dcc_tauri::{
    commands::forge_commands::{
        ForgeCliAccountsInput, ForgeCliAccountsOutput, ForgeCliSelectLoginInput,
        ForgeCliStatusInput, ForgeCliStatusOutput, GithubCliStatusInput, GithubCliStatusOutput,
    },
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
pub async fn workspace_forge_cli_select_login(
    state: State<'_, WorkspaceCommandState>,
    input: ForgeCliSelectLoginInput,
) -> Result<(), String> {
    dcc_tauri::commands::forge_commands::workspace_forge_cli_select_login(state, input).await
}
