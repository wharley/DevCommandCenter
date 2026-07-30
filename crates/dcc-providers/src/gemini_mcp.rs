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

pub(crate) const GEMINI_ACP_PROTOCOL_VERSION: u64 = 1;
pub(crate) const SUPPORTED_GEMINI_CLI_VERSION: &str = "0.32.1";
pub(crate) const GEMINI_MCP_RUNTIME_VERSION: &str = "gemini-cli@0.32.1+experimental-acp-v1";

pub(crate) type GeminiMcpDefinitionMap = HashMap<String, McpDefinitionId>;
pub(crate) type GeminiMcpToolPolicyMap =
    HashMap<McpDefinitionId, HashMap<String, McpToolPolicyDecision>>;

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: GeminiSessionNewParams<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiSessionNewParams<'a> {
    cwd: &'a str,
    mcp_servers: Vec<GeminiAcpMcpServer<'a>>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GeminiAcpMcpServer<'a> {
    Stdio {
        name: String,
        command: &'a str,
        args: &'a [String],
        env: Vec<GeminiAcpNameValue<'a>>,
    },
    Http {
        r#type: &'static str,
        name: String,
        url: &'a str,
        headers: Vec<GeminiAcpNameValue<'a>>,
    },
}

#[derive(Serialize)]
struct GeminiAcpNameValue<'a> {
    name: &'a str,
    value: &'a str,
}

struct SensitiveGeminiAcpPayload(Vec<u8>);

impl SensitiveGeminiAcpPayload {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SensitiveGeminiAcpPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveGeminiAcpPayload([REDACTED])")
    }
}

impl Drop for SensitiveGeminiAcpPayload {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) struct PreparedGeminiAcpSessionRequest {
    payload: SensitiveGeminiAcpPayload,
    definitions_by_wire_name: GeminiMcpDefinitionMap,
    tool_policies_by_definition: GeminiMcpToolPolicyMap,
    allowed_server_names: Vec<String>,
}

impl PreparedGeminiAcpSessionRequest {
    pub(crate) fn definitions_by_wire_name(&self) -> &GeminiMcpDefinitionMap {
        &self.definitions_by_wire_name
    }

    pub(crate) fn tool_policies_by_definition(&self) -> &GeminiMcpToolPolicyMap {
        &self.tool_policies_by_definition
    }

    pub(crate) fn allowed_server_names(&self) -> &[String] {
        &self.allowed_server_names
    }

    pub(crate) async fn write_to<W>(&self, writer: &mut W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        writer
            .write_all(self.payload.as_bytes())
            .await
            .map_err(|_| gemini_configuration_error())?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|_| gemini_configuration_error())?;
        writer
            .flush()
            .await
            .map_err(|_| gemini_configuration_error())
    }
}

impl fmt::Debug for PreparedGeminiAcpSessionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedGeminiAcpSessionRequest([REDACTED])")
    }
}

pub(crate) fn prepare_gemini_acp_session_request(
    request_id: u64,
    cwd: &str,
    servers: &[ProviderMcpServerConfig],
    initialize_result: &Value,
) -> Result<PreparedGeminiAcpSessionRequest> {
    validate_runtime_contract(initialize_result, servers)?;
    validate_absolute_path(cwd)?;
    if servers.is_empty() || servers.len() > MAX_SERVER_COUNT {
        return Err(gemini_configuration_error());
    }

    let namespace = Uuid::new_v4().simple().to_string();
    let mut seen_names = HashSet::with_capacity(servers.len());
    let mut seen_definition_ids = HashSet::with_capacity(servers.len());
    let mut definitions_by_wire_name = HashMap::with_capacity(servers.len());
    let mut tool_policies_by_definition = HashMap::with_capacity(servers.len());
    let mut allowed_server_names = Vec::with_capacity(servers.len());
    let mut wire_servers = Vec::with_capacity(servers.len());

    for (index, server) in servers.iter().enumerate() {
        validate_server(server, &mut seen_names, &mut seen_definition_ids)?;
        let wire_name = format!("dcc-{namespace}-{index}");
        definitions_by_wire_name.insert(wire_name.clone(), server.definition_id.clone());
        tool_policies_by_definition.insert(
            server.definition_id.clone(),
            collect_tool_policies(&server.tool_policies)?,
        );
        allowed_server_names.push(wire_name.clone());

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
                    return Err(gemini_configuration_error());
                }
                GeminiAcpMcpServer::Stdio {
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
                GeminiAcpMcpServer::Http {
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
        params: GeminiSessionNewParams {
            cwd,
            mcp_servers: wire_servers,
        },
    })
    .map(SensitiveGeminiAcpPayload)
    .map_err(|_| gemini_configuration_error())?;

    Ok(PreparedGeminiAcpSessionRequest {
        payload,
        definitions_by_wire_name,
        tool_policies_by_definition,
        allowed_server_names,
    })
}

