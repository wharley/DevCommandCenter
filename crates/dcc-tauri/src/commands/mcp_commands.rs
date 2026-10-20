use std::collections::{HashMap, HashSet};

use chrono::Utc;
use dcc_core::{
    application::{activate_mcp_definition, ActivateMcpDefinitionInput},
    domain::mcp::{
        McpBinding, McpBindingId, McpBindingScope, McpDefinition, McpDefinitionId,
        McpDefinitionOwnership, McpSecretBinding, McpSecretReferenceId, McpSecretTarget,
        McpToolPolicy, McpToolPolicyDecision, McpTransport, McpTrust, McpTrustDecision,
        McpTrustFingerprint,
    },
    domain::provider::ProviderId,
    ports::{CredentialStore, McpRepo, SecretValue},
    CoreError,
};
use dcc_infra::{credential_store::SystemCredentialStore, mcp_db::SqliteMcpRepo};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    connector_gateway::ConnectorGateway,
    mcp_connect::{ConnectStart, ConnectStores, McpConnectCoordinator, DCC_OAUTH_GRANT_OWNER},
    state::WorkspaceCommandState,
};
use dcc_core::domain::project::ProjectId;
use dcc_core::{
    application::probe_mcp_definition,
    domain::mcp::{McpConnectionCheck, McpConnectionState, McpConnectorDetails, McpErrorCategory},
};
use dcc_infra::mcp_probe::{CommandLookup, McpProbeLimits, SecureMcpProbe};
use std::time::Duration;
use dcc_infra::mcp_oauth::{McpOauthError, UserProvidedClient};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpIntegrationRecord {
    pub definition: McpDefinition,
    pub bindings: Vec<McpBinding>,
    pub tool_policies: Vec<McpToolPolicy>,
    pub credential_count: usize,
    pub oauth_provider_ids: Vec<ProviderId>,
    /// Last connection check DCC ran itself; `None` until one ran.
    pub connection_check: Option<McpConnectionCheck>,
    /// Projects where this integration is switched off.
    #[serde(default)]
    pub excluded_project_ids: Vec<ProjectId>,
    /// OAuth scopes the user chose; `None` uses the server's suggestion.
    pub scopes: Option<String>,
    /// `data:image/...` logo the user set or DCC found on the server's site.
    pub logo: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMcpIntegrationsOutput {
    pub integrations: Vec<McpIntegrationRecord>,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpCredentialInput {
    pub target: McpSecretTarget,
    /// Write-only renderer input. It is moved into `SecretValue`, zeroized on
    /// drop, and never included in a command output.
    pub secret: String,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpIntegrationInput {
    pub display_name: String,
    pub transport: McpTransport,
    pub scope: McpBindingScope,
    #[serde(default)]
    pub credentials: Vec<McpCredentialInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpIntegrationOutput {
    pub integration: McpIntegrationRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivateMcpIntegrationOutput {
    pub integration: McpIntegrationRecord,
    pub changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisableMcpIntegrationInput {
    pub definition_id: McpDefinitionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisableMcpIntegrationOutput {
    pub integration: McpIntegrationRecord,
    pub changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMcpIntegrationInput {
    pub definition_id: McpDefinitionId,
    #[serde(default)]
    pub delete_credentials: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMcpIntegrationOutput {
    pub removed: bool,
    pub deleted_credentials: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetMcpToolPolicyInput {
    pub definition_id: McpDefinitionId,
    pub tool_name: String,
    pub decision: McpToolPolicyDecision,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetMcpToolPolicyOutput {
    pub integration: McpIntegrationRecord,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartMcpConnectInput {
    pub definition_id: McpDefinitionId,
    /// The user's own OAuth app, for servers that register no clients
    /// (e.g. Slack). Write-only, like credential secrets.
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StartMcpConnectOutput {
    /// The server works without a sign-in.
    NoAuthRequired,
    /// Open `authorizationUrl` in the browser, then call `wait_mcp_connect`.
    Authorize {
        login_id: String,
        authorization_url: String,
    },
    Failed {
        reason: McpConnectFailure,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectLoginInput {
    pub login_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WaitMcpConnectOutput {
    /// Signed in. `check` lists the tools when the follow-up check ran.
    Connected {
        check: Option<McpConnectionCheck>,
    },
    Failed {
        reason: McpConnectFailure,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpConnectFailure {
    NotMcpServer,
    /// The server needs the user's own OAuth app (client id/secret).
    ClientRequired,
    ClientRejected,
    Declined,
    Cancelled,
    TimedOut,
    Transient,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelMcpConnectOutput {
    pub cancelled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetMcpConnectorScopesInput {
    pub definition_id: McpDefinitionId,
    /// Space-separated scopes; `null` or blank goes back to the server's.
    pub scopes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetMcpConnectorLogoInput {
    pub definition_id: McpDefinitionId,
    /// `data:image/...;base64,` URI, or `null` to remove the logo.
    pub logo: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectorDetailsOutput {
    pub scopes: Option<String>,
    pub logo: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FetchMcpConnectorLogoInput {
    pub definition_id: McpDefinitionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetMcpProjectEnabledInput {
    pub definition_id: McpDefinitionId,
    pub project_id: ProjectId,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetMcpProjectEnabledOutput {
    pub excluded_project_ids: Vec<ProjectId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TestMcpIntegrationInput {
    pub definition_id: McpDefinitionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TestMcpIntegrationOutput {
    pub check: McpConnectionCheck,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectMcpConnectorInput {
    pub definition_id: McpDefinitionId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectMcpConnectorOutput {
    pub disconnected: bool,
}

#[derive(Debug, Error)]
enum McpCommandError {
    #[error("{0}")]
    Core(#[from] CoreError),
    #[error("credential operation failed: {0}")]
    Credential(#[from] dcc_core::ports::CredentialStoreError),
}

type McpCommandResult<T> = Result<T, McpCommandError>;

#[tauri::command]
pub async fn list_mcp_integrations(
    state: State<'_, WorkspaceCommandState>,
) -> Result<ListMcpIntegrationsOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    list_integrations(&repo)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: CreateMcpIntegrationInput,
) -> Result<CreateMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    create_integration(&repo, &SystemCredentialStore::default(), input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn activate_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: ActivateMcpDefinitionInput,
) -> Result<ActivateMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    activate_integration(&repo, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn disable_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: DisableMcpIntegrationInput,
) -> Result<DisableMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    disable_integration(&repo, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    input: RemoveMcpIntegrationInput,
) -> Result<RemoveMcpIntegrationOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    remove_integration(&repo, &SystemCredentialStore::default(), input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_mcp_tool_policy(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpToolPolicyInput,
) -> Result<SetMcpToolPolicyOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    set_tool_policy(&repo, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn start_mcp_connect(
    state: State<'_, WorkspaceCommandState>,
    coordinator: State<'_, McpConnectCoordinator>,
    input: StartMcpConnectInput,
) -> Result<StartMcpConnectOutput, String> {
    let stores = connect_stores(&state)?;
    let definition = stores
        .repo
        .get_mcp_definition(&input.definition_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "MCP integration not found".to_string())?;
    let user_client = input
        .client_id
        .map(|client_id| client_id.trim().to_string())
        .filter(|client_id| !client_id.is_empty())
        .map(|client_id| UserProvidedClient {
            client_id,
            client_secret: input
                .client_secret
                .map(|secret| secret.trim().to_string())
                .filter(|secret| !secret.is_empty()),
        });
    Ok(
        match coordinator.start(stores, &definition, user_client).await {
            Ok(ConnectStart::NoAuthRequired) => StartMcpConnectOutput::NoAuthRequired,
            Ok(ConnectStart::Authorize {
                login_id,
                authorization_url,
            }) => StartMcpConnectOutput::Authorize {
                login_id,
                authorization_url,
            },
            Err(error) => StartMcpConnectOutput::Failed {
                reason: connect_failure(&error),
                message: error.to_string(),
            },
        },
    )
}

#[tauri::command]
pub async fn wait_mcp_connect(
    coordinator: State<'_, McpConnectCoordinator>,
    gateway: State<'_, ConnectorGateway>,
    input: McpConnectLoginInput,
) -> Result<WaitMcpConnectOutput, String> {
    Ok(match coordinator.wait(&input.login_id).await {
        // The follow-up check gives Settings the tool count right away.
        Ok(definition) => WaitMcpConnectOutput::Connected {
            check: Some(gateway.check(&definition).await),
        },
        Err(error) => WaitMcpConnectOutput::Failed {
            reason: connect_failure(&error),
            message: error.to_string(),
        },
    })
}

#[tauri::command]
pub async fn cancel_mcp_connect(
    coordinator: State<'_, McpConnectCoordinator>,
    input: McpConnectLoginInput,
) -> Result<CancelMcpConnectOutput, String> {
    Ok(CancelMcpConnectOutput {
        cancelled: coordinator.cancel(&input.login_id),
    })
}

#[tauri::command]
pub async fn disconnect_mcp_connector(
    state: State<'_, WorkspaceCommandState>,
    coordinator: State<'_, McpConnectCoordinator>,
    input: DisconnectMcpConnectorInput,
) -> Result<DisconnectMcpConnectorOutput, String> {
    let stores = connect_stores(&state)?;
    let disconnected = coordinator
        .disconnect(&stores, &input.definition_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(DisconnectMcpConnectorOutput { disconnected })
}

/// Sets the scopes requested at the next sign-in. Blank means "use what the
/// server suggests".
#[tauri::command]
pub async fn set_mcp_connector_scopes(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpConnectorScopesInput,
) -> Result<McpConnectorDetailsOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let scopes = input
        .scopes
        .map(|scopes| scopes.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|scopes| !scopes.is_empty());
    update_connector_details(&repo, &input.definition_id, |details| {
        details.scopes = scopes;
    })
    .await
}

/// Sets or removes a connector's logo.
#[tauri::command]
pub async fn set_mcp_connector_logo(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpConnectorLogoInput,
) -> Result<McpConnectorDetailsOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    update_connector_details(&repo, &input.definition_id, |details| {
        details.logo = input.logo;
    })
    .await
}

/// Looks for the icon of the connector's own site and keeps it as its logo.
/// Leaves the current logo when the site has none.
#[tauri::command]
pub async fn fetch_mcp_connector_logo(
    state: State<'_, WorkspaceCommandState>,
    input: FetchMcpConnectorLogoInput,
) -> Result<McpConnectorDetailsOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let definition = repo
        .get_mcp_definition(&input.definition_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "MCP integration not found".to_string())?;
    let McpTransport::Http { url } = &definition.transport else {
        return Err("Only remote connectors have a site".to_string());
    };
    let found = dcc_infra::connector_icon::fetch_site_icon(url)
        .await
        .filter(|logo| dcc_core::domain::mcp::is_valid_logo_uri(logo));
    update_connector_details(&repo, &input.definition_id, |details| {
        if found.is_some() {
            details.logo = found;
        }
    })
    .await
}

async fn update_connector_details(
    repo: &SqliteMcpRepo,
    definition_id: &McpDefinitionId,
    change: impl FnOnce(&mut McpConnectorDetails),
) -> Result<McpConnectorDetailsOutput, String> {
    repo.get_mcp_definition(definition_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "MCP integration not found".to_string())?;
    let mut details = repo
        .list_mcp_connector_details()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|details| &details.definition_id == definition_id)
        .unwrap_or(McpConnectorDetails {
            definition_id: definition_id.clone(),
            scopes: None,
            logo: None,
        });
    change(&mut details);
    details
        .validate()
        .map_err(|error| error.to_string())?;
    repo.save_mcp_connector_details(&details)
        .await
        .map_err(|error| error.to_string())?;
    Ok(McpConnectorDetailsOutput {
        scopes: details.scopes,
        logo: details.logo,
    })
}

/// Switches an integration off (or back on) in one project. New sessions in
/// that project leave it out; other projects keep it.
#[tauri::command]
pub async fn set_mcp_project_enabled(
    state: State<'_, WorkspaceCommandState>,
    input: SetMcpProjectEnabledInput,
) -> Result<SetMcpProjectEnabledOutput, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    repo.get_mcp_definition(&input.definition_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "MCP integration not found".to_string())?;
    repo.set_mcp_project_exclusion(&input.definition_id, &input.project_id, !input.enabled)
        .await
        .map_err(|error| error.to_string())?;
    let excluded_project_ids = repo
        .list_mcp_project_exclusions()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|(definition_id, _)| definition_id == &input.definition_id)
        .map(|(_, project_id)| project_id)
        .collect();
    Ok(SetMcpProjectEnabledOutput {
        excluded_project_ids,
    })
}

#[tauri::command]
pub async fn test_mcp_integration(
    state: State<'_, WorkspaceCommandState>,
    gateway: State<'_, ConnectorGateway>,
    input: TestMcpIntegrationInput,
) -> Result<TestMcpIntegrationOutput, String> {
    let stores = connect_stores(&state)?;
    let definition = stores
        .repo
        .get_mcp_definition(&input.definition_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "MCP integration not found".to_string())?;
    let check = run_connection_check(&gateway, &stores, &definition).await;
    Ok(TestMcpIntegrationOutput { check })
}

/// Signed-in connectors are checked through the gateway (vendor token and
/// refresh); everything else through the standalone probe, which finds bare
/// commands on the user's PATH like provider runtimes do.
pub async fn run_connection_check(
    gateway: &ConnectorGateway,
    stores: &ConnectStores,
    definition: &McpDefinition,
) -> McpConnectionCheck {
    let signed_in = stores
        .repo
        .get_mcp_oauth_grant(
            &definition.id,
            &ProviderId(DCC_OAUTH_GRANT_OWNER.to_string()),
        )
        .await
        .ok()
        .flatten()
        .is_some();
    if signed_in && matches!(definition.transport, McpTransport::Http { .. }) {
        return gateway.check(definition).await;
    }
    let limits = McpProbeLimits {
        // A first `npx -y` run downloads the package.
        initialize_timeout: Duration::from_secs(30),
        list_tools_timeout: Duration::from_secs(15),
        ..McpProbeLimits::default()
    };
    let probed = match SecureMcpProbe::with_limits(Arc::clone(&stores.credentials), limits) {
        Ok(probe) => {
            let probe = probe.with_command_lookup(CommandLookup::from_environment());
            probe_mcp_definition(&probe, definition).await
        }
        Err(error) => Err(error),
    };
    let check = match probed {
        Ok(report) => McpConnectionCheck {
            definition_id: definition.id.clone(),
            state: McpConnectionState::Connected,
            tools: report.tools,
            message: None,
            checked_at: Utc::now().to_rfc3339(),
        },
        Err(error) => McpConnectionCheck {
            definition_id: definition.id.clone(),
            state: if error.category == McpErrorCategory::Authentication {
                McpConnectionState::NeedsAuth
            } else {
                McpConnectionState::Failed
            },
            tools: Vec::new(),
            message: Some(error.message),
            checked_at: Utc::now().to_rfc3339(),
        },
    };
    let _ = stores.repo.save_mcp_connection_check(&check).await;
    check
}

fn connect_stores(state: &WorkspaceCommandState) -> Result<ConnectStores, String> {
    let repo = SqliteMcpRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    Ok(ConnectStores {
        repo: Arc::new(repo),
        credentials: Arc::new(SystemCredentialStore::default()),
    })
}

fn connect_failure(error: &McpOauthError) -> McpConnectFailure {
    match error {
        McpOauthError::NotMcpServer => McpConnectFailure::NotMcpServer,
        McpOauthError::ClientRegistrationRequired => McpConnectFailure::ClientRequired,
        McpOauthError::ClientRegistrationRejected(_) => McpConnectFailure::ClientRejected,
        McpOauthError::AuthorizationDenied(_) => McpConnectFailure::Declined,
        McpOauthError::Cancelled => McpConnectFailure::Cancelled,
        McpOauthError::Timeout => McpConnectFailure::TimedOut,
        McpOauthError::Transient(_) => McpConnectFailure::Transient,
        McpOauthError::Discovery(_)
        | McpOauthError::StateMismatch
        | McpOauthError::IssuerMismatch
        | McpOauthError::TokenExchange(_)
        | McpOauthError::ReauthorizationRequired => McpConnectFailure::Other,
    }
}

async fn list_integrations<R>(repo: &R) -> McpCommandResult<ListMcpIntegrationsOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    let definitions = repo.list_mcp_definitions().await?;
    let mut bindings_by_definition = HashMap::<McpDefinitionId, Vec<McpBinding>>::new();
    for binding in repo.list_mcp_bindings(None).await? {
        bindings_by_definition
            .entry(binding.definition_id.clone())
            .or_default()
            .push(binding);
    }
    let mut policies_by_definition = HashMap::<McpDefinitionId, Vec<McpToolPolicy>>::new();
    for policy in repo.list_mcp_tool_policies(None).await? {
        policies_by_definition
            .entry(policy.definition_id.clone())
            .or_default()
            .push(policy);
    }
    let mut grants_by_definition =
        HashMap::<McpDefinitionId, Vec<dcc_core::domain::mcp::McpOauthGrant>>::new();
    for grant in repo.list_mcp_oauth_grants(None).await? {
        grants_by_definition
            .entry(grant.definition_id.clone())
            .or_default()
            .push(grant);
    }

    let mut checks_by_definition = repo
        .list_mcp_connection_checks()
        .await?
        .into_iter()
        .map(|check| (check.definition_id.clone(), check))
        .collect::<HashMap<_, _>>();
    let mut details_by_definition = repo
        .list_mcp_connector_details()
        .await?
        .into_iter()
        .map(|details| (details.definition_id.clone(), details))
        .collect::<HashMap<_, _>>();
    let mut exclusions_by_definition = HashMap::<McpDefinitionId, Vec<ProjectId>>::new();
    for (definition_id, project_id) in repo.list_mcp_project_exclusions().await? {
        exclusions_by_definition
            .entry(definition_id)
            .or_default()
            .push(project_id);
    }
    let integrations = definitions
        .into_iter()
        .map(|definition| {
            let connection_check = checks_by_definition.remove(&definition.id);
            let excluded_project_ids = exclusions_by_definition
                .remove(&definition.id)
                .unwrap_or_default();
            let bindings = bindings_by_definition
                .remove(&definition.id)
                .unwrap_or_default();
            let policies = policies_by_definition
                .remove(&definition.id)
                .unwrap_or_default();
            let grants = grants_by_definition
                .remove(&definition.id)
                .unwrap_or_default();
            let details = details_by_definition.remove(&definition.id);
            McpIntegrationRecord {
                connection_check,
                excluded_project_ids,
                scopes: details.as_ref().and_then(|details| details.scopes.clone()),
                logo: details.and_then(|details| details.logo),
                ..integration_record(definition, bindings, policies, grants)
            }
        })
        .collect();
    Ok(ListMcpIntegrationsOutput { integrations })
}

async fn create_integration<R, C>(
    repo: &R,
    credential_store: &C,
    input: CreateMcpIntegrationInput,
) -> McpCommandResult<CreateMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
    C: CredentialStore + Sync + ?Sized,
{
    let definition_id = McpDefinitionId(format!("mcp-{}", Uuid::new_v4()));
    let mut credential_values = Vec::with_capacity(input.credentials.len());
    let mut secret_refs = Vec::with_capacity(input.credentials.len());
    for credential in input.credentials {
        let reference =
            McpSecretReferenceId(format!("credential:{}:{}", definition_id.0, Uuid::new_v4()));
        let secret = SecretValue::new(credential.secret.into_bytes())?;
        secret_refs.push(McpSecretBinding {
            target: credential.target,
            secret_ref: reference.clone(),
        });
        credential_values.push((reference, secret));
    }

    let now = Utc::now().to_rfc3339();
    let mut definition = McpDefinition {
        id: definition_id.clone(),
        display_name: input.display_name.trim().to_string(),
        transport: input.transport,
        secret_refs,
        enabled: false,
        ownership: McpDefinitionOwnership::DccManaged,
        trust: McpTrust {
            current_fingerprint: McpTrustFingerprint("0".repeat(64)),
            decision: McpTrustDecision::Untrusted,
        },
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    definition.synchronize_trust_fingerprint();
    definition
        .validate()
        .map_err(|error| CoreError::InvalidInput(error.to_string()))?;

    let binding = McpBinding {
        id: McpBindingId(format!("binding-{}", Uuid::new_v4())),
        definition_id,
        scope: input.scope,
        enabled: true,
        provider_exclusions: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    binding
        .validate()
        .map_err(|error| CoreError::InvalidInput(error.to_string()))?;

    let mut stored_references = Vec::with_capacity(credential_values.len());
    for (reference, secret) in credential_values {
        if let Err(error) = credential_store.store_secret(&reference, secret).await {
            delete_credentials_best_effort(credential_store, &stored_references).await;
            return Err(error.into());
        }
        stored_references.push(reference);
    }

    if let Err(error) = repo.save_mcp_definition(&definition).await {
        delete_credentials_best_effort(credential_store, &stored_references).await;
        return Err(error.into());
    }
    if let Err(error) = repo.save_mcp_binding(&binding).await {
        let _ = repo.delete_mcp_definition(&definition.id).await;
        delete_credentials_best_effort(credential_store, &stored_references).await;
        return Err(error.into());
    }

    Ok(CreateMcpIntegrationOutput {
        integration: integration_record(definition, vec![binding], Vec::new(), Vec::new()),
    })
}

async fn activate_integration<R>(
    repo: &R,
    input: ActivateMcpDefinitionInput,
) -> McpCommandResult<ActivateMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    let output = activate_mcp_definition(repo, input).await?;
    let bindings = repo.list_mcp_bindings(Some(&output.definition.id)).await?;
    let policies = repo
        .list_mcp_tool_policies(Some(&output.definition.id))
        .await?;
    let grants = repo
        .list_mcp_oauth_grants(Some(&output.definition.id))
        .await?;
    Ok(ActivateMcpIntegrationOutput {
        integration: integration_record(output.definition, bindings, policies, grants),
        changed: output.changed,
    })
}

async fn disable_integration<R>(
    repo: &R,
    input: DisableMcpIntegrationInput,
) -> McpCommandResult<DisableMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    let mut definition = repo
        .get_mcp_definition(&input.definition_id)
        .await?
        .ok_or_else(|| CoreError::InvalidInput("MCP definition was not found".to_string()))?;
    let changed = definition.enabled;
    if changed {
        definition.enabled = false;
        definition.updated_at = Utc::now().to_rfc3339();
        repo.save_mcp_definition(&definition).await?;
    }
    let bindings = repo.list_mcp_bindings(Some(&definition.id)).await?;
    let policies = repo.list_mcp_tool_policies(Some(&definition.id)).await?;
    let grants = repo.list_mcp_oauth_grants(Some(&definition.id)).await?;
    Ok(DisableMcpIntegrationOutput {
        integration: integration_record(definition, bindings, policies, grants),
        changed,
    })
}

async fn remove_integration<R, C>(
    repo: &R,
    credential_store: &C,
    input: RemoveMcpIntegrationInput,
) -> McpCommandResult<RemoveMcpIntegrationOutput>
where
    R: McpRepo + Sync + ?Sized,
    C: CredentialStore + Sync + ?Sized,
{
    let Some(mut definition) = repo.get_mcp_definition(&input.definition_id).await? else {
        return Ok(RemoveMcpIntegrationOutput {
            removed: false,
            deleted_credentials: 0,
        });
    };
    let oauth_grants = repo.list_mcp_oauth_grants(Some(&definition.id)).await?;

    let mut deleted_credentials = 0;
    if input.delete_credentials {
        if definition.enabled {
            definition.enabled = false;
            definition.updated_at = Utc::now().to_rfc3339();
            repo.save_mcp_definition(&definition).await?;
        }

        let mut unique_references = HashSet::new();
        for binding in &definition.secret_refs {
            if unique_references.insert(binding.secret_ref.clone())
                && credential_store.delete_secret(&binding.secret_ref).await?
            {
                deleted_credentials += 1;
            }
        }
        for grant in &oauth_grants {
            if unique_references.insert(grant.secret_ref.clone())
                && credential_store.delete_secret(&grant.secret_ref).await?
            {
                deleted_credentials += 1;
            }
        }
    }

    repo.delete_mcp_definition(&definition.id).await?;
    Ok(RemoveMcpIntegrationOutput {
        removed: true,
        deleted_credentials,
    })
}

async fn set_tool_policy<R>(
    repo: &R,
    input: SetMcpToolPolicyInput,
) -> McpCommandResult<SetMcpToolPolicyOutput>
where
    R: McpRepo + Sync + ?Sized,
{
    let mut definition = repo
        .get_mcp_definition(&input.definition_id)
        .await?
        .ok_or_else(|| CoreError::InvalidInput("MCP definition was not found".to_string()))?;
    let tool_name = input.tool_name.trim().to_string();
    let policy = McpToolPolicy {
        definition_id: definition.id.clone(),
        tool_name: tool_name.clone(),
        decision: input.decision,
        updated_at: Utc::now().to_rfc3339(),
    };
    policy
        .validate()
        .map_err(|error| CoreError::InvalidInput(error.to_string()))?;

    definition.updated_at = policy.updated_at.clone();
    repo.save_mcp_definition(&definition).await?;

    match policy.decision {
        McpToolPolicyDecision::Ask => {
            repo.delete_mcp_tool_policy(&definition.id, &tool_name)
                .await?;
        }
        McpToolPolicyDecision::Allow | McpToolPolicyDecision::Deny => {
            repo.save_mcp_tool_policy(&policy).await?;
        }
    }

    let bindings = repo.list_mcp_bindings(Some(&definition.id)).await?;
    let policies = repo.list_mcp_tool_policies(Some(&definition.id)).await?;
    let grants = repo.list_mcp_oauth_grants(Some(&definition.id)).await?;
    Ok(SetMcpToolPolicyOutput {
        integration: integration_record(definition, bindings, policies, grants),
    })
}

fn integration_record(
    definition: McpDefinition,
    bindings: Vec<McpBinding>,
    tool_policies: Vec<McpToolPolicy>,
    mut oauth_grants: Vec<dcc_core::domain::mcp::McpOauthGrant>,
) -> McpIntegrationRecord {
    oauth_grants.sort_unstable_by(|left, right| left.provider_id.0.cmp(&right.provider_id.0));
    let credential_count = definition.secret_refs.len() + oauth_grants.len();
    McpIntegrationRecord {
        definition,
        bindings,
        tool_policies,
        credential_count,
        oauth_provider_ids: oauth_grants
            .into_iter()
            .map(|grant| grant.provider_id)
            .collect(),
        connection_check: None,
        excluded_project_ids: Vec::new(),
        scopes: None,
        logo: None,
    }
}

async fn delete_credentials_best_effort<C>(
    credential_store: &C,
    references: &[McpSecretReferenceId],
) where
    C: CredentialStore + Sync + ?Sized,
{
    for reference in references {
        let _ = credential_store.delete_secret(reference).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };

    use async_trait::async_trait;
    use dcc_core::{
        domain::mcp::{McpSecretTarget, McpTransport},
        ports::{CredentialStoreError, CredentialStoreResult},
    };
    use dcc_infra::mcp_db::SqliteMcpRepo;
    use rusqlite::Connection;

    use super::*;

    #[derive(Clone, Default)]
    struct TestCredentialStore {
        values: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        fail_deletes: Arc<AtomicBool>,
    }

    #[async_trait]
    impl CredentialStore for TestCredentialStore {
        async fn store_secret(
            &self,
            reference: &McpSecretReferenceId,
            secret: SecretValue,
        ) -> CredentialStoreResult<()> {
            self.values
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .insert(reference.0.clone(), secret.expose_secret().to_vec());
            Ok(())
        }

        async fn resolve_secret(
            &self,
            reference: &McpSecretReferenceId,
        ) -> CredentialStoreResult<Option<SecretValue>> {
            self.values
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .get(&reference.0)
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        async fn delete_secret(
            &self,
            reference: &McpSecretReferenceId,
        ) -> CredentialStoreResult<bool> {
            if self.fail_deletes.load(Ordering::Relaxed) {
                return Err(CredentialStoreError::AccessDenied);
            }
            Ok(self
                .values
                .lock()
                .map_err(|_| CredentialStoreError::OperationFailed)?
                .remove(&reference.0)
                .is_some())
        }
    }

    fn repo() -> SqliteMcpRepo {
        SqliteMcpRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().expect("open sqlite"),
        )))
        .expect("create MCP repo")
    }

    fn create_input(secret: &str) -> CreateMcpIntegrationInput {
        CreateMcpIntegrationInput {
            display_name: "Payments".to_string(),
            transport: McpTransport::Http {
                url: "https://mcp.example.test".to_string(),
            },
            scope: McpBindingScope::Global,
            credentials: vec![McpCredentialInput {
                target: McpSecretTarget::HttpHeader {
                    name: "Authorization".to_string(),
                },
                secret: secret.to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn create_and_list_return_only_opaque_credential_metadata() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let secret = "renderer-write-only-token";
        let created = create_integration(&repo, &credentials, create_input(secret))
            .await
            .expect("create integration");

        assert!(!created.integration.definition.enabled);
        assert!(created.integration.definition.trust.requires_confirmation());
        assert_eq!(created.integration.credential_count, 1);
        let serialized = serde_json::to_string(&created).expect("serialize output");
        assert!(!serialized.contains(secret));

        let reference = &created.integration.definition.secret_refs[0].secret_ref;
        assert_eq!(
            credentials
                .resolve_secret(reference)
                .await
                .expect("resolve credential")
                .expect("stored credential")
                .expose_secret(),
            secret.as_bytes()
        );
        let listed = list_integrations(&repo).await.expect("list integrations");
        assert_eq!(listed.integrations, vec![created.integration]);
    }

    #[tokio::test]
    async fn activation_is_fingerprint_bound_and_disable_is_idempotent() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let created = create_integration(&repo, &credentials, create_input("token"))
            .await
            .expect("create integration");
        let definition_id = created.integration.definition.id.clone();

        let stale = activate_integration(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition_id.clone(),
                expected_fingerprint: McpTrustFingerprint("f".repeat(64)),
            },
        )
        .await;
        assert!(matches!(
            stale,
            Err(McpCommandError::Core(CoreError::InvalidInput(_)))
        ));

        let activated = activate_integration(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition_id.clone(),
                expected_fingerprint: created
                    .integration
                    .definition
                    .trust
                    .current_fingerprint
                    .clone(),
            },
        )
        .await
        .expect("activate integration");
        assert!(activated.changed);
        assert!(activated.integration.definition.enabled);

        let disabled = disable_integration(
            &repo,
            DisableMcpIntegrationInput {
                definition_id: definition_id.clone(),
            },
        )
        .await
        .expect("disable integration");
        assert!(disabled.changed);
        assert!(!disabled.integration.definition.enabled);
        assert!(
            !disable_integration(&repo, DisableMcpIntegrationInput { definition_id })
                .await
                .expect("disable again")
                .changed
        );
    }

    #[tokio::test]
    async fn tool_policy_defaults_to_ask_and_persists_only_explicit_overrides() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let created = create_integration(&repo, &credentials, create_input("token"))
            .await
            .expect("create integration");
        let definition_id = created.integration.definition.id;

        let denied = set_tool_policy(
            &repo,
            SetMcpToolPolicyInput {
                definition_id: definition_id.clone(),
                tool_name: "payments.refund".to_string(),
                decision: McpToolPolicyDecision::Deny,
            },
        )
        .await
        .expect("deny tool");
        assert_eq!(denied.integration.tool_policies.len(), 1);
        assert_eq!(
            denied.integration.tool_policies[0].decision,
            McpToolPolicyDecision::Deny
        );

        let reset = set_tool_policy(
            &repo,
            SetMcpToolPolicyInput {
                definition_id,
                tool_name: "payments.refund".to_string(),
                decision: McpToolPolicyDecision::Ask,
            },
        )
        .await
        .expect("reset tool");
        assert!(reset.integration.tool_policies.is_empty());
    }

    #[tokio::test]
    async fn removal_deletes_credentials_only_when_explicitly_requested() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let kept = create_integration(&repo, &credentials, create_input("kept"))
            .await
            .expect("create kept integration");
        let kept_reference = kept.integration.definition.secret_refs[0]
            .secret_ref
            .clone();
        let removed = remove_integration(
            &repo,
            &credentials,
            RemoveMcpIntegrationInput {
                definition_id: kept.integration.definition.id,
                delete_credentials: false,
            },
        )
        .await
        .expect("remove and keep credential");
        assert!(removed.removed);
        assert_eq!(removed.deleted_credentials, 0);
        assert!(credentials
            .resolve_secret(&kept_reference)
            .await
            .expect("resolve kept credential")
            .is_some());

        let deleted = create_integration(&repo, &credentials, create_input("deleted"))
            .await
            .expect("create deleted integration");
        let deleted_reference = deleted.integration.definition.secret_refs[0]
            .secret_ref
            .clone();
        let removed = remove_integration(
            &repo,
            &credentials,
            RemoveMcpIntegrationInput {
                definition_id: deleted.integration.definition.id,
                delete_credentials: true,
            },
        )
        .await
        .expect("remove and delete credential");
        assert!(removed.removed);
        assert_eq!(removed.deleted_credentials, 1);
        assert!(credentials
            .resolve_secret(&deleted_reference)
            .await
            .expect("resolve deleted credential")
            .is_none());
    }

    #[tokio::test]
    async fn credential_deletion_failure_keeps_the_definition_disabled_for_retry() {
        let repo = repo();
        let credentials = TestCredentialStore::default();
        let created = create_integration(&repo, &credentials, create_input("protected"))
            .await
            .expect("create integration");
        let definition_id = created.integration.definition.id.clone();
        activate_integration(
            &repo,
            ActivateMcpDefinitionInput {
                definition_id: definition_id.clone(),
                expected_fingerprint: created.integration.definition.trust.current_fingerprint,
            },
        )
        .await
        .expect("activate integration");

        credentials.fail_deletes.store(true, Ordering::Relaxed);
        let removal = remove_integration(
            &repo,
            &credentials,
            RemoveMcpIntegrationInput {
                definition_id: definition_id.clone(),
                delete_credentials: true,
            },
        )
        .await;
        assert!(matches!(
            removal,
            Err(McpCommandError::Credential(
                CredentialStoreError::AccessDenied
            ))
        ));

        let retained = repo
            .get_mcp_definition(&definition_id)
            .await
            .expect("load retained definition")
            .expect("definition retained");
        assert!(!retained.enabled);
    }
}
