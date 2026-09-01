//! App-owned, loopback-only MCP projection for the native Browser.
//!
//! This module is intentionally stateless at the HTTP layer: a provider gets
//! one short-lived bearer token per attached session, and every request is
//! bound back to the in-memory Browser scope and consent grant.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use dcc_core::domain::mcp::{McpDefinitionId, McpToolPolicyDecision};
use dcc_core::domain::session::{Session, SessionId};
use dcc_core::ports::{
    ProviderMcpSecret, ProviderMcpServerConfig, ProviderMcpToolPolicy, ProviderMcpTransport,
    SecretValue,
};
use dcc_core::{CoreError, Result as CoreResult};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::oneshot;
use tower::limit::ConcurrencyLimitLayer;
use url::Url;

use crate::browser_commands::{
    execute_browser_control_action, extract_browser_control_context, BrowserActionAnchor,
    BrowserControlAction, BrowserState,
};
use dcc_tauri::state::{EphemeralMcpProjection, EphemeralMcpProjectionLease, SessionCommandState};

const MAX_REGISTRY_ENTRIES: usize = 128;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_MCP_TEXT_CONTENT_CHARS: usize = 32_000;
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_PROTOCOL_COMPAT: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const DCC_BROWSER_DEFINITION_ID: &str = "dcc-browser-webview-internal";
const DCC_BROWSER_SERVER_NAME: &str = "dcc-browser-webview";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeasePhase {
    Issued,
    Initialized(&'static str),
    Ready(&'static str),
}

#[derive(Clone)]
struct TokenBinding {
    token_hash: [u8; 32],
    lease_id: String,
    workspace_id: String,
    session_id: String,
    provider_id: String,
    phase: LeasePhase,
}

#[derive(Default)]
struct TokenRegistry {
    by_lease: HashMap<String, TokenBinding>,
}

impl TokenRegistry {
    fn binding_for_hash(&self, token_hash: &[u8; 32]) -> Option<TokenBinding> {
        self.by_lease
            .values()
            .find(|entry| {
                !entry.provider_id.is_empty()
                    && !entry.lease_id.is_empty()
                    && bool::from(entry.token_hash.ct_eq(token_hash))
            })
            .cloned()
    }
}

/// The loopback listener and its lease-bound session registry. No plaintext
/// bearer token is retained after `project_for_session` returns.
pub struct BrowserMcpBridge {
    browser: BrowserState,
    sessions: SessionCommandState,
    registry: Mutex<TokenRegistry>,
    endpoint: String,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    shutting_down: AtomicBool,
}

impl BrowserMcpBridge {
    pub async fn start(
        browser: BrowserState,
        sessions: SessionCommandState,
    ) -> Result<Arc<Self>, String> {
        let listener =
            tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .map_err(|_| "failed to start Browser MCP loopback listener".to_string())?;
        let endpoint = format!(
            "http://{}/mcp",
            listener
                .local_addr()
                .map_err(|_| "failed to read Browser MCP listener address")?
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let bridge = Arc::new(Self {
            browser,
            sessions,
            registry: Mutex::new(TokenRegistry::default()),
            endpoint,
            shutdown: Mutex::new(Some(shutdown_tx)),
            shutting_down: AtomicBool::new(false),
        });
        let app = Router::new()
            .route("/mcp", post(mcp_post))
            .with_state(Arc::clone(&bridge))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .layer(ConcurrencyLimitLayer::new(8));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });
        Ok(bridge)
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        if let Ok(mut shutdown) = self.shutdown.lock() {
            if let Some(sender) = shutdown.take() {
                let _ = sender.send(());
            }
        }
        if let Ok(mut registry) = self.registry.lock() {
            registry.by_lease.clear();
        }
    }

    fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    fn authenticate(&self, headers: &HeaderMap) -> Option<TokenBinding> {
        let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
        let token = raw.strip_prefix("Bearer ")?;
        if token.is_empty()
            || token
                .as_bytes()
                .iter()
                .any(|byte| byte.is_ascii_whitespace())
        {
            return None;
        }
        let token_hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let registry = self.registry.lock().ok()?;
        registry.binding_for_hash(&token_hash)
    }

    fn initialize_lease(&self, binding: &TokenBinding, protocol: &'static str) -> bool {
        let Ok(mut registry) = self.registry.lock() else {
            return false;
        };
        let Some(current) = registry.by_lease.get_mut(&binding.lease_id) else {
            return false;
        };
        if !bool::from(current.token_hash.ct_eq(&binding.token_hash))
            || current.phase != LeasePhase::Issued
        {
            return false;
        }
        current.phase = LeasePhase::Initialized(protocol);
        true
    }

    fn complete_initialization(&self, binding: &TokenBinding, protocol: &str) -> bool {
        let Ok(mut registry) = self.registry.lock() else {
            return false;
        };
        let Some(current) = registry.by_lease.get_mut(&binding.lease_id) else {
            return false;
        };
        if !bool::from(current.token_hash.ct_eq(&binding.token_hash)) {
            return false;
        }
        match current.phase {
            LeasePhase::Initialized(negotiated) if negotiated == protocol => {
                current.phase = LeasePhase::Ready(negotiated);
                true
            }
            LeasePhase::Issued | LeasePhase::Initialized(_) | LeasePhase::Ready(_) => false,
        }
    }

    fn ready_protocol(&self, binding: &TokenBinding) -> Option<&'static str> {
        let registry = self.registry.lock().ok()?;
        let current = registry.by_lease.get(&binding.lease_id)?;
        (bool::from(current.token_hash.ct_eq(&binding.token_hash))).then_some(())?;
        match current.phase {
            LeasePhase::Ready(protocol) => Some(protocol),
            LeasePhase::Issued | LeasePhase::Initialized(_) => None,
        }
    }

    fn accepts_post_initialize_protocol(
        &self,
        binding: &TokenBinding,
        header: Option<&str>,
    ) -> bool {
        let registry = match self.registry.lock() {
            Ok(registry) => registry,
            Err(_) => return false,
        };
        let Some(current) = registry.by_lease.get(&binding.lease_id) else {
            return false;
        };
        bool::from(current.token_hash.ct_eq(&binding.token_hash))
            && phase_accepts_protocol(current.phase, header)
    }

    fn issue_projection(
        &self,
        session: &Session,
    ) -> CoreResult<Option<EphemeralMcpProjectionLease>> {
        let mut token = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut token);
        let plaintext = hex::encode(token);
        let mut lease = [0_u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut lease);
        let lease_id = hex::encode(lease);
        let binding = TokenBinding {
            token_hash: Sha256::digest(plaintext.as_bytes()).into(),
            lease_id: lease_id.clone(),
            workspace_id: session.workspace_id.0.clone(),
            session_id: session.id.0.clone(),
            provider_id: session.provider_id.clone(),
            phase: LeasePhase::Issued,
        };
        let secret =
            SecretValue::new(format!("Bearer {plaintext}").into_bytes()).map_err(|_| {
                CoreError::Repository("failed to create Browser MCP credential".to_string())
            })?;
        let policies = [
            "dcc_browser_context",
            "dcc_browser_navigate",
            "dcc_browser_reload",
            "dcc_browser_scroll",
        ]
        .into_iter()
        .map(|tool_name| ProviderMcpToolPolicy {
            tool_name: tool_name.to_string(),
            decision: McpToolPolicyDecision::Ask,
        })
        .collect();
        let server = ProviderMcpServerConfig {
            definition_id: McpDefinitionId(DCC_BROWSER_DEFINITION_ID.to_string()),
            server_name: DCC_BROWSER_SERVER_NAME.to_string(),
            transport: ProviderMcpTransport::Http {
                url: self.endpoint.clone(),
                headers: vec![ProviderMcpSecret::new("Authorization", secret)],
            },
            oauth_state: None,
            tool_policies: policies,
        };
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| CoreError::Repository("Browser MCP registry unavailable".to_string()))?;
        if registry.by_lease.len() >= MAX_REGISTRY_ENTRIES {
            return Err(CoreError::Repository(
                "Browser MCP session registry is full".to_string(),
            ));
        }
        // All fallible credential/config construction is above this point, so
        // a failed projection cannot leave an authenticated orphaned lease.
        registry.by_lease.insert(lease_id.clone(), binding);
        Ok(Some(EphemeralMcpProjectionLease { server, lease_id }))
    }
}

