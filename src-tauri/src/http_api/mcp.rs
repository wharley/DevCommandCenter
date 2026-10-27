//! Settings → Conectores for the native app: the MCP integrations the
//! desktop keeps, adding and activating one, DCC's own sign-in (start, the
//! browser, wait, cancel), signing out, the scopes asked for and testing
//! one. The sign-in coordinator and the connector gateway are the desktop's
//! (one of each serves Settings and the agents); without them, 404.
use super::*;
use dcc_core::application::ActivateMcpDefinitionInput;
use dcc_tauri::commands::mcp_commands::{
    self as commands, CreateMcpIntegrationInput, DisconnectMcpConnectorInput,
    FetchMcpConnectorLogoInput, ImportNativeMcpServersInput, McpConnectLoginInput,
    RemoveMcpIntegrationInput, SetMcpConnectorLogoInput, SetMcpConnectorScopesInput,
    SetMcpProjectEnabledInput, SetMcpToolPolicyInput, StartMcpConnectInput,
    TestMcpIntegrationInput,
};
use dcc_tauri::connector_gateway::ConnectorGateway;
use dcc_tauri::mcp_connect::McpConnectCoordinator;
use tauri::Manager;

fn reply<T: Serialize>(result: Result<T, String>) -> Result<Json<Value>, HttpApiError> {
    let value = result.map_err(HttpApiError::bad_request)?;
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

fn app(backend: &HttpBackend) -> Result<tauri::AppHandle, HttpApiError> {
    backend
        .app
        .get()
        .cloned()
        .ok_or_else(|| HttpApiError::not_found("connectors are not available"))
}

fn coordinator(
    app: &tauri::AppHandle,
) -> Result<tauri::State<'_, McpConnectCoordinator>, HttpApiError> {
    app.try_state::<McpConnectCoordinator>()
        .ok_or_else(|| HttpApiError::not_found("connector sign-in is not available"))
}

fn gateway(app: &tauri::AppHandle) -> Result<tauri::State<'_, ConnectorGateway>, HttpApiError> {
    app.try_state::<ConnectorGateway>()
        .ok_or_else(|| HttpApiError::not_found("the connector gateway is not available"))
}

pub(super) async fn list(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::list_mcp_integrations_with(&backend.workspace).await)
}

pub(super) async fn create(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<CreateMcpIntegrationInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::create_mcp_integration_with(&backend.workspace, input).await)
}

pub(super) async fn activate(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ActivateMcpDefinitionInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::activate_mcp_integration_with(&backend.workspace, input).await)
}

pub(super) async fn remove(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<RemoveMcpIntegrationInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::remove_mcp_integration_with(&backend.workspace, input).await)
}

pub(super) async fn set_scopes(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<SetMcpConnectorScopesInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::set_mcp_connector_scopes_with(&backend.workspace, input).await)
}

pub(super) async fn start_connect(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<StartMcpConnectInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = app(&backend)?;
    let coordinator = coordinator(&app)?;
    reply(commands::start_mcp_connect_with(&backend.workspace, &coordinator, input).await)
}

/// Returns once the person authorized in the browser (or gave up).
pub(super) async fn wait_connect(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<McpConnectLoginInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = app(&backend)?;
    let (coordinator, gateway) = (coordinator(&app)?, gateway(&app)?);
    reply(commands::wait_mcp_connect_with(&coordinator, &gateway, input).await)
}

pub(super) async fn cancel_connect(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<McpConnectLoginInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = app(&backend)?;
    let coordinator = coordinator(&app)?;
    reply(commands::cancel_mcp_connect_with(&coordinator, input).await)
}

pub(super) async fn disconnect(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<DisconnectMcpConnectorInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = app(&backend)?;
    let coordinator = coordinator(&app)?;
    reply(commands::disconnect_mcp_connector_with(&backend.workspace, &coordinator, input).await)
}

pub(super) async fn test(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<TestMcpIntegrationInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = app(&backend)?;
    let gateway = gateway(&app)?;
    reply(commands::test_mcp_integration_with(&backend.workspace, &gateway, input).await)
}

pub(super) async fn set_logo(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<SetMcpConnectorLogoInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::set_mcp_connector_logo_with(&backend.workspace, input).await)
}

/// The icon of the connector's own site, kept as its logo.
pub(super) async fn fetch_logo(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<FetchMcpConnectorLogoInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::fetch_mcp_connector_logo_with(&backend.workspace, input).await)
}

pub(super) async fn set_project(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<SetMcpProjectEnabledInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::set_mcp_project_enabled_with(&backend.workspace, input).await)
}

pub(super) async fn set_tool_policy(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<SetMcpToolPolicyInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::set_mcp_tool_policy_with(&backend.workspace, input).await)
}

/// The remote servers Claude Code, Codex and Cursor have configured.
pub(super) async fn native_servers() -> Result<Json<Value>, HttpApiError> {
    reply(commands::list_native_mcp_servers().await)
}

pub(super) async fn import_native(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ImportNativeMcpServersInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::import_native_mcp_servers_with(&backend.workspace, input).await)
}
