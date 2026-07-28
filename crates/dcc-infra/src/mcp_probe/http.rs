use chrono::Utc;
use dcc_core::{
    domain::mcp::{McpDefinition, McpErrorCategory, McpProbeReport, McpSecretTarget, McpTransport},
    ports::{CredentialStore, McpProbeResult},
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE},
    Response, StatusCode,
};
use serde_json::Value;
use tokio::time::timeout;

use super::{
    probe_error,
    protocol::{
        initialize_request, initialized_notification, list_tools_request,
        parse_initialize_response, parse_tools_response, PROTOCOL_VERSION,
    },
    SecureMcpProbe,
};

const MCP_PROTOCOL_VERSION: &str = "mcp-protocol-version";
const MCP_SESSION_ID: &str = "mcp-session-id";
const MAX_SESSION_ID_BYTES: usize = 256;
const MAX_SSE_EVENTS: usize = 32;

pub(super) async fn probe_http<C>(
    probe: &SecureMcpProbe<C>,
    definition: &McpDefinition,
) -> McpProbeResult<McpProbeReport>
where
    C: CredentialStore + ?Sized,
{
    let McpTransport::Http { url } = &definition.transport else {
        unreachable!("HTTP probe called for another transport");
    };
    let secret_headers = resolve_secret_headers(probe, definition).await?;

    let initialize_response = timeout(
        probe.limits.initialize_timeout,
        send_rpc_request(
            probe,
            url,
            &secret_headers,
            None,
            None,
            initialize_request(),
            1,
        ),
    )
    .await
    .map_err(|_| probe_error(McpErrorCategory::Timeout, "MCP initialization timed out"))??;
    let session_id = session_id(&initialize_response.headers)?;
    let protocol_version = match parse_initialize_response(initialize_response.message) {
        Ok(protocol_version) => protocol_version,
        Err(error) => {
            if let Some(session_id) = session_id.as_ref() {
                close_session(probe, url, &secret_headers, session_id, PROTOCOL_VERSION).await;
            }
            return Err(error);
        }
    };

    let result = async {
        timeout(
            probe.limits.initialize_timeout,
            send_notification(
                probe,
                url,
                &secret_headers,
                session_id.as_ref(),
                &protocol_version,
                initialized_notification(),
            ),
        )
        .await
        .map_err(|_| {
            probe_error(
                McpErrorCategory::Timeout,
                "MCP initialization notification timed out",
            )
        })??;

        let tools = timeout(
            probe.limits.list_tools_timeout,
            send_rpc_request(
                probe,
                url,
                &secret_headers,
                session_id.as_ref(),
                Some(&protocol_version),
                list_tools_request(),
                2,
            ),
        )
        .await
        .map_err(|_| probe_error(McpErrorCategory::Timeout, "MCP tool discovery timed out"))??;
        let tools = parse_tools_response(tools.message, &probe.limits)?;

        Ok(McpProbeReport {
            definition_id: definition.id.clone(),
            transport: definition.transport.kind(),
            protocol_version: protocol_version.clone(),
            tools,
            checked_at: Utc::now().to_rfc3339(),
        })
    }
    .await;

    if let Some(session_id) = session_id.as_ref() {
        close_session(probe, url, &secret_headers, session_id, &protocol_version).await;
    }
    result
}

struct RpcHttpResponse {
    headers: HeaderMap,
    message: Value,
}

async fn send_rpc_request<C>(
    probe: &SecureMcpProbe<C>,
    url: &str,
    secret_headers: &HeaderMap,
    session_id: Option<&HeaderValue>,
    protocol_version: Option<&str>,
    message: Value,
    expected_id: u64,
) -> McpProbeResult<RpcHttpResponse>
where
    C: CredentialStore + ?Sized,
{
    let request = request_builder(
        probe,
        reqwest::Method::POST,
        url,
        secret_headers,
        session_id,
        protocol_version,
    )
    .json(&message);
    let response = request.send().await.map_err(request_error)?;
    ensure_success(response.status())?;
    let headers = response.headers().clone();
    let message = read_rpc_response(response, expected_id, probe.limits.max_response_bytes).await?;
    Ok(RpcHttpResponse { headers, message })
}

async fn send_notification<C>(
    probe: &SecureMcpProbe<C>,
    url: &str,
    secret_headers: &HeaderMap,
    session_id: Option<&HeaderValue>,
    protocol_version: &str,
    message: Value,
) -> McpProbeResult<()>
where
    C: CredentialStore + ?Sized,
{
    let response = request_builder(
        probe,
        reqwest::Method::POST,
        url,
        secret_headers,
        session_id,
        Some(protocol_version),
    )
    .json(&message)
    .send()
    .await
    .map_err(request_error)?;
    if response.status() == StatusCode::ACCEPTED {
        Ok(())
    } else {
        ensure_success(response.status())?;
        Err(probe_error(
            McpErrorCategory::Protocol,
            "MCP server did not acknowledge a notification",
        ))
    }
}