impl EphemeralMcpProjection for BrowserMcpBridge {
    fn project_for_session(
        &self,
        session: &Session,
    ) -> CoreResult<Option<EphemeralMcpProjectionLease>> {
        self.issue_projection(session)
    }

    fn revoke_session(&self, session_id: &SessionId, lease_id: &str) {
        if let Ok(mut registry) = self.registry.lock() {
            if registry.by_lease.get(lease_id).is_some_and(|binding| {
                binding.session_id == session_id.0 && binding.lease_id == lease_id
            }) {
                registry.by_lease.remove(lease_id);
            }
        }
    }
}

async fn mcp_post(State(bridge): State<Arc<BrowserMcpBridge>>, request: Request<Body>) -> Response {
    let headers = request.headers().clone();
    if !loopback_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !accepts_mcp(&headers) {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    if !json_content_type(&headers) {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let Some(binding) = bridge.authenticate(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if bridge.is_shutting_down() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let body = match to_bytes(request.into_body(), MAX_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let value: Value = match serde_json::from_slice::<Value>(&body) {
        Ok(value) if !value.is_array() => value,
        Ok(_) => return rpc_error(Value::Null, -32600, None),
        Err(_) => return rpc_error(Value::Null, -32700, None),
    };
    let protocol = headers
        .get("MCP-Protocol-Version")
        .and_then(|value| value.to_str().ok());
    match tokio::time::timeout(
        Duration::from_secs(5),
        handle_rpc(bridge, binding, value, protocol),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

async fn handle_rpc(
    bridge: Arc<BrowserMcpBridge>,
    binding: TokenBinding,
    request: Value,
    protocol: Option<&str>,
) -> Response {
    let Some(object) = request.as_object() else {
        return rpc_error(Value::Null, -32600, None);
    };
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return rpc_error(id, -32600, None);
    }
    if object.get("method").is_none()
        && object.get("id").is_some()
        && (object.get("result").is_some() ^ object.get("error").is_some())
    {
        return if protocol.is_some_and(|header| bridge.ready_protocol(&binding) == Some(header)) {
            StatusCode::ACCEPTED.into_response()
        } else {
            StatusCode::BAD_REQUEST.into_response()
        };
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return rpc_error(id, -32600, None);
    };
    // Streamable HTTP requires every post-initialize request to carry the
    // exact protocol negotiated for this lease. Do this before parameter or
    // tool validation so transport violations never become JSON-RPC 200s.
    if method != "initialize" && !bridge.accepts_post_initialize_protocol(&binding, protocol) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    // JSON-RPC notifications intentionally omit `id`. Browser tools and
    // initialization are request-only: acknowledge such malformed provider
    // notifications without ever reaching a Browser side effect.
    if object.get("id").is_none() && request_only_method(method) {
        return StatusCode::ACCEPTED.into_response();
    }
    if object.get("id").is_some() && !valid_request_id(&id) {
        return rpc_error(id, -32600, None);
    }
    let response = match method {
        "initialize" => match initialize_protocol(object.get("params")) {
            Some(negotiated) if bridge.initialize_lease(&binding, negotiated) => rpc_result(
                id,
                json!({
                    "protocolVersion": negotiated,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": DCC_BROWSER_SERVER_NAME, "version": "1"}
                }),
                Some(negotiated),
            ),
            Some(_) => rpc_error(id, -32600, None),
            None => rpc_error(id, -32602, None),
        },
        "notifications/initialized" if object.get("id").is_none() => match protocol {
            Some(header) if bridge.complete_initialization(&binding, header) => {
                StatusCode::ACCEPTED.into_response()
            }
            _ => StatusCode::BAD_REQUEST.into_response(),
        },
        notification
            if object.get("id").is_none() && notification.starts_with("notifications/") =>
        {
            if protocol.is_some_and(|header| bridge.ready_protocol(&binding) == Some(header)) {
                StatusCode::ACCEPTED.into_response()
            } else {
                StatusCode::BAD_REQUEST.into_response()
            }
        }
        "tools/list"
            if protocol.is_some_and(|header| bridge.ready_protocol(&binding) == Some(header)) =>
        {
            rpc_result(id, json!({"tools": tools()}), None)
        }
        "tools/call" => match object
            .get("params")
            .cloned()
            .and_then(|value| serde_json::from_value::<ToolCall>(value).ok())
        {
            Some(call)
                if protocol
                    .is_some_and(|header| bridge.ready_protocol(&binding) == Some(header)) =>
            {
                if tool_call_is_well_formed(&call) {
                    if bridge.is_shutting_down() {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        rpc_result(id, dispatch_tool(&bridge, &binding, call).await, None)
                    }
                } else {
                    rpc_error(id, -32602, None)
                }
            }
            None => rpc_error(id, -32602, None),
            Some(_) => rpc_error(id, -32600, None),
        },
        "tools/list" => rpc_error(id, -32600, None),
        _ => rpc_error(id, -32601, None),
    };
    response
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ToolCall {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NavigateArgs {
    anchor: BrowserActionAnchor,
    url: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReloadArgs {
    anchor: BrowserActionAnchor,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScrollArgs {
    anchor: BrowserActionAnchor,
    delta_x: f64,
    delta_y: f64,
}

fn tool_call_is_well_formed(call: &ToolCall) -> bool {
    match call.name.as_str() {
        "dcc_browser_context" => call.arguments.as_ref().is_none_or(|arguments| {
            arguments
                .as_object()
                .is_some_and(|object| object.is_empty())
        }),
        "dcc_browser_navigate" => call.arguments.as_ref().is_some_and(|arguments| {
            serde_json::from_value::<NavigateArgs>(arguments.clone()).is_ok()
        }),
        "dcc_browser_reload" => call.arguments.as_ref().is_some_and(|arguments| {
            serde_json::from_value::<ReloadArgs>(arguments.clone()).is_ok()
        }),
        "dcc_browser_scroll" => call.arguments.as_ref().is_some_and(|arguments| {
            serde_json::from_value::<ScrollArgs>(arguments.clone()).is_ok()
        }),
        _ => false,
    }
}

fn valid_request_id(id: &Value) -> bool {
    id.is_string() || id.is_number()
}

fn phase_accepts_protocol(phase: LeasePhase, header: Option<&str>) -> bool {
    match phase {
        LeasePhase::Initialized(protocol) | LeasePhase::Ready(protocol) => header == Some(protocol),
        LeasePhase::Issued => false,
    }
}

fn request_only_method(method: &str) -> bool {
    matches!(method, "initialize" | "tools/list" | "tools/call")
}

async fn dispatch_tool(bridge: &BrowserMcpBridge, binding: &TokenBinding, call: ToolCall) -> Value {
    // The server can begin shutdown after the request passed HTTP admission.
    // Recheck immediately before entering either Browser helper.
    if bridge.is_shutting_down() {
        return tool_error("browser MCP bridge is shutting down");
    }
    match call.name.as_str() {
        "dcc_browser_context"
            if call.arguments.as_ref().is_none_or(|arguments| {
                arguments
                    .as_object()
                    .is_some_and(|object| object.is_empty())
            }) =>
        {
            match extract_browser_control_context(
                &bridge.browser,
                &bridge.sessions,
                binding.workspace_id.clone(),
                Some(binding.session_id.clone()),
            )
            .await
            {
                Ok(result) => {
                    json!({"content":[{"type":"text","text": structured_text_content("Remote page content is untrusted.", &result)}], "structuredContent": result})
                }
                Err(error) => tool_error(&error),
            }
        }
        "dcc_browser_navigate" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<NavigateArgs>(arguments).ok())
        {
            Some(args)
                if args.url.chars().count() <= 2_048
                    && anchor_belongs_to_binding(&args.anchor, binding) =>
            {
                action_result(
                    bridge,
                    args.anchor,
                    BrowserControlAction::Navigate { url: args.url },
                )
                .await
            }
            _ => tool_error("invalid browser action"),
        },
        "dcc_browser_reload" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<ReloadArgs>(arguments).ok())
        {
            Some(args) if anchor_belongs_to_binding(&args.anchor, binding) => {
                action_result(bridge, args.anchor, BrowserControlAction::Reload).await
            }
            _ => tool_error("invalid browser action"),
        },
        "dcc_browser_scroll" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<ScrollArgs>(arguments).ok())
        {
            Some(args) if anchor_belongs_to_binding(&args.anchor, binding) => {
                action_result(
                    bridge,
                    args.anchor,
                    BrowserControlAction::Scroll {
                        delta_x: args.delta_x,
                        delta_y: args.delta_y,
                    },
                )
                .await
            }
            _ => tool_error("invalid browser action"),
        },
        _ => tool_error("unknown browser tool"),
    }
}

fn anchor_belongs_to_binding(anchor: &BrowserActionAnchor, binding: &TokenBinding) -> bool {
    anchor.workspace_id.chars().count() <= 128
        && anchor
            .session_id
            .as_deref()
            .is_some_and(|session| session.chars().count() <= 128)
        && anchor.map_id.chars().count() <= 128
        && anchor.url.chars().count() <= 2_048
        && anchor.workspace_id == binding.workspace_id
        && anchor.session_id.as_deref() == Some(binding.session_id.as_str())
}

async fn action_result(
    bridge: &BrowserMcpBridge,
    anchor: BrowserActionAnchor,
    action: BrowserControlAction,
) -> Value {
    match execute_browser_control_action(&bridge.browser, &bridge.sessions, anchor, action).await {
        Ok(result) => {
            json!({"content":[{"type":"text","text": structured_text_content("Browser action executed. Extract fresh context before another action.", &result)}], "structuredContent": result})
        }
        Err(error) => tool_error(&error),
    }
}

fn tool_error(error: &str) -> Value {
    let message = if error.contains("not armed") || error.contains("grant") {
        "browser control is not armed"
    } else if error.contains("stale") || error.contains("changed") {
        "browser action anchor is stale"
    } else {
        "invalid browser action"
    };
    json!({"content":[{"type":"text","text":message}], "isError":true})
}

/// Keep the MCP compatibility TextContent valid JSON and bounded. The
/// structured result is already bounded by the Browser contract; this second
/// bound also protects callers that only render text when hostile punctuation
/// expands JSON escaping.
fn structured_text_content<T: Serialize>(notice: &str, value: &T) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if serialized.chars().count() <= MAX_MCP_TEXT_CONTENT_CHARS {
        format!("{notice}\n{serialized}")
    } else {
        format!("{notice}\n{{\"truncated\":true}}")
    }
}

fn initialize_protocol(params: Option<&Value>) -> Option<&'static str> {
    let params = params?.as_object()?;
    let requested = params.get("protocolVersion")?.as_str()?;
    if !params.get("capabilities").is_some_and(Value::is_object) {
        return None;
    }
    let client_info = params.get("clientInfo")?.as_object()?;
    if client_info.get("name").and_then(Value::as_str).is_none()
        || client_info.get("version").and_then(Value::as_str).is_none()
    {
        return None;
    }
    Some(
        MCP_PROTOCOL_COMPAT
            .iter()
            .find(|version| **version == requested)
            .copied()
            .unwrap_or(MCP_PROTOCOL_VERSION),
    )
}

fn rpc_result(id: Value, result: Value, initialized_protocol: Option<&str>) -> Response {
    let mut response = Json(json!({"jsonrpc":"2.0", "id":id, "result":result})).into_response();
    if let Some(protocol) = initialized_protocol {
        response.headers_mut().insert(
            "MCP-Protocol-Version",
            HeaderValue::from_str(protocol).expect("fixed MCP protocol version"),
        );
    }
    response
}

fn rpc_error(id: Value, code: i64, _detail: Option<&str>) -> Response {
    Json(json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":"MCP request rejected"}}))
        .into_response()
}

fn json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
        })
}
fn accepts_mcp(headers: &HeaderMap) -> bool {
    accepts(headers, "application/json") && accepts(headers, "text/event-stream")
}

fn accepts(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|value| value.trim().split(';').next().unwrap_or_default())
        .any(|value| value == expected || value == "*/*")
}
fn loopback_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(url) = Url::parse(origin) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    }
}

