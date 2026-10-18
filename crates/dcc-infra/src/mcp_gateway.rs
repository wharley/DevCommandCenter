//! Upstream half of DCC's connector gateway. A provider talks to DCC's
//! loopback endpoint; DCC re-sends each JSON-RPC exchange to the vendor with
//! the vendor token and returns plain JSON.
//!
//! Streamed (SSE) answers are collected until every request id in the call
//! has its response; server-initiated messages inside the stream are dropped,
//! because the gateway offers no standalone stream back to the provider.

use std::{collections::HashSet, time::Duration};

use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    Method, StatusCode,
};
use serde_json::Value;
use url::Url;

const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
pub const MAX_UPSTREAM_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SESSION_ID_BYTES: usize = 256;
const MCP_SESSION_ID: &str = "mcp-session-id";
const MCP_PROTOCOL_VERSION: &str = "mcp-protocol-version";

#[derive(Clone, Debug)]
pub struct ConnectorUpstream {
    client: reqwest::Client,
}

pub struct UpstreamRequest<'a> {
    pub url: &'a Url,
    /// `Bearer …` for signed-in connectors; `None` for servers without auth.
    pub authorization: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub protocol_version: Option<&'a str>,
    pub body: &'a Value,
}

#[derive(Debug, PartialEq)]
pub enum UpstreamOutcome {
    /// JSON-RPC answer(s) for the call. `body` is `None` for notifications
    /// and responses, which the server acknowledges with 202.
    Reply {
        session_id: Option<String>,
        body: Option<Value>,
    },
    /// The server rejected the token. The caller refreshes and retries once.
    Unauthorized,
    /// The server no longer knows the `Mcp-Session-Id`; the client must
    /// initialize again.
    SessionExpired,
    /// Anything else, with a short message safe to show the model.
    Failed(String),
}

impl ConnectorUpstream {
    pub fn new() -> Result<Self, String> {
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(UPSTREAM_TIMEOUT)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("DevCommandCenter/", env!("CARGO_PKG_VERSION")))
            .build()
            .map(|client| Self { client })
            .map_err(|_| "Could not start the connector HTTP client".to_string())
    }

    pub async fn forward(&self, request: UpstreamRequest<'_>) -> UpstreamOutcome {
        if !upstream_allowed(request.url) {
            return UpstreamOutcome::Failed("The connector URL must use HTTPS".to_string());
        }
        let host = request
            .url
            .host_str()
            .unwrap_or("the connector")
            .to_string();
        let headers = match request_headers(&request) {
            Some(headers) => headers,
            None => return UpstreamOutcome::Failed("Invalid connector request headers".into()),
        };
        let response = match self
            .client
            .request(Method::POST, request.url.clone())
            .headers(headers)
            .json(request.body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) if error.is_timeout() => {
                return UpstreamOutcome::Failed(format!("{host} did not answer in time"))
            }
            Err(_) => return UpstreamOutcome::Failed(format!("Could not reach {host}")),
        };

        let status = response.status();
        let session_id = response
            .headers()
            .get(MCP_SESSION_ID)
            .and_then(|value| value.to_str().ok())
            .filter(|value| valid_session_id(value))
            .map(str::to_string);
        let is_stream = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));

        match status {
            StatusCode::UNAUTHORIZED => return UpstreamOutcome::Unauthorized,
            StatusCode::NOT_FOUND if request.session_id.is_some() => {
                return UpstreamOutcome::SessionExpired
            }
            StatusCode::ACCEPTED => {
                return UpstreamOutcome::Reply {
                    session_id,
                    body: None,
                }
            }
            StatusCode::FORBIDDEN => {
                return UpstreamOutcome::Failed(format!(
                    "{host} refused this action for the signed-in account"
                ))
            }
            status if !status.is_success() => {
                return UpstreamOutcome::Failed(format!("{host} answered HTTP {}", status.as_u16()))
            }
            _ => {}
        }

        let expected = expected_ids(request.body);
        let body = if is_stream {
            collect_stream(response, &expected).await
        } else {
            read_bounded(response)
                .await
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        };
        match body {
            Some(body) => UpstreamOutcome::Reply {
                session_id,
                body: Some(body),
            },
            None if expected.is_empty() => UpstreamOutcome::Reply {
                session_id,
                body: None,
            },
            None => UpstreamOutcome::Failed(format!("{host} sent an unreadable answer")),
        }
    }

    /// Ends the upstream MCP session (`DELETE` with its id), best effort.
    pub async fn close_session(&self, url: &Url, authorization: Option<&str>, session_id: &str) {
        if !upstream_allowed(url) || !valid_session_id(session_id) {
            return;
        }
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(session_id) {
            headers.insert(MCP_SESSION_ID, value);
        }
        if let Some(value) = authorization.and_then(|value| HeaderValue::from_str(value).ok()) {
            headers.insert(AUTHORIZATION, value);
        }
        let _ = self
            .client
            .request(Method::DELETE, url.clone())
            .headers(headers)
            .send()
            .await;
    }
}

