use dcc_core::application::ActivateMcpDefinitionInput;
use dcc_tauri::{
    commands::mcp_commands::{
        self as mcp_command_impl, ActivateMcpIntegrationOutput, CancelMcpConnectOutput,
        CreateMcpIntegrationInput, CreateMcpIntegrationOutput, DisableMcpIntegrationInput,
        DisableMcpIntegrationOutput, DisconnectMcpConnectorInput, DisconnectMcpConnectorOutput,
        DisconnectMcpOauthInput, DisconnectMcpOauthOutput, ListMcpIntegrationsOutput,
        McpConnectLoginInput, RemoveMcpIntegrationInput, RemoveMcpIntegrationOutput,
        SetMcpToolPolicyInput, SetMcpToolPolicyOutput, StartMcpConnectInput, StartMcpConnectOutput,
        FetchMcpConnectorLogoInput, McpConnectorDetailsOutput, SetMcpConnectorLogoInput,
        SetMcpConnectorScopesInput, SetMcpProjectEnabledInput, SetMcpProjectEnabledOutput,
        TestMcpIntegrationInput,
        TestMcpIntegrationOutput, WaitMcpConnectOutput,
    },
    connector_gateway::ConnectorGateway,
    mcp_connect::McpConnectCoordinator,
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
pub async fn disconnect_mcp_oauth(
    state: State<'_, WorkspaceCommandState>,
    input: DisconnectMcpOauthInput,
) -> Result<DisconnectMcpOauthOutput, String> {
    mcp_command_impl::disconnect_mcp_oauth(state, input).await
}

#[tauri::command]
pub async fn set_mcp_tool_policy(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpToolPolicyInput,
) -> Result<SetMcpToolPolicyOutput, String> {
    mcp_command_impl::set_mcp_tool_policy(state, input).await
}

#[tauri::command]
pub async fn start_mcp_connect(
    state: State<'_, WorkspaceCommandState>,
    coordinator: State<'_, McpConnectCoordinator>,
    input: StartMcpConnectInput,
) -> Result<StartMcpConnectOutput, String> {
    mcp_command_impl::start_mcp_connect(state, coordinator, input).await
}

#[tauri::command]
pub async fn wait_mcp_connect(
    window: tauri::WebviewWindow,
    coordinator: State<'_, McpConnectCoordinator>,
    gateway: State<'_, ConnectorGateway>,
    input: McpConnectLoginInput,
) -> Result<WaitMcpConnectOutput, String> {
    let output = mcp_command_impl::wait_mcp_connect(coordinator, gateway, input).await;
    // The sign-in finished in the browser; bring DCC back in front.
    let _ = window.unminimize();
    let _ = window.set_focus();
    output
}

#[tauri::command]
pub async fn set_mcp_connector_scopes(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpConnectorScopesInput,
) -> Result<McpConnectorDetailsOutput, String> {
    mcp_command_impl::set_mcp_connector_scopes(state, input).await
}

#[tauri::command]
pub async fn set_mcp_connector_logo(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpConnectorLogoInput,
) -> Result<McpConnectorDetailsOutput, String> {
    mcp_command_impl::set_mcp_connector_logo(state, input).await
}

#[tauri::command]
pub async fn fetch_mcp_connector_logo(
    state: State<'_, WorkspaceCommandState>,
    input: FetchMcpConnectorLogoInput,
) -> Result<McpConnectorDetailsOutput, String> {
    mcp_command_impl::fetch_mcp_connector_logo(state, input).await
}

#[tauri::command]
pub async fn set_mcp_project_enabled(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpProjectEnabledInput,
) -> Result<SetMcpProjectEnabledOutput, String> {
    mcp_command_impl::set_mcp_project_enabled(state, input).await
}

#[tauri::command]
pub async fn test_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    gateway: State<'_, ConnectorGateway>,
    input: TestMcpIntegrationInput,
) -> Result<TestMcpIntegrationOutput, String> {
    mcp_command_impl::test_mcp_integration(state, gateway, input).await
}

#[tauri::command]
pub async fn cancel_mcp_connect(
    coordinator: State<'_, McpConnectCoordinator>,
    input: McpConnectLoginInput,
) -> Result<CancelMcpConnectOutput, String> {
    mcp_command_impl::cancel_mcp_connect(coordinator, input).await
}

#[tauri::command]
pub async fn disconnect_mcp_connector(
    state: State<'_, WorkspaceCommandState>,
    coordinator: State<'_, McpConnectCoordinator>,
    input: DisconnectMcpConnectorInput,
) -> Result<DisconnectMcpConnectorOutput, String> {
    mcp_command_impl::disconnect_mcp_connector(state, coordinator, input).await
}
