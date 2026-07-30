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
const MAX_REQUEST_ID_CHARS: usize = 128;

pub(crate) const DROID_SDK_VERSION: &str = "0.6.0";
pub(crate) const FACTORY_API_VERSION: &str = "1.0.0";
pub(crate) const FACTORY_PROTOCOL_VERSION: &str = "1.51.0";
pub(crate) const DROID_MCP_PROTOCOL_EVIDENCE: &str = "droid-sdk@0.6.0+factory-protocol@1.51.0";

pub(crate) type DroidMcpDefinitionMap = HashMap<String, McpDefinitionId>;
pub(crate) type DroidMcpToolPolicyMap =
    HashMap<McpDefinitionId, HashMap<String, McpToolPolicyDecision>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DroidInitializeRequest<'a> {
    jsonrpc: &'static str,
    factory_api_version: &'static str,
    factory_protocol_version: &'static str,
    r#type: &'static str,
    id: &'a str,
    method: &'static str,
    params: DroidInitializeParams<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DroidInitializeParams<'a> {
    machine_id: &'static str,
    cwd: &'a str,
    mcp_servers: Vec<DroidMcpServer<'a>>,
    interaction_mode: &'static str,
    autonomy_level: &'static str,
    skip_permissions_unsafe: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
enum DroidMcpServer<'a> {
    Stdio {
        name: String,
        command: &'a str,
        args: &'a [String],
        env: BTreeMap<&'a str, &'a str>,
    },
    Http {
        r#type: &'static str,
        name: String,
        url: &'a str,
        headers: Vec<DroidHttpHeader<'a>>,
    },
}

#[derive(Serialize)]
struct DroidHttpHeader<'a> {
    name: &'a str,
    value: &'a str,
}

/// One-shot Factory JSON-RPC payload that may contain MCP credentials.
///
/// The payload is never exposed through `Debug` and is zeroized on drop.
struct SensitiveDroidInitialization(Vec<u8>);

impl SensitiveDroidInitialization {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SensitiveDroidInitialization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveDroidInitialization([REDACTED])")
    }
}

impl Drop for SensitiveDroidInitialization {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) struct PreparedDroidJsonRpcInitialization {
    payload: SensitiveDroidInitialization,
    definitions_by_wire_name: DroidMcpDefinitionMap,
    tool_policies_by_definition: DroidMcpToolPolicyMap,
}

impl PreparedDroidJsonRpcInitialization {
    pub(crate) fn definitions_by_wire_name(&self) -> &DroidMcpDefinitionMap {
        &self.definitions_by_wire_name
    }

    pub(crate) fn tool_policies_by_definition(&self) -> &DroidMcpToolPolicyMap {
        &self.tool_policies_by_definition
    }

    pub(crate) async fn write_to<W>(&self, writer: &mut W) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        writer
            .write_all(self.payload.as_bytes())
            .await
            .map_err(|_| droid_configuration_error())?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|_| droid_configuration_error())?;
        writer
            .flush()
            .await
            .map_err(|_| droid_configuration_error())
    }
}

impl fmt::Debug for PreparedDroidJsonRpcInitialization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedDroidJsonRpcInitialization([REDACTED])")
    }
}

/// Builds the protocol-pinned initialization payload for a future Droid
/// bridge.
///
/// This function is intentionally not connected to `DroidProvider`: the
/// public protocol does not identify the owning MCP server in permission
/// requests, and this environment has no exact Droid CLI runtime to audit.
pub(crate) fn prepare_droid_jsonrpc_initialization(
    request_id: &str,
    cwd: &str,
    servers: &[ProviderMcpServerConfig],
) -> Result<PreparedDroidJsonRpcInitialization> {
    validate_request_id(request_id)?;
    validate_absolute_path(cwd)?;
    if servers.is_empty() || servers.len() > MAX_SERVER_COUNT {
        return Err(droid_configuration_error());
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
                    return Err(droid_configuration_error());
                }
                DroidMcpServer::Stdio {
                    name: wire_name,
                    command: executable,
                    args,
                    env: collect_secret_map(
                        environment,
                        validate_environment_name,
                        validate_environment_value,
                    )?,
                }
            }
            ProviderMcpTransport::Http { url, headers } => {
                validate_http_url(url)?;
                DroidMcpServer::Http {
                    r#type: "http",
                    name: wire_name,
                    url,
                    headers: collect_headers(headers)?,
                }
            }
        };
        wire_servers.push(wire_server);
    }

    let payload = serde_json::to_vec(&DroidInitializeRequest {
        jsonrpc: "2.0",
        factory_api_version: FACTORY_API_VERSION,
        factory_protocol_version: FACTORY_PROTOCOL_VERSION,
        r#type: "request",
        id: request_id,
        method: "droid.initialize_session",
        params: DroidInitializeParams {
            machine_id: "dcc",
            cwd,
            mcp_servers: wire_servers,
            interaction_mode: "auto",
            autonomy_level: "off",
            skip_permissions_unsafe: false,
        },
    })
    .map(SensitiveDroidInitialization)
    .map_err(|_| droid_configuration_error())?;

    Ok(PreparedDroidJsonRpcInitialization {
        payload,
        definitions_by_wire_name,
        tool_policies_by_definition,
    })
}