fn request_headers(request: &UpstreamRequest<'_>) -> Option<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    if let Some(authorization) = request.authorization {
        let mut value = HeaderValue::from_str(authorization).ok()?;
        value.set_sensitive(true);
        headers.insert(AUTHORIZATION, value);
    }
    if let Some(session_id) = request.session_id.filter(|value| valid_session_id(value)) {
        headers.insert(MCP_SESSION_ID, HeaderValue::from_str(session_id).ok()?);
    }
    if let Some(version) = request.protocol_version {
        headers.insert(MCP_PROTOCOL_VERSION, HeaderValue::from_str(version).ok()?);
    }
    Some(headers)
}

/// HTTPS, or plain HTTP to a loopback development server.
pub fn upstream_allowed(url: &Url) -> bool {
    let loopback = match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    (url.scheme() == "https" || (url.scheme() == "http" && loopback))
        && url.username().is_empty()
        && url.password().is_none()
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_ID_BYTES
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

/// Ids of the requests in a call (single object or batch). Notifications and
/// responses have none.
fn expected_ids(body: &Value) -> HashSet<String> {
    let messages: Vec<&Value> = match body {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    messages
        .into_iter()
        .filter(|message| message.get("method").is_some())
        .filter_map(|message| message.get("id"))
        .filter(|id| !id.is_null())
        .map(Value::to_string)
        .collect()
}

async fn read_bounded(mut response: reqwest::Response) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if body.len() + chunk.len() > MAX_UPSTREAM_RESPONSE_BYTES {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    Some(body)
}

async fn collect_stream(
    mut response: reqwest::Response,
    expected: &HashSet<String>,
) -> Option<Value> {
    let mut collector = SseCollector::new(expected.clone());
    let mut total = 0_usize;
    while let Ok(Some(chunk)) = response.chunk().await {
        total += chunk.len();
        if total > MAX_UPSTREAM_RESPONSE_BYTES {
            return None;
        }
        collector.push(&chunk);
        if collector.is_complete() {
            break;
        }
    }
    collector.finish()
}

/// Accumulates `data:` lines into events and keeps the JSON-RPC responses
/// whose ids were requested.
struct SseCollector {
    expected: HashSet<String>,
    pending: Vec<u8>,
    data: String,
    responses: Vec<Value>,
}

impl SseCollector {
    fn new(expected: HashSet<String>) -> Self {
        Self {
            expected,
            pending: Vec::new(),
            data: String::new(),
            responses: Vec::new(),
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        self.pending.extend_from_slice(chunk);
        while let Some(position) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line = self.pending.drain(..=position).collect::<Vec<_>>();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                self.dispatch();
            } else if let Some(data) = line.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
            }
        }
    }

    fn dispatch(&mut self) {
        let data = std::mem::take(&mut self.data);
        let Ok(message) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        let messages = match message {
            Value::Array(items) => items,
            other => vec![other],
        };
        for message in messages {
            let is_response = message.get("method").is_none()
                && (message.get("result").is_some() || message.get("error").is_some());
            let id = message.get("id").map(Value::to_string);
            if is_response && id.is_some_and(|id| self.expected.remove(&id)) {
                self.responses.push(message);
            }
        }
    }

    fn is_complete(&self) -> bool {
        self.expected.is_empty() && !self.responses.is_empty()
    }

    fn finish(mut self) -> Option<Value> {
        self.dispatch();
        match self.responses.len() {
            0 => None,
            1 => self.responses.pop(),
            _ => Some(Value::Array(self.responses)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn collects_only_the_requested_responses() {
        let mut collector = SseCollector::new(expected_ids(&json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {}
        })));
        collector.push(b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n\n");
        assert!(!collector.is_complete());
        collector.push(b"data: {\"jsonrpc\":\"2.0\",\"id\":7,\"res");
        collector.push(b"ult\":{\"content\":[]}}\r\n\r\n");
        assert!(collector.is_complete());
        assert_eq!(
            collector.finish(),
            Some(json!({"jsonrpc":"2.0","id":7,"result":{"content":[]}}))
        );
    }

    #[test]
    fn ignores_server_requests_that_reuse_an_id() {
        let mut collector = SseCollector::new(expected_ids(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call"
        })));
        collector.push(
            b"data: {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"sampling/createMessage\"}\n\n",
        );
        assert!(!collector.is_complete());
        assert_eq!(collector.finish(), None);
    }

    #[test]
    fn string_and_numeric_ids_do_not_collide() {
        let expected = expected_ids(&json!([
            {"jsonrpc": "2.0", "id": 1, "method": "a"},
            {"jsonrpc": "2.0", "id": "1", "method": "b"},
            {"jsonrpc": "2.0", "method": "notifications/initialized"}
        ]));
        assert_eq!(expected.len(), 2);
    }

    #[test]
    fn upstream_must_be_https_or_loopback() {
        assert!(upstream_allowed(
            &Url::parse("https://mcp.clickup.com/mcp").unwrap()
        ));
        assert!(upstream_allowed(
            &Url::parse("http://127.0.0.1:3000/mcp").unwrap()
        ));
        assert!(!upstream_allowed(
            &Url::parse("http://mcp.example.com/mcp").unwrap()
        ));
        assert!(!upstream_allowed(
            &Url::parse("https://user:pw@mcp.example.com/").unwrap()
        ));
    }
}
