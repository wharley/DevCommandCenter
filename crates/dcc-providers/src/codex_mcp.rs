use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt, str,
};

use dcc_core::{
    domain::{
        mcp::{
            McpDefinitionId, McpErrorCategory, McpRuntimeError, McpRuntimeState, McpRuntimeStatus,
            McpToolSummary,
        },
        provider::ProviderId,
        session::SessionId,
    },
    ports::{ProviderMcpSecret, ProviderMcpServerConfig, ProviderMcpTransport},
    CoreError, Result,
};
use reqwest::{
    header::{HeaderName, HeaderValue},
    Url,
};
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use uuid::Uuid;
use zeroize::Zeroize;

const MAX_SERVER_COUNT: usize = 32;
const MAX_SERVER_NAME_CHARS: usize = 64;
const MAX_ARGUMENT_COUNT: usize = 128;
const MAX_SECRET_COUNT: usize = 64;
const MAX_STATUS_ITEM_COUNT: usize = 512;
const MAX_TOOL_COUNT: usize = 256;

pub(crate) const CODEX_MCP_RUNTIME_VERSION: &str = "codex-cli@0.145.0+app-server-protocol-v2";
pub(crate) const SUPPORTED_CODEX_CLI_VERSION: &str = "0.145.0";
pub(crate) type CodexMcpDefinitionMap = HashMap<String, McpDefinitionId>;

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: CodexThreadStartParams<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexThreadStartParams<'a> {
    cwd: &'a str,
    approval_policy: CodexMcpApprovalPolicy,
    sandbox: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime_workspace_roots: Option<Vec<&'a str>>,
    config: CodexThreadConfig<'a>,
}

#[derive(Serialize)]
struct CodexThreadConfig<'a> {
    mcp_servers: BTreeMap<String, CodexMcpServer<'a>>,
}

#[derive(Serialize)]
pub(crate) struct CodexMcpApprovalPolicy {
    granular: CodexMcpGranularApprovalPolicy,
}

#[derive(Serialize)]
struct CodexMcpGranularApprovalPolicy {
    sandbox_approval: bool,
    rules: bool,
    skill_approval: bool,
    request_permissions: bool,
    mcp_elicitations: bool,
}

pub(crate) const fn codex_mcp_approval_policy() -> CodexMcpApprovalPolicy {
    CodexMcpApprovalPolicy {
        granular: CodexMcpGranularApprovalPolicy {
            sandbox_approval: false,
            rules: false,
            skill_approval: false,
            request_permissions: false,
            mcp_elicitations: true,
        },
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum CodexMcpServer<'a> {
    Stdio {
        command: &'a str,
        args: &'a [String],
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<&'a str>,
        env: BTreeMap<&'a str, &'a str>,
        default_tools_approval_mode: &'static str,
    },
    Http {
        url: &'a str,
        http_headers: BTreeMap<&'a str, &'a str>,
        default_tools_approval_mode: &'static str,
    },
}

/// A one-shot JSON-RPC request that can contain MCP credentials.
///
/// It is deliberately never converted into `String` or `serde_json::Value`:
/// debug output is redacted and the allocation is zeroized after the request
/// has been written to the app-server stdin.
struct SensitiveRpcPayload(Vec<u8>);

impl SensitiveRpcPayload {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SensitiveRpcPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveRpcPayload([REDACTED])")
    }
}

impl Drop for SensitiveRpcPayload {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) struct PreparedCodexMcpThreadStartRequest {
    payload: SensitiveRpcPayload,
    definitions_by_wire_name: CodexMcpDefinitionMap,
}

impl PreparedCodexMcpThreadStartRequest {
    pub(crate) fn definitions_by_wire_name(&self) -> &CodexMcpDefinitionMap {
        &self.definitions_by_wire_name
    }

    pub(crate) async fn write_to<W>(&self, writer: &mut W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        writer
            .write_all(self.payload.as_bytes())
            .await
            .map_err(|_| {
                CoreError::Provider("failed to configure Codex MCP servers".to_string())
            })?;
        writer.write_all(b"\n").await.map_err(|_| {
            CoreError::Provider("failed to configure Codex MCP servers".to_string())
        })?;
        writer
            .flush()
            .await
            .map_err(|_| CoreError::Provider("failed to configure Codex MCP servers".to_string()))
    }
}

