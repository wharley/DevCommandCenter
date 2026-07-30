use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    path::Path,
    str,
};

use dcc_core::{
    domain::mcp::{McpDefinitionId, McpToolPolicyDecision},
    ports::{
        ProviderMcpSecret, ProviderMcpServerConfig, ProviderMcpToolPolicy, ProviderMcpTransport,
    },
    CoreError, Result,
};
use reqwest::{
    header::{HeaderName, HeaderValue},
    Url,
};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use uuid::Uuid;
use zeroize::Zeroize;

const MAX_SERVER_COUNT: usize = 32;
const MAX_SERVER_NAME_CHARS: usize = 64;
const MAX_ARGUMENT_COUNT: usize = 128;
const MAX_SECRET_COUNT: usize = 64;
const MAX_TOOL_COUNT: usize = 256;

pub(crate) const GROK_ACP_PROTOCOL_VERSION: u64 = 1;
pub(crate) const SUPPORTED_GROK_CLI_VERSION: &str = "0.2.101";
pub(crate) const GROK_MCP_RUNTIME_VERSION: &str = "grok-build@0.2.101+acp-v1";

pub(crate) type GrokMcpDefinitionMap = HashMap<String, McpDefinitionId>;
pub(crate) type GrokMcpToolPolicyMap =
    HashMap<McpDefinitionId, HashMap<String, McpToolPolicyDecision>>;

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: GrokSessionNewParams<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GrokSessionNewParams<'a> {
    cwd: &'a str,
    mcp_servers: Vec<GrokAcpMcpServer<'a>>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GrokAcpMcpServer<'a> {
    Stdio {
        name: String,
        command: &'a str,
        args: &'a [String],
        env: Vec<GrokAcpNameValue<'a>>,
    },
    Http {
        r#type: &'static str,
        name: String,
        url: &'a str,
        headers: Vec<GrokAcpNameValue<'a>>,
    },
}

#[derive(Serialize)]
struct GrokAcpNameValue<'a> {
    name: &'a str,
    value: &'a str,
}

/// One-shot ACP payload that may contain MCP credentials.
///
/// The payload is kept as bytes, has fixed redacted debug output, and is
/// zeroized when dropped.
struct SensitiveGrokAcpPayload(Vec<u8>);

impl SensitiveGrokAcpPayload {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SensitiveGrokAcpPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveGrokAcpPayload([REDACTED])")
    }
}

impl Drop for SensitiveGrokAcpPayload {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) struct PreparedGrokAcpSessionRequest {
    payload: SensitiveGrokAcpPayload,
    definitions_by_wire_name: GrokMcpDefinitionMap,
    tool_policies_by_definition: GrokMcpToolPolicyMap,
}

impl PreparedGrokAcpSessionRequest {
    pub(crate) fn definitions_by_wire_name(&self) -> &GrokMcpDefinitionMap {
        &self.definitions_by_wire_name
    }

    pub(crate) fn tool_policies_by_definition(&self) -> &GrokMcpToolPolicyMap {
        &self.tool_policies_by_definition
    }

    pub(crate) async fn write_to<W>(&self, writer: &mut W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        writer
            .write_all(self.payload.as_bytes())
            .await
            .map_err(|_| grok_configuration_error())?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|_| grok_configuration_error())?;
        writer.flush().await.map_err(|_| grok_configuration_error())
    }
}

impl fmt::Debug for PreparedGrokAcpSessionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedGrokAcpSessionRequest([REDACTED])")
    }
}

