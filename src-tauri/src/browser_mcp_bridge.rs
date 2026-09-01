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
    append_browser_audit, browser_audit_active_grant_state, browser_audit_grant_state,
    browser_audit_outcome, discard_browser_evidence_capture, execute_browser_control_action,
    extract_browser_control_context, read_browser_evidence_capture, start_browser_evidence_capture,
    BrowserActionAnchor, BrowserAuditGrantState, BrowserAuditOrigin, BrowserAuditOutcome,
    BrowserAuditTool, BrowserControlAction, BrowserState,
};
use dcc_tauri::state::{EphemeralMcpProjection, EphemeralMcpProjectionLease, SessionCommandState};

const MAX_REGISTRY_ENTRIES: usize = 128;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_MCP_TEXT_CONTENT_CHARS: usize = 32_000;
/// A single consented Browser fill stays small enough for the bounded MCP
/// request envelope. The text is never echoed in a result or error.
const MAX_BROWSER_FILL_TEXT_CHARS: usize = 2_000;
const MAX_BROWSER_REFERENCE_CHARS: usize = 3;
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_PROTOCOL_COMPAT: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const DCC_BROWSER_DEFINITION_ID: &str = "dcc-browser-webview-internal";
const DCC_BROWSER_SERVER_NAME: &str = "dcc-browser-webview";
const BROWSER_MCP_TOOL_NAMES: [&str; 8] = [
    "dcc_browser_context",
    "dcc_browser_navigate",
    "dcc_browser_reload",
    "dcc_browser_scroll",
    "dcc_browser_click",
    "dcc_browser_fill",
    "dcc_browser_evidence_start",
    "dcc_browser_evidence_read",
];

fn browser_mcp_tool_policies() -> Vec<ProviderMcpToolPolicy> {
    BROWSER_MCP_TOOL_NAMES
        .into_iter()
        .map(|tool_name| ProviderMcpToolPolicy {
            tool_name: tool_name.to_string(),
            decision: McpToolPolicyDecision::Ask,
        })
        .collect()
}

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
    /// One active evidence handle per lease, bounded by the lease registry.
    /// The capture itself remains owned by BrowserState and is one-shot there.
    evidence_capture_leases: HashMap<String, String>,
}

impl TokenRegistry {
    fn binding_is_current(&self, binding: &TokenBinding) -> bool {
        self.by_lease
            .get(&binding.lease_id)
            .is_some_and(|current| bool::from(current.token_hash.ct_eq(&binding.token_hash)))
    }

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

    fn bind_evidence_capture(&mut self, binding: &TokenBinding, capture_id: &str) -> bool {
        if !self
            .by_lease
            .get(&binding.lease_id)
            .is_some_and(|current| bool::from(current.token_hash.ct_eq(&binding.token_hash)))
        {
            return false;
        }
        // A later capture replaces the prior bridge association for this
        // lease. BrowserState keeps only one active capture per scope.
        self.evidence_capture_leases
            .retain(|_, owner| owner != &binding.lease_id);
        self.evidence_capture_leases
            .insert(capture_id.to_string(), binding.lease_id.clone());
        true
    }

    fn claim_evidence_capture(&mut self, lease_id: &str, capture_id: &str) -> bool {
        if self
            .evidence_capture_leases
            .get(capture_id)
            .is_some_and(|owner| owner == lease_id)
        {
            self.evidence_capture_leases.remove(capture_id);
            true
        } else {
            false
        }
    }

    fn remove_lease(&mut self, lease_id: &str) -> Vec<String> {
        let capture_ids = self
            .evidence_capture_leases
            .iter()
            .filter_map(|(capture_id, owner)| (owner == lease_id).then(|| capture_id.clone()))
            .collect::<Vec<_>>();
        self.by_lease.remove(lease_id);
        self.evidence_capture_leases
            .retain(|_, owner| owner != lease_id);
        capture_ids
    }