impl fmt::Debug for PreparedCodexMcpThreadStartRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedCodexMcpThreadStartRequest([REDACTED])")
    }
}

pub(crate) fn prepare_thread_start_request(
    request_id: u64,
    cwd: &str,
    additional_working_directories: &[String],
    servers: &[ProviderMcpServerConfig],
) -> Result<PreparedCodexMcpThreadStartRequest> {
    if cwd.trim().is_empty()
        || cwd.contains('\0')
        || additional_working_directories
            .iter()
            .any(|path| path.trim().is_empty() || path.contains('\0'))
        || servers.is_empty()
        || servers.len() > MAX_SERVER_COUNT
    {
        return Err(invalid_configuration());
    }

    let mut names = HashSet::with_capacity(servers.len());
    let mut definition_ids = HashSet::with_capacity(servers.len());
    let mut wire_servers = BTreeMap::new();
    let mut definitions_by_wire_name = HashMap::with_capacity(servers.len());
    let session_namespace = Uuid::new_v4().simple().to_string();
    for (index, server) in servers.iter().enumerate() {
        validate_server_name(&server.server_name)?;
        if !names.insert(server.server_name.as_str())
            || server.definition_id.0.trim().is_empty()
            || !definition_ids.insert(server.definition_id.0.as_str())
        {
            return Err(invalid_configuration());
        }

        let transport = match &server.transport {
            ProviderMcpTransport::Stdio {
                executable,
                args,
                cwd,
                environment,
            } => {
                if executable.trim().is_empty()
                    || executable.contains('\0')
                    || args.len() > MAX_ARGUMENT_COUNT
                    || args.iter().any(|argument| argument.contains('\0'))
                    || cwd
                        .as_deref()
                        .is_some_and(|path| path.trim().is_empty() || path.contains('\0'))
                {
                    return Err(invalid_configuration());
                }
                CodexMcpServer::Stdio {
                    command: executable,
                    args,
                    cwd: cwd.as_deref(),
                    env: collect_secret_map(
                        environment,
                        validate_environment_name,
                        validate_environment_value,
                    )?,
                    default_tools_approval_mode: "prompt",
                }
            }
            ProviderMcpTransport::Http { url, headers } => {
                validate_http_url(url)?;
                CodexMcpServer::Http {
                    url,
                    http_headers: collect_header_map(headers)?,
                    default_tools_approval_mode: "prompt",
                }
            }
        };

        let wire_name = format!("dcc-{session_namespace}-{index}");
        definitions_by_wire_name.insert(wire_name.clone(), server.definition_id.clone());
        wire_servers.insert(wire_name, transport);
    }

    let runtime_workspace_roots = if additional_working_directories.is_empty() {
        None
    } else {
        let mut roots = Vec::with_capacity(additional_working_directories.len() + 1);
        roots.push(cwd);
        roots.extend(additional_working_directories.iter().map(String::as_str));
        Some(roots)
    };
    let payload = serde_json::to_vec(&RpcRequest {
        jsonrpc: "2.0",
        id: request_id,
        method: "thread/start",
        params: CodexThreadStartParams {
            cwd,
            approval_policy: codex_mcp_approval_policy(),
            sandbox: "workspace-write",
            runtime_workspace_roots,
            config: CodexThreadConfig {
                mcp_servers: wire_servers,
            },
        },
    })
    .map(SensitiveRpcPayload)
    .map_err(|_| invalid_configuration())?;
    Ok(PreparedCodexMcpThreadStartRequest {
        payload,
        definitions_by_wire_name,
    })
}

pub(crate) fn initial_codex_mcp_status_snapshot(
    definitions_by_wire_name: &CodexMcpDefinitionMap,
    provider_id: &ProviderId,
    session_id: &SessionId,
) -> Vec<McpRuntimeStatus> {
    status_snapshot_for_all(
        definitions_by_wire_name,
        provider_id,
        session_id,
        McpRuntimeState::AttachingProvider,
        None,
    )
}