pub(crate) fn gemini_acp_launch_args(
    cwd: &str,
    additional_working_directories: &[String],
    policy_path: &str,
    allowed_server_names: &[String],
) -> Result<Vec<String>> {
    validate_absolute_path(cwd)?;
    validate_absolute_path(policy_path)?;
    if allowed_server_names.is_empty() || allowed_server_names.len() > MAX_SERVER_COUNT {
        return Err(gemini_configuration_error());
    }

    let mut roots = HashSet::with_capacity(additional_working_directories.len());
    for root in additional_working_directories {
        validate_absolute_path(root)?;
        if root == cwd || !roots.insert(root) {
            return Err(gemini_configuration_error());
        }
    }

    let mut names = HashSet::with_capacity(allowed_server_names.len());
    for name in allowed_server_names {
        if !valid_wire_name(name) || !names.insert(name) {
            return Err(gemini_configuration_error());
        }
    }

    let mut args = vec![
        "--experimental-acp".to_string(),
        "--approval-mode".to_string(),
        "default".to_string(),
        "--allowed-tools".to_string(),
        String::new(),
        "--policy".to_string(),
        policy_path.to_string(),
    ];
    for name in allowed_server_names {
        args.push("--allowed-mcp-server-names".to_string());
        args.push(name.clone());
    }
    for root in additional_working_directories {
        args.push("--include-directories".to_string());
        args.push(root.clone());
    }
    Ok(args)
}

pub(crate) fn parse_gemini_cli_version_output(output: &str) -> Option<&str> {
    let version = output.trim();
    (!version.is_empty()
        && !version.contains(char::is_whitespace)
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')))
    .then_some(version)
}

pub(crate) fn projection_version_for_gemini_output(output: &str) -> Option<&'static str> {
    (parse_gemini_cli_version_output(output) == Some(SUPPORTED_GEMINI_CLI_VERSION))
        .then_some(GEMINI_MCP_RUNTIME_VERSION)
}

fn validate_runtime_contract(
    initialize_result: &Value,
    servers: &[ProviderMcpServerConfig],
) -> Result<()> {
    let supported_runtime = initialize_result
        .get("protocolVersion")
        .and_then(Value::as_u64)
        == Some(GEMINI_ACP_PROTOCOL_VERSION)
        && initialize_result
            .pointer("/agentInfo/name")
            .and_then(Value::as_str)
            == Some("gemini-cli")
        && initialize_result
            .pointer("/agentInfo/version")
            .and_then(Value::as_str)
            == Some(SUPPORTED_GEMINI_CLI_VERSION);
    if !supported_runtime {
        return Err(gemini_runtime_error());
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
        return Err(gemini_runtime_error());
    }
    Ok(())
}