pub(crate) fn prepare_grok_acp_session_request(
    request_id: u64,
    cwd: &str,
    servers: &[ProviderMcpServerConfig],
    initialize_result: &Value,
) -> Result<PreparedGrokAcpSessionRequest> {
    validate_runtime_contract(initialize_result, servers)?;
    validate_absolute_path(cwd)?;
    if servers.is_empty() || servers.len() > MAX_SERVER_COUNT {
        return Err(grok_configuration_error());
    }

    let namespace = Uuid::new_v4().simple().to_string();
    let mut seen_names = HashSet::with_capacity(servers.len());
    let mut seen_definition_ids = HashSet::with_capacity(servers.len());
    let mut definitions_by_wire_name = HashMap::with_capacity(servers.len());
    let mut tool_policies_by_definition = HashMap::with_capacity(servers.len());
    let mut wire_servers = Vec::with_capacity(servers.len());

    for (index, server) in servers.iter().enumerate() {
        validate_server(server, &mut seen_names, &mut seen_definition_ids)?;
        let wire_name = format!("dcc-{namespace}-{index}");
        definitions_by_wire_name.insert(wire_name.clone(), server.definition_id.clone());
        tool_policies_by_definition.insert(
            server.definition_id.clone(),
            collect_tool_policies(&server.tool_policies)?,
        );

        let wire_server = match &server.transport {
            ProviderMcpTransport::Stdio {
                executable,
                args,
                cwd,
                environment,
            } => {
                if executable.trim().is_empty()
                    || executable.contains('\0')
                    || !Path::new(executable).is_absolute()
                    || args.len() > MAX_ARGUMENT_COUNT
                    || args.iter().any(|argument| argument.contains('\0'))
                    || cwd.is_some()
                {
                    return Err(grok_configuration_error());
                }
                GrokAcpMcpServer::Stdio {
                    name: wire_name,
                    command: executable,
                    args,
                    env: collect_secret_pairs(
                        environment,
                        validate_environment_name,
                        validate_environment_value,
                    )?,
                }
            }
            ProviderMcpTransport::Http { url, headers } => {
                validate_http_url(url)?;
                GrokAcpMcpServer::Http {
                    r#type: "http",
                    name: wire_name,
                    url,
                    headers: collect_secret_pairs(
                        headers,
                        validate_header_name,
                        validate_header_value,
                    )?,
                }
            }
        };
        wire_servers.push(wire_server);
    }

    let payload = serde_json::to_vec(&RpcRequest {
        jsonrpc: "2.0",
        id: request_id,
        method: "session/new",
        params: GrokSessionNewParams {
            cwd,
            mcp_servers: wire_servers,
        },
    })
    .map(SensitiveGrokAcpPayload)
    .map_err(|_| grok_configuration_error())?;

    Ok(PreparedGrokAcpSessionRequest {
        payload,
        definitions_by_wire_name,
        tool_policies_by_definition,
    })
}

pub(crate) fn projection_version_for_grok_initialize(
    initialize_result: &Value,
) -> Option<&'static str> {
    (initialize_result
        .get("protocolVersion")
        .and_then(Value::as_u64)
        == Some(GROK_ACP_PROTOCOL_VERSION)
        && initialize_result
            .pointer("/_meta/grokShell")
            .and_then(Value::as_bool)
            == Some(true)
        && initialize_result
            .pointer("/_meta/agentVersion")
            .and_then(Value::as_str)
            == Some(SUPPORTED_GROK_CLI_VERSION))
    .then_some(GROK_MCP_RUNTIME_VERSION)
}

fn validate_runtime_contract(
    initialize_result: &Value,
    servers: &[ProviderMcpServerConfig],
) -> Result<()> {
    if projection_version_for_grok_initialize(initialize_result).is_none() {
        return Err(grok_runtime_error());
    }

    let http_supported = initialize_result
        .pointer("/agentCapabilities/mcpCapabilities/http")
        .and_then(Value::as_bool)
        == Some(true);
    if servers
        .iter()
        .any(|server| matches!(server.transport, ProviderMcpTransport::Http { .. }))
        && !http_supported
    {
        return Err(grok_runtime_error());
    }
    Ok(())
}

fn validate_absolute_path(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\0') || !Path::new(value).is_absolute() {
        return Err(grok_configuration_error());
    }
    Ok(())
}

fn validate_server(
    server: &ProviderMcpServerConfig,
    seen_names: &mut HashSet<String>,
    seen_definition_ids: &mut HashSet<String>,
) -> Result<()> {
    let name = server.server_name.as_str();
    if !valid_definition_name(name)
        || !seen_names.insert(name.to_string())
        || server.definition_id.0.trim().is_empty()
        || !seen_definition_ids.insert(server.definition_id.0.clone())
    {
        return Err(grok_configuration_error());
    }
    Ok(())
}