fn request_builder<C>(
    probe: &SecureMcpProbe<C>,
    method: reqwest::Method,
    url: &str,
    secret_headers: &HeaderMap,
    session_id: Option<&HeaderValue>,
    protocol_version: Option<&str>,
) -> reqwest::RequestBuilder
where
    C: CredentialStore + ?Sized,
{
    let mut request = probe
        .http_client
        .request(method, url)
        .headers(secret_headers.clone())
        .header(ACCEPT, "application/json, text/event-stream")
        .header(CONTENT_TYPE, "application/json");
    if let Some(session_id) = session_id {
        request = request.header(MCP_SESSION_ID, session_id.clone());
    }
    if let Some(protocol_version) = protocol_version {
        request = request.header(MCP_PROTOCOL_VERSION, protocol_version);
    }
    request
}

async fn close_session<C>(
    probe: &SecureMcpProbe<C>,
    url: &str,
    secret_headers: &HeaderMap,
    session_id: &HeaderValue,
    protocol_version: &str,
) where
    C: CredentialStore + ?Sized,
{
    let request = request_builder(
        probe,
        reqwest::Method::DELETE,
        url,
        secret_headers,
        Some(session_id),
        Some(protocol_version),
    );
    let _ = timeout(probe.limits.shutdown_timeout, request.send()).await;
}

async fn resolve_secret_headers<C>(
    probe: &SecureMcpProbe<C>,
    definition: &McpDefinition,
) -> McpProbeResult<HeaderMap>
where
    C: CredentialStore + ?Sized,
{
    let mut headers = HeaderMap::new();
    for binding in &definition.secret_refs {
        let McpSecretTarget::HttpHeader { name } = &binding.target else {
            return Err(probe_error(
                McpErrorCategory::InvalidDefinition,
                "MCP secret target does not match HTTP transport",
            ));
        };
        if is_reserved_header(name) {
            return Err(probe_error(
                McpErrorCategory::InvalidDefinition,
                "MCP credential cannot replace a transport header",
            ));
        }
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            probe_error(
                McpErrorCategory::InvalidDefinition,
                "MCP credential header name is invalid",
            )
        })?;
        let secret = probe
            .credentials
            .resolve_secret(&binding.secret_ref)
            .await
            .map_err(|_| credential_error())?
            .ok_or_else(credential_error)?;
        let mut value =
            HeaderValue::from_bytes(secret.expose_secret()).map_err(|_| credential_error())?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

fn is_reserved_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
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

fn session_id(headers: &HeaderMap) -> McpProbeResult<Option<HeaderValue>> {
    let Some(session_id) = headers.get(MCP_SESSION_ID) else {
        return Ok(None);
    };
    if session_id.as_bytes().is_empty() || session_id.as_bytes().len() > MAX_SESSION_ID_BYTES {
        return Err(probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned an invalid session identifier",
        ));
    }
    let mut session_id = session_id.clone();
    session_id.set_sensitive(true);
    Ok(Some(session_id))
}

async fn read_rpc_response(
    response: Response,
    expected_id: u64,
    max_bytes: usize,
) -> McpProbeResult<Value> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if content_type.is_some_and(|value| value.eq_ignore_ascii_case("application/json")) {
        read_json_body(response, max_bytes).await
    } else if content_type.is_some_and(|value| value.eq_ignore_ascii_case("text/event-stream")) {
        read_sse_body(response, expected_id, max_bytes).await
    } else {
        Err(probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned an unsupported content type",
        ))
    }
}

async fn read_json_body(mut response: Response, max_bytes: usize) -> McpProbeResult<Value> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        probe_error(
            McpErrorCategory::Transport,
            "failed to read the MCP HTTP response",
        )
    })? {
        if body.len() + chunk.len() > max_bytes {
            return Err(super::protocol::response_too_large());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| {
        probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned malformed JSON",
        )
    })
}

