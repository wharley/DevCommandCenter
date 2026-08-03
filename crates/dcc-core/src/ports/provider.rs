use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    domain::mcp::{McpDefinitionId, McpToolPolicyDecision},
    domain::provider::{
        Capabilities, HealthStatus, ProviderAccountUsage, ProviderApprovalPolicy, ProviderEvent,
        ProviderId, SessionHandle,
    },
    domain::session::SessionId,
    domain::workspace::WorkspaceId,
    ports::credential_store::SecretValue,
    Result,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderMcpOauthStart {
    pub authorization_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SessionConfig {
    pub workspace_id: WorkspaceId,
    pub session_id: SessionId,
    pub model: Option<String>,
    pub working_directory: Option<String>,
    /// Additional isolated worktree roots authorized for this provider session.
    #[serde(default)]
    pub additional_working_directories: Vec<String>,
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
    /// Backend-only MCP projections resolved for this provider session.
    ///
    /// This field is intentionally absent from renderer and persistence
    /// contracts because it may contain credential values. Providers must
    /// deliver it over a bounded private channel and discard it after attach.
    #[serde(skip)]
    #[specta(skip)]
    pub mcp_servers: Vec<ProviderMcpServerConfig>,
}

#[derive(Clone, Debug)]
pub struct ProviderMcpServerConfig {
    pub definition_id: McpDefinitionId,
    /// Provider-local name owned by DCC. It must not collide with or replace
    /// user-configured provider entries.
    pub server_name: String,
    pub transport: ProviderMcpTransport,
    /// Optional adapter-owned remote OAuth state restored from the OS
    /// credential store. It is backend-only and must never enter renderer or
    /// persisted provider configuration contracts.
    pub oauth_state: Option<ProviderMcpOauthState>,
    /// Explicit overrides only. Missing tools always default to Ask.
    pub tool_policies: Vec<ProviderMcpToolPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMcpToolPolicy {
    pub tool_name: String,
    pub decision: McpToolPolicyDecision,
}

#[derive(Clone, Debug)]
pub enum ProviderMcpTransport {
    Stdio {
        executable: String,
        args: Vec<String>,
        /// Optional server working directory. Provider adapters must reject
        /// this explicitly when their documented transport cannot represent it.
        cwd: Option<String>,
        environment: Vec<ProviderMcpSecret>,
    },
    Http {
        url: String,
        headers: Vec<ProviderMcpSecret>,
    },
}

pub struct ProviderMcpSecret {
    pub name: String,
    value: SecretValue,
}

pub struct ProviderMcpOauthState {
    value: SecretValue,
}

impl ProviderMcpOauthState {
    pub fn new(value: SecretValue) -> Self {
        Self { value }
    }

    pub fn expose_secret(&self) -> &[u8] {
        self.value.expose_secret()
    }

    pub fn into_secret(self) -> SecretValue {
        self.value
    }
}

impl Clone for ProviderMcpOauthState {
    fn clone(&self) -> Self {
        Self {
            value: SecretValue::new(self.value.expose_secret().to_vec())
                .expect("an existing provider MCP OAuth state remains valid when cloned"),
        }
    }
}

impl std::fmt::Debug for ProviderMcpOauthState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProviderMcpOauthState([REDACTED])")
    }
}

#[derive(Debug)]
pub struct ProviderMcpOauthUpdate {
    pub definition_id: McpDefinitionId,
    /// `None` means the remote OAuth implementation invalidated its grant.
    pub state: Option<ProviderMcpOauthState>,
}