fn valid_definition_name(value: &str) -> bool {
    value.starts_with("dcc-")
        && value.chars().count() <= MAX_SERVER_NAME_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn collect_tool_policies(
    policies: &[ProviderMcpToolPolicy],
) -> Result<HashMap<String, McpToolPolicyDecision>> {
    if policies.len() > MAX_TOOL_COUNT {
        return Err(grok_configuration_error());
    }
    let mut result = HashMap::with_capacity(policies.len());
    for policy in policies {
        if policy.tool_name.is_empty()
            || policy.tool_name.len() > 128
            || !policy
                .tool_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
            || matches!(policy.decision, McpToolPolicyDecision::Ask)
            || result
                .insert(policy.tool_name.clone(), policy.decision.clone())
                .is_some()
        {
            return Err(grok_configuration_error());
        }
    }
    Ok(result)
}

fn collect_secret_pairs<'a>(
    secrets: &'a [ProviderMcpSecret],
    validate_name: fn(&str) -> bool,
    validate_value: fn(&str) -> bool,
) -> Result<Vec<GrokAcpNameValue<'a>>> {
    if secrets.len() > MAX_SECRET_COUNT {
        return Err(grok_configuration_error());
    }
    let mut seen_names = BTreeMap::new();
    for secret in secrets {
        if !validate_name(&secret.name) || seen_names.contains_key(secret.name.as_str()) {
            return Err(grok_configuration_error());
        }
        let value =
            str::from_utf8(secret.expose_secret()).map_err(|_| grok_configuration_error())?;
        if !validate_value(value) {
            return Err(grok_configuration_error());
        }
        seen_names.insert(secret.name.as_str(), value);
    }
    Ok(seen_names
        .into_iter()
        .map(|(name, value)| GrokAcpNameValue { name, value })
        .collect())
}

fn validate_environment_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn validate_environment_value(value: &str) -> bool {
    !value.contains('\0')
}

fn validate_header_name(value: &str) -> bool {
    HeaderName::from_bytes(value.as_bytes()).is_ok()
}

fn validate_header_value(value: &str) -> bool {
    HeaderValue::from_str(value).is_ok()
}

fn validate_http_url(value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|_| grok_configuration_error())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(grok_configuration_error());
    }
    Ok(())
}

fn grok_configuration_error() -> CoreError {
    CoreError::InvalidInput("Grok ACP MCP configuration is invalid".to_string())
}