/// Recognizes only the exact Factory protocol envelope audited from the public
/// SDK. It is protocol evidence, not Droid CLI runtime identity.
pub(crate) fn has_audited_droid_protocol_envelope(message: &Value) -> bool {
    message.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && message.get("factoryApiVersion").and_then(Value::as_str) == Some(FACTORY_API_VERSION)
        && message
            .get("factoryProtocolVersion")
            .and_then(Value::as_str)
            == Some(FACTORY_PROTOCOL_VERSION)
}

/// The public SDK 0.6.0 permission shape has `details.toolName` but no
/// `details.serverName`. Keep this explicit predicate fail-closed so a title
/// or qualified-name convention cannot silently become ownership evidence.
pub(crate) fn permission_has_structured_mcp_owner(params: &Value) -> bool {
    let Some(tool_uses) = params.get("toolUses").and_then(Value::as_array) else {
        return false;
    };
    !tool_uses.is_empty()
        && tool_uses.iter().all(|item| {
            item.get("confirmationType").and_then(Value::as_str) == Some("mcp_tool")
                && item.pointer("/details/type").and_then(Value::as_str) == Some("mcp_tool")
                && item
                    .pointer("/details/serverName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.is_empty())
                && item
                    .pointer("/details/toolName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.is_empty())
        })
}

fn validate_request_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_REQUEST_ID_CHARS
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(droid_configuration_error());
    }
    Ok(())
}

fn validate_absolute_path(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\0') || !Path::new(value).is_absolute() {
        return Err(droid_configuration_error());
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
        return Err(droid_configuration_error());
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
        return Err(droid_configuration_error());
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
            return Err(droid_configuration_error());
        }
    }
    Ok(result)
}

fn collect_secret_map(
    secrets: &[ProviderMcpSecret],
    validate_name: fn(&str) -> bool,
    validate_value: fn(&str) -> bool,
) -> Result<BTreeMap<&str, &str>> {
    if secrets.len() > MAX_SECRET_COUNT {
        return Err(droid_configuration_error());
    }
    let mut result = BTreeMap::new();
    for secret in secrets {
        if !validate_name(&secret.name) || result.contains_key(secret.name.as_str()) {
            return Err(droid_configuration_error());
        }
        let value =
            str::from_utf8(secret.expose_secret()).map_err(|_| droid_configuration_error())?;
        if !validate_value(value) {
            return Err(droid_configuration_error());
        }
        result.insert(secret.name.as_str(), value);
    }
    Ok(result)
}

fn collect_headers(secrets: &[ProviderMcpSecret]) -> Result<Vec<DroidHttpHeader<'_>>> {
    let headers = collect_secret_map(secrets, validate_header_name, validate_header_value)?;
    Ok(headers
        .into_iter()
        .map(|(name, value)| DroidHttpHeader { name, value })
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
    let url = Url::parse(value).map_err(|_| droid_configuration_error())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(droid_configuration_error());
    }
    Ok(())
}

