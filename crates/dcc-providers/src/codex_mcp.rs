use std::{
    collections::{BTreeMap, HashSet},
    fmt, str,
};

use dcc_core::{
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

pub(crate) const CODEX_MCP_RUNTIME_VERSION: &str = "codex-cli@0.145.0+app-server-protocol-v2";
pub(crate) const SUPPORTED_CODEX_CLI_VERSION: &str = "0.145.0";

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
    approval_policy: &'static str,
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
#[serde(untagged)]
enum CodexMcpServer<'a> {
    Stdio {
        command: &'a str,
        args: &'a [String],
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<&'a str>,
        env: BTreeMap<&'a str, &'a str>,
    },
    Http {
        url: &'a str,
        http_headers: BTreeMap<&'a str, &'a str>,
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

pub(crate) async fn write_thread_start_request<W>(
    writer: &mut W,
    request_id: u64,
    cwd: &str,
    additional_working_directories: &[String],
    servers: &[ProviderMcpServerConfig],
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let payload =
        encode_thread_start_request(request_id, cwd, additional_working_directories, servers)?;
    writer
        .write_all(payload.as_bytes())
        .await
        .map_err(|_| CoreError::Provider("failed to configure Codex MCP servers".to_string()))?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|_| CoreError::Provider("failed to configure Codex MCP servers".to_string()))?;
    writer
        .flush()
        .await
        .map_err(|_| CoreError::Provider("failed to configure Codex MCP servers".to_string()))
}

fn encode_thread_start_request(
    request_id: u64,
    cwd: &str,
    additional_working_directories: &[String],
    servers: &[ProviderMcpServerConfig],
) -> Result<SensitiveRpcPayload> {
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
    let mut wire_servers = BTreeMap::new();
    let session_namespace = Uuid::new_v4().simple().to_string();
    for (index, server) in servers.iter().enumerate() {
        validate_server_name(&server.server_name)?;
        if !names.insert(server.server_name.as_str()) || server.definition_id.0.trim().is_empty() {
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
                }
            }
            ProviderMcpTransport::Http { url, headers } => {
                validate_http_url(url)?;
                CodexMcpServer::Http {
                    url,
                    http_headers: collect_header_map(headers)?,
                }
            }
        };

        wire_servers.insert(format!("dcc-{session_namespace}-{index}"), transport);
    }

    let runtime_workspace_roots = if additional_working_directories.is_empty() {
        None
    } else {
        let mut roots = Vec::with_capacity(additional_working_directories.len() + 1);
        roots.push(cwd);
        roots.extend(additional_working_directories.iter().map(String::as_str));
        Some(roots)
    };
    serde_json::to_vec(&RpcRequest {
        jsonrpc: "2.0",
        id: request_id,
        method: "thread/start",
        params: CodexThreadStartParams {
            cwd,
            approval_policy: "never",
            sandbox: "workspace-write",
            runtime_workspace_roots,
            config: CodexThreadConfig {
                mcp_servers: wire_servers,
            },
        },
    })
    .map(SensitiveRpcPayload)
    .map_err(|_| invalid_configuration())
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
        let payload =
            encode_thread_start_request(7, "/workspace", &["/shared".to_string()], &fixtures())
                .expect("valid configuration");
        assert_eq!(format!("{payload:?}"), "SensitiveRpcPayload([REDACTED])");

        let value: serde_json::Value =
            serde_json::from_slice(payload.as_bytes()).expect("valid JSON");
        assert_eq!(
            value.get("method").and_then(serde_json::Value::as_str),
            Some("thread/start")
        );
        assert_eq!(
            value.pointer("/params/runtimeWorkspaceRoots"),
            Some(&serde_json::json!(["/workspace", "/shared"]))
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
                    .pointer("/env/FIXTURE_TOKEN")
                    .and_then(serde_json::Value::as_str)
                    == Some("stdio-secret-canary")
        }));
        assert!(servers.values().any(|server| {
            server.get("url").and_then(serde_json::Value::as_str)
                == Some("https://mcp.example.test/rpc")
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
        assert!(encode_thread_start_request(1, "/workspace", &[], &duplicate).is_err());

        let mut bad_header = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut bad_header[1].transport else {
            panic!("HTTP fixture");
        };
        *headers = vec![secret("X-Test", "safe\r\nInjected: value")];
        assert!(encode_thread_start_request(1, "/workspace", &[], &bad_header).is_err());

        let mut reserved_header = fixtures();
        let ProviderMcpTransport::Http { headers, .. } = &mut reserved_header[1].transport else {
            panic!("HTTP fixture");
        };
        *headers = vec![secret("Mcp-Session-Id", "user-controlled")];
        assert!(encode_thread_start_request(1, "/workspace", &[], &reserved_header).is_err());

        let mut bad_cwd = fixtures();
        let ProviderMcpTransport::Stdio { cwd, .. } = &mut bad_cwd[0].transport else {
            panic!("stdio fixture");
        };
        *cwd = Some("bad\0path".to_string());
        assert!(encode_thread_start_request(1, "/workspace", &[], &bad_cwd).is_err());
    }

    #[tokio::test]
    async fn writes_one_json_rpc_line() {
        let mut output = Vec::new();
        write_thread_start_request(&mut output, 9, "/workspace", &[], &fixtures())
            .await
            .expect("request should write");
        assert_eq!(output.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(output.last(), Some(&b'\n'));
    }
}
