use dcc_core::application::ActivateMcpDefinitionInput;
use dcc_tauri::{
    commands::mcp_commands::{
        self as mcp_command_impl, ActivateMcpIntegrationOutput, CreateMcpIntegrationInput,
        CreateMcpIntegrationOutput, DisableMcpIntegrationInput, DisableMcpIntegrationOutput,
        ListMcpIntegrationsOutput, RemoveMcpIntegrationInput, RemoveMcpIntegrationOutput,
        SetMcpToolPolicyInput, SetMcpToolPolicyOutput,
    },
    state::WorkspaceCommandState,
};
use tauri::State;

#[tauri::command]
pub async fn list_mcp_integrations(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListMcpIntegrationsOutput, String> {
    mcp_command_impl::list_mcp_integrations(state).await
}

#[tauri::command]
pub async fn create_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: CreateMcpIntegrationInput,
) -> Result<CreateMcpIntegrationOutput, String> {
    mcp_command_impl::create_mcp_integration(state, input).await
}

#[tauri::command]
pub async fn activate_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: ActivateMcpDefinitionInput,
) -> Result<ActivateMcpIntegrationOutput, String> {
    mcp_command_impl::activate_mcp_integration(state, input).await
}

#[tauri::command]
pub async fn disable_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: DisableMcpIntegrationInput,
) -> Result<DisableMcpIntegrationOutput, String> {
    mcp_command_impl::disable_mcp_integration(state, input).await
}

#[tauri::command]
pub async fn remove_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: RemoveMcpIntegrationInput,
) -> Result<RemoveMcpIntegrationOutput, String> {
    mcp_command_impl::remove_mcp_integration(state, input).await
}

#[tauri::command]
pub async fn set_mcp_tool_policy(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpToolPolicyInput,
) -> Result<SetMcpToolPolicyOutput, String> {
    mcp_command_impl::set_mcp_tool_policy(state, input).await
}