pub(crate) fn failed_codex_mcp_status_snapshot(
    definitions_by_wire_name: &CodexMcpDefinitionMap,
    provider_id: &ProviderId,
    session_id: &SessionId,
    category: McpErrorCategory,
    message: &'static str,
) -> Vec<McpRuntimeStatus> {
    status_snapshot_for_all(
        definitions_by_wire_name,
        provider_id,
        session_id,
        McpRuntimeState::Failed,
        Some((category, message)),
    )
}

pub(crate) fn parse_codex_mcp_status_snapshot(
    value: &serde_json::Value,
    definitions_by_wire_name: &CodexMcpDefinitionMap,
    provider_id: &ProviderId,
    session_id: &SessionId,
) -> Result<Vec<McpRuntimeStatus>> {
    let raw_servers = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(invalid_status_payload)?;
    if raw_servers.len() > MAX_STATUS_ITEM_COUNT {
        return Err(invalid_status_payload());
    }

    let mut known_servers = HashMap::with_capacity(definitions_by_wire_name.len());
    for raw_server in raw_servers {
        let name = raw_server
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid_status_payload)?;
        if definitions_by_wire_name.contains_key(name)
            && known_servers.insert(name, raw_server).is_some()
        {
            return Err(invalid_status_payload());
        }
    }

    let checked_at = chrono::Utc::now().to_rfc3339();
    let mut statuses = Vec::with_capacity(definitions_by_wire_name.len());
    for (wire_name, definition_id) in definitions_by_wire_name {
        let Some(raw_server) = known_servers.get(wire_name.as_str()) else {
            statuses.push(runtime_status(
                definition_id.clone(),
                provider_id,
                session_id,
                McpRuntimeState::AttachingProvider,
                Vec::new(),
                None,
                &checked_at,
            )?);
            continue;
        };

        let auth_status = raw_server
            .get("authStatus")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid_status_payload)?;
        let (state, tools, bounded_error) = match auth_status {
            "notLoggedIn" => (
                McpRuntimeState::Failed,
                Vec::new(),
                Some(McpRuntimeError::bounded(
                    McpErrorCategory::Authentication,
                    "MCP provider authentication required",
                )),
            ),
            "unsupported" | "bearerToken" | "oAuth" => (
                McpRuntimeState::Connected,
                parse_tool_inventory(raw_server)?,
                None,
            ),
            _ => return Err(invalid_status_payload()),
        };
        statuses.push(runtime_status(
            definition_id.clone(),
            provider_id,
            session_id,
            state,
            tools,
            bounded_error,
            &checked_at,
        )?);
    }
    statuses.sort_unstable_by(|left, right| left.definition_id.0.cmp(&right.definition_id.0));
    Ok(statuses)
}

pub(crate) fn parse_codex_mcp_startup_status(
    value: &serde_json::Value,
    definitions_by_wire_name: &CodexMcpDefinitionMap,
    provider_id: &ProviderId,
    session_id: &SessionId,
    active_thread_id: Option<&str>,
) -> Option<Result<McpRuntimeStatus>> {
    if let (Some(notified_thread_id), Some(active_thread_id)) = (
        value.get("threadId").and_then(serde_json::Value::as_str),
        active_thread_id,
    ) {
        if notified_thread_id != active_thread_id {
            return None;
        }
    }
    let name = value.get("name").and_then(serde_json::Value::as_str)?;
    let definition_id = definitions_by_wire_name.get(name)?.clone();
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(invalid_status_payload);
    Some(status.and_then(|status| {
        let (state, bounded_error) = match status {
            "starting" => (McpRuntimeState::AttachingProvider, None),
            "ready" => (McpRuntimeState::Connected, None),
            "failed" => {
                let category = if value
                    .get("failureReason")
                    .and_then(serde_json::Value::as_str)
                    == Some("reauthenticationRequired")
                {
                    McpErrorCategory::Authentication
                } else {
                    McpErrorCategory::Provider
                };
                let message = if category == McpErrorCategory::Authentication {
                    "MCP provider authentication required"
                } else {
                    "MCP provider attachment failed"
                };
                (
                    McpRuntimeState::Failed,
                    Some(McpRuntimeError::bounded(category, message)),
                )
            }
            "cancelled" => (
                McpRuntimeState::Failed,
                Some(McpRuntimeError::bounded(
                    McpErrorCategory::Provider,
                    "MCP provider attachment cancelled",
                )),
            ),
            _ => return Err(invalid_status_payload()),
        };
        runtime_status(
            definition_id,
            provider_id,
            session_id,
            state,
            Vec::new(),
            bounded_error,
            &chrono::Utc::now().to_rfc3339(),
        )
    }))
}

