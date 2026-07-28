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

pub(crate) const CURSOR_ACP_PROTOCOL_VERSION: u64 = 1;
pub(crate) const SUPPORTED_CURSOR_CLI_VERSION: &str = "2026.07.23-e383d2b";
pub(crate) const CURSOR_MCP_RUNTIME_VERSION: &str = "cursor-agent@2026.07.23-e383d2b+acp-v1";

pub(crate) type CursorMcpDefinitionMap = HashMap<String, McpDefinitionId>;
pub(crate) type CursorMcpToolPolicyMap =
    HashMap<McpDefinitionId, HashMap<String, McpToolPolicyDecision>>;

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: CursorSessionNewParams<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CursorSessionNewParams<'a> {
    cwd: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    additional_directories: Vec<&'a str>,
    mcp_servers: Vec<CursorAcpMcpServer<'a>>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum CursorAcpMcpServer<'a> {
    Stdio {
        name: String,
        command: &'a str,
        args: &'a [String],
        env: Vec<CursorAcpNameValue<'a>>,
    },
    Http {
        r#type: &'static str,
        name: String,
        url: &'a str,
        headers: Vec<CursorAcpNameValue<'a>>,
    },
}

#[derive(Serialize)]
struct CursorAcpNameValue<'a> {
    name: &'a str,
    value: &'a str,
}

/// One-shot ACP payload that may contain MCP credentials.
///
/// It is never exposed as a `String` or `serde_json::Value`, its debug output
/// is fixed, and its allocation is zeroized after the session request is sent.
struct SensitiveCursorAcpPayload(Vec<u8>);

impl SensitiveCursorAcpPayload {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SensitiveCursorAcpPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveCursorAcpPayload([REDACTED])")
    }
}

impl Drop for SensitiveCursorAcpPayload {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) struct PreparedCursorAcpSessionRequest {
    payload: SensitiveCursorAcpPayload,
    definitions_by_wire_name: CursorMcpDefinitionMap,
    tool_policies_by_definition: CursorMcpToolPolicyMap,
}

impl PreparedCursorAcpSessionRequest {
    pub(crate) fn definitions_by_wire_name(&self) -> &CursorMcpDefinitionMap {
        &self.definitions_by_wire_name
    }

    pub(crate) fn tool_policies_by_definition(&self) -> &CursorMcpToolPolicyMap {
        &self.tool_policies_by_definition
    }

    pub(crate) async fn write_to<W>(&self, writer: &mut W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        writer
            .write_all(self.payload.as_bytes())
            .await
            .map_err(|_| cursor_configuration_error())?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|_| cursor_configuration_error())?;
        writer
            .flush()
            .await
            .map_err(|_| cursor_configuration_error())
    }
}

impl fmt::Debug for PreparedCursorAcpSessionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedCursorAcpSessionRequest([REDACTED])")
    }
}