fn validate_absolute_path(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\0') || !Path::new(value).is_absolute() {
        return Err(gemini_configuration_error());
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
        return Err(gemini_configuration_error());
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

fn valid_wire_name(value: &str) -> bool {
    let Some(value) = value.strip_prefix("dcc-") else {
        return false;
    };
    let Some((namespace, index)) = value.split_once('-') else {
        return false;
    };
    namespace.len() == 32
        && namespace.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !index.is_empty()
        && index.bytes().all(|byte| byte.is_ascii_digit())
        && index
            .parse::<usize>()
            .is_ok_and(|index| index < MAX_SERVER_COUNT)
}

fn collect_tool_policies(
    policies: &[ProviderMcpToolPolicy],
) -> Result<HashMap<String, McpToolPolicyDecision>> {
    if policies.len() > MAX_TOOL_COUNT {
        return Err(gemini_configuration_error());
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
            return Err(gemini_configuration_error());
        }
    }
    Ok(result)
}

fn collect_secret_pairs<'a>(
    secrets: &'a [ProviderMcpSecret],
    validate_name: fn(&str) -> bool,
    validate_value: fn(&str) -> bool,
) -> Result<Vec<GeminiAcpNameValue<'a>>> {
    if secrets.len() > MAX_SECRET_COUNT {
        return Err(gemini_configuration_error());
    }
    let mut seen_names = BTreeMap::new();
    for secret in secrets {
        if !validate_name(&secret.name) || seen_names.contains_key(secret.name.as_str()) {
            return Err(gemini_configuration_error());
        }
        let value =
            str::from_utf8(secret.expose_secret()).map_err(|_| gemini_configuration_error())?;
        if !validate_value(value) {
            return Err(gemini_configuration_error());
        }
        seen_names.insert(secret.name.as_str(), value);
    }
    Ok(seen_names
        .into_iter()
        .map(|(name, value)| GeminiAcpNameValue { name, value })
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
    let url = Url::parse(value).map_err(|_| gemini_configuration_error())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(gemini_configuration_error());
    }
    Ok(())
}

fn gemini_configuration_error() -> CoreError {
    CoreError::InvalidInput("Gemini ACP MCP configuration is invalid".to_string())
}