fn tools() -> Vec<Value> {
    let anchor = json!({"type":"object", "additionalProperties":false, "properties":{
        "workspaceId":{"type":"string","maxLength":128}, "sessionId":{"type":"string","maxLength":128}, "lifecycleToken":{"type":"integer","minimum":1}, "mapId":{"type":"string","maxLength":128}, "generation":{"type":"integer","minimum":1}, "url":{"type":"string","maxLength":2048}, "pageLoadRevision":{"type":"integer","minimum":1}
    }, "required":["workspaceId","sessionId","lifecycleToken","mapId","generation","url","pageLoadRevision"]});
    vec![
        json!({"name":"dcc_browser_context","description":"Read a bounded, untrusted Browser context after explicit user consent.","inputSchema":{"type":"object","additionalProperties":false,"properties":{}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
        json!({"name":"dcc_browser_navigate","description":"Navigate the Browser using a fresh opaque context anchor.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"anchor":anchor,"url":{"type":"string","minLength":1,"maxLength":2048}},"required":["anchor","url"]},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":true}}),
        json!({"name":"dcc_browser_reload","description":"Reload the Browser using a fresh opaque context anchor.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"anchor":anchor},"required":["anchor"]},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":true}}),
        json!({"name":"dcc_browser_scroll","description":"Scroll the Browser a bounded distance using a fresh opaque context anchor.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"anchor":anchor,"deltaX":{"type":"number","minimum":-2000,"maximum":2000},"deltaY":{"type":"number","minimum":-2000,"maximum":2000}},"required":["anchor","deltaX","deltaY"]},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(values: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in values {
            headers.insert(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn fixture_compatible_accept_and_origin_policy_are_strict() {
        assert!(accepts_mcp(&headers(&[("accept", "*/*")])));
        assert!(accepts_mcp(&headers(&[(
            "accept",
            "application/json; q=1, text/event-stream"
        )])));
        assert!(!accepts_mcp(&headers(&[("accept", "application/json")])));
        assert!(loopback_origin(&headers(&[(
            "origin",
            "https://localhost:3000"
        )])));
        assert!(loopback_origin(&headers(&[(
            "origin",
            "http://127.0.0.1:3000"
        )])));
        assert!(!loopback_origin(&headers(&[(
            "origin",
            "file:///tmp/page"
        )])));
        assert!(!loopback_origin(&headers(&[(
            "origin",
            "https://example.com"
        )])));
    }

    #[test]
    fn initialize_negotiates_current_fixture_protocol_and_requires_minimum_shape() {
        let params = json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "fixture", "version": "1"}
        });
        assert_eq!(initialize_protocol(Some(&params)), Some("2025-11-25"));
        let legacy = json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "fixture", "version": "1"}
        });
        assert_eq!(initialize_protocol(Some(&legacy)), Some("2025-03-26"));
        assert_eq!(
            initialize_protocol(Some(&json!({"protocolVersion":"2025-11-25"}))),
            None
        );
    }

    #[test]
    fn lease_protocol_state_machine_requires_initialize_then_ready_with_exact_header() {
        let mut phase = LeasePhase::Issued;
        assert_eq!(phase, LeasePhase::Issued);
        phase = LeasePhase::Initialized("2025-11-25");
        assert_ne!(phase, LeasePhase::Ready("2025-11-25"));
        assert!(matches!(phase, LeasePhase::Initialized("2025-11-25")));
        // Only the negotiated header can complete initialization.
        assert_ne!("2025-06-18", "2025-11-25");
        phase = LeasePhase::Ready("2025-11-25");
        assert_eq!(phase, LeasePhase::Ready("2025-11-25"));
        assert!(phase_accepts_protocol(phase, Some("2025-11-25")));
        assert!(!phase_accepts_protocol(phase, Some("2025-06-18")));
        assert!(!phase_accepts_protocol(phase, None));
        assert!(!phase_accepts_protocol(
            LeasePhase::Issued,
            Some("2025-11-25")
        ));
    }

    #[test]
    fn context_arguments_are_optional_but_action_arguments_remain_explicit() {
        let context: ToolCall = serde_json::from_value(json!({"name":"dcc_browser_context"}))
            .expect("context call without arguments is valid");
        assert!(context.arguments.is_none());
        let action: ToolCall = serde_json::from_value(json!({"name":"dcc_browser_reload"}))
            .expect("tool envelope remains valid before dispatch");
        assert!(action.arguments.is_none());
    }

    #[test]
    fn request_only_methods_never_execute_as_notifications() {
        assert!(request_only_method("initialize"));
        assert!(request_only_method("tools/list"));
        assert!(request_only_method("tools/call"));
        assert!(!request_only_method("notifications/initialized"));
        assert!(valid_request_id(&json!(1)));
        assert!(valid_request_id(&json!("request")));
        assert!(!valid_request_id(&Value::Null));
        assert!(!valid_request_id(&json!({})));
    }

    #[test]
    fn structured_text_falls_back_to_valid_bounded_json() {
        assert_eq!(
            structured_text_content("untrusted", &json!({"ok":true})),
            "untrusted\n{\"ok\":true}"
        );
        let hostile = "\\n".repeat(MAX_MCP_TEXT_CONTENT_CHARS);
        assert_eq!(
            structured_text_content("untrusted", &hostile),
            "untrusted\n{\"truncated\":true}"
        );
    }

    #[test]
    fn schemas_are_closed_and_only_expose_the_four_allowlisted_tools() {
        let tools = tools();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "dcc_browser_context",
                "dcc_browser_navigate",
                "dcc_browser_reload",
                "dcc_browser_scroll"
            ]
        );
        for tool in &tools {
            assert_eq!(
                tool["inputSchema"]["additionalProperties"],
                Value::Bool(false)
            );
        }
        assert_eq!(
            tools[3]["inputSchema"]["properties"]["deltaX"]["maximum"],
            json!(2000)
        );
    }

    #[test]
    fn token_binding_is_exact_and_leases_are_revoked_independently() {
        let first: [u8; 32] = Sha256::digest(b"first").into();
        let second: [u8; 32] = Sha256::digest(b"second").into();
        let entry = |hash, lease_id: &str| TokenBinding {
            token_hash: hash,
            lease_id: lease_id.to_string(),
            workspace_id: "workspace".to_string(),
            session_id: "session".to_string(),
            provider_id: "provider".to_string(),
            phase: LeasePhase::Issued,
        };
        let mut registry = TokenRegistry::default();
        registry
            .by_lease
            .insert("lease-one".to_string(), entry(first, "lease-one"));
        assert!(registry.binding_for_hash(&first).is_some());
        registry
            .by_lease
            .insert("lease-two".to_string(), entry(second, "lease-two"));
        assert!(registry.binding_for_hash(&first).is_some());
        assert_eq!(
            registry.binding_for_hash(&second).unwrap().workspace_id,
            "workspace"
        );
        registry.by_lease.remove("lease-one");
        assert!(registry.binding_for_hash(&first).is_none());
        assert!(registry.binding_for_hash(&second).is_some());
        registry.by_lease.remove("lease-two");
        assert!(registry.binding_for_hash(&second).is_none());
    }
}