pub(crate) fn merge_codex_mcp_status(
    statuses: &mut Vec<McpRuntimeStatus>,
    status: McpRuntimeStatus,
) {
    if let Some(current) = statuses
        .iter_mut()
        .find(|current| current.definition_id == status.definition_id)
    {
        *current = status;
    } else {
        statuses.push(status);
    }
    statuses.sort_unstable_by(|left, right| left.definition_id.0.cmp(&right.definition_id.0));
}

fn parse_tool_inventory(raw_server: &serde_json::Value) -> Result<Vec<McpToolSummary>> {
    let raw_tools = raw_server
        .get("tools")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(invalid_status_payload)?;
    if raw_tools.len() > MAX_TOOL_COUNT {
        return Err(invalid_status_payload());
    }
    let mut tools = Vec::with_capacity(raw_tools.len());
    for (key, tool) in raw_tools {
        let name = tool
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid_status_payload)?;
        if key != name {
            return Err(invalid_status_payload());
        }
        tools.push(McpToolSummary {
            name: name.to_string(),
        });
    }
    tools.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    Ok(tools)
}

fn status_snapshot_for_all(
    definitions_by_wire_name: &CodexMcpDefinitionMap,
    provider_id: &ProviderId,
    session_id: &SessionId,
    state: McpRuntimeState,
    error: Option<(McpErrorCategory, &'static str)>,
) -> Vec<McpRuntimeStatus> {
    let checked_at = chrono::Utc::now().to_rfc3339();
    let mut statuses = definitions_by_wire_name
        .values()
        .cloned()
        .map(|definition_id| McpRuntimeStatus {
            definition_id,
            provider_id: provider_id.clone(),
            provider_version: CODEX_MCP_RUNTIME_VERSION.to_string(),
            session_id: session_id.clone(),
            state: state.clone(),
            tools: Vec::new(),
            checked_at: checked_at.clone(),
            bounded_error: error
                .as_ref()
                .map(|(category, message)| McpRuntimeError::bounded(category.clone(), message)),
        })
        .collect::<Vec<_>>();
    statuses.sort_unstable_by(|left, right| left.definition_id.0.cmp(&right.definition_id.0));
    statuses
}

fn runtime_status(
    definition_id: McpDefinitionId,
    provider_id: &ProviderId,
    session_id: &SessionId,
    state: McpRuntimeState,
    tools: Vec<McpToolSummary>,
    bounded_error: Option<McpRuntimeError>,
    checked_at: &str,
) -> Result<McpRuntimeStatus> {
    let status = McpRuntimeStatus {
        definition_id,
        provider_id: provider_id.clone(),
        provider_version: CODEX_MCP_RUNTIME_VERSION.to_string(),
        session_id: session_id.clone(),
        state,
        tools,
        checked_at: checked_at.to_string(),
        bounded_error,
    };
    status.validate().map_err(|_| invalid_status_payload())?;
    Ok(status)
}

fn collect_secret_map(
    secrets: &[ProviderMcpSecret],
    validate_name: fn(&str) -> bool,
    validate_value: fn(&str) -> bool,
) -> Result<BTreeMap<&str, &str>> {
    if secrets.len() > MAX_SECRET_COUNT {
        return Err(invalid_configuration());
    }
    let mut result = BTreeMap::new();
    for secret in secrets {
        if !validate_name(&secret.name) || result.contains_key(secret.name.as_str()) {
            return Err(invalid_configuration());
        }
        let value = str::from_utf8(secret.expose_secret()).map_err(|_| invalid_configuration())?;
        if !validate_value(value) {
            return Err(invalid_configuration());
        }
        result.insert(secret.name.as_str(), value);
    }
    Ok(result)
}

fn collect_header_map(secrets: &[ProviderMcpSecret]) -> Result<BTreeMap<&str, &str>> {
    if secrets.len() > MAX_SECRET_COUNT {
        return Err(invalid_configuration());
    }
    let mut normalized_names = HashSet::with_capacity(secrets.len());
    let mut result = BTreeMap::new();
    for secret in secrets {
        let normalized_name = secret.name.to_ascii_lowercase();
        if !validate_header_name(&secret.name)
            || is_reserved_header(&normalized_name)
            || !normalized_names.insert(normalized_name)
        {
            return Err(invalid_configuration());
        }
        let value = str::from_utf8(secret.expose_secret()).map_err(|_| invalid_configuration())?;
        if !validate_header_value(value) {
            return Err(invalid_configuration());
        }
        result.insert(secret.name.as_str(), value);
    }
    Ok(result)
}

fn validate_server_name(value: &str) -> Result<()> {
    let valid = value.starts_with("dcc-")
        && value.chars().count() <= MAX_SERVER_NAME_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(invalid_configuration())
    }
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

fn is_reserved_header(normalized_name: &str) -> bool {
    matches!(
        normalized_name,
        "accept"
            | "connection"
            | "content-length"
            | "content-type"
            | "host"
            | "mcp-protocol-version"
            | "mcp-session-id"
            | "origin"
            | "proxy-authorization"
            | "proxy-connection"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "user-agent"
    )
}

fn validate_http_url(value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|_| invalid_configuration())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid_configuration());
    }
    Ok(())
}

