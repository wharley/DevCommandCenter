use std::{
    collections::{BTreeMap, HashSet},
    fmt, str,
};

use dcc_core::{
    domain::{
        mcp::{
            McpErrorCategory, McpRuntimeError, McpRuntimeState, McpRuntimeStatus, McpToolSummary,
        },
        provider::ProviderId,
        session::SessionId,
    },
    ports::{ProviderMcpServerConfig, ProviderMcpTransport},
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

pub(crate) const CLAUDE_MCP_RUNTIME_VERSION: &str = "claude-agent-sdk@0.2.126+claude-code@2.1.126";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeMcpConfiguration<'a> {
    r#type: &'static str,
    servers: Vec<ClaudeMcpServer<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeMcpServer<'a> {
    definition_id: &'a str,
    name: String,
    transport: ClaudeMcpTransport<'a>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClaudeMcpTransport<'a> {
    Stdio {
        command: &'a str,
        args: &'a [String],
        env: BTreeMap<&'a str, &'a str>,
    },
    Http {
        url: &'a str,
        headers: BTreeMap<&'a str, &'a str>,
    },
}

/// JSON bytes that may contain MCP credentials. Debug output is redacted and
/// the allocation is zeroized when it leaves the one-shot sidecar write path.
struct SensitiveMcpPayload(Vec<u8>);

impl SensitiveMcpPayload {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SensitiveMcpPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveMcpPayload([REDACTED])")
    }
}

impl Drop for SensitiveMcpPayload {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) async fn write_initial_mcp_configuration<W>(
    writer: &mut W,
    servers: &[ProviderMcpServerConfig],
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let payload = encode_mcp_configuration(servers)?;
    writer
        .write_all(payload.as_bytes())
        .await
        .map_err(|_| CoreError::Provider("failed to configure Claude MCP servers".to_string()))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|_| CoreError::Provider("failed to configure Claude MCP servers".to_string()))?;
    writer
        .flush()
        .await
        .map_err(|_| CoreError::Provider("failed to configure Claude MCP servers".to_string()))
}

fn encode_mcp_configuration(servers: &[ProviderMcpServerConfig]) -> Result<SensitiveMcpPayload> {
    if servers.len() > MAX_SERVER_COUNT {
        return Err(invalid_configuration());
    }

    let mut names = HashSet::with_capacity(servers.len());
    let mut wire_servers = Vec::with_capacity(servers.len());
    let session_namespace = Uuid::new_v4().simple().to_string();
    for (index, server) in servers.iter().enumerate() {
        validate_server_name(&server.server_name)?;
        if !names.insert(server.server_name.as_str()) {
            return Err(invalid_configuration());
        }
        if server.definition_id.0.trim().is_empty() {
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
                    || cwd.is_some()
                {
                    return Err(invalid_configuration());
                }
                ClaudeMcpTransport::Stdio {
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
                ClaudeMcpTransport::Http {
                    url,
                    headers: collect_secret_map(
                        headers,
                        validate_header_name,
                        validate_header_value,
                    )?,
                }
            }
        };

        wire_servers.push(ClaudeMcpServer {
            definition_id: &server.definition_id.0,
            name: format!("dcc-{session_namespace}-{index}"),
            transport,
        });
    }

    serde_json::to_vec(&ClaudeMcpConfiguration {
        r#type: "configure_mcp",
        servers: wire_servers,
    })
    .map(SensitiveMcpPayload)
    .map_err(|_| invalid_configuration())
}

pub(crate) fn parse_claude_mcp_status_snapshot(
    value: &Value,
    provider_id: &ProviderId,
    session_id: &SessionId,
) -> Option<Result<Vec<McpRuntimeStatus>>> {
    if value.get("type").and_then(Value::as_str) != Some("dcc_mcp_status") {
        return None;
    }
    Some(parse_claude_mcp_status_snapshot_inner(
        value,
        provider_id,
        session_id,
    ))
}