pub(crate) fn prepare_cursor_acp_session_request(
    request_id: u64,
    cwd: &str,
    additional_working_directories: &[String],
    servers: &[ProviderMcpServerConfig],
    initialize_result: &Value,
    cursor_cli_version: &str,
) -> Result<PreparedCursorAcpSessionRequest> {
    validate_runtime_contract(initialize_result, cursor_cli_version, servers)?;
    validate_workspace_roots(cwd, additional_working_directories, initialize_result)?;
    if servers.is_empty() || servers.len() > MAX_SERVER_COUNT {
        return Err(cursor_configuration_error());
    }

    let namespace = Uuid::new_v4().simple().to_string();
    let mut seen_names = HashSet::with_capacity(servers.len());
    let mut definitions_by_wire_name = HashMap::with_capacity(servers.len());
    let mut tool_policies_by_definition = HashMap::with_capacity(servers.len());
    let mut wire_servers = Vec::with_capacity(servers.len());

    for (index, server) in servers.iter().enumerate() {
        validate_server(server, &mut seen_names)?;
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
                    return Err(cursor_configuration_error());
                }
                CursorAcpMcpServer::Stdio {
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
                CursorAcpMcpServer::Http {
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

    let additional_directories = additional_working_directories
        .iter()
        .map(String::as_str)
        .collect();
    let payload = serde_json::to_vec(&RpcRequest {
        jsonrpc: "2.0",
        id: request_id,
        method: "session/new",
        params: CursorSessionNewParams {
            cwd,
            additional_directories,
            mcp_servers: wire_servers,
        },
    })
    .map(SensitiveCursorAcpPayload)
    .map_err(|_| cursor_configuration_error())?;

    Ok(PreparedCursorAcpSessionRequest {
        payload,
        definitions_by_wire_name,
        tool_policies_by_definition,
    })
}

pub(crate) fn parse_cursor_cli_version_output(output: &str) -> Option<&str> {
    let version = output.trim();
    (!version.is_empty()
        && !version.contains(char::is_whitespace)
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')))
    .then_some(version)
}

pub(crate) fn projection_version_for_cursor_output(output: &str) -> Option<&'static str> {
    (parse_cursor_cli_version_output(output) == Some(SUPPORTED_CURSOR_CLI_VERSION))
        .then_some(CURSOR_MCP_RUNTIME_VERSION)
}

fn validate_runtime_contract(
    initialize_result: &Value,
    cursor_cli_version: &str,
    servers: &[ProviderMcpServerConfig],
) -> Result<()> {
    if cursor_cli_version != SUPPORTED_CURSOR_CLI_VERSION
        || initialize_result
            .get("protocolVersion")
            .and_then(Value::as_u64)
            != Some(CURSOR_ACP_PROTOCOL_VERSION)
    {
        return Err(cursor_runtime_error());
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
        return Err(cursor_runtime_error());
    }
    Ok(())
}

fn validate_workspace_roots(
    cwd: &str,
    additional_working_directories: &[String],
    initialize_result: &Value,
) -> Result<()> {
    if cwd.trim().is_empty() || !Path::new(cwd).is_absolute() || cwd.contains('\0') {
        return Err(cursor_configuration_error());
    }
    let additional_supported = initialize_result
        .pointer("/agentCapabilities/sessionCapabilities/additionalDirectories")
        .is_some();
    if !additional_working_directories.is_empty() && !additional_supported {
        return Err(cursor_runtime_error());
    }
    let mut roots = HashSet::with_capacity(additional_working_directories.len());
    for root in additional_working_directories {
        if root.trim().is_empty()
            || root.contains('\0')
            || !Path::new(root).is_absolute()
            || root == cwd
            || !roots.insert(root)
        {
            return Err(cursor_configuration_error());
        }
    }
    Ok(())
}

fn validate_server(
    server: &ProviderMcpServerConfig,
    seen_names: &mut HashSet<String>,
) -> Result<()> {
    let name = server.server_name.as_str();
    let valid_name = name.starts_with("dcc-")
        && name.chars().count() <= MAX_SERVER_NAME_CHARS
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid_name
        || !seen_names.insert(name.to_string())
        || server.definition_id.0.trim().is_empty()
    {
        return Err(cursor_configuration_error());
    }
    Ok(())
}

fn collect_tool_policies(
    policies: &[ProviderMcpToolPolicy],
) -> Result<HashMap<String, McpToolPolicyDecision>> {
    if policies.len() > MAX_TOOL_COUNT {
        return Err(cursor_configuration_error());
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
            return Err(cursor_configuration_error());
        }
    }
    Ok(result)
}

fn collect_secret_pairs<'a>(
    secrets: &'a [ProviderMcpSecret],
    validate_name: fn(&str) -> bool,
    validate_value: fn(&str) -> bool,
) -> Result<Vec<CursorAcpNameValue<'a>>> {
    if secrets.len() > MAX_SECRET_COUNT {
        return Err(cursor_configuration_error());
    }
    let mut seen_names = BTreeMap::new();
    for secret in secrets {
        if !validate_name(&secret.name) || seen_names.contains_key(secret.name.as_str()) {
            return Err(cursor_configuration_error());
        }
        let value =
            str::from_utf8(secret.expose_secret()).map_err(|_| cursor_configuration_error())?;
        if !validate_value(value) {
            return Err(cursor_configuration_error());
        }
        seen_names.insert(secret.name.as_str(), value);
    }
    Ok(seen_names
        .into_iter()
        .map(|(name, value)| CursorAcpNameValue { name, value })
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
    let url = Url::parse(value).map_err(|_| cursor_configuration_error())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(cursor_configuration_error());
    }
    Ok(())
}

fn cursor_configuration_error() -> CoreError {
    CoreError::InvalidInput("Cursor ACP MCP configuration is invalid".to_string())
}