fn invalid_configuration() -> CoreError {
    CoreError::InvalidInput("Codex MCP configuration is invalid".to_string())
}

fn invalid_status_payload() -> CoreError {
    CoreError::Provider("invalid Codex MCP status payload".to_string())
}

#[cfg(test)]
mod tests {
    use dcc_core::{
        domain::mcp::McpDefinitionId,
        ports::{ProviderMcpSecret, SecretValue},
    };

    use super::*;

    fn secret(name: &str, value: &str) -> ProviderMcpSecret {
        ProviderMcpSecret::new(
            name,
            SecretValue::new(value.as_bytes().to_vec()).expect("test secret"),
        )
    }

    fn fixtures() -> Vec<ProviderMcpServerConfig> {
        vec![
            ProviderMcpServerConfig {
                definition_id: McpDefinitionId("stdio-fixture".to_string()),
                server_name: "dcc-stdio-fixture".to_string(),
                transport: ProviderMcpTransport::Stdio {
                    executable: "fixture-bin".to_string(),
                    args: vec!["--stdio".to_string()],
                    cwd: Some("/workspace/tools".to_string()),
                    environment: vec![secret("FIXTURE_TOKEN", "stdio-secret-canary")],
                },
            },
            ProviderMcpServerConfig {
                definition_id: McpDefinitionId("http-fixture".to_string()),
                server_name: "dcc-http-fixture".to_string(),
                transport: ProviderMcpTransport::Http {
                    url: "https://mcp.example.test/rpc".to_string(),
                    headers: vec![secret("Authorization", "Bearer http-secret-canary")],
                },
            },
        ]
    }