fn gemini_runtime_error() -> CoreError {
    CoreError::Provider(
        "Gemini ACP MCP projection is unsupported by this runtime version or capability set"
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
            "agentInfo": {
                "name": "gemini-cli",
                "version": SUPPORTED_GEMINI_CLI_VERSION
            },
            "agentCapabilities": {
                "mcpCapabilities": {
                    "http": http,
                    "sse": true
                }
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
    async fn serializes_private_acp_transports_and_isolating_launch_args() {
        let request =
            prepare_gemini_acp_session_request(7, workspace(), &fixtures(), &initialize(true))
                .expect("prepare Gemini ACP request");

        assert_eq!(
            format!("{request:?}"),
            "PreparedGeminiAcpSessionRequest([REDACTED])"
        );
        assert_eq!(request.definitions_by_wire_name().len(), 2);
        assert_eq!(
            request
                .tool_policies_by_definition()
                .get(&McpDefinitionId("stdio-fixture".to_string()))
                .and_then(|policies| policies.get("fixture.mutate")),
            Some(&McpToolPolicyDecision::Deny)
        );

        let policy_path = if cfg!(windows) {
            r"C:\dcc\policy"
        } else {
            "/dcc/policy"
        };
        let additional_root = if cfg!(windows) {
            r"C:\additional".to_string()
        } else {
            "/additional".to_string()
        };
        let args = gemini_acp_launch_args(
            workspace(),
            &[additional_root],
            policy_path,
            request.allowed_server_names(),
        )
        .expect("isolating launch args");
        assert_eq!(args[0], "--experimental-acp");
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "default"]));
        assert!(args.windows(2).any(|pair| pair == ["--allowed-tools", ""]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--policy", policy_path]));
        for name in request.allowed_server_names() {
            assert!(args
                .windows(2)
                .any(|pair| pair == ["--allowed-mcp-server-names", name]));
        }
        assert!(!args.join(" ").contains("stdio-secret"));
        assert!(!args.join(" ").contains("http-secret"));

        let mut output = Vec::new();
        request
            .write_to(&mut output)
            .await
            .expect("write Gemini ACP request");
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
    }

    #[test]
    fn gates_projection_on_the_exact_audited_gemini_version() {
        assert_eq!(
            projection_version_for_gemini_output("0.32.1\n"),
            Some(GEMINI_MCP_RUNTIME_VERSION)
        );
        assert_eq!(projection_version_for_gemini_output("0.32.2"), None);
        assert_eq!(projection_version_for_gemini_output("gemini 0.32.1"), None);
        assert_eq!(parse_gemini_cli_version_output(""), None);
    }

    #[test]
    fn rejects_wrong_agent_version_or_http_capability() {
        let mut wrong_agent = initialize(true);
        wrong_agent["agentInfo"]["name"] = json!("other-agent");
        assert!(matches!(
            prepare_gemini_acp_session_request(1, workspace(), &fixtures(), &wrong_agent),
            Err(CoreError::Provider(_))
        ));

        let mut wrong_version = initialize(true);
        wrong_version["agentInfo"]["version"] = json!("0.32.2");
        assert!(matches!(
            prepare_gemini_acp_session_request(1, workspace(), &fixtures(), &wrong_version),
            Err(CoreError::Provider(_))
        ));

        assert!(matches!(
            prepare_gemini_acp_session_request(1, workspace(), &fixtures(), &initialize(false)),
            Err(CoreError::Provider(_))
        ));
    }

    #[test]
    fn rejects_lossy_cwd_ask_policy_and_unsafe_secrets() {
        let mut cwd_fixture = fixtures();
        let ProviderMcpTransport::Stdio { cwd, .. } = &mut cwd_fixture[0].transport else {
            panic!("stdio fixture");
        };
        *cwd = Some(workspace().to_string());
        assert!(prepare_gemini_acp_session_request(
            1,
            workspace(),
            &cwd_fixture,
            &initialize(true)
        )
        .is_err());

        let mut ask_fixture = fixtures();
        ask_fixture[0].tool_policies[0].decision = McpToolPolicyDecision::Ask;
        assert!(prepare_gemini_acp_session_request(
            1,
            workspace(),
            &ask_fixture,
            &initialize(true)
        )
        .is_err());

        let mut duplicate_header = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut duplicate_header[1].transport else {
            panic!("HTTP fixture");
        };
        headers.push(secret("Authorization", "other-secret"));
        assert!(prepare_gemini_acp_session_request(
            1,
            workspace(),
            &duplicate_header,
            &initialize(true)
        )
        .is_err());
    }

    #[test]
    fn rejects_unsafe_urls_and_ambiguous_launch_scope() {
        let mut unsafe_url = fixtures();
        let ProviderMcpTransport::Http { url, .. } = &mut unsafe_url[1].transport else {
            panic!("HTTP fixture");
        };
        *url = "https://user:secret@fixture.example/mcp".to_string();
        assert!(
            prepare_gemini_acp_session_request(1, workspace(), &unsafe_url, &initialize(true))
                .is_err()
        );

        let policy_path = if cfg!(windows) {
            r"C:\dcc\policy"
        } else {
            "/dcc/policy"
        };
        assert!(gemini_acp_launch_args(
            workspace(),
            &[],
            policy_path,
            &["user-server".to_string()]
        )
        .is_err());
        assert!(gemini_acp_launch_args(
            workspace(),
            &[workspace().to_string()],
            policy_path,
            &["dcc-0123456789abcdef0123456789abcdef-0".to_string()]
        )
        .is_err());

        let mut duplicate_definition = fixtures();
        duplicate_definition[1].definition_id = duplicate_definition[0].definition_id.clone();
        assert!(prepare_gemini_acp_session_request(
            1,
            workspace(),
            &duplicate_definition,
            &initialize(true)
        )
        .is_err());
    }
}