fn cursor_runtime_error() -> CoreError {
    CoreError::Provider(
        "Cursor ACP MCP projection is unsupported by this runtime version or capability set"
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

    fn initialize(http: bool, additional_directories: bool) -> Value {
        let mut value = json!({
            "protocolVersion": 1,
            "agentCapabilities": {
                "mcpCapabilities": {
                    "http": http,
                    "sse": true
                },
                "sessionCapabilities": {}
            }
        });
        if additional_directories {
            value["agentCapabilities"]["sessionCapabilities"]["additionalDirectories"] = json!({});
        }
        value
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
                tool_policies: vec![ProviderMcpToolPolicy {
                    tool_name: "fixture.echo".to_string(),
                    decision: McpToolPolicyDecision::Allow,
                }],
            },
        ]
    }

    #[tokio::test]
    async fn serializes_acp_v1_transports_without_exposing_debug_secrets() {
        let request = prepare_cursor_acp_session_request(
            7,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[],
            &fixtures(),
            &initialize(true, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .expect("prepare Cursor ACP request");

        assert_eq!(
            format!("{request:?}"),
            "PreparedCursorAcpSessionRequest([REDACTED])"
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
            .expect("write Cursor ACP request");
        let value: Value = serde_json::from_slice(&output).expect("valid JSON request");
        assert_eq!(value["method"], "session/new");
        assert_eq!(
            value["params"]["mcpServers"][0]["command"],
            if cfg!(windows) {
                json!(r"C:\fixtures\dcc-mcp-fixture.exe")
            } else {
                json!("/fixtures/dcc-mcp-fixture")
            }
        );
        assert_eq!(
            value["params"]["mcpServers"][0]["env"][0],
            json!({ "name": "FIXTURE_TOKEN", "value": "stdio-secret" })
        );
        assert_eq!(value["params"]["mcpServers"][1]["type"], "http");
        assert_eq!(
            value["params"]["mcpServers"][1]["headers"][0],
            json!({ "name": "Authorization", "value": "http-secret" })
        );
        assert!(value["params"]["mcpServers"][0]
            .get("toolPolicies")
            .is_none());
    }

    #[test]
    fn gates_projection_on_the_exact_audited_cursor_version() {
        assert_eq!(
            projection_version_for_cursor_output("2026.07.23-e383d2b\n"),
            Some(CURSOR_MCP_RUNTIME_VERSION)
        );
        assert_eq!(
            projection_version_for_cursor_output("2026.07.24-unknown"),
            None
        );
        assert_eq!(projection_version_for_cursor_output("agent 1.0"), None);
        assert_eq!(parse_cursor_cli_version_output(""), None);
    }

    #[test]
    fn rejects_http_without_the_advertised_acp_capability() {
        let error = prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[],
            &fixtures(),
            &initialize(false, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .expect_err("HTTP capability is required");
        assert!(matches!(error, CoreError::Provider(_)));
    }

    #[test]
    fn rejects_unadvertised_or_invalid_additional_roots() {
        let root = if cfg!(windows) {
            r"C:\additional".to_string()
        } else {
            "/additional".to_string()
        };
        let unsupported = prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            std::slice::from_ref(&root),
            &fixtures(),
            &initialize(true, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .expect_err("additional roots require an advertised capability");
        assert!(matches!(unsupported, CoreError::Provider(_)));

        prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[root],
            &fixtures(),
            &initialize(true, true),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .expect("advertised absolute additional root");
    }

    #[test]
    fn rejects_per_server_cwd_and_ask_policy_overrides() {
        let mut cwd_fixture = fixtures();
        let ProviderMcpTransport::Stdio { cwd, .. } = &mut cwd_fixture[0].transport else {
            panic!("stdio fixture");
        };
        *cwd = Some(if cfg!(windows) {
            r"C:\server".to_string()
        } else {
            "/server".to_string()
        });
        assert!(prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[],
            &cwd_fixture,
            &initialize(true, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .is_err());

        let mut ask_fixture = fixtures();
        ask_fixture[0].tool_policies[0].decision = McpToolPolicyDecision::Ask;
        assert!(prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[],
            &ask_fixture,
            &initialize(true, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .is_err());
    }

    #[test]
    fn rejects_unsafe_urls_names_and_duplicate_secret_targets() {
        let mut unsafe_url = fixtures();
        let ProviderMcpTransport::Http { url, .. } = &mut unsafe_url[1].transport else {
            panic!("HTTP fixture");
        };
        *url = "https://user:secret@fixture.example/mcp".to_string();
        assert!(prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[],
            &unsafe_url,
            &initialize(true, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .is_err());

        let mut unsafe_name = fixtures();
        unsafe_name[0].server_name = "user-owned".to_string();
        assert!(prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[],
            &unsafe_name,
            &initialize(true, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .is_err());

        let mut duplicate_secret = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut duplicate_secret[1].transport else {
            panic!("HTTP fixture");
        };
        headers.push(secret("Authorization", "second-secret"));
        assert!(prepare_cursor_acp_session_request(
            1,
            if cfg!(windows) {
                r"C:\workspace"
            } else {
                "/workspace"
            },
            &[],
            &duplicate_secret,
            &initialize(true, false),
            SUPPORTED_CURSOR_CLI_VERSION,
        )
        .is_err());
    }
}