    #[test]
    fn encodes_bounded_ephemeral_thread_configuration() {
        let prepared =
            prepare_thread_start_request(7, "/workspace", &["/shared".to_string()], &fixtures())
                .expect("valid configuration");
        assert_eq!(
            format!("{prepared:?}"),
            "PreparedCodexMcpThreadStartRequest([REDACTED])"
        );
        assert_eq!(prepared.definitions_by_wire_name().len(), 2);
        assert!(prepared
            .definitions_by_wire_name()
            .values()
            .any(|definition| definition.0 == "stdio-fixture"));

        let value: serde_json::Value =
            serde_json::from_slice(prepared.payload.as_bytes()).expect("valid JSON");
        assert_eq!(
            value.get("method").and_then(serde_json::Value::as_str),
            Some("thread/start")
        );
        assert_eq!(
            value.pointer("/params/runtimeWorkspaceRoots"),
            Some(&serde_json::json!(["/workspace", "/shared"]))
        );
        assert_eq!(
            value.pointer("/params/approvalPolicy"),
            Some(&serde_json::json!({
                "granular": {
                    "sandbox_approval": false,
                    "rules": false,
                    "skill_approval": false,
                    "request_permissions": false,
                    "mcp_elicitations": true
                }
            }))
        );
        let servers = value
            .pointer("/params/config/mcp_servers")
            .and_then(serde_json::Value::as_object)
            .expect("MCP config");
        assert_eq!(servers.len(), 2);
        assert!(servers.keys().all(|name| name.starts_with("dcc-")));
        assert!(servers
            .keys()
            .all(|name| { name != "dcc-stdio-fixture" && name != "dcc-http-fixture" }));
        assert!(servers.values().any(|server| {
            server.get("command").and_then(serde_json::Value::as_str) == Some("fixture-bin")
                && server.get("cwd").and_then(serde_json::Value::as_str) == Some("/workspace/tools")
                && server
                    .get("default_tools_approval_mode")
                    .and_then(serde_json::Value::as_str)
                    == Some("prompt")
                && server
                    .pointer("/env/FIXTURE_TOKEN")
                    .and_then(serde_json::Value::as_str)
                    == Some("stdio-secret-canary")
        }));
        assert!(servers.values().any(|server| {
            server.get("url").and_then(serde_json::Value::as_str)
                == Some("https://mcp.example.test/rpc")
                && server
                    .get("default_tools_approval_mode")
                    .and_then(serde_json::Value::as_str)
                    == Some("prompt")
                && server
                    .pointer("/http_headers/Authorization")
                    .and_then(serde_json::Value::as_str)
                    == Some("Bearer http-secret-canary")
        }));
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_configuration() {
        let mut duplicate = fixtures();
        duplicate[1].server_name = duplicate[0].server_name.clone();
        assert!(prepare_thread_start_request(1, "/workspace", &[], &duplicate).is_err());

        let mut duplicate_definition = fixtures();
        duplicate_definition[1].definition_id = duplicate_definition[0].definition_id.clone();
        assert!(prepare_thread_start_request(1, "/workspace", &[], &duplicate_definition).is_err());

        let mut bad_header = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut bad_header[1].transport else {
            panic!("HTTP fixture");
        };
        *headers = vec![secret("X-Test", "safe\r\nInjected: value")];
        assert!(prepare_thread_start_request(1, "/workspace", &[], &bad_header).is_err());

        let mut reserved_header = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut reserved_header[1].transport else {
            panic!("HTTP fixture");
        };
        *headers = vec![secret("Mcp-Session-Id", "user-controlled")];
        assert!(prepare_thread_start_request(1, "/workspace", &[], &reserved_header).is_err());

        let mut bad_cwd = fixtures();
        let ProviderMcpTransport::Stdio { cwd, .. } = &mut bad_cwd[0].transport else {
            panic!("stdio fixture");
        };
        *cwd = Some("bad\0path".to_string());
        assert!(prepare_thread_start_request(1, "/workspace", &[], &bad_cwd).is_err());
    }

    #[tokio::test]
    async fn writes_one_json_rpc_line() {
        let mut output = Vec::new();
        let prepared = prepare_thread_start_request(9, "/workspace", &[], &fixtures())
            .expect("request should prepare");
        assert_eq!(prepared.definitions_by_wire_name().len(), 2);
        prepared
            .write_to(&mut output)
            .await
            .expect("request should write");
        assert_eq!(output.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(output.last(), Some(&b'\n'));
    }

    #[test]
    fn normalizes_only_dcc_owned_status_inventory() {
        let definitions = HashMap::from([
            (
                "dcc-session-0".to_string(),
                McpDefinitionId("stdio-fixture".to_string()),
            ),
            (
                "dcc-session-1".to_string(),
                McpDefinitionId("http-fixture".to_string()),
            ),
        ]);
        let statuses = parse_codex_mcp_status_snapshot(
            &serde_json::json!({
                "data": [
                    {
                        "name": "user-native-server",
                        "authStatus": "unsupported",
                        "tools": {
                            "native.tool": { "name": "native.tool", "inputSchema": {} }
                        },
                        "resources": [],
                        "resourceTemplates": []
                    },
                    {
                        "name": "dcc-session-0",
                        "authStatus": "unsupported",
                        "tools": {
                            "fixture.echo": { "name": "fixture.echo", "inputSchema": {} },
                            "fixture.fail": { "name": "fixture.fail", "inputSchema": {} }
                        },
                        "resources": [],
                        "resourceTemplates": []
                    },
                    {
                        "name": "dcc-session-1",
                        "authStatus": "notLoggedIn",
                        "tools": {},
                        "resources": [],
                        "resourceTemplates": []
                    }
                ]
            }),
            &definitions,
            &ProviderId("codex".to_string()),
            &SessionId("session-1".to_string()),
        )
        .expect("valid status snapshot");

        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].definition_id.0, "http-fixture");
        assert_eq!(statuses[0].state, McpRuntimeState::Failed);
        assert_eq!(
            statuses[0]
                .bounded_error
                .as_ref()
                .map(|error| &error.category),
            Some(&McpErrorCategory::Authentication)
        );
        assert_eq!(statuses[1].definition_id.0, "stdio-fixture");
        assert_eq!(statuses[1].state, McpRuntimeState::Connected);
        assert_eq!(
            statuses[1]
                .tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            vec!["fixture.echo", "fixture.fail"]
        );
    }