fn parse_claude_mcp_status_snapshot_inner(
    value: &Value,
    provider_id: &ProviderId,
    session_id: &SessionId,
) -> Result<Vec<McpRuntimeStatus>> {
    let raw_servers = value
        .get("servers")
        .and_then(Value::as_array)
        .ok_or_else(invalid_status_payload)?;
    if raw_servers.len() > MAX_SERVER_COUNT {
        return Err(invalid_status_payload());
    }

    let mut definition_ids = HashSet::with_capacity(raw_servers.len());
    let mut statuses = Vec::with_capacity(raw_servers.len());
    for raw_server in raw_servers {
        let definition_id = raw_server
            .get("definitionId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(invalid_status_payload)?;
        if !definition_ids.insert(definition_id) {
            return Err(invalid_status_payload());
        }

        let raw_state = raw_server
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(invalid_status_payload)?;
        let (state, bounded_error) = match raw_state {
            "connected" => (McpRuntimeState::Connected, None),
            "pending" => (McpRuntimeState::AttachingProvider, None),
            "disabled" => (McpRuntimeState::Disabled, None),
            "needs-auth" => (
                McpRuntimeState::Failed,
                Some(McpRuntimeError::bounded(
                    McpErrorCategory::Authentication,
                    "MCP provider authentication required",
                )),
            ),
            "failed" => (
                McpRuntimeState::Failed,
                Some(McpRuntimeError::bounded(
                    McpErrorCategory::Provider,
                    "MCP provider attachment failed",
                )),
            ),
            _ => return Err(invalid_status_payload()),
        };

        let tools = if state == McpRuntimeState::Connected {
            let raw_tools = raw_server
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(invalid_status_payload)?;
            if raw_tools.len() > MAX_TOOL_COUNT {
                return Err(invalid_status_payload());
            }
            raw_tools
                .iter()
                .map(|tool| {
                    tool.as_str()
                        .map(|name| McpToolSummary {
                            name: name.to_string(),
                        })
                        .ok_or_else(invalid_status_payload)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };

        let status = McpRuntimeStatus {
            definition_id: dcc_core::domain::mcp::McpDefinitionId(definition_id.to_string()),
            provider_id: provider_id.clone(),
            provider_version: CLAUDE_MCP_RUNTIME_VERSION.to_string(),
            session_id: session_id.clone(),
            state,
            tools,
            checked_at: chrono::Utc::now().to_rfc3339(),
            bounded_error,
        };
        status.validate().map_err(|_| invalid_status_payload())?;
        statuses.push(status);
    }
    statuses.sort_unstable_by(|left, right| left.definition_id.0.cmp(&right.definition_id.0));
    Ok(statuses)
}

fn collect_secret_map(
    secrets: &[dcc_core::ports::ProviderMcpSecret],
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

fn validate_header_name(value: &str) -> bool {
    HeaderName::from_bytes(value.as_bytes()).is_ok()
}

fn validate_environment_value(value: &str) -> bool {
    !value.contains('\0')
}

fn validate_header_value(value: &str) -> bool {
    HeaderValue::from_str(value).is_ok()
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
    CoreError::InvalidInput("Claude MCP configuration is invalid".to_string())
}

fn invalid_status_payload() -> CoreError {
    CoreError::Provider("invalid Claude MCP status payload".to_string())
}

#[cfg(test)]
mod tests {
    use dcc_core::{
        domain::mcp::McpDefinitionId,
        domain::{provider::ProviderId, session::SessionId},
        ports::{ProviderMcpSecret, ProviderMcpServerConfig, ProviderMcpTransport, SecretValue},
    };

    use super::*;

    fn secret(name: &str, value: &str) -> ProviderMcpSecret {
        ProviderMcpSecret::new(
            name,
            SecretValue::new(value.as_bytes().to_vec()).expect("test secret"),
        )
    }

    #[test]
    fn serializes_both_sdk_transports_without_debugging_secrets() {
        let servers = vec![
            ProviderMcpServerConfig {
                definition_id: McpDefinitionId("command-fixture".to_string()),
                server_name: "dcc-command-fixture".to_string(),
                transport: ProviderMcpTransport::Stdio {
                    executable: "/absolute/fixture".to_string(),
                    args: vec!["stdio".to_string()],
                    cwd: None,
                    environment: vec![secret("FIXTURE_TOKEN", "secret-canary")],
                },
            },
            ProviderMcpServerConfig {
                definition_id: McpDefinitionId("http-fixture".to_string()),
                server_name: "dcc-http-fixture".to_string(),
                transport: ProviderMcpTransport::Http {
                    url: "http://127.0.0.1:8765/mcp".to_string(),
                    headers: vec![secret("Authorization", "Bearer secret-canary")],
                },
            },
        ];

        let payload = encode_mcp_configuration(&servers).expect("encode MCP configuration");
        let value: serde_json::Value =
            serde_json::from_slice(payload.as_bytes()).expect("parse payload");

        assert_eq!(value["type"], "configure_mcp");
        assert_eq!(value["servers"][0]["transport"]["type"], "stdio");
        assert_eq!(value["servers"][1]["transport"]["type"], "http");
        assert!(value["servers"][0]["name"]
            .as_str()
            .expect("generated name")
            .starts_with("dcc-"));
        assert_ne!(
            value["servers"][0]["name"],
            serde_json::json!("dcc-command-fixture")
        );
        assert_eq!(format!("{payload:?}"), "SensitiveMcpPayload([REDACTED])");
        assert!(!format!("{servers:?}").contains("secret-canary"));
    }

    #[test]
    fn allows_multiline_environment_secrets_without_treating_them_as_headers() {
        let server = ProviderMcpServerConfig {
            definition_id: McpDefinitionId("fixture".to_string()),
            server_name: "dcc-fixture".to_string(),
            transport: ProviderMcpTransport::Stdio {
                executable: "/absolute/fixture".to_string(),
                args: Vec::new(),
                cwd: None,
                environment: vec![secret(
                    "PRIVATE_KEY",
                    "-----BEGIN KEY-----\nabc\n-----END KEY-----",
                )],
            },
        };

        assert!(encode_mcp_configuration(&[server]).is_ok());
    }

    #[test]
    fn rejects_names_not_owned_by_dcc_and_credential_injection() {
        let user_owned_name = ProviderMcpServerConfig {
            definition_id: McpDefinitionId("fixture".to_string()),
            server_name: "user-config-name".to_string(),
            transport: ProviderMcpTransport::Http {
                url: "https://example.com/mcp".to_string(),
                headers: Vec::new(),
            },
        };
        assert!(encode_mcp_configuration(&[user_owned_name]).is_err());

        let injected_header = ProviderMcpServerConfig {
            definition_id: McpDefinitionId("fixture".to_string()),
            server_name: "dcc-fixture".to_string(),
            transport: ProviderMcpTransport::Http {
                url: "https://example.com/mcp".to_string(),
                headers: vec![secret("Authorization", "Bearer ok\r\nX-Evil: yes")],
            },
        };
        assert!(encode_mcp_configuration(&[injected_header]).is_err());
    }

    #[test]
    fn rejects_stdio_working_directory_the_installed_sdk_cannot_represent() {
        let server = ProviderMcpServerConfig {
            definition_id: McpDefinitionId("fixture".to_string()),
            server_name: "dcc-fixture".to_string(),
            transport: ProviderMcpTransport::Stdio {
                executable: "/absolute/fixture".to_string(),
                args: Vec::new(),
                cwd: Some("/workspace".to_string()),
                environment: Vec::new(),
            },
        };

        assert!(encode_mcp_configuration(&[server]).is_err());
    }

    #[test]
    fn normalizes_sdk_statuses_without_forwarding_raw_provider_data() {
        let value = serde_json::json!({
            "type": "dcc_mcp_status",
            "servers": [
                {
                    "definitionId": "pending",
                    "name": "dcc-pending",
                    "status": "pending",
                    "tools": []
                },
                {
                    "definitionId": "connected",
                    "name": "dcc-connected",
                    "status": "connected",
                    "tools": ["fixture.echo", "fixture.mutate"],
                    "url": "https://secret.example/mcp"
                },
                {
                    "definitionId": "auth",
                    "name": "dcc-auth",
                    "status": "needs-auth",
                    "tools": [],
                    "error": "Bearer secret-canary"
                },
                {
                    "definitionId": "failed",
                    "name": "dcc-failed",
                    "status": "failed",
                    "tools": [],
                    "error": "raw provider failure"
                },
                {
                    "definitionId": "disabled",
                    "name": "dcc-disabled",
                    "status": "disabled",
                    "tools": []
                }
            ]
        });

        let statuses = parse_claude_mcp_status_snapshot(
            &value,
            &ProviderId("claude_code".to_string()),
            &SessionId("session-1".to_string()),
        )
        .expect("recognized status event")
        .expect("valid status snapshot");

        assert_eq!(
            statuses
                .iter()
                .map(|status| (&status.definition_id.0, &status.state))
                .collect::<Vec<_>>(),
            vec![
                (&"auth".to_string(), &McpRuntimeState::Failed),
                (&"connected".to_string(), &McpRuntimeState::Connected),
                (&"disabled".to_string(), &McpRuntimeState::Disabled),
                (&"failed".to_string(), &McpRuntimeState::Failed),
                (&"pending".to_string(), &McpRuntimeState::AttachingProvider),
            ]
        );
        assert_eq!(statuses[1].tools.len(), 2);
        assert_eq!(
            statuses[0]
                .bounded_error
                .as_ref()
                .expect("auth error")
                .category,
            McpErrorCategory::Authentication
        );
        let debug = format!("{statuses:?}");
        assert!(!debug.contains("secret-canary"));
        assert!(!debug.contains("secret.example"));
        assert!(!debug.contains("raw provider failure"));
    }

    #[test]
    fn rejects_malformed_status_with_a_fixed_error() {
        let value = serde_json::json!({
            "type": "dcc_mcp_status",
            "servers": [{
                "definitionId": "fixture",
                "status": "connected",
                "tools": ["valid", "invalid tool name", "secret-canary"]
            }]
        });

        let error = parse_claude_mcp_status_snapshot(
            &value,
            &ProviderId("claude_code".to_string()),
            &SessionId("session-1".to_string()),
        )
        .expect("recognized status event")
        .expect_err("invalid tool must fail");

        assert_eq!(
            error.to_string(),
            "provider error: invalid Claude MCP status payload"
        );
        assert!(!error.to_string().contains("secret-canary"));
    }

    #[test]
    fn runtime_version_is_pinned_to_the_bundled_provider_dependencies() {
        let package: Value =
            serde_json::from_str(include_str!("../../../package.json")).expect("root package");
        let dependencies = package["dependencies"]
            .as_object()
            .expect("root dependencies");
        let expected = format!(
            "claude-agent-sdk@{}+claude-code@{}",
            dependencies["@anthropic-ai/claude-agent-sdk"]
                .as_str()
                .expect("SDK version"),
            dependencies["@anthropic-ai/claude-code"]
                .as_str()
                .expect("Claude Code version"),
        );

        assert_eq!(CLAUDE_MCP_RUNTIME_VERSION, expected);
    }

    #[tokio::test]
    async fn one_shot_configuration_is_newline_delimited() {
        let mut output = Vec::new();
        write_initial_mcp_configuration(&mut output, &[])
            .await
            .expect("write empty config");

        assert!(output.ends_with(b"\n"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output[..output.len() - 1])
                .expect("parse config")["servers"],
            serde_json::json!([])
        );
    }
}