async fn read_sse_body(
    mut response: Response,
    expected_id: u64,
    max_bytes: usize,
) -> McpProbeResult<Value> {
    let mut buffered = Vec::new();
    let mut event_data = Vec::new();
    let mut total = 0_usize;
    let mut events = 0_usize;

    while let Some(chunk) = response.chunk().await.map_err(|_| {
        probe_error(
            McpErrorCategory::Transport,
            "failed to read the MCP event stream",
        )
    })? {
        total = total.saturating_add(chunk.len());
        if total > max_bytes {
            return Err(super::protocol::response_too_large());
        }
        buffered.extend_from_slice(&chunk);

        for message in drain_sse_messages(&mut buffered, &mut event_data)? {
            events += 1;
            if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
                return Ok(message);
            }
            if events >= MAX_SSE_EVENTS {
                return Err(probe_error(
                    McpErrorCategory::Protocol,
                    "MCP server sent too many unrelated events",
                ));
            }
        }
    }

    if let Some(message) = parse_sse_event(&event_data)? {
        if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
            return Ok(message);
        }
    }
    Err(probe_error(
        McpErrorCategory::Protocol,
        "MCP event stream ended before the response",
    ))
}

fn drain_sse_messages(
    buffered: &mut Vec<u8>,
    event_data: &mut Vec<u8>,
) -> McpProbeResult<Vec<Value>> {
    let mut messages = Vec::new();
    while let Some(newline) = buffered.iter().position(|byte| *byte == b'\n') {
        let mut line = buffered.drain(..=newline).collect::<Vec<_>>();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            if let Some(message) = parse_sse_event(event_data)? {
                messages.push(message);
            }
            event_data.clear();
        } else if let Some(data) = line.strip_prefix(b"data:") {
            if !event_data.is_empty() {
                event_data.push(b'\n');
            }
            event_data.extend_from_slice(data.strip_prefix(b" ").unwrap_or(data));
        }
    }
    Ok(messages)
}

fn parse_sse_event(data: &[u8]) -> McpProbeResult<Option<Value>> {
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(data).map(Some).map_err(|_| {
        probe_error(
            McpErrorCategory::Protocol,
            "MCP server returned a malformed event",
        )
    })
}

fn ensure_success(status: StatusCode) -> McpProbeResult<()> {
    if status.is_success() {
        return Ok(());
    }
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return Err(probe_error(
            McpErrorCategory::Authentication,
            "MCP HTTP server rejected authentication",
        ));
    }
    Err(probe_error(
        McpErrorCategory::Transport,
        "MCP HTTP server returned an unsuccessful status",
    ))
}

fn request_error(error: reqwest::Error) -> dcc_core::domain::mcp::McpRuntimeError {
    if error.is_timeout() {
        probe_error(McpErrorCategory::Timeout, "MCP HTTP request timed out")
    } else {
        probe_error(McpErrorCategory::Transport, "MCP HTTP request failed")
    }
}

fn credential_error() -> dcc_core::domain::mcp::McpRuntimeError {
    probe_error(
        McpErrorCategory::Authentication,
        "MCP credential is unavailable or invalid",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_transport_headers_cannot_be_replaced_by_credentials() {
        for name in [
            "Accept",
            "Connection",
            "Content-Length",
            "Content-Type",
            "Host",
            "MCP-Protocol-Version",
            "MCP-Session-Id",
            "Origin",
            "Proxy-Authorization",
            "TE",
            "Transfer-Encoding",
            "Upgrade",
            "User-Agent",
        ] {
            assert!(is_reserved_header(name), "{name}");
        }
        assert!(!is_reserved_header("Authorization"));
    }

    #[test]
    fn sse_event_parser_does_not_echo_malformed_payloads() {
        let error = parse_sse_event(b"secret-not-json").expect_err("malformed event");
        assert_eq!(error.category, McpErrorCategory::Protocol);
        assert!(!error.message.contains("secret"));
    }

    #[test]
    fn http_statuses_are_normalized_without_response_payloads() {
        let authentication =
            ensure_success(StatusCode::UNAUTHORIZED).expect_err("authentication error");
        assert_eq!(authentication.category, McpErrorCategory::Authentication);

        let transport =
            ensure_success(StatusCode::INTERNAL_SERVER_ERROR).expect_err("transport error");
        assert_eq!(transport.category, McpErrorCategory::Transport);
    }

    #[test]
    fn sse_decoder_handles_json_rpc_split_across_network_chunks() {
        let mut buffered = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":".to_vec();
        let mut event_data = Vec::new();
        assert!(drain_sse_messages(&mut buffered, &mut event_data)
            .expect("first chunk")
            .is_empty());

        buffered.extend_from_slice(b"2,\"result\":{\"tools\":[]}}\r\n\r\n");
        let messages = drain_sse_messages(&mut buffered, &mut event_data).expect("second chunk");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["id"], 2);
    }
}