    #[test]
    fn normalizes_startup_without_forwarding_provider_errors() {
        let definitions = HashMap::from([(
            "dcc-session-0".to_string(),
            McpDefinitionId("fixture".to_string()),
        )]);
        let status = parse_codex_mcp_startup_status(
            &serde_json::json!({
                "name": "dcc-session-0",
                "status": "failed",
                "failureReason": "reauthenticationRequired",
                "error": "secret-bearing provider failure",
                "threadId": "thread-1"
            }),
            &definitions,
            &ProviderId("codex".to_string()),
            &SessionId("session-1".to_string()),
            Some("thread-1"),
        )
        .expect("known DCC server")
        .expect("valid startup status");

        assert_eq!(status.state, McpRuntimeState::Failed);
        let error = status.bounded_error.expect("bounded failure");
        assert_eq!(error.category, McpErrorCategory::Authentication);
        assert!(!error.message.contains("secret-bearing"));
        assert!(parse_codex_mcp_startup_status(
            &serde_json::json!({
                "name": "dcc-session-0",
                "status": "starting",
                "threadId": "thread-before-start-response"
            }),
            &definitions,
            &ProviderId("codex".to_string()),
            &SessionId("session-1".to_string()),
            None,
        )
        .is_some());
        assert!(parse_codex_mcp_startup_status(
            &serde_json::json!({
                "name": "dcc-session-0",
                "status": "starting",
                "threadId": "different-thread"
            }),
            &definitions,
            &ProviderId("codex".to_string()),
            &SessionId("session-1".to_string()),
            Some("active-thread"),
        )
        .is_none());
        assert!(parse_codex_mcp_startup_status(
            &serde_json::json!({
                "name": "user-native-server",
                "status": "failed"
            }),
            &definitions,
            &ProviderId("codex".to_string()),
            &SessionId("session-1".to_string()),
            None,
        )
        .is_none());
    }

    #[test]
    fn rejects_malformed_known_inventory_and_leaves_missing_servers_attaching() {
        let definitions = HashMap::from([
            (
                "dcc-session-0".to_string(),
                McpDefinitionId("fixture".to_string()),
            ),
            (
                "dcc-session-1".to_string(),
                McpDefinitionId("pending".to_string()),
            ),
        ]);
        let statuses = parse_codex_mcp_status_snapshot(
            &serde_json::json!({
                "data": [{
                    "name": "dcc-session-0",
                    "authStatus": "unsupported",
                    "tools": {
                        "fixture.echo": { "name": "fixture.echo", "inputSchema": {} }
                    },
                    "resources": [],
                    "resourceTemplates": []
                }]
            }),
            &definitions,
            &ProviderId("codex".to_string()),
            &SessionId("session-1".to_string()),
        )
        .expect("valid partial snapshot");
        assert_eq!(
            statuses
                .iter()
                .find(|status| status.definition_id.0 == "pending")
                .map(|status| &status.state),
            Some(&McpRuntimeState::AttachingProvider)
        );

        assert!(parse_codex_mcp_status_snapshot(
            &serde_json::json!({
                "data": [{
                    "name": "dcc-session-0",
                    "authStatus": "unsupported",
                    "tools": {
                        "fixture.echo": { "name": "different.name", "inputSchema": {} }
                    },
                    "resources": [],
                    "resourceTemplates": []
                }]
            }),
            &definitions,
            &ProviderId("codex".to_string()),
            &SessionId("session-1".to_string()),
        )
        .is_err());
    }
}