impl ProviderMcpSecret {
    pub fn new(name: impl Into<String>, value: SecretValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    pub fn expose_secret(&self) -> &[u8] {
        self.value.expose_secret()
    }
}

impl Clone for ProviderMcpSecret {
    fn clone(&self) -> Self {
        let value = SecretValue::new(self.value.expose_secret().to_vec())
            .expect("an existing provider MCP secret remains valid when cloned");
        Self {
            name: self.name.clone(),
            value,
        }
    }
}

impl std::fmt::Debug for ProviderMcpSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderMcpSecret")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeConfig {
    #[serde(default)]
    pub home_path: Option<String>,
    #[serde(default)]
    pub shadow_home_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTurnInput {
    pub prompt: String,
    #[serde(default)]
    pub tool_instructions: Option<String>,
    #[serde(default)]
    pub plan_mode: Option<bool>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub fast_mode: Option<bool>,
    #[serde(default)]
    pub approval_policy: Option<ProviderApprovalPolicy>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<ProviderUserInputOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputAnswer {
    pub question: String,
    pub answer: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputResponse {
    pub request_id: String,
    #[serde(default)]
    pub answers: Vec<ProviderUserInputAnswer>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPermissionRequest {
    pub request_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPermissionResponse {
    pub request_id: String,
    pub behavior: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum Input {
    Text(String),
    Turn(ProviderTurnInput),
    UserInputResponse(ProviderUserInputResponse),
    PermissionResponse(ProviderPermissionResponse),
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> Capabilities;
    /// Exact runtime version for this adapter's backend-only DCC MCP
    /// projection channel.
    ///
    /// `None` means this adapter must receive no DCC projection. A version is
    /// an internal wiring contract, not conformance evidence or a
    /// renderer-facing compatibility claim.
    fn dcc_mcp_projection_version(&self) -> Option<&str> {
        None
    }
    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle>;
    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()>;
    async fn steer(&self, _handle: &SessionHandle, _prompt: &str) -> Result<()> {
        Err(crate::CoreError::Provider(
            "This provider does not support steering an active turn".to_string(),
        ))
    }
    async fn start_mcp_oauth(
        &self,
        _handle: &SessionHandle,
        _definition_id: &McpDefinitionId,
    ) -> Result<ProviderMcpOauthStart> {
        Err(crate::CoreError::Provider(
            "MCP OAuth is not supported by this provider runtime".to_string(),
        ))
    }
    /// Drains adapter-owned OAuth grant updates captured on a backend-private
    /// channel. Provider-native OAuth implementations return no updates.
    async fn take_mcp_oauth_updates(
        &self,
        _handle: &SessionHandle,
    ) -> Result<Vec<ProviderMcpOauthUpdate>> {
        Ok(Vec::new())
    }
    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>>;
    async fn cancel(&self, handle: &SessionHandle) -> Result<()>;
    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle>;
    async fn healthcheck(&self) -> Result<HealthStatus>;
    async fn account_usage(
        &self,
        _runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<Option<ProviderAccountUsage>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::workspace::WorkspaceId;

    #[test]
    fn session_mcp_credentials_are_absent_from_serialized_and_debug_contracts() {
        let config = SessionConfig {
            workspace_id: WorkspaceId("workspace".to_string()),
            session_id: SessionId("session".to_string()),
            model: None,
            working_directory: Some("/workspace".to_string()),
            additional_working_directories: Vec::new(),
            provider_runtime: None,
            mcp_servers: vec![ProviderMcpServerConfig {
                definition_id: McpDefinitionId("fixture".to_string()),
                server_name: "dcc-fixture".to_string(),
                transport: ProviderMcpTransport::Http {
                    url: "https://example.com/mcp".to_string(),
                    headers: vec![ProviderMcpSecret::new(
                        "Authorization",
                        SecretValue::new(b"secret-canary".to_vec()).expect("test secret"),
                    )],
                },
                oauth_state: Some(ProviderMcpOauthState::new(
                    SecretValue::new(b"oauth-state-canary".to_vec()).expect("OAuth state"),
                )),
                tool_policies: Vec::new(),
            }],
        };

        let serialized = serde_json::to_string(&config).expect("serialize session config");
        assert!(!serialized.contains("mcp_servers"));
        assert!(!serialized.contains("secret-canary"));
        assert!(!serialized.contains("oauth-state-canary"));
        assert!(!format!("{config:?}").contains("secret-canary"));
        assert!(!format!("{config:?}").contains("oauth-state-canary"));
    }
}