fn grok_runtime_error() -> CoreError {
    CoreError::Provider(
        "Grok ACP MCP projection is unsupported by this runtime version or capability set"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use dcc_core::{
        domain::mcp::{McpDefinitionId, McpToolPolicyDecision},
        ports::{
            ProviderMcpSecret, ProviderMcpServerConfig, ProviderMcpToolPolicy,
            ProviderMcpTransport, SecretValue,
        },
    };
    use serde_json::json;

    use super::*;

    fn secret(name: &str, value: &str) -> ProviderMcpSecret {
        ProviderMcpSecret::new(
            name,
            SecretValue::new(value.as_bytes().to_vec()).expect("test secret"),
        )
    }

    fn initialize(http: bool) -> Value {
        json!({
            "protocolVersion": 1,
            "agentCapabilities": {
                "mcpCapabilities": {
                    "http": http,
                    "sse": true
                }
            },
            "_meta": {
                "grokShell": true,
                "agentVersion": SUPPORTED_GROK_CLI_VERSION
            }
        })
    }

    fn fixtures() -> Vec<ProviderMcpServerConfig> {
        vec![
            ProviderMcpServerConfig {
                definition_id: McpDefinitionId("stdio-fixture".to_string()),
                server_name: "dcc-stdio-fixture".to_string(),
                transport: ProviderMcpTransport::Stdio {
                    executable: if cfg!(windows) {
                        r"C:\fixtures\dcc-mcp-fixture.exe".to_string()
                    } else {
                        "/fixtures/dcc-mcp-fixture".to_string()
                    },
                    args: vec!["stdio".to_string()],
                    cwd: None,
                    environment: vec![secret("FIXTURE_TOKEN", "stdio-secret")],
                },
                oauth_state: None,
                tool_policies: vec![ProviderMcpToolPolicy {
                    tool_name: "fixture.mutate".to_string(),
                    decision: McpToolPolicyDecision::Deny,
                }],
            },
            ProviderMcpServerConfig {
                definition_id: McpDefinitionId("http-fixture".to_string()),
                server_name: "dcc-http-fixture".to_string(),
                transport: ProviderMcpTransport::Http {
                    url: "https://fixture.example/mcp".to_string(),
                    headers: vec![secret("Authorization", "http-secret")],
                },
                oauth_state: None,
                tool_policies: vec![ProviderMcpToolPolicy {
                    tool_name: "fixture.echo".to_string(),
                    decision: McpToolPolicyDecision::Allow,
                }],
            },
        ]
    }

    fn workspace() -> &'static str {
        if cfg!(windows) {
            r"C:\workspace"
        } else {
            "/workspace"
        }
    }

    #[tokio::test]
    async fn serializes_private_acp_transports_and_keeps_ownership_in_memory() {
        let request =
            prepare_grok_acp_session_request(7, workspace(), &fixtures(), &initialize(true))
                .expect("prepare Grok ACP request");

        assert_eq!(
            format!("{request:?}"),
            "PreparedGrokAcpSessionRequest([REDACTED])"
        );
        assert_eq!(request.definitions_by_wire_name().len(), 2);
        assert_eq!(
            request
                .tool_policies_by_definition()
                .get(&McpDefinitionId("stdio-fixture".to_string()))
                .and_then(|policies| policies.get("fixture.mutate")),
            Some(&McpToolPolicyDecision::Deny)
        );

        let mut output = Vec::new();
        request
            .write_to(&mut output)
            .await
            .expect("write Grok ACP request");
        let value: Value = serde_json::from_slice(&output).expect("valid JSON request");
        assert_eq!(value["method"], "session/new");
        assert_eq!(
            value["params"]["mcpServers"][0]["env"][0],
            json!({ "name": "FIXTURE_TOKEN", "value": "stdio-secret" })
        );
        assert_eq!(value["params"]["mcpServers"][1]["type"], "http");
        assert_eq!(
            value["params"]["mcpServers"][1]["headers"][0],
            json!({ "name": "Authorization", "value": "http-secret" })
        );

        for wire_name in request.definitions_by_wire_name().keys() {
            assert!(wire_name.starts_with("dcc-"));
            assert!(!wire_name.contains("fixture"));
        }
    }

    #[test]
    fn gates_projection_on_the_exact_audited_grok_handshake() {
        assert_eq!(
            projection_version_for_grok_initialize(&initialize(true)),
            Some(GROK_MCP_RUNTIME_VERSION)
        );

        let mut wrong_version = initialize(true);
        wrong_version["_meta"]["agentVersion"] = json!("0.2.102");
        assert_eq!(projection_version_for_grok_initialize(&wrong_version), None);

        let mut wrong_identity = initialize(true);
        wrong_identity["_meta"]["grokShell"] = json!(false);
        assert_eq!(
            projection_version_for_grok_initialize(&wrong_identity),
            None
        );
    }

    #[test]
    fn rejects_wrong_runtime_http_capability_and_ask_policy() {
        assert!(matches!(
            prepare_grok_acp_session_request(1, workspace(), &fixtures(), &initialize(false)),
            Err(CoreError::Provider(_))
        ));

        let mut ask_fixture = fixtures();
        ask_fixture[0].tool_policies[0].decision = McpToolPolicyDecision::Ask;
        assert!(
            prepare_grok_acp_session_request(1, workspace(), &ask_fixture, &initialize(true))
                .is_err()
        );
    }

    #[test]
    fn rejects_lossy_stdio_cwd_unsafe_secrets_and_urls() {
        let mut cwd_fixture = fixtures();
        let ProviderMcpTransport::Stdio { cwd, .. } = &mut cwd_fixture[0].transport else {
            panic!("stdio fixture");
        };
        *cwd = Some(workspace().to_string());
        assert!(
            prepare_grok_acp_session_request(1, workspace(), &cwd_fixture, &initialize(true))
                .is_err()
        );

        let mut duplicate_header = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut duplicate_header[1].transport else {
            panic!("HTTP fixture");
        };
        headers.push(secret("Authorization", "other-secret"));
        assert!(prepare_grok_acp_session_request(
            1,
            workspace(),
            &duplicate_header,
            &initialize(true)
        )
        .is_err());

        let mut unsafe_url = fixtures();
        let ProviderMcpTransport::Http { url, .. } = &mut unsafe_url[1].transport else {
            panic!("HTTP fixture");
        };
        *url = "https://user:secret@fixture.example/mcp".to_string();
        assert!(
            prepare_grok_acp_session_request(1, workspace(), &unsafe_url, &initialize(true))
                .is_err()
        );
    }

    #[test]
    fn rejects_duplicate_definition_identity() {
        let mut duplicate_definition = fixtures();
        duplicate_definition[1].definition_id = duplicate_definition[0].definition_id.clone();
        assert!(prepare_grok_acp_session_request(
            1,
            workspace(),
            &duplicate_definition,
            &initialize(true)
        )
        .is_err());
    }
}