fn droid_configuration_error() -> CoreError {
    CoreError::InvalidInput("Droid JSON-RPC MCP configuration is invalid".to_string())
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
    async fn serializes_protocol_pinned_private_initialization() {
        let request = prepare_droid_jsonrpc_initialization("request-7", workspace(), &fixtures())
            .expect("prepare Droid initialization");

        assert_eq!(
            format!("{request:?}"),
            "PreparedDroidJsonRpcInitialization([REDACTED])"
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
            .expect("write Droid initialization");
        let value: Value = serde_json::from_slice(&output).expect("valid JSON request");
        assert_eq!(value["factoryApiVersion"], FACTORY_API_VERSION);
        assert_eq!(value["factoryProtocolVersion"], FACTORY_PROTOCOL_VERSION);
        assert_eq!(value["type"], "request");
        assert_eq!(value["method"], "droid.initialize_session");
        assert_eq!(value["params"]["machineId"], "dcc");
        assert_eq!(value["params"]["interactionMode"], "auto");
        assert_eq!(value["params"]["autonomyLevel"], "off");
        assert_eq!(value["params"]["skipPermissionsUnsafe"], false);
        assert_eq!(
            value["params"]["mcpServers"][0]["env"]["FIXTURE_TOKEN"],
            "stdio-secret"
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
    fn recognizes_protocol_evidence_without_claiming_runtime_identity() {
        let envelope = json!({
            "jsonrpc": "2.0",
            "factoryApiVersion": FACTORY_API_VERSION,
            "factoryProtocolVersion": FACTORY_PROTOCOL_VERSION,
            "type": "response",
            "id": "request-7",
            "result": {}
        });
        assert!(has_audited_droid_protocol_envelope(&envelope));

        let mut newer = envelope;
        newer["factoryProtocolVersion"] = json!("1.52.0");
        assert!(!has_audited_droid_protocol_envelope(&newer));
    }

    #[test]
    fn public_sdk_permission_shape_is_not_ownership_evidence() {
        let public_shape = json!({
            "toolUses": [{
                "toolUse": {
                    "type": "tool_use",
                    "id": "tool-1",
                    "input": {},
                    "name": "fixture.echo"
                },
                "confirmationType": "mcp_tool",
                "details": {
                    "type": "mcp_tool",
                    "toolName": "fixture.echo",
                    "impactLevel": "low"
                }
            }],
            "options": [
                { "label": "Proceed Once", "value": "proceed_once" },
                { "label": "Cancel", "value": "cancel" }
            ]
        });
        assert!(!permission_has_structured_mcp_owner(&public_shape));

        let explicit_future_shape = json!({
            "toolUses": [{
                "confirmationType": "mcp_tool",
                "details": {
                    "type": "mcp_tool",
                    "serverName": "dcc-random-0",
                    "toolName": "fixture.echo"
                }
            }]
        });
        assert!(permission_has_structured_mcp_owner(&explicit_future_shape));
    }

    #[test]
    fn rejects_ask_policy_lossy_cwd_and_unsafe_configuration() {
        let mut ask_fixture = fixtures();
        ask_fixture[0].tool_policies[0].decision = McpToolPolicyDecision::Ask;
        assert!(
            prepare_droid_jsonrpc_initialization("request-1", workspace(), &ask_fixture).is_err()
        );

        let mut cwd_fixture = fixtures();
        let ProviderMcpTransport::Stdio { cwd, .. } = &mut cwd_fixture[0].transport else {
            panic!("stdio fixture");
        };
        *cwd = Some(workspace().to_string());
        assert!(
            prepare_droid_jsonrpc_initialization("request-1", workspace(), &cwd_fixture).is_err()
        );

        let mut duplicate_header = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut duplicate_header[1].transport else {
            panic!("HTTP fixture");
        };
        headers.push(secret("Authorization", "other-secret"));
        assert!(
            prepare_droid_jsonrpc_initialization("request-1", workspace(), &duplicate_header)
                .is_err()
        );

        let mut unsafe_url = fixtures();
        let ProviderMcpTransport::Http { url, .. } = &mut unsafe_url[1].transport else {
            panic!("HTTP fixture");
        };
        *url = "https://user:secret@fixture.example/mcp".to_string();
        assert!(
            prepare_droid_jsonrpc_initialization("request-1", workspace(), &unsafe_url).is_err()
        );
    }

    #[test]
    fn rejects_ambiguous_identity_and_request_ids() {
        let mut duplicate_definition = fixtures();
        duplicate_definition[1].definition_id = duplicate_definition[0].definition_id.clone();
        assert!(prepare_droid_jsonrpc_initialization(
            "request-1",
            workspace(),
            &duplicate_definition
        )
        .is_err());

        assert!(prepare_droid_jsonrpc_initialization("", workspace(), &fixtures()).is_err());
        assert!(prepare_droid_jsonrpc_initialization(
            "request with spaces",
            workspace(),
            &fixtures()
        )
        .is_err());
    }
}