    fn take_all_evidence_captures(&mut self) -> Vec<(TokenBinding, String)> {
        let captures = std::mem::take(&mut self.evidence_capture_leases);
        captures
            .into_iter()
            .filter_map(|(capture_id, lease_id)| {
                self.by_lease
                    .get(&lease_id)
                    .cloned()
                    .map(|binding| (binding, capture_id))
            })
            .collect()
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
        let cleanup = if let Ok(mut registry) = self.registry.lock() {
            let cleanup = registry.take_all_evidence_captures();
            registry.by_lease.clear();
            cleanup
        } else {
            Vec::new()
        };
        for (binding, capture_id) in cleanup {
            discard_browser_evidence_capture(
                &self.browser,
                &binding.workspace_id,
                Some(&binding.session_id),
                &capture_id,
            );
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

    fn bind_evidence_capture(&self, binding: &TokenBinding, capture_id: &str) -> bool {
        self.registry
            .lock()
            .is_ok_and(|mut registry| registry.bind_evidence_capture(binding, capture_id))
    }

    /// Claiming removes the association before the core drain callback. A
    /// lease cannot retry or read another lease's capture after any failure.
    fn claim_evidence_capture(&self, binding: &TokenBinding, capture_id: &str) -> bool {
        self.registry.lock().is_ok_and(|mut registry| {
            registry
                .by_lease
                .get(&binding.lease_id)
                .is_some_and(|current| bool::from(current.token_hash.ct_eq(&binding.token_hash)))
                && registry.claim_evidence_capture(&binding.lease_id, capture_id)
        })
    }

    fn lease_is_current(&self, binding: &TokenBinding) -> bool {
        self.registry
            .lock()
            .is_ok_and(|registry| registry.binding_is_current(binding))
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
        let policies = browser_mcp_tool_policies();
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
        let cleanup = if let Ok(mut registry) = self.registry.lock() {
            let binding = registry.by_lease.get(lease_id).cloned();
            if binding.as_ref().is_some_and(|binding| {
                binding.session_id == session_id.0 && binding.lease_id == lease_id
            }) {
                binding.map(|binding| (binding, registry.remove_lease(lease_id)))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((binding, capture_ids)) = cleanup {
            for capture_id in capture_ids {
                discard_browser_evidence_capture(
                    &self.browser,
                    &binding.workspace_id,
                    Some(&binding.session_id),
                    &capture_id,
                );
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
                        let dispatched = ToolDispatch::failed();
                        append_mcp_tool_audit(&bridge.browser, &binding, &call.name, &dispatched);
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        let tool_name = call.name.clone();
                        let dispatched = dispatch_tool(&bridge, &binding, call).await;
                        append_mcp_tool_audit(&bridge.browser, &binding, &tool_name, &dispatched);
                        rpc_result(id, dispatched.response, None)
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClickArgs {
    anchor: BrowserActionAnchor,
    #[serde(rename = "ref")]
    reference: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FillArgs {
    anchor: BrowserActionAnchor,
    #[serde(rename = "ref")]
    reference: String,
    text: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceStartArgs {
    anchor: BrowserActionAnchor,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceReadArgs {
    capture_id: String,
}

/// References are public opaque capabilities only within the map. This keeps
/// malformed MCP arguments out of the dispatcher, before map consumption.
fn valid_browser_reference(reference: &str) -> bool {
    let Some(number) = reference.strip_prefix('e') else {
        return false;
    };
    !number.is_empty()
        && reference.chars().count() <= MAX_BROWSER_REFERENCE_CHARS
        && !number.starts_with('0')
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && number
            .parse::<usize>()
            .ok()
            .is_some_and(|value| (1..=80).contains(&value))
}

fn valid_fill_text(text: &str) -> bool {
    text.chars().count() <= MAX_BROWSER_FILL_TEXT_CHARS
        && !text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t' | '\r'))
}

/// Public evidence handles are deliberately narrower than generic opaque
/// strings: a lowercase 128-bit hex credential generated by BrowserState.
fn valid_evidence_capture_id(capture_id: &str) -> bool {
    capture_id.len() == 34
        && capture_id.starts_with("c-")
        && capture_id.as_bytes()[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn click_args_are_well_formed(arguments: &Value) -> bool {
    serde_json::from_value::<ClickArgs>(arguments.clone())
        .is_ok_and(|args| valid_browser_reference(&args.reference))
}

fn fill_args_are_well_formed(arguments: &Value) -> bool {
    serde_json::from_value::<FillArgs>(arguments.clone())
        .is_ok_and(|args| valid_browser_reference(&args.reference) && valid_fill_text(&args.text))
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
        "dcc_browser_click" => call
            .arguments
            .as_ref()
            .is_some_and(click_args_are_well_formed),
        "dcc_browser_fill" => call
            .arguments
            .as_ref()
            .is_some_and(fill_args_are_well_formed),
        "dcc_browser_evidence_start" => call.arguments.as_ref().is_some_and(|arguments| {
            serde_json::from_value::<EvidenceStartArgs>(arguments.clone()).is_ok()
        }),
        "dcc_browser_evidence_read" => call.arguments.as_ref().is_some_and(|arguments| {
            serde_json::from_value::<EvidenceReadArgs>(arguments.clone())
                .is_ok_and(|args| valid_evidence_capture_id(&args.capture_id))
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

struct ToolDispatch {
    response: Value,
    outcome: BrowserAuditOutcome,
    /// Captured before entering the controlled helper. Audit must describe the
    /// grant that admitted the attempt, not a later expiry/revoke race.
    grant_state: BrowserAuditGrantState,
}

impl ToolDispatch {
    fn executed(response: Value, grant_state: BrowserAuditGrantState) -> Self {
        Self {
            response,
            outcome: BrowserAuditOutcome::Executed,
            grant_state,
        }
    }

    fn rejected() -> Self {
        Self {
            response: tool_error("invalid browser action"),
            outcome: BrowserAuditOutcome::Rejected,
            grant_state: BrowserAuditGrantState::NotApplicable,
        }
    }

    fn failed() -> Self {
        Self {
            response: tool_error("browser MCP bridge is shutting down"),
            outcome: BrowserAuditOutcome::Failed,
            grant_state: BrowserAuditGrantState::NotApplicable,
        }
    }

    fn from_error(error: &str, grant_state: BrowserAuditGrantState) -> Self {
        Self {
            response: tool_error(error),
            outcome: browser_audit_outcome(Some(error)),
            grant_state,
        }
    }
}

fn browser_audit_tool_for_mcp(name: &str) -> Option<BrowserAuditTool> {
    match name {
        "dcc_browser_context" => Some(BrowserAuditTool::Context),
        "dcc_browser_navigate" => Some(BrowserAuditTool::Navigate),
        "dcc_browser_reload" => Some(BrowserAuditTool::Reload),
        "dcc_browser_scroll" => Some(BrowserAuditTool::Scroll),
        "dcc_browser_click" => Some(BrowserAuditTool::Click),
        "dcc_browser_fill" => Some(BrowserAuditTool::Fill),
        "dcc_browser_evidence_start" => Some(BrowserAuditTool::EvidenceStart),
        "dcc_browser_evidence_read" => Some(BrowserAuditTool::EvidenceRead),
        _ => None,
    }
}

/// A short, non-reversible correlation label. This hashes the random lease id
/// only; bearer credentials and their hash never enter Browser audit.
fn lease_fingerprint(lease_id: &str) -> String {
    let digest = Sha256::digest(lease_id.as_bytes());
    hex::encode(&digest[..12])
}

/// Called exactly once after an admitted, well-formed Browser `tools/call` has
/// dispatched. The Browser operation lock is released before this best-effort
/// bounded append, and no request payload enters the record.
fn append_mcp_tool_audit(
    browser: &BrowserState,
    binding: &TokenBinding,
    tool_name: &str,
    dispatched: &ToolDispatch,
) {
    let Some(tool) = browser_audit_tool_for_mcp(tool_name) else {
        return;
    };
    let fingerprint = lease_fingerprint(&binding.lease_id);
    append_browser_audit(
        browser,
        BrowserAuditOrigin::Mcp,
        Some(&binding.provider_id),
        Some(&fingerprint),
        &binding.workspace_id,
        Some(&binding.session_id),
        tool,
        dispatched.grant_state,
        dispatched.outcome,
    );
}

async fn dispatch_tool(
    bridge: &BrowserMcpBridge,
    binding: &TokenBinding,
    call: ToolCall,
) -> ToolDispatch {
    // The server can begin shutdown after the request passed HTTP admission.
    // Recheck immediately before entering either Browser helper.
    if bridge.is_shutting_down() {
        return ToolDispatch::failed();
    }
    match call.name.as_str() {
        "dcc_browser_context"
            if call.arguments.as_ref().is_none_or(|arguments| {
                arguments
                    .as_object()
                    .is_some_and(|object| object.is_empty())
            }) =>
        {
            let grant_state = browser_audit_active_grant_state(
                &bridge.browser,
                &binding.workspace_id,
                Some(&binding.session_id),
            );
            if bridge.is_shutting_down() {
                return ToolDispatch::failed();
            }
            // This is the final revocation gate immediately before the
            // controlled read. A revoke that wins after it is the documented
            // admitted request race; the core still revalidates
            // grant/scope/lifecycle.
            if !bridge.lease_is_current(binding) {
                return ToolDispatch::rejected();
            }
            match extract_browser_control_context(
                &bridge.browser,
                &bridge.sessions,
                binding.workspace_id.clone(),
                Some(binding.session_id.clone()),
            )
            .await
            {
                Ok(result) => ToolDispatch::executed(
                    json!({"content":[{"type":"text","text": structured_text_content("Remote page content is untrusted.", &result)}], "structuredContent": result}),
                    grant_state,
                ),
                Err(error) => ToolDispatch::from_error(&error, grant_state),
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
                    binding,
                    args.anchor,
                    BrowserControlAction::Navigate { url: args.url },
                )
                .await
            }
            _ => ToolDispatch::rejected(),
        },
        "dcc_browser_reload" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<ReloadArgs>(arguments).ok())
        {
            Some(args) if anchor_belongs_to_binding(&args.anchor, binding) => {
                action_result(bridge, binding, args.anchor, BrowserControlAction::Reload).await
            }
            _ => ToolDispatch::rejected(),
        },
        "dcc_browser_scroll" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<ScrollArgs>(arguments).ok())
        {
            Some(args) if anchor_belongs_to_binding(&args.anchor, binding) => {
                action_result(
                    bridge,
                    binding,
                    args.anchor,
                    BrowserControlAction::Scroll {
                        delta_x: args.delta_x,
                        delta_y: args.delta_y,
                    },
                )
                .await
            }
            _ => ToolDispatch::rejected(),
        },
        "dcc_browser_click" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<ClickArgs>(arguments).ok())
        {
            Some(args)
                if valid_browser_reference(&args.reference)
                    && anchor_belongs_to_binding(&args.anchor, binding) =>
            {
                action_result(
                    bridge,
                    binding,
                    args.anchor,
                    BrowserControlAction::Click {
                        reference: args.reference,
                    },
                )
                .await
            }
            _ => ToolDispatch::rejected(),
        },
        "dcc_browser_fill" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<FillArgs>(arguments).ok())
        {
            Some(args)
                if valid_browser_reference(&args.reference)
                    && valid_fill_text(&args.text)
                    && anchor_belongs_to_binding(&args.anchor, binding) =>
            {
                action_result(
                    bridge,
                    binding,
                    args.anchor,
                    BrowserControlAction::Fill {
                        reference: args.reference,
                        text: args.text,
                    },
                )
                .await
            }
            _ => ToolDispatch::rejected(),
        },
        "dcc_browser_evidence_start" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<EvidenceStartArgs>(arguments).ok())
        {
            Some(args) if anchor_belongs_to_binding(&args.anchor, binding) => {
                let lifecycle_token = Some(args.anchor.lifecycle_token);
                let grant_state = browser_audit_grant_state(
                    &bridge.browser,
                    &binding.workspace_id,
                    Some(&binding.session_id),
                    lifecycle_token,
                );
                if bridge.is_shutting_down() {
                    return ToolDispatch::failed();
                }
                // Final lease gate before the helper that installs wrappers.
                // A later revoke is an admitted request race; core validation
                // still fails closed for scope/lifecycle/grant changes.
                if !bridge.lease_is_current(binding) {
                    return ToolDispatch::rejected();
                }
                match start_browser_evidence_capture(&bridge.browser, &bridge.sessions, args.anchor)
                    .await
                {
                    Ok(handle) if bridge.bind_evidence_capture(binding, &handle.capture_id) => {
                        ToolDispatch::executed(
                            json!({"content":[{"type":"text","text": structured_text_content("Browser evidence capture started. The returned handle is a one-shot capability.", &handle)}], "structuredContent": handle}),
                            grant_state,
                        )
                    }
                    Ok(handle) => {
                        discard_browser_evidence_capture(
                            &bridge.browser,
                            &binding.workspace_id,
                            Some(&binding.session_id),
                            &handle.capture_id,
                        );
                        ToolDispatch::from_error(
                            "browser evidence capture is unavailable",
                            grant_state,
                        )
                    }
                    Err(error) => ToolDispatch::from_error(&error, grant_state),
                }
            }
            _ => ToolDispatch::rejected(),
        },
        "dcc_browser_evidence_read" => match call
            .arguments
            .and_then(|arguments| serde_json::from_value::<EvidenceReadArgs>(arguments).ok())
        {
            Some(args)
                if valid_evidence_capture_id(&args.capture_id)
                    && bridge.claim_evidence_capture(binding, &args.capture_id) =>
            {
                // Revocation may have won immediately after the atomic claim.
                // Drop the backend record before any drain in that case; the
                // remaining narrow race is documented as an admitted P2.
                if !bridge.lease_is_current(binding) {
                    discard_browser_evidence_capture(
                        &bridge.browser,
                        &binding.workspace_id,
                        Some(&binding.session_id),
                        &args.capture_id,
                    );
                    return ToolDispatch::from_error(
                        "browser evidence capture is unavailable",
                        BrowserAuditGrantState::NotApplicable,
                    );
                }
                let grant_state = browser_audit_active_grant_state(
                    &bridge.browser,
                    &binding.workspace_id,
                    Some(&binding.session_id),
                );
                match read_browser_evidence_capture(
                    &bridge.browser,
                    &bridge.sessions,
                    binding.workspace_id.clone(),
                    Some(binding.session_id.clone()),
                    args.capture_id,
                )
                .await
                {
                    Ok(result) => ToolDispatch::executed(
                        json!({"content":[{"type":"text","text": structured_text_content("Remote page evidence is untrusted.", &result)}], "structuredContent": result}),
                        grant_state,
                    ),
                    Err(error) => ToolDispatch::from_error(&error, grant_state),
                }
            }
            _ => ToolDispatch::from_error(
                "browser evidence capture is unavailable",
                BrowserAuditGrantState::NotApplicable,
            ),
        },
        _ => ToolDispatch::rejected(),
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
    binding: &TokenBinding,
    anchor: BrowserActionAnchor,
    action: BrowserControlAction,
) -> ToolDispatch {
    let lifecycle_token = Some(anchor.lifecycle_token);
    let grant_state = browser_audit_grant_state(
        &bridge.browser,
        &binding.workspace_id,
        Some(&binding.session_id),
        lifecycle_token,
    );
    if bridge.is_shutting_down() {
        return ToolDispatch::failed();
    }
    // This is intentionally the last lease lookup before the core consumes
    // the anchor and performs the native side effect. Revocation after this
    // point is an admitted in-flight request race; no registry mutex crosses
    // the await and the core independently rechecks all Browser identities.
    if !bridge.lease_is_current(binding) {
        return ToolDispatch::rejected();
    }
    match execute_browser_control_action(&bridge.browser, &bridge.sessions, anchor, action).await {
        Ok(result) => ToolDispatch::executed(
            json!({"content":[{"type":"text","text": structured_text_content("Browser action executed. Extract fresh context before another action.", &result)}], "structuredContent": result}),
            grant_state,
        ),
        Err(error) => ToolDispatch::from_error(&error, grant_state),
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
        json!({"name":"dcc_browser_click","description":"Click one fresh opaque Browser context reference after explicit user approval.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"anchor":anchor,"ref":{"type":"string","minLength":2,"maxLength":MAX_BROWSER_REFERENCE_CHARS,"pattern":"^e(?:[1-9]|[1-7][0-9]|80)$"}},"required":["anchor","ref"]},"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":true}}),
        json!({"name":"dcc_browser_fill","description":"Fill one fresh opaque Browser text reference after explicit user approval.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"anchor":anchor,"ref":{"type":"string","minLength":2,"maxLength":MAX_BROWSER_REFERENCE_CHARS,"pattern":"^e(?:[1-9]|[1-7][0-9]|80)$"},"text":{"type":"string","maxLength":MAX_BROWSER_FILL_TEXT_CHARS}},"required":["anchor","ref","text"]},"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":false}}),
        json!({"name":"dcc_browser_evidence_start","description":"Start one short-lived, bounded Browser evidence capture using a fresh opaque context anchor.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"anchor":anchor},"required":["anchor"]},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}),
        json!({"name":"dcc_browser_evidence_read","description":"Read and consume one bounded Browser evidence capture handle.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"captureId":{"type":"string","minLength":34,"maxLength":34,"pattern":"^c-[a-f0-9]{32}$"}},"required":["captureId"]},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_commands::{bounded_browser_audit_provider_id, read_browser_audit};

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
    fn schemas_are_closed_and_expose_eight_allowlisted_tools() {
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
                "dcc_browser_scroll",
                "dcc_browser_click",
                "dcc_browser_fill",
                "dcc_browser_evidence_start",
                "dcc_browser_evidence_read",
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
        for tool in &tools[4..6] {
            assert_eq!(
                tool["inputSchema"]["properties"]["anchor"]["additionalProperties"],
                Value::Bool(false)
            );
            assert_eq!(
                tool["inputSchema"]["properties"]["ref"]["maxLength"],
                json!(MAX_BROWSER_REFERENCE_CHARS)
            );
            assert_eq!(tool["annotations"]["readOnlyHint"], Value::Bool(false));
            assert_eq!(tool["annotations"]["destructiveHint"], Value::Bool(true));
            assert_eq!(tool["annotations"]["idempotentHint"], Value::Bool(false));
        }
        assert_eq!(
            tools[5]["inputSchema"]["properties"]["text"]["maxLength"],
            json!(MAX_BROWSER_FILL_TEXT_CHARS)
        );
        assert_eq!(tools[4]["annotations"]["openWorldHint"], Value::Bool(true));
        assert_eq!(tools[5]["annotations"]["openWorldHint"], Value::Bool(false));
        assert_eq!(
            tools[6]["annotations"],
            json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false})
        );
        assert_eq!(
            tools[7]["annotations"],
            json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false})
        );
        assert_eq!(
            tools[7]["inputSchema"]["properties"]["captureId"]["pattern"],
            json!("^c-[a-f0-9]{32}$")
        );
        assert_eq!(BROWSER_MCP_TOOL_NAMES.len(), tools.len());
        let policies = browser_mcp_tool_policies();
        assert_eq!(policies.len(), tools.len());
        assert!(policies
            .iter()
            .all(|policy| policy.decision == McpToolPolicyDecision::Ask));
    }

    #[test]
    fn click_and_fill_args_are_closed_bounded_and_never_echo_input() {
        let anchor = json!({
            "workspaceId":"workspace", "sessionId":"session", "lifecycleToken":8,
            "mapId":"m-8-2", "generation":2, "url":"https://example.test/page",
            "pageLoadRevision":4
        });
        let click = ToolCall {
            name: "dcc_browser_click".to_string(),
            arguments: Some(json!({"anchor":anchor, "ref":"e80"})),
        };
        assert!(tool_call_is_well_formed(&click));
        let valid_fill = ToolCall {
            name: "dcc_browser_fill".to_string(),
            arguments: Some(
                json!({"anchor":click.arguments.as_ref().unwrap()["anchor"], "ref":"e1", "text":"safe\ntext"}),
            ),
        };
        assert!(tool_call_is_well_formed(&valid_fill));
        assert!(tool_call_is_well_formed(&ToolCall {
            name: "dcc_browser_fill".to_string(),
            arguments: Some(
                json!({"anchor":click.arguments.as_ref().unwrap()["anchor"], "ref":"e1", "text":"x".repeat(MAX_BROWSER_FILL_TEXT_CHARS)})
            ),
        }));

        for arguments in [
            json!({"anchor":click.arguments.as_ref().unwrap()["anchor"], "ref":"e0"}),
            json!({"anchor":click.arguments.as_ref().unwrap()["anchor"], "ref":"e01"}),
            json!({"anchor":click.arguments.as_ref().unwrap()["anchor"], "ref":"e1", "extra":true}),
            json!({"anchor":{"workspaceId":"workspace","sessionId":"session","lifecycleToken":8,"mapId":"m-8-2","generation":2,"url":"https://example.test/page","pageLoadRevision":4,"extra":true},"ref":"e1"}),
        ] {
            assert!(!tool_call_is_well_formed(&ToolCall {
                name: "dcc_browser_click".to_string(),
                arguments: Some(arguments),
            }));
        }
        for text in [
            "x".repeat(MAX_BROWSER_FILL_TEXT_CHARS + 1),
            "contains\0nul".to_string(),
        ] {
            assert!(!tool_call_is_well_formed(&ToolCall {
                name: "dcc_browser_fill".to_string(),
                arguments: Some(
                    json!({"anchor":click.arguments.as_ref().unwrap()["anchor"], "ref":"e1", "text":text})
                ),
            }));
        }

        let secret = "do-not-return-this";
        let error = tool_error(secret);
        let serialized = serde_json::to_string(&error).unwrap();
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains("e80"));
        assert_eq!(
            tools()[4]["inputSchema"]["properties"]["ref"]["pattern"],
            json!("^e(?:[1-9]|[1-7][0-9]|80)$")
        );
    }

    #[test]
    fn evidence_args_are_closed_and_capture_ids_are_exact() {
        let anchor = json!({
            "workspaceId":"workspace", "sessionId":"session", "lifecycleToken":8,
            "mapId":"m-8-2", "generation":2, "url":"https://example.test/page",
            "pageLoadRevision":4
        });
        let capture_id = "c-0123456789abcdef0123456789abcdef";
        assert!(tool_call_is_well_formed(&ToolCall {
            name: "dcc_browser_evidence_start".to_string(),
            arguments: Some(json!({"anchor":anchor})),
        }));
        assert!(tool_call_is_well_formed(&ToolCall {
            name: "dcc_browser_evidence_read".to_string(),
            arguments: Some(json!({"captureId":capture_id})),
        }));
        for arguments in [
            json!({"anchor":{"workspaceId":"workspace"}, "extra":true}),
            json!({"captureId":"c-0123456789abcdef"}),
            json!({"captureId":"c-0123456789abcdef0123456789abcdeF"}),
            json!({"captureId":capture_id, "extra":true}),
        ] {
            let name = if arguments.get("anchor").is_some() {
                "dcc_browser_evidence_start"
            } else {
                "dcc_browser_evidence_read"
            };
            assert!(!tool_call_is_well_formed(&ToolCall {
                name: name.to_string(),
                arguments: Some(arguments),
            }));
        }
    }

    #[test]
    fn evidence_read_output_and_errors_never_echo_a_capture_handle() {
        let capture_id = "c-0123456789abcdef0123456789abcdef";
        let result = json!({
            "events": [{"kind":"console","level":"warn","message":"bounded"}],
            "truncated": false,
            "untrusted": true
        });
        let read_text = structured_text_content("Remote page evidence is untrusted.", &result);
        assert!(read_text.starts_with("Remote page evidence is untrusted.\n"));
        assert!(!read_text.contains(capture_id));
        let error = serde_json::to_string(&tool_error(capture_id)).unwrap();
        assert!(!error.contains(capture_id));
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

    #[test]
    fn evidence_capture_ownership_is_lease_bound_one_shot_and_revoked() {
        let hash: [u8; 32] = Sha256::digest(b"first").into();
        let other_hash: [u8; 32] = Sha256::digest(b"other").into();
        let entry = |token_hash, lease_id: &str| TokenBinding {
            token_hash,
            lease_id: lease_id.to_string(),
            workspace_id: "workspace".to_string(),
            session_id: "session".to_string(),
            provider_id: "provider".to_string(),
            phase: LeasePhase::Ready(MCP_PROTOCOL_VERSION),
        };
        let first = entry(hash, "lease-one");
        let other = entry(other_hash, "lease-two");
        let mut registry = TokenRegistry::default();
        registry
            .by_lease
            .insert(first.lease_id.clone(), first.clone());
        registry
            .by_lease
            .insert(other.lease_id.clone(), other.clone());
        assert!(registry.bind_evidence_capture(&first, "c-0123456789abcdef0123456789abcdef"));
        assert!(
            !registry.claim_evidence_capture(&other.lease_id, "c-0123456789abcdef0123456789abcdef")
        );
        assert!(
            registry.claim_evidence_capture(&first.lease_id, "c-0123456789abcdef0123456789abcdef")
        );
        assert!(
            !registry.claim_evidence_capture(&first.lease_id, "c-0123456789abcdef0123456789abcdef")
        );
        assert!(registry.bind_evidence_capture(&first, "c-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(registry.bind_evidence_capture(&first, "c-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        assert!(
            !registry.claim_evidence_capture(&first.lease_id, "c-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        let removed = registry.remove_lease(&first.lease_id);
        assert_eq!(removed, vec!["c-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"]);
        assert!(
            !registry.claim_evidence_capture(&first.lease_id, "c-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn shutdown_cleanup_collects_capture_ids_with_their_scope_binding() {
        let hash: [u8; 32] = Sha256::digest(b"shutdown").into();
        let binding = TokenBinding {
            token_hash: hash,
            lease_id: "lease-shutdown".to_string(),
            workspace_id: "workspace".to_string(),
            session_id: "session".to_string(),
            provider_id: "provider".to_string(),
            phase: LeasePhase::Ready(MCP_PROTOCOL_VERSION),
        };
        let mut registry = TokenRegistry::default();
        registry
            .by_lease
            .insert(binding.lease_id.clone(), binding.clone());
        assert!(registry.bind_evidence_capture(&binding, "c-0123456789abcdef0123456789abcdef"));
        let cleanup = registry.take_all_evidence_captures();
        assert_eq!(cleanup.len(), 1);
        assert_eq!(cleanup[0].0.workspace_id, "workspace");
        assert_eq!(cleanup[0].0.session_id, "session");
        assert_eq!(cleanup[0].1, "c-0123456789abcdef0123456789abcdef");
        assert!(registry.evidence_capture_leases.is_empty());
    }

    #[test]
    fn every_allowlisted_mcp_tool_has_one_closed_audit_mapping() {
        let expected = [
            ("dcc_browser_context", BrowserAuditTool::Context),
            ("dcc_browser_navigate", BrowserAuditTool::Navigate),
            ("dcc_browser_reload", BrowserAuditTool::Reload),
            ("dcc_browser_scroll", BrowserAuditTool::Scroll),
            ("dcc_browser_click", BrowserAuditTool::Click),
            ("dcc_browser_fill", BrowserAuditTool::Fill),
            (
                "dcc_browser_evidence_start",
                BrowserAuditTool::EvidenceStart,
            ),
            ("dcc_browser_evidence_read", BrowserAuditTool::EvidenceRead),
        ];
        assert_eq!(expected.len(), BROWSER_MCP_TOOL_NAMES.len());
        for (name, tool) in expected {
            assert_eq!(browser_audit_tool_for_mcp(name), Some(tool));
        }
        assert_eq!(browser_audit_tool_for_mcp("unknown"), None);
    }

    #[test]
    fn audit_lease_fingerprint_is_short_deterministic_and_never_the_lease() {
        let lease_id = "lease-secret-must-not-be-persisted";
        let fingerprint = lease_fingerprint(lease_id);
        assert_eq!(fingerprint, lease_fingerprint(lease_id));
        assert_eq!(fingerprint.len(), 24);
        assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(fingerprint, lease_id);
        assert_ne!(
            fingerprint,
            hex::encode(Sha256::digest(lease_id.as_bytes()))
        );
    }

    #[test]
    fn audit_provider_is_control_free_and_bounded_without_losing_the_record() {
        let provider = format!("provider\0{}", "x".repeat(200));
        let bounded = bounded_browser_audit_provider_id(&provider);
        assert_eq!(bounded.chars().count(), 128);
        assert!(!bounded.chars().any(char::is_control));
        assert_eq!(bounded_browser_audit_provider_id("\0\n"), "unknown");
    }

    #[test]
    fn admitted_dispatches_append_exactly_one_content_free_audit_record_each() {
        let state = BrowserState::default();
        let binding = TokenBinding {
            token_hash: Sha256::digest(b"audit-token-hash-only").into(),
            lease_id: "lease-secret-must-not-appear".to_string(),
            workspace_id: "workspace".to_string(),
            session_id: "session".to_string(),
            provider_id: "provider\0with-control".to_string(),
            phase: LeasePhase::Ready(MCP_PROTOCOL_VERSION),
        };
        let sensitive_payload = "https://example.test/path?secret=payload&e1&fill-text&c-0123456789abcdef0123456789abcdef";
        let dispatched = [
            (
                "dcc_browser_context",
                ToolDispatch::executed(
                    json!({"unused":sensitive_payload}),
                    BrowserAuditGrantState::Armed,
                ),
            ),
            (
                "dcc_browser_navigate",
                ToolDispatch::from_error(
                    "browser control is not armed",
                    BrowserAuditGrantState::Missing,
                ),
            ),
            (
                "dcc_browser_reload",
                ToolDispatch::from_error(
                    "browser action anchor is stale",
                    BrowserAuditGrantState::Armed,
                ),
            ),
            ("dcc_browser_scroll", ToolDispatch::failed()),
            // A shutdown admitted just before the boundary remains one failed
            // audit event; the closed enum deliberately exposes no detail.
            ("dcc_browser_click", ToolDispatch::failed()),
        ];
        for (tool, dispatched) in &dispatched {
            append_mcp_tool_audit(&state, &binding, tool, dispatched);
        }
        let records = read_browser_audit(&state, "workspace", Some("session"), 10).unwrap();
        assert_eq!(records.len(), dispatched.len());
        assert_eq!(records[0].outcome, BrowserAuditOutcome::Failed);
        assert_eq!(records[1].outcome, BrowserAuditOutcome::Failed);
        assert_eq!(records[2].outcome, BrowserAuditOutcome::Stale);
        assert_eq!(records[3].outcome, BrowserAuditOutcome::NotArmed);
        assert_eq!(records[4].outcome, BrowserAuditOutcome::Executed);
        assert_eq!(
            records[0].grant_state,
            BrowserAuditGrantState::NotApplicable
        );
        assert_eq!(
            records[1].grant_state,
            BrowserAuditGrantState::NotApplicable
        );
        assert_eq!(records[2].grant_state, BrowserAuditGrantState::Armed);
        assert_eq!(records[3].grant_state, BrowserAuditGrantState::Missing);
        assert_eq!(records[4].grant_state, BrowserAuditGrantState::Armed);
        let rendered = serde_json::to_string(&records).unwrap();
        for forbidden in [
            sensitive_payload,
            "lease-secret-must-not-appear",
            "audit-token-hash-only",
            "fill-text",
            "c-0123456789abcdef0123456789abcdef",
        ] {
            assert!(!rendered.contains(forbidden));
        }
    }

    #[test]
    fn revoked_lease_fails_the_pre_dispatch_gate_before_any_controlled_helper() {
        let binding = TokenBinding {
            token_hash: Sha256::digest(b"lease-token").into(),
            lease_id: "lease-to-revoke".to_string(),
            workspace_id: "workspace".to_string(),
            session_id: "session".to_string(),
            provider_id: "provider".to_string(),
            phase: LeasePhase::Ready(MCP_PROTOCOL_VERSION),
        };
        let mut registry = TokenRegistry::default();
        registry
            .by_lease
            .insert(binding.lease_id.clone(), binding.clone());
        assert!(registry.binding_is_current(&binding));
        registry.remove_lease(&binding.lease_id);
        // `dispatch_tool`/`action_result` check this immediately before every
        // controlled helper, so a revoke winning before that boundary cannot
        // consume a map, install evidence wrappers, read a page, or act.
        assert!(!registry.binding_is_current(&binding));
    }
}
