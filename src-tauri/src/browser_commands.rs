//! The Browser's bookkeeping: the scope and lifecycle, the agent's control
//! grant, semantic maps, evidence and the audit. The page itself is the native
//! app's (wry + WKWebView there); every page operation goes to it through the
//! bridge (`http_api::browser_bridge`).

// The URL policy lives in dcc-core, shared with the native app's Browser.
pub use dcc_core::domain::browser::validate_browser_url;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::{timeout, Duration};
use url::Url;

pub use dcc_core::domain::browser_context::*;
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;
use dev_command_center_tauri::http_api as dcc_http;

const BROWSER_CONTEXT_TIMEOUT: Duration = Duration::from_secs(2);
/// A deliberate, short-lived consent window. It is in-memory only and must be
/// armed again after expiry, a new Browser lifecycle, or any close; a future UI
/// may renew it only through another explicit user gesture.
const BROWSER_CONTROL_GRANT_TTL: Duration = Duration::from_secs(60);
/// A Browser-open request approved from the agent request queue is still
/// bound to one native surface lifecycle, but is intentionally long enough
/// for a human login handoff and the following bounded agent work.
pub(crate) const APPROVED_BROWSER_OPEN_GRANT_TTL: Duration = Duration::from_secs(10 * 60);
/// Scroll is deliberately constrained to a small single action in CSS pixels.
const MAX_BROWSER_SCROLL_DELTA: f64 = 2_000.0;
const MAX_BROWSER_FILL_CHARS: usize = 2_000;
const MAX_BROWSER_REFERENCE_CHARS: usize = 3;
const MAX_BROWSER_EVIDENCE_EVENTS: usize = 32;
const MAX_BROWSER_EVIDENCE_MESSAGE_CHARS: usize = 240;
const MAX_BROWSER_EVIDENCE_URL_CHARS: usize = 512;
const MAX_BROWSER_EVIDENCE_CHARS: usize = 12_000;
const MAX_BROWSER_EVIDENCE_CALLBACK_CHARS: usize = 24_000;
const MAX_BROWSER_EVIDENCE_LINE: u32 = 1_000_000;
/// Resource Timing duration is rounded to milliseconds and never reveals a
/// timing larger than this short, explicit evidence capture window.
const MAX_BROWSER_EVIDENCE_RESOURCE_DURATION_MS: u32 = 60_000;
const BROWSER_EVIDENCE_TTL: Duration = Duration::from_secs(60);
const BROWSER_EVIDENCE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_BROWSER_AUDIT_ENTRIES: usize = 256;
const MAX_BROWSER_AUDIT_SCOPE_CHARS: usize = 128;
const MAX_BROWSER_AUDIT_PROVIDER_CHARS: usize = 128;
const MAX_BROWSER_AUDIT_LEASE_FINGERPRINT_CHARS: usize = 64;
const MAX_BROWSER_AUDIT_READ_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BrowserAuditOrigin {
    Ui,
    Mcp,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BrowserAuditTool {
    #[serde(rename = "dcc_browser_status")]
    Status,
    #[serde(rename = "dcc_browser_open")]
    Open,
    #[serde(rename = "dcc_browser_context")]
    Context,
    #[serde(rename = "dcc_browser_navigate")]
    Navigate,
    #[serde(rename = "dcc_browser_reload")]
    Reload,
    #[serde(rename = "dcc_browser_scroll")]
    Scroll,
    #[serde(rename = "dcc_browser_click")]
    Click,
    #[serde(rename = "dcc_browser_fill")]
    Fill,
    #[serde(rename = "dcc_browser_select")]
    Select,
    #[serde(rename = "dcc_browser_press")]
    Press,
    #[serde(rename = "dcc_browser_screenshot")]
    Screenshot,
    #[serde(rename = "dcc_browser_evidence_start")]
    EvidenceStart,
    #[serde(rename = "dcc_browser_evidence_read")]
    EvidenceRead,
    #[serde(rename = "browser_arm_control")]
    ArmControl,
    #[serde(rename = "browser_disarm_control")]
    DisarmControl,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BrowserAuditGrantState {
    Armed,
    Expired,
    Missing,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BrowserAuditOutcome {
    Executed,
    Rejected,
    Stale,
    NotArmed,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserAuditRecord {
    pub origin: BrowserAuditOrigin,
    pub provider_id: Option<String>,
    pub lease_fingerprint: Option<String>,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub tool: BrowserAuditTool,
    pub grant_state: BrowserAuditGrantState,
    pub outcome: BrowserAuditOutcome,
    pub timestamp_ms: u64,
}

/// The only audit shape exposed by the trusted viewer. Scope and lease
/// identity have already been validated internally and are intentionally not
/// returned to the renderer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAuditViewRecord {
    pub(crate) origin: BrowserAuditOrigin,
    pub(crate) provider_id: Option<String>,
    pub(crate) tool: BrowserAuditTool,
    pub(crate) grant_state: BrowserAuditGrantState,
    pub(crate) outcome: BrowserAuditOutcome,
    pub(crate) timestamp_ms: u64,
}

impl From<BrowserAuditRecord> for BrowserAuditViewRecord {
    fn from(record: BrowserAuditRecord) -> Self {
        Self {
            origin: record.origin,
            provider_id: record.provider_id,
            tool: record.tool,
            grant_state: record.grant_state,
            outcome: record.outcome,
            timestamp_ms: record.timestamp_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshot {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub lifecycle_token: u64,
    pub visible: bool,
    pub loading: bool,
    pub url: Option<String>,
    pub title: Option<String>,
}

/// Complete identity required for a future provider-neutral Browser control
/// action. It is untrusted input and is compared byte-for-byte with the
/// server-side semantic-map record before anything can happen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserActionAnchor {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub lifecycle_token: u64,
    pub map_id: String,
    pub generation: u64,
    pub url: String,
    pub page_load_revision: u64,
}

/// A short-lived, one-shot handle for a user-requested Browser evidence read.
/// The renderer receives only this opaque id; the page token is backend-owned,
/// never logged, and never persisted.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEvidenceCaptureHandle {
    pub capture_id: String,
    pub remaining_ms: u64,
}

/// Remote page evidence is deliberately labeled untrusted. It contains only
/// bounded, redacted event summaries and is not retained after this response.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEvidenceResult {
    pub events: Vec<BrowserEvidenceEvent>,
    pub truncated: bool,
    pub untrusted: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEvidenceEvent {
    pub kind: String,
    pub sequence: u64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initiator_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

/// This is intentionally narrower than an MCP bridge: actions may carry only
/// a current opaque map reference and bounded fill text; selectors, HTML,
/// runtime values, and arbitrary page script can never be supplied.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BrowserControlAction {
    Navigate { url: String },
    Reload,
    Scroll { delta_x: f64, delta_y: f64 },
    Click { reference: String },
    Fill { reference: String, text: String },
    Select { reference: String, label: String },
    Press { reference: String, key: String },
}

enum PreparedBrowserControlAction {
    Navigate(Url),
    Reload,
    Scroll { delta_x: f64, delta_y: f64 },
    Click { reference: String },
    Fill { reference: String, text: String },
    Select { reference: String, label: String },
    Press { reference: String, key: String },
}

impl PreparedBrowserControlAction {
    fn name(&self) -> &'static str {
        match self {
            Self::Navigate(_) => "navigate",
            Self::Reload => "reload",
            Self::Scroll { .. } => "scroll",
            Self::Click { .. } => "click",
            Self::Fill { .. } => "fill",
            Self::Select { .. } => "select",
            Self::Press { .. } => "press",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserActionResult {
    pub action: String,
    pub status: String,
    pub requires_context_refresh: bool,
}

/// Bridge-only context coupled to an opaque action identity. It never carries
/// a locator, DOM data, credentials, or a renderer-provided script.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserControlContext {
    pub context: BrowserAgentContext,
    pub anchor: BrowserActionAnchor,
}

#[derive(Debug, Clone, Default)]
struct BrowserContext {
    url: Option<String>,
    title: Option<String>,
    /// URL requested by the active scope. Native callbacks do not carry a
    /// scope id, so callbacks are accepted only while this URL still matches.
    expected_url: Option<String>,
    /// The URL explicitly approved when this Browser lifecycle was opened.
    /// `expected_url` follows the main document through redirects, while this
    /// remains the capability check for the original open request.
    approved_open_url: Option<String>,
    /// URL whose finished load was observed for this scope. Context extraction
    /// is unavailable until it matches `expected_url` and the native URL.
    ready_url: Option<String>,
    /// Changes on every navigation/load start, so a map can never survive a
    /// page transition merely because the final URL happens to be identical.
    page_load_revision: u64,
    /// KVO can synchronously replay a reused view's old URL. A ready event is
    /// accepted only after a real load start, unless the URL itself changed
    /// through a same-document history transition.
    main_load_started: bool,
    pending_previous_main_url: Option<String>,
    semantic_map_generation: u64,
    semantic_map: Option<BrowserSemanticMapRecord>,
}

#[derive(Debug, Clone)]
struct BrowserSemanticMapRecord {
    scope: String,
    map_id: String,
    generation: u64,
    lifecycle_token: u64,
    url: String,
    page_load_revision: u64,
    document_identity: f64,
    targets: Vec<BrowserSemanticTargetRecord>,
}

#[derive(Debug, Clone)]
struct BrowserControlGrant {
    scope: String,
    lifecycle_token: u64,
    /// Present only for an agent-approved Browser-open grant. It prevents a
    /// replacement projection for the same session from inheriting consent.
    lease_id: Option<String>,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
struct BrowserEvidenceCaptureRecord {
    scope: String,
    capture_token: String,
    lifecycle_token: u64,
    url: String,
    page_load_revision: u64,
    document_identity: f64,
    expires_at: Instant,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserControlStatus {
    pub armed: bool,
    pub remaining_ms: u64,
}

/// Runtime state for the one browser child WebView. Context is keyed by the
/// active workspace and optional session, while the native view is reused.
#[derive(Clone)]
pub struct BrowserState {
    /// The native app's Browser is the surface: page operations go to it
    /// through the bridge.
    remote: Arc<Mutex<bool>>,
    contexts: Arc<Mutex<HashMap<String, BrowserContext>>>,
    active_scope: Arc<Mutex<Option<String>>>,
    active_workspace: Arc<Mutex<Option<String>>>,
    active_session: Arc<Mutex<Option<String>>>,
    visible: Arc<Mutex<bool>>,
    /// Monotonically identifies one intentional browser-open lifecycle. A
    /// stale temporary-occlusion restore can never affect a later reopen.
    lifecycle_token: Arc<Mutex<u64>>,
    lifecycle_open: Arc<Mutex<bool>>,
    occluded: Arc<Mutex<bool>>,
    /// Temporary backend-only consent. There is no persisted token and no
    /// renderer capability beyond the scoped arm/disarm commands.
    control_grant: Arc<Mutex<Option<BrowserControlGrant>>>,
    /// At most one active evidence capture is expected per scope. The page
    /// owns only a random, non-enumerable key; the backend owns this record and
    /// consumes it before draining the page-side ring.
    evidence_captures: Arc<Mutex<HashMap<String, BrowserEvidenceCaptureRecord>>>,
    /// Content-free runtime audit records. This is intentionally not persisted
    /// or emitted; restart and the bounded ring naturally clear this telemetry.
    browser_audit: Arc<Mutex<VecDeque<BrowserAuditRecord>>>,
    /// Serializes native child-WebView operations in IPC arrival order, also
    /// across async scope validation. Context extraction keeps it through its
    /// short, bounded native callback wait so a new command cannot change its
    /// scope mid-extraction.
    operation_lock: Arc<AsyncMutex<()>>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            remote: Arc::new(Mutex::new(false)),
            contexts: Arc::new(Mutex::new(HashMap::new())),
            active_scope: Arc::new(Mutex::new(None)),
            active_workspace: Arc::new(Mutex::new(None)),
            active_session: Arc::new(Mutex::new(None)),
            visible: Arc::new(Mutex::new(false)),
            lifecycle_token: Arc::new(Mutex::new(0)),
            lifecycle_open: Arc::new(Mutex::new(false)),
            occluded: Arc::new(Mutex::new(false)),
            control_grant: Arc::new(Mutex::new(None)),
            evidence_captures: Arc::new(Mutex::new(HashMap::new())),
            browser_audit: Arc::new(Mutex::new(VecDeque::with_capacity(
                MAX_BROWSER_AUDIT_ENTRIES,
            ))),
            operation_lock: Arc::new(AsyncMutex::new(())),
        }
    }
}

fn scope_key(workspace_id: &str, session_id: Option<&str>) -> String {
    format!("{}\u{1f}|{}", workspace_id, session_id.unwrap_or(""))
}

fn bounded_browser_audit_field(value: &str, max_chars: usize) -> Option<String> {
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.to_string())
}

fn bounded_browser_audit_lease_fingerprint(value: &str) -> Option<String> {
    let value = bounded_browser_audit_field(value, MAX_BROWSER_AUDIT_LEASE_FINGERPRINT_CHARS)?;
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte.is_ascii_hexdigit())
        .then_some(value)
}

/// Provider identity originates in the trusted session registry, but MCP
/// audit must still be bounded even if an imported provider id is hostile.
/// Unlike scope identity, a malformed provider falls back to a fixed label so
/// an otherwise admissible MCP call still gets exactly one audit record.
pub(crate) fn bounded_browser_audit_provider_id(value: &str) -> String {
    let bounded = value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_BROWSER_AUDIT_PROVIDER_CHARS)
        .collect::<String>();
    if bounded.is_empty() {
        "unknown".to_string()
    } else {
        bounded
    }
}

fn browser_audit_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}

/// Appends only closed, content-free Browser telemetry. Audit failure is
/// deliberately best-effort: it must never turn a valid Browser operation
/// into an operation failure or expose a lock/error payload to the caller.
pub(crate) fn append_browser_audit(
    state: &BrowserState,
    origin: BrowserAuditOrigin,
    provider_id: Option<&str>,
    lease_fingerprint: Option<&str>,
    workspace_id: &str,
    session_id: Option<&str>,
    tool: BrowserAuditTool,
    grant_state: BrowserAuditGrantState,
    outcome: BrowserAuditOutcome,
) {
    let Some(workspace_id) =
        bounded_browser_audit_field(workspace_id, MAX_BROWSER_AUDIT_SCOPE_CHARS)
    else {
        return;
    };
    let session_id = match session_id {
        Some(session_id) => Some(
            match bounded_browser_audit_field(session_id, MAX_BROWSER_AUDIT_SCOPE_CHARS) {
                Some(session_id) => session_id,
                None => return,
            },
        ),
        None => None,
    };
    let provider_id = match provider_id {
        Some(provider_id) => Some(bounded_browser_audit_provider_id(provider_id)),
        None => None,
    };
    let lease_fingerprint = match lease_fingerprint {
        Some(lease_fingerprint) => Some(
            match bounded_browser_audit_lease_fingerprint(lease_fingerprint) {
                Some(lease_fingerprint) => lease_fingerprint,
                None => return,
            },
        ),
        None => None,
    };
    if matches!(origin, BrowserAuditOrigin::Ui)
        && (provider_id.is_some() || lease_fingerprint.is_some())
    {
        return;
    }
    if matches!(origin, BrowserAuditOrigin::Mcp)
        && (provider_id.is_none() || lease_fingerprint.is_none())
    {
        return;
    }
    let record = BrowserAuditRecord {
        origin,
        provider_id,
        lease_fingerprint,
        workspace_id,
        session_id,
        tool,
        grant_state,
        outcome,
        timestamp_ms: browser_audit_timestamp_ms(),
    };
    if let Ok(mut audit) = state.browser_audit.lock() {
        if audit.len() >= MAX_BROWSER_AUDIT_ENTRIES {
            audit.pop_front();
        }
        audit.push_back(record);
    }
}

/// Reads a bounded newest-first snapshot for trusted in-process UI code. The
/// Tauri viewer adds the active scope/lifecycle checks before calling this;
/// MCP never receives a query surface for runtime audit records.
pub(crate) fn read_browser_audit(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    limit: usize,
) -> Result<Vec<BrowserAuditRecord>, String> {
    if limit == 0 || limit > MAX_BROWSER_AUDIT_READ_LIMIT {
        return Err("browser audit limit is invalid".to_string());
    }
    if bounded_browser_audit_field(workspace_id, MAX_BROWSER_AUDIT_SCOPE_CHARS).is_none() {
        return Err("browser audit workspace is invalid".to_string());
    }
    if session_id.is_some_and(|session| {
        bounded_browser_audit_field(session, MAX_BROWSER_AUDIT_SCOPE_CHARS).is_none()
    }) {
        return Err("browser audit session is invalid".to_string());
    }
    let audit = state
        .browser_audit
        .lock()
        .map_err(|_| "browser audit is unavailable".to_string())?;
    Ok(audit
        .iter()
        .rev()
        .filter(|record| {
            record.workspace_id == workspace_id && record.session_id.as_deref() == session_id
        })
        .take(limit)
        .cloned()
        .collect())
}

pub(crate) fn browser_audit_grant_state(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: Option<u64>,
) -> BrowserAuditGrantState {
    let Ok(mut grant) = state.control_grant.lock() else {
        return BrowserAuditGrantState::Missing;
    };
    let now = Instant::now();
    if grant.as_ref().is_some_and(|grant| grant.expires_at <= now) {
        *grant = None;
        return BrowserAuditGrantState::Expired;
    }
    let Some(grant) = grant.as_ref() else {
        return BrowserAuditGrantState::Missing;
    };
    if grant.scope != scope_key(workspace_id, session_id)
        || lifecycle_token.is_some_and(|token| grant.lifecycle_token != token)
    {
        return BrowserAuditGrantState::Missing;
    }
    BrowserAuditGrantState::Armed
}

/// Captures the grant state for a controlled read whose lifecycle is owned by
/// the active native Browser surface. The snapshot is audit metadata only: the
/// controlled helper still validates scope, lifecycle, visibility, and grant
/// under its operation lock before it can read or act.
pub(crate) fn browser_audit_active_grant_state(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
) -> BrowserAuditGrantState {
    let expected_scope = scope_key(workspace_id, session_id);
    let active_scope = match state.active_scope.lock() {
        Ok(scope) => scope.clone(),
        Err(_) => return BrowserAuditGrantState::Missing,
    };
    if active_scope.as_deref() != Some(expected_scope.as_str()) {
        return BrowserAuditGrantState::Missing;
    }
    let lifecycle_open = match state.lifecycle_open.lock() {
        Ok(open) => *open,
        Err(_) => return BrowserAuditGrantState::Missing,
    };
    if !lifecycle_open {
        return BrowserAuditGrantState::Missing;
    }
    let lifecycle_token = match state.lifecycle_token.lock() {
        Ok(token) => *token,
        Err(_) => return BrowserAuditGrantState::Missing,
    };
    browser_audit_grant_state(state, workspace_id, session_id, Some(lifecycle_token))
}

pub(crate) fn browser_audit_outcome(error: Option<&str>) -> BrowserAuditOutcome {
    let Some(error) = error else {
        return BrowserAuditOutcome::Executed;
    };
    if error.contains("not armed") {
        BrowserAuditOutcome::NotArmed
    } else if error.contains("stale") || error.contains("changed") {
        BrowserAuditOutcome::Stale
    } else if error.contains("invalid")
        || error.contains("incompatible")
        || error.contains("visible")
        || error.contains("disabled")
    {
        BrowserAuditOutcome::Rejected
    } else {
        BrowserAuditOutcome::Failed
    }
}

fn browser_audit_tool_for_action(action: &BrowserControlAction) -> BrowserAuditTool {
    match action {
        BrowserControlAction::Navigate { .. } => BrowserAuditTool::Navigate,
        BrowserControlAction::Reload => BrowserAuditTool::Reload,
        BrowserControlAction::Scroll { .. } => BrowserAuditTool::Scroll,
        BrowserControlAction::Click { .. } => BrowserAuditTool::Click,
        BrowserControlAction::Fill { .. } => BrowserAuditTool::Fill,
        BrowserControlAction::Select { .. } => BrowserAuditTool::Select,
        BrowserControlAction::Press { .. } => BrowserAuditTool::Press,
    }
}

fn issue_semantic_map(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    url: &str,
    expected_page_load_revision: u64,
    document_identity: f64,
    items: Vec<BrowserSemanticItem>,
    targets: Vec<BrowserSemanticTargetRecord>,
    truncated: bool,
) -> Result<BrowserSemanticMap, String> {
    let scope = scope_key(workspace_id, session_id);
    let mut contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get_mut(&scope)
        .ok_or_else(|| "browser page context is unavailable".to_string())?;
    if !context_is_ready_for_url(context, url)
        || !page_load_revision_is_current(context.page_load_revision, expected_page_load_revision)
    {
        return Err("browser page changed while building semantic map".to_string());
    }
    context.semantic_map_generation = context.semantic_map_generation.wrapping_add(1).max(1);
    let generation = context.semantic_map_generation;
    let map_id = format!("m-{lifecycle_token}-{generation}");
    context.semantic_map = Some(BrowserSemanticMapRecord {
        scope: scope.clone(),
        map_id: map_id.clone(),
        generation,
        lifecycle_token,
        url: url.to_string(),
        page_load_revision: expected_page_load_revision,
        document_identity,
        targets,
    });
    if !semantic_map_record_is_current(
        context.semantic_map.as_ref(),
        &scope,
        lifecycle_token,
        url,
        expected_page_load_revision,
        &map_id,
        generation,
    ) {
        return Err("browser semantic map became stale".to_string());
    }
    Ok(BrowserSemanticMap {
        map_id,
        generation,
        page_load_revision: expected_page_load_revision,
        items,
        truncated,
    })
}

fn current_page_load_revision(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    url: &str,
) -> Result<u64, String> {
    let contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get(&scope_key(workspace_id, session_id))
        .ok_or_else(|| "browser page context is unavailable".to_string())?;
    if !context_is_ready_for_url(context, url) {
        return Err("browser page changed while reading context".to_string());
    }
    Ok(context.page_load_revision)
}

fn clear_browser_control_grant(state: &BrowserState) {
    if let Ok(mut grant) = state.control_grant.lock() {
        *grant = None;
    }
}

fn control_grant_is_current(
    grant: Option<&BrowserControlGrant>,
    scope: &str,
    lifecycle_token: u64,
    now: Instant,
) -> bool {
    grant.is_some_and(|grant| {
        grant.scope == scope && grant.lifecycle_token == lifecycle_token && grant.expires_at > now
    })
}

fn control_grant_status(
    grant: &mut Option<BrowserControlGrant>,
    scope: &str,
    lifecycle_token: u64,
    now: Instant,
) -> BrowserControlStatus {
    if grant.as_ref().is_some_and(|grant| grant.expires_at <= now) {
        *grant = None;
    }
    let Some(grant) = grant.as_ref() else {
        return BrowserControlStatus {
            armed: false,
            remaining_ms: 0,
        };
    };
    if !control_grant_is_current(Some(grant), scope, lifecycle_token, now) {
        return BrowserControlStatus {
            armed: false,
            remaining_ms: 0,
        };
    }
    BrowserControlStatus {
        armed: true,
        remaining_ms: grant
            .expires_at
            .duration_since(now)
            .as_millis()
            .min(u64::MAX as u128) as u64,
    }
}

fn arm_browser_control_grant(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    now: Instant,
) -> Result<BrowserControlStatus, String> {
    arm_browser_control_grant_for_duration(
        state,
        workspace_id,
        session_id,
        lifecycle_token,
        now,
        BROWSER_CONTROL_GRANT_TTL,
        None,
    )
}

fn arm_browser_control_grant_for_duration(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    now: Instant,
    duration: Duration,
    lease_id: Option<&str>,
) -> Result<BrowserControlStatus, String> {
    let mut grant = state
        .control_grant
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    *grant = Some(BrowserControlGrant {
        scope: scope_key(workspace_id, session_id),
        lifecycle_token,
        lease_id: lease_id.map(str::to_string),
        expires_at: now + duration,
    });
    Ok(control_grant_status(
        &mut grant,
        &scope_key(workspace_id, session_id),
        lifecycle_token,
        now,
    ))
}

/// Completes an agent-originated open only after the renderer has opened the
/// native Browser. The renderer supplies a lifecycle number but cannot mint a
/// grant: the active scope, open lifecycle and visible native surface are all
/// checked from backend state immediately before the grant is installed.
pub(crate) fn arm_browser_control_for_approved_open(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    lease_id: &str,
) -> Result<BrowserControlStatus, String> {
    if lease_id.is_empty()
        || lease_id.chars().count() > 128
        || lease_id.chars().any(char::is_control)
    {
        return Err("browser open lease is invalid".to_string());
    }
    require_current_lifecycle(
        state,
        workspace_id,
        session_id,
        lifecycle_token,
        "browser open lifecycle is stale",
    )?;
    let snapshot = current_snapshot(state)?;
    if !browser_surface_matches_visibility_policy(
        state,
        snapshot.visible,
        BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
    )? {
        return Err("browser is not visible".to_string());
    }
    arm_browser_control_grant_for_duration(
        state,
        workspace_id,
        session_id,
        lifecycle_token,
        Instant::now(),
        APPROVED_BROWSER_OPEN_GRANT_TTL,
        Some(lease_id),
    )
}

/// Returns true while the active grant is either the legacy explicit UI grant
/// or was created by this exact projection lease. A lease-bound grant never
/// becomes usable by a replacement projection for the same session.
pub(crate) fn browser_approved_lease_matches(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    lease_id: &str,
) -> bool {
    state.control_grant.lock().is_ok_and(|mut grant| {
        if grant
            .as_ref()
            .is_some_and(|grant| grant.expires_at <= Instant::now())
        {
            *grant = None;
        }
        grant.as_ref().is_some_and(|grant| {
            grant.scope == scope_key(workspace_id, session_id)
                && grant.lifecycle_token == lifecycle_token
                && grant
                    .lease_id
                    .as_deref()
                    .is_none_or(|bound| bound == lease_id)
        })
    })
}

pub(crate) fn revoke_browser_approved_lease(state: &BrowserState, lease_id: &str) {
    if let Ok(mut grant) = state.control_grant.lock() {
        if grant
            .as_ref()
            .is_some_and(|grant| grant.lease_id.as_deref() == Some(lease_id))
        {
            *grant = None;
        }
    }
}

/// Captures only the integrated Browser viewport for an already authenticated
/// projection. Both authorization checks run under the Browser operation lock:
/// a revoke, scope change, lifecycle replacement or expired grant that occurs
/// during the native asynchronous snapshot causes the bytes to be discarded.
pub(crate) async fn capture_browser_viewport_for_lease(
    state: &BrowserState,
    workspace_id: &str,
    session_id: &str,
    lifecycle_token: u64,
    lease_id: &str,
) -> Result<Vec<u8>, String> {
    let _operation = state.operation_lock.lock().await;
    let authorize = || -> Result<(), String> {
        require_current_lifecycle(
            state,
            workspace_id,
            Some(session_id),
            lifecycle_token,
            "browser screenshot lifecycle is stale",
        )?;
        if !browser_approved_lease_matches(
            state,
            workspace_id,
            Some(session_id),
            lifecycle_token,
            lease_id,
        ) {
            return Err(
                "browser screenshot grant does not belong to this provider session".to_string(),
            );
        }
        require_browser_control_grant(
            state,
            workspace_id,
            Some(session_id),
            lifecycle_token,
            Instant::now(),
        )
    };
    authorize()?;
    let before = current_snapshot(state)?;
    if !browser_surface_matches_visibility_policy(
        state,
        before.visible,
        BrowserVisibilityPolicy::RequireVisible,
    )? {
        return Err("browser is not visible".to_string());
    }
    let initial_url = before
        .url
        .as_deref()
        .ok_or_else(|| "browser page URL is unavailable".to_string())?;
    let revision = current_page_load_revision(state, workspace_id, Some(session_id), initial_url)?;
    if !context_ready_for_scope(state, workspace_id, Some(session_id), initial_url)? {
        return Err("browser page is still loading".to_string());
    }
    let bytes = match page_driver(state)? {
        PageDriver::Remote => remote_capture().await?,
    };
    authorize()?;
    let after = current_snapshot(state)?;
    if !browser_surface_matches_visibility_policy(
        state,
        after.visible,
        BrowserVisibilityPolicy::RequireVisible,
    )? || after.url != before.url
        || !context_ready_for_scope(state, workspace_id, Some(session_id), initial_url)?
        || current_page_load_revision(state, workspace_id, Some(session_id), initial_url)?
            != revision
    {
        return Err("browser page changed while taking screenshot".to_string());
    }
    Ok(bytes)
}

fn require_browser_control_grant(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    now: Instant,
) -> Result<(), String> {
    let scope = scope_key(workspace_id, session_id);
    let mut grant = state
        .control_grant
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    if grant.as_ref().is_some_and(|grant| grant.expires_at <= now) {
        *grant = None;
    }
    if control_grant_is_current(grant.as_ref(), &scope, lifecycle_token, now) {
        // An agent-approved grant lasts while the agent keeps using it; the
        // short manual grant does not.
        if let Some(grant) = grant.as_mut().filter(|grant| grant.lease_id.is_some()) {
            grant.expires_at = now + APPROVED_BROWSER_OPEN_GRANT_TTL;
        }
        Ok(())
    } else {
        Err("browser control is not armed".to_string())
    }
}

fn browser_control_grant_expiry(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    now: Instant,
) -> Result<Instant, String> {
    let scope = scope_key(workspace_id, session_id);
    let mut grant = state
        .control_grant
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    if grant.as_ref().is_some_and(|grant| grant.expires_at <= now) {
        *grant = None;
    }
    let Some(grant) = grant.as_ref() else {
        return Err("browser control is not armed".to_string());
    };
    if grant.scope != scope || grant.lifecycle_token != lifecycle_token {
        return Err("browser control is not armed".to_string());
    }
    Ok(grant.expires_at)
}

fn new_browser_evidence_credential(prefix: &str) -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    format!("{prefix}-{}", hex::encode(bytes))
}

fn validate_browser_evidence_capture_id(capture_id: &str) -> Result<(), String> {
    // This is a public, one-shot opaque handle, not an arbitrary identifier.
    // Keep it exactly aligned with the MCP schema so malformed handles are
    // rejected before any capture lookup or side effect.
    if capture_id.len() != 34
        || !capture_id.starts_with("c-")
        || !capture_id.as_bytes()[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err("browser evidence capture id is invalid".to_string());
    }
    Ok(())
}

fn invalidate_browser_evidence_for_scope(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
) {
    let scope = scope_key(workspace_id, session_id);
    invalidate_browser_evidence_for_scope_key(state, &scope);
}

fn invalidate_browser_evidence_for_scope_key(state: &BrowserState, scope: &str) {
    let removed = state.evidence_captures.lock().ok().map(|mut captures| {
        let capture_ids = captures
            .iter()
            .filter_map(|(capture_id, capture)| {
                (capture.scope == scope).then(|| capture_id.clone())
            })
            .collect::<Vec<_>>();
        capture_ids
            .into_iter()
            .filter_map(|capture_id| captures.remove(&capture_id))
            .collect::<Vec<_>>()
    });
    for record in removed.unwrap_or_default() {
        schedule_browser_evidence_cleanup(state, record);
    }
}

fn clear_browser_evidence_captures(state: &BrowserState) {
    let removed = state.evidence_captures.lock().ok().map(|mut captures| {
        captures
            .drain()
            .map(|(_, record)| record)
            .collect::<Vec<_>>()
    });
    for record in removed.unwrap_or_default() {
        schedule_browser_evidence_cleanup(state, record);
    }
}

/// Atomically take an evidence record only when its private page capability
/// still matches. Callers must schedule cleanup only after this returns it.
fn take_browser_evidence_capture_with_token(
    captures: &mut HashMap<String, BrowserEvidenceCaptureRecord>,
    capture_id: &str,
    capture_token: &str,
) -> Option<BrowserEvidenceCaptureRecord> {
    captures
        .get(capture_id)
        .is_some_and(|capture| capture.capture_token == capture_token)
        .then(|| captures.remove(capture_id))
        .flatten()
}

fn remove_browser_evidence_capture_with_token(
    state: &BrowserState,
    capture_id: &str,
    capture_token: &str,
) -> Option<BrowserEvidenceCaptureRecord> {
    state
        .evidence_captures
        .lock()
        .ok()
        .and_then(|mut captures| {
            take_browser_evidence_capture_with_token(&mut captures, capture_id, capture_token)
        })
}

/// The record must already have been removed from backend state before this is
/// called. The fixed page-side drain is best effort: it never returns buffered
/// evidence and it fails closed if the child WebView navigated or closed.
fn schedule_browser_evidence_cleanup(state: &BrowserState, record: BrowserEvidenceCaptureRecord) {
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        let _operation = state.operation_lock.lock().await;
        let Ok(script) = browser_evidence_cleanup_script(
            &record.capture_token,
            &record.url,
            record.document_identity,
        ) else {
            return;
        };
        if let Ok(raw) = eval_browser_evidence_with_callback(&state, script).await {
            let _ = parse_browser_evidence_cleanup_callback(&raw);
        }
    });
}

fn expire_browser_evidence_capture(state: &BrowserState, capture_id: &str, capture_token: &str) {
    if let Some(record) =
        remove_browser_evidence_capture_with_token(state, capture_id, capture_token)
    {
        schedule_browser_evidence_cleanup(state, record);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserEvidenceCallback {
    ok: bool,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    events: Vec<BrowserEvidenceEventRaw>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserEvidenceEventRaw {
    kind: String,
    sequence: u64,
    message: String,
    url: Option<String>,
    line: Option<u64>,
    column: Option<u64>,
    initiator_type: Option<String>,
    duration_ms: Option<u64>,
    status: Option<u64>,
}

fn contains_sensitive_browser_evidence_term(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "token",
        "secret",
        "password",
        "bearer",
        "api key",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

fn normalize_browser_evidence_message(raw: &str) -> String {
    let mut normalized = raw
        .chars()
        .filter(|character| !character.is_control() || *character == '\n' || *character == '\t')
        .take(MAX_BROWSER_EVIDENCE_MESSAGE_CHARS)
        .collect::<String>();
    if contains_sensitive_browser_evidence_term(&normalized) {
        normalized = "[redacted sensitive browser evidence]".to_string();
    }
    normalized
}

fn sanitize_browser_evidence_url(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    let parsed = Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    let path = parsed.path();
    let path = path
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_BROWSER_EVIDENCE_URL_CHARS)
        .collect::<String>();
    let host = parsed.host_str()?;
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let authority = parsed
        .port()
        .map(|port| format!("{host}:{port}"))
        .unwrap_or(host);
    let sanitized = format!("{}://{}{}", parsed.scheme(), authority, path);
    if contains_sensitive_browser_evidence_term(&sanitized) {
        return Some("[redacted sensitive browser evidence URL]".to_string());
    }
    Some(sanitized)
}

fn normalize_browser_evidence_resource_initiator_type(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    match raw {
        "script" | "link" | "img" | "css" | "font" | "fetch" | "xmlhttprequest" => {
            Some(raw.to_string())
        }
        _ => None,
    }
}

fn normalize_browser_evidence_resource_status(raw: Option<u64>) -> Option<u16> {
    raw.filter(|status| (100..=599).contains(status))
        .map(|status| status as u16)
}

fn normalize_browser_evidence_events(
    raw_events: Vec<BrowserEvidenceEventRaw>,
    mut truncated: bool,
) -> (Vec<BrowserEvidenceEvent>, bool) {
    let mut events = Vec::with_capacity(MAX_BROWSER_EVIDENCE_EVENTS);
    for raw in raw_events.into_iter().take(MAX_BROWSER_EVIDENCE_EVENTS + 1) {
        if events.len() >= MAX_BROWSER_EVIDENCE_EVENTS {
            truncated = true;
            break;
        }
        if raw.sequence == 0 {
            truncated = true;
            continue;
        }
        let event = match raw.kind.as_str() {
            "consoleWarn" | "consoleError" | "error" | "resourceError" | "unhandledRejection" => {
                BrowserEvidenceEvent {
                    kind: raw.kind,
                    sequence: raw.sequence,
                    message: normalize_browser_evidence_message(&raw.message),
                    url: sanitize_browser_evidence_url(raw.url.as_deref()),
                    line: raw
                        .line
                        .filter(|line| *line <= u64::from(MAX_BROWSER_EVIDENCE_LINE))
                        .map(|line| line as u32),
                    column: raw
                        .column
                        .filter(|column| *column <= u64::from(MAX_BROWSER_EVIDENCE_LINE))
                        .map(|column| column as u32),
                    initiator_type: None,
                    duration_ms: None,
                    status: None,
                }
            }
            "resource" => {
                let Some(url) = sanitize_browser_evidence_url(raw.url.as_deref()) else {
                    truncated = true;
                    continue;
                };
                BrowserEvidenceEvent {
                    kind: "resource".to_string(),
                    sequence: raw.sequence,
                    // Resource Timing data must never supply its own message:
                    // it may carry server-originated or page-controlled text.
                    message: "resource timing observed".to_string(),
                    url: Some(url),
                    line: None,
                    column: None,
                    initiator_type: normalize_browser_evidence_resource_initiator_type(
                        raw.initiator_type.as_deref(),
                    ),
                    duration_ms: raw.duration_ms.map(|duration| {
                        duration.min(u64::from(MAX_BROWSER_EVIDENCE_RESOURCE_DURATION_MS)) as u32
                    }),
                    status: normalize_browser_evidence_resource_status(raw.status),
                }
            }
            _ => {
                truncated = true;
                continue;
            }
        };
        let mut candidate = events.clone();
        candidate.push(event.clone());
        let serialized_chars = serde_json::to_string(&candidate)
            .map(|value| value.chars().count())
            .unwrap_or(MAX_BROWSER_EVIDENCE_CHARS + 1);
        if serialized_chars > MAX_BROWSER_EVIDENCE_CHARS {
            truncated = true;
            break;
        }
        events.push(event);
    }
    (events, truncated)
}

fn normalize_browser_scroll_delta(delta_x: f64, delta_y: f64) -> Result<(f64, f64), String> {
    if !delta_x.is_finite()
        || !delta_y.is_finite()
        || delta_x.abs() > MAX_BROWSER_SCROLL_DELTA
        || delta_y.abs() > MAX_BROWSER_SCROLL_DELTA
    {
        return Err("browser scroll action is invalid".to_string());
    }
    Ok((delta_x, delta_y))
}

/// Reject malformed user input before consuming an otherwise valid map. Once
/// prepared, the map is consumed immediately before the native side effect.
fn prepare_browser_control_action(
    action: BrowserControlAction,
) -> Result<PreparedBrowserControlAction, String> {
    match action {
        BrowserControlAction::Navigate { url } => validate_browser_url(&url)
            .map(PreparedBrowserControlAction::Navigate)
            .map_err(|_| "browser navigate action is invalid".to_string()),
        BrowserControlAction::Reload => Ok(PreparedBrowserControlAction::Reload),
        BrowserControlAction::Scroll { delta_x, delta_y } => {
            let (delta_x, delta_y) = normalize_browser_scroll_delta(delta_x, delta_y)?;
            Ok(PreparedBrowserControlAction::Scroll { delta_x, delta_y })
        }
        BrowserControlAction::Click { reference } => {
            validate_browser_reference(&reference)?;
            Ok(PreparedBrowserControlAction::Click { reference })
        }
        BrowserControlAction::Fill { reference, text } => {
            validate_browser_reference(&reference)?;
            let text = normalize_browser_fill_text(&text)?;
            Ok(PreparedBrowserControlAction::Fill { reference, text })
        }
        BrowserControlAction::Select { reference, label } => {
            validate_browser_reference(&reference)?;
            let label = normalize_browser_fill_text(&label)?;
            if label.is_empty() || label.chars().count() > MAX_BROWSER_SEMANTIC_NAME_CHARS {
                return Err("browser option label is invalid".into());
            }
            Ok(PreparedBrowserControlAction::Select { reference, label })
        }
        BrowserControlAction::Press { reference, key } => {
            validate_browser_reference(&reference)?;
            if !crate::browser_input::valid_key(&key) {
                return Err("browser key is invalid".into());
            }
            Ok(PreparedBrowserControlAction::Press { reference, key })
        }
    }
}

/// Only finite, bounded numeric literals reach this fixed page-side script.
/// It has no selector, DOM traversal, input value access, or renderer-provided JS.
fn browser_scroll_script(delta_x: f64, delta_y: f64) -> String {
    format!("(() => {{ window.scrollBy({delta_x}, {delta_y}); }})()")
}

/// Rechecks the live document without using wry's panic-prone WKWebView.URL
/// accessor. It returns only a closed confirmation and never exposes the page
/// URL to the renderer or to an MCP caller.
fn browser_action_url_check_script(expected_url: &str) -> Result<String, String> {
    let url_json = serde_json::to_string(expected_url)
        .map_err(|_| "browser action URL is invalid".to_string())?;
    Ok(format!(
        r#"(() => {{
  try {{
    return window.location.href === {url_json}
      ? {{ ok: true, reason: "ok" }}
      : {{ ok: false, reason: "stale" }};
  }} catch (_) {{
    return {{ ok: false, reason: "stale" }};
  }}
}})()"#
    ))
}

fn browser_semantic_action_script(
    target: &BrowserSemanticTargetRecord,
    fill_text: Option<&str>,
    expected_url: &str,
    expected_document_identity: f64,
) -> Result<String, String> {
    browser_element_action_script(
        target,
        if fill_text.is_some() { "fill" } else { "click" },
        fill_text.unwrap_or_default(),
        expected_url,
        expected_document_identity,
    )
}

fn browser_element_action_script(
    target: &BrowserSemanticTargetRecord,
    mode: &str,
    text: &str,
    expected_url: &str,
    expected_document_identity: f64,
) -> Result<String, String> {
    let target_json = serde_json::to_string(target)
        .map_err(|_| "browser action target is invalid".to_string())?;
    let text_json =
        serde_json::to_string(text).map_err(|_| "browser fill text is invalid".to_string())?;
    let url_json = serde_json::to_string(expected_url)
        .map_err(|_| "browser action URL is invalid".to_string())?;
    if !expected_document_identity.is_finite() || expected_document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    Ok(format!(
        r#"(() => {{
  try {{
  const expected = {target_json};
  const mode = {mode:?};
  const fillText = {text_json};
  const expectedUrl = {url_json};
  const expectedDocumentIdentity = {expected_document_identity};
  const maxAncestors = {MAX_BROWSER_SEMANTIC_ANCESTORS};
  const maxNameChars = {MAX_BROWSER_SEMANTIC_NAME_CHARS};
  const maxDestinationChars = {MAX_BROWSER_SEMANTIC_DESTINATION_CHARS};
  const maxScanNodes = {MAX_BROWSER_SEMANTIC_SCAN_NODES};
  const clean = (value, max = maxNameChars) => String(value ?? "").slice(0, Math.max(max * 4, max))
    .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ").replace(/\s+/g, " ").trim().slice(0, max);
  const result = (ok, reason) => ({{ ok, reason }});
  if (window.location.href !== expectedUrl) return result(false, "stale");
  if (!Number.isFinite(performance.timeOrigin) || performance.timeOrigin !== expectedDocumentIdentity) return result(false, "stale");
  const isVisible = (element) => {{
    let node = element;
    for (let depth = 0; node && depth < maxAncestors; depth += 1, node = node.parentElement) {{
      if (node.hidden || node.inert || node.getAttribute("aria-hidden") === "true") return false;
      const style = window.getComputedStyle(node);
      if (style.display === "none" || style.visibility === "hidden" || Number.parseFloat(style.opacity) === 0) return false;
    }}
    if (node) return false;
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.bottom > 0
      && rect.left < window.innerWidth && rect.top < window.innerHeight;
  }};
  const boundedText = (root) => {{
    if (!root) return "";
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const parts = [];
    let seen = 0;
      let node;
      while (seen < {MAX_BROWSER_SEMANTIC_NAME_TEXT_NODES} && (node = walker.nextNode())) {{
        seen += 1;
        let parent = node.parentElement;
        let safe = true;
        let reachedBody = false;
        for (let depth = 0; parent && depth < maxAncestors; depth += 1, parent = parent.parentElement) {{
          const tag = parent.tagName.toLowerCase();
          if (["input", "textarea", "select", "option"].includes(tag) || parent.isContentEditable) {{ safe = false; break; }}
          if (parent === document.body) {{ reachedBody = true; break; }}
        }}
        if (!reachedBody) safe = false;
        if (safe && node.parentElement && isVisible(node.parentElement)) parts.push(clean(node.data));
    }}
    return clean(parts.join(" "));
  }};
  const accessibleName = (element) => {{
    const ariaLabel = clean(element.getAttribute("aria-label"));
    if (ariaLabel) return ariaLabel;
    const labelledBy = (element.getAttribute("aria-labelledby") || "").trim().split(/\s+/).filter(Boolean).slice(0, 4)
      .map((id) => document.getElementById(id))
      .filter((label) => label && label !== element && isVisible(label))
      .map((label) => boundedText(label))
      .filter(Boolean).join(" ");
    if (labelledBy) return clean(labelledBy);
    const labels = "labels" in element && element.labels ? Array.from(element.labels).slice(0, 3) : [];
    const associated = labels.map((label) => boundedText(label)).filter(Boolean).join(" ");
    if (associated) return clean(associated);
    const alt = clean(element.getAttribute("alt"));
    if (alt) return alt;
    const title = clean(element.getAttribute("title"));
    if (title) return title;
    const placeholder = clean(element.getAttribute("placeholder"));
    if (placeholder) return placeholder;
    if (element.isContentEditable || ["input", "textarea", "select"].includes(element.tagName.toLowerCase())) return "";
    return boundedText(element);
  }};
  const canonicalRole = (element) => {{
    const tag = element.tagName.toLowerCase();
    const explicit = (element.getAttribute("role") || "").toLowerCase();
    const inputType = tag === "input" ? (element.getAttribute("type") || "text").toLowerCase() : "";
    if (tag === "input" && ["hidden", "password"].includes(inputType)) return null;
    if (/^h[1-6]$/.test(tag)) return "heading";
    if (explicit === "heading") return "heading";
    if (tag === "button" || explicit === "button") return "button";
    if ((tag === "a" && element.hasAttribute("href")) || explicit === "link") return "link";
    if (tag === "textarea" || element.isContentEditable || ["textbox", "searchbox"].includes(explicit)) return "textbox";
    if (tag === "select" || ["combobox", "listbox"].includes(explicit)) return "select";
    if (explicit === "checkbox") return "checkbox";
    if (explicit === "radio") return "radio";
    if (explicit === "switch") return "switch";
    if (explicit === "slider") return "slider";
    if (explicit === "spinbutton") return "spinbutton";
    if (explicit === "option") return "option";
    if (["tab", "menuitem"].includes(explicit)) return explicit;
    if (tag !== "input") return null;
    if (["button", "submit", "reset", "image"].includes(inputType)) return "button";
    if (inputType === "checkbox") return "checkbox";
    if (inputType === "radio") return "radio";
    if (inputType === "range") return "slider";
    if (inputType === "number") return "spinbutton";
    if (inputType === "file") return "file-input";
    return "textbox";
  }};
  const booleanState = (element, property, ariaName) => {{
    const aria = element.getAttribute(ariaName);
    if (aria === "true") return true;
    if (aria === "false") return false;
    return typeof element[property] === "boolean" ? element[property] : null;
  }};
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
  let scanned = 0;
  let candidate;
  while (scanned < maxScanNodes && (candidate = walker.nextNode())) {{
    scanned += 1;
    if (scanned === expected.ordinal) break;
    candidate = null;
  }}
  if (!candidate || scanned !== expected.ordinal || !isVisible(candidate)) return result(false, "stale");
  const tag = candidate.tagName.toLowerCase();
  const inputType = tag === "input" ? (candidate.getAttribute("type") || "text").toLowerCase() : null;
  const role = canonicalRole(candidate);
  const destination = role === "link"
    ? (() => {{ try {{ const url = new URL(candidate.href, window.location.href); if (!["http:", "https:"].includes(url.protocol)) return null; url.username = ""; url.password = ""; url.search = ""; url.hash = ""; return clean(url.toString(), maxDestinationChars); }} catch (_) {{ return null; }} }})()
    : null;
  const actual = {{ reference: expected.reference, ordinal: scanned, role, name: accessibleName(candidate), destination,
    disabled: role === "heading" ? null : booleanState(candidate, "disabled", "aria-disabled"),
    checked: ["checkbox", "radio", "switch"].includes(role) ? booleanState(candidate, "checked", "aria-checked") : null,
    selected: ["select", "option"].includes(role) ? booleanState(candidate, "selected", "aria-selected") : null,
    expanded: booleanState(candidate, "ariaExpanded", "aria-expanded"),
    pressed: role === "button" ? booleanState(candidate, "ariaPressed", "aria-pressed") : null,
    tag, inputType,
    contentEditable: Boolean(candidate.isContentEditable) }};
  if (JSON.stringify(actual) !== JSON.stringify(expected)) return result(false, "stale");
  if (actual.disabled === true) return result(false, "disabled");
  const customControl = !["input", "textarea", "select", "option", "a"].includes(actual.tag) && !actual.contentEditable;
  const clickCompatible = customControl && ["button", "link", "checkbox", "radio", "switch", "select", "option", "tab", "menuitem"].includes(actual.role)
    || actual.role === "button" && actual.tag === "input" && ["button", "submit", "reset", "image"].includes(actual.inputType)
    || actual.role === "link" && actual.tag === "a" && Boolean(actual.destination)
    || ["checkbox", "radio", "switch"].includes(actual.role) && actual.tag === "input" && ["checkbox", "radio"].includes(actual.inputType);
  const fillCompatible = actual.contentEditable && actual.role === "textbox" || !actual.contentEditable && (actual.role === "textbox" && (actual.tag === "textarea"
    || actual.tag === "input" && ["text", "search", "email", "tel", "url"].includes(actual.inputType))
    || actual.role === "spinbutton" && actual.tag === "input" && actual.inputType === "number");
  if (mode === "point") {{
    if (!clickCompatible) return result(false, "incompatible");
    const rect = candidate.getBoundingClientRect();
    const x = (Math.max(0, rect.left) + Math.min(window.innerWidth, rect.right)) / 2;
    const y = (Math.max(0, rect.top) + Math.min(window.innerHeight, rect.bottom)) / 2;
    const hit = document.elementFromPoint(x, y);
    if (!hit || !(hit === candidate || candidate.contains(hit))) return result(false, "stale");
    return {{ok: true, reason: "ok", x: x / window.innerWidth, y: y / window.innerHeight}};
  }}
  if (mode === "focus") {{
    if (!["textbox", "spinbutton", "button", "link", "checkbox", "radio", "switch", "select", "option", "tab", "menuitem", "slider"].includes(actual.role)) return result(false, "incompatible");
    candidate.focus({{ preventScroll: true }});
    return result(document.activeElement === candidate, "failed");
  }}
  if (mode === "select") {{
    if (actual.tag !== "select" || actual.role !== "select" || candidate.multiple) return result(false, "incompatible");
    const options = Array.from(candidate.options).slice(0, 1000).filter(option => clean(option.label) === fillText && !option.disabled && !option.hidden && !option.closest("optgroup[disabled]"));
    if (options.length !== 1) return result(false, "incompatible");
    candidate.focus();
    candidate.selectedIndex = options[0].index;
    candidate.dispatchEvent(new Event("input", {{ bubbles: true, composed: true }}));
    candidate.dispatchEvent(new Event("change", {{ bubbles: true, composed: true }}));
    return result(true, "ok");
  }}
  if (mode === "click") {{
    if (!clickCompatible) return result(false, "incompatible");
    if (typeof candidate.click !== "function") return result(false, "failed");
    candidate.click();
    return result(true, "ok");
  }}
  if (!fillCompatible || Array.from(fillText).length > {MAX_BROWSER_FILL_CHARS} || /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/.test(fillText)) return result(false, "incompatible");
  if (actual.contentEditable) {{
    candidate.focus();
    const selection = window.getSelection();
    if (!selection) return result(false, "failed");
    const range = document.createRange(); range.selectNodeContents(candidate);
    selection.removeAllRanges(); selection.addRange(range);
    // WebKit's editing command preserves undo and dispatches native editor input.
    const changed = document.execCommand("insertText", false, fillText);
    return result(changed && candidate.textContent === fillText, "failed");
  }}
  const prototype = actual.tag === "textarea" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
  if (typeof setter !== "function") return result(false, "failed");
  if (candidate.readOnly) return result(false, "disabled");
  candidate.focus();
  setter.call(candidate, fillText);
  if (candidate.value !== fillText) return result(false, "incompatible");
  candidate.dispatchEvent(new Event("input", {{ bubbles: true, composed: true }}));
  candidate.dispatchEvent(new Event("change", {{ bubbles: true, composed: true }}));
  return result(true, "ok");
  }} catch (_) {{
    return {{ ok: false, reason: "failed" }};
  }}
}})()"#
    ))
}

#[derive(Debug, Deserialize)]
struct BrowserActionCallback {
    ok: bool,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
}

fn parse_browser_action_callback(raw: &str) -> Result<(), String> {
    let callback: BrowserActionCallback = serde_json::from_str(raw)
        .map_err(|_| "browser action confirmation was invalid".to_string())?;
    if callback.ok {
        return Ok(());
    }
    match callback.reason.as_deref() {
        Some("stale") => Err("browser action anchor is stale".to_string()),
        Some("disabled") => Err("browser action target is disabled".to_string()),
        Some("incompatible") => Err("browser action target is incompatible".to_string()),
        _ => Err("browser action failed".to_string()),
    }
}

fn parse_browser_evidence_callback(raw: &str) -> Result<BrowserEvidenceCallback, String> {
    if raw.chars().count() > MAX_BROWSER_EVIDENCE_CALLBACK_CHARS {
        return Err("browser evidence response exceeded its budget".to_string());
    }
    let callback: BrowserEvidenceCallback = serde_json::from_str(raw)
        .map_err(|_| "browser evidence response was invalid".to_string())?;
    if callback.ok {
        Ok(callback)
    } else {
        match callback.reason.as_deref() {
            Some("url_changed" | "origin_changed") => {
                Err("browser page changed during evidence capture".to_string())
            }
            Some("document_changed") => {
                Err("browser page reloaded during evidence capture".to_string())
            }
            Some("collector_missing") => {
                Err("browser evidence collector ended before collection".to_string())
            }
            _ => Err("browser evidence capture was unavailable".to_string()),
        }
    }
}

async fn eval_browser_action_with_callback(
    state: &BrowserState,
    script: String,
) -> Result<(), String> {
    let raw = eval_browser_action_response(state, script).await?;
    parse_browser_action_callback(&raw)
}

async fn eval_browser_action_response(
    state: &BrowserState,
    script: String,
) -> Result<String, String> {
    let raw = eval_page(state, script, BROWSER_CONTEXT_TIMEOUT)
        .await
        .map_err(|error| match error {
            PageEvalError::NotOpen(message) => message,
            PageEvalError::Start(_) => "browser action failed".to_string(),
            PageEvalError::Timeout => "browser action timed out".to_string(),
            PageEvalError::Cancelled => "browser action confirmation was cancelled".to_string(),
        })?;
    if raw.len() > 1024 {
        return Err("browser action response is invalid".into());
    }
    Ok(raw)
}

fn browser_action_anchor_matches_context(
    context: &BrowserContext,
    anchor: &BrowserActionAnchor,
    scope: &str,
) -> bool {
    context_is_ready_for_url(context, &anchor.url)
        && context.page_load_revision == anchor.page_load_revision
        && semantic_map_record_is_current(
            context.semantic_map.as_ref(),
            scope,
            anchor.lifecycle_token,
            &anchor.url,
            anchor.page_load_revision,
            &anchor.map_id,
            anchor.generation,
        )
}

fn consume_semantic_map_for_anchor(
    context: &mut BrowserContext,
    anchor: &BrowserActionAnchor,
    scope: &str,
) -> bool {
    if !browser_action_anchor_matches_context(context, anchor, scope) {
        return false;
    }
    context.semantic_map = None;
    true
}

fn validate_browser_reference(reference: &str) -> Result<(), String> {
    if reference.is_empty() || reference.chars().count() > MAX_BROWSER_REFERENCE_CHARS {
        return Err("browser action reference is invalid".to_string());
    }
    let Some(number) = reference.strip_prefix('e') else {
        return Err("browser action reference is invalid".to_string());
    };
    if number.is_empty()
        || (number.len() > 1 && number.starts_with('0'))
        || !number.bytes().all(|byte| byte.is_ascii_digit())
        || number
            .parse::<usize>()
            .ok()
            .is_none_or(|value| value == 0 || value > MAX_BROWSER_SEMANTIC_ITEMS)
    {
        return Err("browser action reference is invalid".to_string());
    }
    Ok(())
}

fn click_target_is_compatible(target: &BrowserSemanticTargetRecord) -> bool {
    if target.disabled == Some(true) || target.content_editable {
        return false;
    }
    let custom_control = !matches!(
        target.tag.as_str(),
        "input" | "textarea" | "select" | "option" | "a"
    );
    if custom_control
        && matches!(
            target.role.as_str(),
            "button"
                | "link"
                | "checkbox"
                | "radio"
                | "switch"
                | "select"
                | "option"
                | "tab"
                | "menuitem"
        )
    {
        return true;
    }
    match target.role.as_str() {
        "button" => {
            target.tag == "input"
                && target
                    .input_type
                    .as_deref()
                    .is_some_and(|t| matches!(t, "button" | "submit" | "reset" | "image"))
        }
        "link" => target.tag == "a" && target.destination.is_some(),
        "checkbox" | "radio" | "switch" => {
            target.tag == "input"
                && target
                    .input_type
                    .as_deref()
                    .is_some_and(|t| matches!(t, "checkbox" | "radio"))
        }
        _ => false,
    }
}

fn fill_target_is_compatible(target: &BrowserSemanticTargetRecord) -> bool {
    if target.disabled == Some(true) {
        return false;
    }
    if target.content_editable {
        return target.role == "textbox";
    }
    match (target.role.as_str(), target.tag.as_str()) {
        ("textbox", "textarea") => true,
        ("textbox", "input") => target
            .input_type
            .as_deref()
            .is_some_and(|t| matches!(t, "text" | "search" | "email" | "tel" | "url")),
        ("spinbutton", "input") => target.input_type.as_deref() == Some("number"),
        _ => false,
    }
}

fn normalize_browser_fill_text(text: &str) -> Result<String, String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text.chars().count() > MAX_BROWSER_FILL_CHARS
        || text
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err("browser fill text is invalid".to_string());
    }
    Ok(text)
}

fn consume_browser_action_target(
    state: &BrowserState,
    anchor: &BrowserActionAnchor,
    reference: &str,
) -> Result<(BrowserSemanticTargetRecord, f64), String> {
    validate_browser_reference(reference)?;
    require_current_lifecycle(
        state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        "browser action anchor is stale",
    )?;
    let snapshot = current_snapshot(state)?;
    if !snapshot.visible || snapshot.url.as_deref() != Some(anchor.url.as_str()) {
        return Err("browser action anchor is stale".to_string());
    }
    let scope = scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
    let mut contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get_mut(&scope)
        .ok_or_else(|| "browser action anchor is stale".to_string())?;
    if !browser_action_anchor_matches_context(context, anchor, &scope) {
        return Err("browser action anchor is stale".to_string());
    }
    let record = context
        .semantic_map
        .as_ref()
        .ok_or_else(|| "browser action anchor is stale".to_string())?;
    let target = record
        .targets
        .iter()
        .find(|target| target.reference == reference)
        .cloned()
        .ok_or_else(|| "browser action reference is stale".to_string())?;
    let document_identity = record.document_identity;
    context.semantic_map = None;
    Ok((target, document_identity))
}

fn browser_action_target(
    state: &BrowserState,
    anchor: &BrowserActionAnchor,
    reference: &str,
) -> Result<(BrowserSemanticTargetRecord, f64), String> {
    validate_browser_reference(reference)?;
    let scope = scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
    let contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get(&scope)
        .ok_or_else(|| "browser action anchor is stale".to_string())?;
    if !browser_action_anchor_matches_context(context, anchor, &scope) {
        return Err("browser action anchor is stale".to_string());
    }
    let record = context
        .semantic_map
        .as_ref()
        .ok_or_else(|| "browser action anchor is stale".to_string())?;
    let target = record
        .targets
        .iter()
        .find(|target| target.reference == reference)
        .cloned()
        .ok_or_else(|| "browser action reference is stale".to_string())?;
    Ok((target, record.document_identity))
}

fn require_action_page_revision(
    state: &BrowserState,
    anchor: &BrowserActionAnchor,
) -> Result<(), String> {
    let scope = scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
    let contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get(&scope)
        .ok_or_else(|| "browser action anchor is stale".to_string())?;
    if context.page_load_revision == anchor.page_load_revision
        && context_is_ready_for_url(context, &anchor.url)
    {
        Ok(())
    } else {
        Err("browser action anchor is stale".to_string())
    }
}

/// Validates the full untrusted client anchor against the current backend map,
/// then consumes the map while the caller holds `operation_lock`. Consuming
/// first makes both a successful action and a native side-effect failure require
/// a new explicit context extraction.
fn consume_browser_action_anchor(
    state: &BrowserState,
    anchor: &BrowserActionAnchor,
) -> Result<(), String> {
    require_current_lifecycle(
        state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        "browser action anchor is stale",
    )?;
    let snapshot = current_snapshot(state)?;
    if !snapshot.visible || snapshot.url.as_deref() != Some(anchor.url.as_str()) {
        return Err("browser action anchor is stale".to_string());
    }
    let scope = scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
    let mut contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get_mut(&scope)
        .ok_or_else(|| "browser action anchor is stale".to_string())?;
    if !consume_semantic_map_for_anchor(context, anchor, &scope) {
        return Err("browser action anchor is stale".to_string());
    }
    Ok(())
}

fn consume_browser_evidence_anchor(
    state: &BrowserState,
    anchor: &BrowserActionAnchor,
    visibility_policy: BrowserVisibilityPolicy,
) -> Result<f64, String> {
    require_current_lifecycle(
        state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        "browser evidence anchor is stale",
    )?;
    let snapshot = current_snapshot(state)?;
    if !browser_surface_matches_visibility_policy(state, snapshot.visible, visibility_policy)?
        || snapshot.url.as_deref() != Some(anchor.url.as_str())
    {
        return Err("browser evidence anchor is stale".to_string());
    }
    let scope = scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
    let mut contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get_mut(&scope)
        .ok_or_else(|| "browser evidence anchor is stale".to_string())?;
    if !browser_action_anchor_matches_context(context, anchor, &scope) {
        return Err("browser evidence anchor is stale".to_string());
    }
    let record = context
        .semantic_map
        .as_ref()
        .ok_or_else(|| "browser evidence anchor is stale".to_string())?;
    let document_identity = record.document_identity;
    context.semantic_map = None;
    Ok(document_identity)
}

async fn eval_browser_evidence_with_callback(
    state: &BrowserState,
    script: String,
) -> Result<String, String> {
    eval_page(state, script, BROWSER_EVIDENCE_TIMEOUT)
        .await
        .map_err(|error| match error {
            PageEvalError::NotOpen(message) => message,
            PageEvalError::Start(_) => "browser evidence script failed".to_string(),
            PageEvalError::Timeout => "browser evidence script timed out".to_string(),
            PageEvalError::Cancelled => "browser evidence response was cancelled".to_string(),
        })
}

fn browser_evidence_origin_json(expected_url: &str) -> Result<String, String> {
    let expected_origin = Url::parse(expected_url)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
        .map(|url| url.origin().ascii_serialization())
        .ok_or_else(|| "browser evidence URL is invalid".to_string())?;
    serde_json::to_string(&expected_origin)
        .map_err(|_| "browser evidence URL is invalid".to_string())
}

fn browser_evidence_start_script(
    capture_token: &str,
    expected_url: &str,
    document_identity: f64,
    ttl_ms: u64,
) -> Result<String, String> {
    let token_json = serde_json::to_string(capture_token)
        .map_err(|_| "browser evidence token is invalid".to_string())?;
    let origin_json = browser_evidence_origin_json(expected_url)?;
    if !document_identity.is_finite() || document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    let ttl_ms = ttl_ms.min(BROWSER_EVIDENCE_TTL.as_millis() as u64).max(1);
    let max_resource_entries = MAX_BROWSER_EVIDENCE_EVENTS + 1;
    Ok(format!(
        r#"(() => {{
  let stop = () => {{}};
  try {{
    const key = {token_json};
    const expectedOrigin = {origin_json};
    const expectedIdentity = {document_identity};
    // SPA routers may change path/query/hash without replacing the document.
    // The document identity remains authoritative; origin prevents this
    // capability from following a cross-site document.
    if (window.location.origin !== expectedOrigin) return {{ ok: false, reason: "origin_changed" }};
    if (!Number.isFinite(performance.timeOrigin)
      || performance.timeOrigin !== expectedIdentity) return {{ ok: false, reason: "document_changed" }};
    if (Object.prototype.hasOwnProperty.call(window, key)) return {{ ok: false, reason: "collector_exists" }};
    const maxEvents = {MAX_BROWSER_EVIDENCE_EVENTS};
    // A single observer callback may be very large. Inspect at most one more
    // entry than the retained ring, so overflow is explicit without walking
    // an unbounded browser-supplied list.
    const maxResourceEntries = {max_resource_entries};
    const maxMessageChars = {MAX_BROWSER_EVIDENCE_MESSAGE_CHARS};
    const maxUrlChars = {MAX_BROWSER_EVIDENCE_URL_CHARS};
    const events = [];
    let sequence = 0;
    let truncated = false;
    let timer = 0;
    let resourceObserver = null;
    let drainListener = null;
    const bounded = (value) => String(value ?? "").slice(0, maxMessageChars);
    const safeUrl = (value) => {{
      if (typeof value !== "string") return null;
      try {{
        const url = new URL(value, window.location.href);
        if (!["http:", "https:"].includes(url.protocol) || !url.host) return null;
        return `${{url.protocol}}//${{url.host}}${{url.pathname}}`.slice(0, maxUrlChars);
      }} catch (_) {{ return null; }}
    }};
    const messageOf = (value) => {{
      if (typeof value === "string") return bounded(value);
      if (value instanceof Error && typeof value.message === "string") return bounded(value.message);
      return "";
    }};
    const lineOf = (value) => Number.isSafeInteger(value) && value >= 0 && value <= 1000000 ? value : null;
    const add = (kind, message, url, line, column, resource = null) => {{
      if (typeof message !== "string" || !message) return;
      sequence += 1;
      if (events.length >= maxEvents) {{ events.shift(); truncated = true; }}
      events.push({{
        kind,
        sequence,
        message: bounded(message),
        url: safeUrl(url),
        line: lineOf(line),
        column: lineOf(column),
        initiatorType: resource?.initiatorType ?? null,
        durationMs: resource?.durationMs ?? null,
        status: resource?.status ?? null,
      }});
    }};
    const originalWarn = console.warn;
    const originalError = console.error;
    const warn = (...args) => {{
      try {{ if (typeof originalWarn === "function") originalWarn.apply(console, args); }}
      finally {{ add("consoleWarn", messageOf(args[0]), null, null, null); }}
    }};
    const error = (...args) => {{
      try {{ if (typeof originalError === "function") originalError.apply(console, args); }}
      finally {{ add("consoleError", messageOf(args[0]), null, null, null); }}
    }};
    const onError = (event) => {{
      if (event.target === window) {{
        add("error", typeof event.message === "string" ? bounded(event.message) : "", event.filename, event.lineno, event.colno);
        return;
      }}
      const target = event.target;
      const url = target && typeof target.src === "string" ? target.src : target && typeof target.href === "string" ? target.href : null;
      add("resourceError", "resource load failed", url, null, null);
    }};
    const onRejection = (event) => {{
      const reason = event.reason;
      const message = typeof reason === "string" ? reason : reason instanceof Error && typeof reason.message === "string" ? reason.message : "";
      add("unhandledRejection", bounded(message), null, null, null);
    }};
    const onResources = (entries) => {{
      try {{
        // This list is delivered by this observer after explicit installation;
        // no historical Performance timeline is queried or requested.
        const resourceEntries = entries.getEntries();
        let processed = 0;
        for (const entry of resourceEntries) {{
          if (processed >= maxResourceEntries) {{ truncated = true; break; }}
          processed += 1;
          if (!entry || entry.entryType !== "resource") continue;
          const initiator = typeof entry.initiatorType === "string" && ["script", "link", "img", "css", "font", "fetch", "xmlhttprequest"].includes(entry.initiatorType)
            ? entry.initiatorType : null;
          const durationMs = Number.isFinite(entry.duration)
            ? Math.min({MAX_BROWSER_EVIDENCE_RESOURCE_DURATION_MS}, Math.max(0, Math.round(entry.duration))) : null;
          const status = Number.isSafeInteger(entry.responseStatus) && entry.responseStatus >= 100 && entry.responseStatus <= 599
            ? entry.responseStatus : null;
          add("resource", "resource timing observed", entry.name, null, null, {{ initiatorType: initiator, durationMs, status }});
        }}
      }} catch (_) {{}}
    }};
    let installedWarn = false;
    let installedError = false;
    let installedErrorListener = false;
    let installedRejectionListener = false;
    let installedDrainListener = false;
    stop = () => {{
      // Every cleanup step is isolated. Disconnect first: a tampered console
      // or event API must not leave the observer collecting after a drain.
      try {{ if (resourceObserver) resourceObserver.disconnect(); }} catch (_) {{}}
      resourceObserver = null;
      try {{ clearTimeout(timer); }} catch (_) {{}}
      try {{ if (installedWarn && console.warn === warn) console.warn = originalWarn; }} catch (_) {{}}
      try {{ if (installedError && console.error === error) console.error = originalError; }} catch (_) {{}}
      try {{ if (installedErrorListener) window.removeEventListener("error", onError, true); }} catch (_) {{}}
      try {{ if (installedRejectionListener) window.removeEventListener("unhandledrejection", onRejection, false); }} catch (_) {{}}
      try {{ if (installedDrainListener && drainListener) window.removeEventListener(key, drainListener, false); }} catch (_) {{}}
      try {{ delete window[key]; }} catch (_) {{}}
    }};
    const drain = () => {{
      const result = {{ events: events.slice(0, maxEvents), truncated }};
      stop();
      return result;
    }};
    drainListener = (event) => {{
      try {{
        const holder = event?.detail;
        if (!holder || typeof holder !== "object" || Object.prototype.hasOwnProperty.call(holder, "result")) return;
        holder.result = drain();
      }} catch (_) {{}}
    }};
    console.warn = warn;
    installedWarn = true;
    console.error = error;
    installedError = true;
    Object.defineProperty(window, key, {{ value: {{ drain }}, configurable: true, enumerable: false, writable: false }});
    window.addEventListener("error", onError, true);
    installedErrorListener = true;
    window.addEventListener("unhandledrejection", onRejection, false);
    installedRejectionListener = true;
    window.addEventListener(key, drainListener, false);
    installedDrainListener = true;
    if (typeof PerformanceObserver === "function") {{
      try {{
        resourceObserver = new PerformanceObserver(onResources);
        // Only entries occurring after this user-requested start may reach
        // the bounded ring; no prior timeline entries are requested.
        resourceObserver.observe({{ type: "resource" }});
      }} catch (_) {{
        try {{ resourceObserver?.disconnect(); }} catch (_) {{}}
        resourceObserver = null;
      }}
    }}
    timer = window.setTimeout(stop, {ttl_ms});
    return {{ ok: true, reason: "started" }};
  }} catch (_) {{
    try {{ stop(); }} catch (_) {{}}
    return {{ ok: false, reason: "install_failed" }};
  }}
}})()"#
    ))
}

fn browser_evidence_read_script(
    capture_token: &str,
    expected_url: &str,
    document_identity: f64,
) -> Result<String, String> {
    let token_json = serde_json::to_string(capture_token)
        .map_err(|_| "browser evidence token is invalid".to_string())?;
    let origin_json = browser_evidence_origin_json(expected_url)?;
    if !document_identity.is_finite() || document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    Ok(format!(
        r#"(() => {{
  try {{
    const key = {token_json};
    if (window.location.origin !== {origin_json}) return {{ ok: false, reason: "origin_changed", events: [], truncated: true }};
    if (!Number.isFinite(performance.timeOrigin)
      || performance.timeOrigin !== {document_identity}) return {{ ok: false, reason: "document_changed", events: [], truncated: true }};
    const state = window[key];
    let result = state && typeof state.drain === "function" ? state.drain() : null;
    if (!result) {{
      const holder = {{}};
      try {{ window.dispatchEvent(new CustomEvent(key, {{ detail: holder }})); }} catch (_) {{}}
      result = holder.result ?? null;
    }}
    if (!result) return {{ ok: false, reason: "collector_missing", events: [], truncated: true }};
    return {{ ok: true, reason: "collected", events: Array.isArray(result.events) ? result.events : [], truncated: Boolean(result.truncated) }};
  }} catch (_) {{ return {{ ok: false, reason: "collection_failed", events: [], truncated: true }}; }}
}})()"#
    ))
}

/// Stops an installed evidence wrapper without exposing its buffered events.
/// The fixed script is document-bound; a navigation turns this into a safe
/// `{ ok: false }` no-op and the page's unload/timer remains the fallback.
fn browser_evidence_cleanup_script(
    capture_token: &str,
    expected_url: &str,
    document_identity: f64,
) -> Result<String, String> {
    let token_json = serde_json::to_string(capture_token)
        .map_err(|_| "browser evidence token is invalid".to_string())?;
    let origin_json = browser_evidence_origin_json(expected_url)?;
    if !document_identity.is_finite() || document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    Ok(format!(
        r#"(() => {{
  try {{
    const key = {token_json};
    if (window.location.origin !== {origin_json}) return {{ ok: false, reason: "origin_changed" }};
    if (!Number.isFinite(performance.timeOrigin)
      || performance.timeOrigin !== {document_identity}) return {{ ok: false, reason: "document_changed" }};
    const state = window[key];
    let result = state && typeof state.drain === "function" ? state.drain() : null;
    if (!result) {{
      const holder = {{}};
      try {{ window.dispatchEvent(new CustomEvent(key, {{ detail: holder }})); }} catch (_) {{}}
      result = holder.result ?? null;
    }}
    if (!result) return {{ ok: false, reason: "collector_missing" }};
    return {{ ok: true, reason: "cleaned" }};
  }} catch (_) {{ return {{ ok: false, reason: "cleanup_failed" }}; }}
}})()"#
    ))
}

fn parse_browser_evidence_cleanup_callback(raw: &str) -> Result<(), String> {
    if raw.chars().count() > 64 {
        return Err("browser evidence cleanup response was invalid".to_string());
    }
    let callback: BrowserEvidenceCallback = serde_json::from_str(raw)
        .map_err(|_| "browser evidence cleanup response was invalid".to_string())?;
    if callback.ok && callback.events.is_empty() && !callback.truncated {
        Ok(())
    } else {
        Err("browser evidence cleanup was unavailable".to_string())
    }
}

fn tracked_browser_url(context: &BrowserContext) -> Option<String> {
    context.expected_url.clone().or_else(|| context.url.clone())
}

fn browser_context_is_loading(context: &BrowserContext) -> bool {
    tracked_browser_url(context)
        .as_deref()
        .is_none_or(|url| !context_is_ready_for_url(context, url))
}

fn current_snapshot(state: &BrowserState) -> Result<BrowserSnapshot, String> {
    let workspace_id = state
        .active_workspace
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "browser is not open".to_string())?;
    let session_id = state
        .active_session
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .clone();
    let visible = *state
        .visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let lifecycle_token = *state
        .lifecycle_token
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    // WKWebView.URL is nullable while a navigation has no committed page
    // (notably after a localhost server disappears). wry 0.55 unwraps that
    // nullable value, so Webview::url() can panic instead of returning Err.
    // The navigation/page-load callbacks already maintain the requested and
    // committed URLs; snapshots must use that state and never make closing an
    // unavailable page capable of terminating the whole application.
    let (url, title, loading) = state
        .contexts
        .lock()
        .ok()
        .and_then(|contexts| {
            contexts
                .get(&scope_key(&workspace_id, session_id.as_deref()))
                .map(|context| {
                    let url = tracked_browser_url(context);
                    let loading = browser_context_is_loading(context);
                    (url, context.title.clone(), loading)
                })
        })
        .unwrap_or((None, None, true));
    Ok(BrowserSnapshot {
        workspace_id,
        session_id,
        lifecycle_token,
        visible,
        loading,
        url,
        title,
    })
}

/// Content-free status for an already authenticated projection scope. This is
/// intentionally not a general page query: callers get only the existing
/// native lifecycle snapshot after the active scope has matched exactly.
pub(crate) async fn browser_status_for_scope(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
) -> Result<BrowserSnapshot, String> {
    require_active_scope(state, workspace_id, session_id)?;
    current_snapshot(state)
}

/// Checks the Browser surface against the original URL approved by the user.
/// Main-frame tracking may advance `expected_url` through redirects, while
/// `approved_open_url` preserves the user-approved lifecycle origin.
pub(crate) fn browser_open_lifecycle_matches_request(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    requested_url: &str,
) -> Result<(), String> {
    require_current_lifecycle(
        state,
        workspace_id,
        session_id,
        lifecycle_token,
        "browser open lifecycle is stale",
    )?;
    let contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let context = contexts
        .get(&scope_key(workspace_id, session_id))
        .ok_or_else(|| "browser open context is unavailable".to_string())?;
    let normalized_url = validate_browser_url(requested_url)?;
    if context.approved_open_url.as_deref() != Some(normalized_url.as_str()) {
        return Err("browser open URL does not match the approved request".to_string());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserVisibilityPolicy {
    RequireVisible,
    AllowTemporaryUiOcclusion,
}

/// The renderer hides the whole native child WebView while a DCC portal (such
/// as the Browser agent menu) overlaps it. An explicit UI action from that
/// menu still belongs to the open Browser surface. MCP/agent operations keep
/// using `RequireVisible` and cannot rely on this compositing exception.
fn browser_surface_matches_visibility_policy(
    state: &BrowserState,
    visible: bool,
    policy: BrowserVisibilityPolicy,
) -> Result<bool, String> {
    let lifecycle_open = *state
        .lifecycle_open
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    if !lifecycle_open {
        return Ok(false);
    }
    if visible {
        return Ok(true);
    }
    if policy == BrowserVisibilityPolicy::RequireVisible {
        return Ok(false);
    }
    let temporarily_occluded = *state
        .occluded
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    Ok(temporarily_occluded)
}

fn persist_current_context(state: &BrowserState) {
    let Ok(workspace) = state.active_workspace.lock() else {
        return;
    };
    let Some(workspace_id) = workspace.as_deref() else {
        return;
    };
    let Ok(session) = state.active_session.lock() else {
        return;
    };
    let key = scope_key(workspace_id, session.as_deref());
    let Ok(mut contexts) = state.contexts.lock() else {
        return;
    };
    let context = contexts.entry(key).or_default();
    let ready_url = context.ready_url.clone();
    if should_persist_context_url(context, ready_url.as_deref()) {
        context.url = ready_url;
    }
}

async fn validate_scope(
    sessions: &SessionCommandState,
    workspace_id: &str,
    session_id: Option<&str>,
) -> Result<(), String> {
    if workspace_id.trim().is_empty() {
        return Err("workspace_id is required".to_string());
    }
    let Some(session_id) = session_id else {
        return Ok(());
    };
    let session = sessions
        .peek_session(&SessionId(session_id.to_string()))
        .await
        .map_err(|error| format!("failed to validate browser session: {error}"))?
        .ok_or_else(|| "browser session does not exist".to_string())?;
    let authorized = session.workspace_id.0 == workspace_id
        || session
            .additional_workspace_ids
            .iter()
            .any(|workspace| workspace.0 == workspace_id);
    if authorized {
        Ok(())
    } else {
        Err("browser session does not belong to the requested workspace".to_string())
    }
}

fn set_active_scope(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
) -> Result<(), String> {
    persist_current_context(state);
    *state
        .active_workspace
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = Some(workspace_id.to_string());
    *state
        .active_session
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = session_id.map(str::to_string);
    *state
        .active_scope
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? =
        Some(scope_key(workspace_id, session_id));
    Ok(())
}

fn require_active_scope(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
) -> Result<(), String> {
    let expected = scope_key(workspace_id, session_id);
    let active = state
        .active_scope
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .clone();
    if active.as_deref() == Some(expected.as_str()) {
        Ok(())
    } else {
        Err("browser scope is no longer active".to_string())
    }
}

fn mark_context_loading(context: &mut BrowserContext, expected_url: String) {
    let previous_url = context.url.clone();
    context.expected_url = Some(expected_url);
    context.ready_url = None;
    context.title = None;
    context.page_load_revision = context.page_load_revision.wrapping_add(1).max(1);
    context.main_load_started = false;
    context.pending_previous_main_url = previous_url;
    context.semantic_map = None;
}

fn mark_context_open_request(
    context: &mut BrowserContext,
    url: String,
    previous_main_url: Option<String>,
) {
    mark_context_loading(context, url.clone());
    if previous_main_url.is_some() {
        context.pending_previous_main_url = previous_main_url;
    }
    context.approved_open_url = Some(url);
}

fn invalidate_semantic_map(context: &mut BrowserContext) {
    context.semantic_map = None;
}

fn invalidate_semantic_map_for_scope(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
) {
    if let Ok(mut contexts) = state.contexts.lock() {
        if let Some(context) = contexts.get_mut(&scope_key(workspace_id, session_id)) {
            invalidate_semantic_map(context);
        }
    }
}

fn context_is_ready_for_url(context: &BrowserContext, url: &str) -> bool {
    context.expected_url.as_deref() == Some(url) && context.ready_url.as_deref() == Some(url)
}

fn should_persist_context_url(context: &BrowserContext, native_url: Option<&str>) -> bool {
    native_url.is_some_and(|url| context_is_ready_for_url(context, url))
}

fn semantic_map_record_is_current(
    record: Option<&BrowserSemanticMapRecord>,
    scope: &str,
    lifecycle_token: u64,
    url: &str,
    page_load_revision: u64,
    map_id: &str,
    generation: u64,
) -> bool {
    record.is_some_and(|record| {
        record.scope == scope
            && record.lifecycle_token == lifecycle_token
            && record.url == url
            && record.page_load_revision == page_load_revision
            && record.map_id == map_id
            && record.generation == generation
    })
}

fn page_load_revision_is_current(current: u64, expected: u64) -> bool {
    current == expected
}

fn occlusion_request_is_current(
    lifecycle_open: bool,
    current_token: u64,
    requested_token: u64,
) -> bool {
    lifecycle_open && current_token == requested_token
}

fn advance_lifecycle_token(token: &mut u64) {
    *token = token.wrapping_add(1).max(1);
}

fn require_current_lifecycle(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    requested_token: u64,
    stale_message: &str,
) -> Result<(), String> {
    require_active_scope(state, workspace_id, session_id)?;
    let current_token = *state
        .lifecycle_token
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let lifecycle_open = *state
        .lifecycle_open
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    if !occlusion_request_is_current(lifecycle_open, current_token, requested_token) {
        return Err(stale_message.to_string());
    }
    Ok(())
}

fn context_ready_for_scope(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    url: &str,
) -> Result<bool, String> {
    let contexts = state
        .contexts
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    Ok(contexts
        .get(&scope_key(workspace_id, session_id))
        .is_some_and(|context| context_is_ready_for_url(context, url)))
}

/// Arms the in-memory Browser-control capability after a future explicit UI
/// gesture. The grant is intentionally scoped and short lived; no bearer token
/// is returned, persisted, or logged.
#[tauri::command]
pub async fn browser_arm_control(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserControlStatus, String> {
    let _operation = state.operation_lock.lock().await;
    let result = async {
        validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
        require_current_lifecycle(
            &state,
            &workspace_id,
            session_id.as_deref(),
            lifecycle_token,
            "browser control lifecycle is stale",
        )?;
        let snapshot = current_snapshot(&state)?;
        if !browser_surface_matches_visibility_policy(
            &state,
            snapshot.visible,
            BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
        )? {
            return Err("browser is not visible".to_string());
        }
        arm_browser_control_grant(
            &state,
            &workspace_id,
            session_id.as_deref(),
            lifecycle_token,
            Instant::now(),
        )
    }
    .await;
    let grant_state = browser_audit_grant_state(
        &state,
        &workspace_id,
        session_id.as_deref(),
        Some(lifecycle_token),
    );
    append_browser_audit(
        &state,
        BrowserAuditOrigin::Ui,
        None,
        None,
        &workspace_id,
        session_id.as_deref(),
        BrowserAuditTool::ArmControl,
        grant_state,
        browser_audit_outcome(result.as_ref().err().map(String::as_str)),
    );
    result
}

/// Reports the current in-memory Browser-control grant for the active scope.
/// Occlusion does not revoke the grant, and visibility is intentionally not
/// required for this read-only status query.
#[tauri::command]
pub async fn browser_control_status(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserControlStatus, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser control lifecycle is stale",
    )?;
    let mut grant = state
        .control_grant
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    Ok(control_grant_status(
        &mut grant,
        &scope_key(&workspace_id, session_id.as_deref()),
        lifecycle_token,
        Instant::now(),
    ))
}

/// Returns the bounded, content-free audit snapshot for the currently active
/// Browser lifecycle. This is a trusted Tauri diagnostic surface only; it does
/// not expose the MCP projection or any page payload.
#[tauri::command]
pub async fn browser_read_audit(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    limit: usize,
) -> Result<Vec<BrowserAuditViewRecord>, String> {
    let _operation = state.operation_lock.lock().await;
    if limit == 0 || limit > MAX_BROWSER_AUDIT_READ_LIMIT {
        return Err("browser audit limit is invalid".to_string());
    }
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser audit lifecycle is stale",
    )?;
    read_browser_audit(&state, &workspace_id, session_id.as_deref(), limit).map(|records| {
        records
            .into_iter()
            .map(BrowserAuditViewRecord::from)
            .collect()
    })
}

/// Removes the temporary Browser-control capability without exposing a token.
#[tauri::command]
pub async fn browser_disarm_control(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    requests: State<'_, crate::browser_agent_requests::BrowserAgentRequestBroker>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserControlStatus, String> {
    let _operation = state.operation_lock.lock().await;
    let result = async {
        validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
        require_current_lifecycle(
            &state,
            &workspace_id,
            session_id.as_deref(),
            lifecycle_token,
            "browser control lifecycle is stale",
        )?;
        clear_browser_control_grant(&state);
        invalidate_browser_evidence_for_scope(&state, &workspace_id, session_id.as_deref());
        // Turning control off also ends the task's standing approval.
        if let Some(session_id) = session_id.as_deref() {
            requests.forget_task_approval(session_id);
        }
        Ok(BrowserControlStatus {
            armed: false,
            remaining_ms: 0,
        })
    }
    .await;
    let grant_state = if result.is_ok() {
        BrowserAuditGrantState::NotApplicable
    } else {
        browser_audit_grant_state(
            &state,
            &workspace_id,
            session_id.as_deref(),
            Some(lifecycle_token),
        )
    };
    append_browser_audit(
        &state,
        BrowserAuditOrigin::Ui,
        None,
        None,
        &workspace_id,
        session_id.as_deref(),
        BrowserAuditTool::DisarmControl,
        grant_state,
        browser_audit_outcome(result.as_ref().err().map(String::as_str)),
    );
    result
}

/// Executes only a small allowlist of page-level actions. This is an internal
/// action engine, not an MCP bridge and not element automation.
#[tauri::command]
pub async fn browser_execute_action(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    anchor: BrowserActionAnchor,
    action: BrowserControlAction,
) -> Result<BrowserActionResult, String> {
    let tool = browser_audit_tool_for_action(&action);
    let workspace_id = anchor.workspace_id.clone();
    let session_id = anchor.session_id.clone();
    let lifecycle_token = anchor.lifecycle_token;
    let grant_state = browser_audit_grant_state(
        &state,
        &workspace_id,
        session_id.as_deref(),
        Some(lifecycle_token),
    );
    let result = execute_browser_control_action(&state, &sessions, anchor, action).await;
    append_browser_audit(
        &state,
        BrowserAuditOrigin::Ui,
        None,
        None,
        &workspace_id,
        session_id.as_deref(),
        tool,
        grant_state,
        browser_audit_outcome(result.as_ref().err().map(String::as_str)),
    );
    result
}

/// Shared by the Tauri command and the app-owned MCP listener. It owns the
/// operation lock exactly once, before validation and map consumption.
pub(crate) async fn execute_browser_control_action(
    state: &BrowserState,
    sessions: &SessionCommandState,
    anchor: BrowserActionAnchor,
    action: BrowserControlAction,
) -> Result<BrowserActionResult, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(
        &sessions,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
    )
    .await?;
    require_current_lifecycle(
        &state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        "browser action anchor is stale",
    )?;
    require_browser_control_grant(
        &state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        Instant::now(),
    )?;
    let action = prepare_browser_control_action(action)?;
    let target = match &action {
        PreparedBrowserControlAction::Click { reference } => {
            let (target, document_identity) = browser_action_target(state, &anchor, reference)?;
            if !click_target_is_compatible(&target) {
                return Err("browser action target is incompatible".to_string());
            }
            Some((target, document_identity))
        }
        PreparedBrowserControlAction::Fill { reference, .. } => {
            let (target, document_identity) = browser_action_target(state, &anchor, reference)?;
            if !fill_target_is_compatible(&target) {
                return Err("browser action target is incompatible".to_string());
            }
            Some((target, document_identity))
        }
        PreparedBrowserControlAction::Select { reference, .. }
        | PreparedBrowserControlAction::Press { reference, .. } => {
            let (target, document_identity) = browser_action_target(state, &anchor, reference)?;
            if target.disabled == Some(true)
                || target.role == "heading"
                || target.role == "file-input"
            {
                return Err("browser action target is incompatible".into());
            }
            if matches!(&action, PreparedBrowserControlAction::Select { .. })
                && (target.tag != "select" || target.role != "select")
            {
                return Err("browser action target is incompatible".into());
            }
            Some((target, document_identity))
        }
        _ => None,
    };
    if let Some((target, _document_identity)) = target.as_ref() {
        let reference = &target.reference;
        let _ = consume_browser_action_target(state, &anchor, reference)?;
    } else {
        consume_browser_action_anchor(&state, &anchor)?;
    }

    // Preserve the final live-document check without calling Webview::url().
    // On macOS wry 0.55 unwraps WKWebView.URL even though WebKit may return nil
    // for an unavailable localhost page.
    eval_browser_action_with_callback(state, browser_action_url_check_script(&anchor.url)?).await?;

    let action_name = action.name().to_string();
    match action {
        PreparedBrowserControlAction::Navigate(url) => {
            let page = page_driver(state)?;
            require_action_page_revision(state, &anchor)?;
            require_browser_control_grant(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
                anchor.lifecycle_token,
                Instant::now(),
            )?;
            if let Ok(mut contexts) = state.contexts.lock() {
                mark_context_loading(
                    contexts
                        .entry(scope_key(
                            &anchor.workspace_id,
                            anchor.session_id.as_deref(),
                        ))
                        .or_default(),
                    url.to_string(),
                );
            }
            invalidate_browser_evidence_for_scope(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
            );
            page.navigate(&url)
                .map_err(|_| "browser navigate action failed".to_string())?;
        }
        PreparedBrowserControlAction::Reload => {
            let page = page_driver(state)?;
            require_action_page_revision(state, &anchor)?;
            require_browser_control_grant(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
                anchor.lifecycle_token,
                Instant::now(),
            )?;
            if let Ok(mut contexts) = state.contexts.lock() {
                mark_context_loading(
                    contexts
                        .entry(scope_key(
                            &anchor.workspace_id,
                            anchor.session_id.as_deref(),
                        ))
                        .or_default(),
                    anchor.url.clone(),
                );
            }
            invalidate_browser_evidence_for_scope(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
            );
            page.reload()
                .map_err(|_| "browser reload action failed".to_string())?;
        }
        PreparedBrowserControlAction::Scroll { delta_x, delta_y } => {
            let page = page_driver(state)?;
            require_action_page_revision(state, &anchor)?;
            require_browser_control_grant(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
                anchor.lifecycle_token,
                Instant::now(),
            )?;
            page.eval_detached(browser_scroll_script(delta_x, delta_y))
                .map_err(|_| "browser scroll action failed".to_string())?;
        }
        PreparedBrowserControlAction::Click { .. } => {
            let (target, document_identity) =
                target.ok_or_else(|| "browser action target is stale".to_string())?;
            require_action_page_revision(state, &anchor)?;
            require_browser_control_grant(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
                anchor.lifecycle_token,
                Instant::now(),
            )?;
            // A real click where a native view receives it: the desktop's on
            // macOS, or the native app's Browser; elsewhere through the page.
            if remote_attached(state) || cfg!(target_os = "macos") {
                let raw = eval_browser_action_response(
                    state,
                    browser_element_action_script(
                        &target,
                        "point",
                        "",
                        &anchor.url,
                        document_identity,
                    )?,
                )
                .await?;
                parse_browser_action_callback(&raw)?;
                let point: BrowserActionCallback =
                    serde_json::from_str(&raw).map_err(|_| "browser click point is invalid")?;
                require_action_page_revision(state, &anchor)?;
                require_browser_control_grant(
                    state,
                    &anchor.workspace_id,
                    anchor.session_id.as_deref(),
                    anchor.lifecycle_token,
                    Instant::now(),
                )?;
                deliver_click(
                    state,
                    point.x.ok_or("browser click point is unavailable")?,
                    point.y.ok_or("browser click point is unavailable")?,
                )
                .await?;
            } else {
                eval_browser_action_with_callback(
                    state,
                    browser_semantic_action_script(&target, None, &anchor.url, document_identity)?,
                )
                .await?;
            }
        }
        PreparedBrowserControlAction::Fill { text, .. } => {
            let (target, document_identity) =
                target.ok_or_else(|| "browser action target is stale".to_string())?;
            require_action_page_revision(state, &anchor)?;
            require_browser_control_grant(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
                anchor.lifecycle_token,
                Instant::now(),
            )?;
            eval_browser_action_with_callback(
                state,
                browser_semantic_action_script(
                    &target,
                    Some(&text),
                    &anchor.url,
                    document_identity,
                )?,
            )
            .await?;
        }
        PreparedBrowserControlAction::Select { label, .. }
        | PreparedBrowserControlAction::Press { key: label, .. } => {
            let (target, document_identity) =
                target.ok_or_else(|| "browser action target is stale".to_string())?;
            require_action_page_revision(state, &anchor)?;
            require_browser_control_grant(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
                anchor.lifecycle_token,
                Instant::now(),
            )?;
            let is_key = action_name == "press";
            eval_browser_action_with_callback(
                state,
                browser_element_action_script(
                    &target,
                    if is_key { "focus" } else { "select" },
                    &label,
                    &anchor.url,
                    document_identity,
                )?,
            )
            .await?;
            if is_key {
                require_action_page_revision(state, &anchor)?;
                require_browser_control_grant(
                    state,
                    &anchor.workspace_id,
                    anchor.session_id.as_deref(),
                    anchor.lifecycle_token,
                    Instant::now(),
                )?;
                deliver_press(state, &label).await?;
            }
        }
    }

    Ok(BrowserActionResult {
        action: action_name,
        status: "executed".to_string(),
        requires_context_refresh: true,
    })
}

/// Starts a short-lived, one-shot remote-page evidence ring. The map anchor is
/// consumed before installation; no event buffer exists between explicit
/// starts, and the returned id is the only renderer-visible way to drain this
/// capture. The page token remains backend-only.
#[tauri::command]
pub async fn browser_start_evidence_capture(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    anchor: BrowserActionAnchor,
) -> Result<BrowserEvidenceCaptureHandle, String> {
    let workspace_id = anchor.workspace_id.clone();
    let session_id = anchor.session_id.clone();
    let lifecycle_token = anchor.lifecycle_token;
    let grant_state = browser_audit_grant_state(
        &state,
        &workspace_id,
        session_id.as_deref(),
        Some(lifecycle_token),
    );
    let result = start_browser_evidence_capture_with_visibility_policy(
        &state,
        &sessions,
        anchor,
        BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
    )
    .await;
    append_browser_audit(
        &state,
        BrowserAuditOrigin::Ui,
        None,
        None,
        &workspace_id,
        session_id.as_deref(),
        BrowserAuditTool::EvidenceStart,
        grant_state,
        browser_audit_outcome(result.as_ref().err().map(String::as_str)),
    );
    result
}

/// Shared by the Tauri command and app-owned MCP projection. It consumes a
/// fresh semantic anchor before installing the temporary evidence wrappers.
pub(crate) async fn start_browser_evidence_capture(
    state: &BrowserState,
    sessions: &SessionCommandState,
    anchor: BrowserActionAnchor,
) -> Result<BrowserEvidenceCaptureHandle, String> {
    start_browser_evidence_capture_with_visibility_policy(
        state,
        sessions,
        anchor,
        BrowserVisibilityPolicy::RequireVisible,
    )
    .await
}

async fn start_browser_evidence_capture_with_visibility_policy(
    state: &BrowserState,
    sessions: &SessionCommandState,
    anchor: BrowserActionAnchor,
    visibility_policy: BrowserVisibilityPolicy,
) -> Result<BrowserEvidenceCaptureHandle, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(
        &sessions,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
    )
    .await?;
    require_current_lifecycle(
        &state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        "browser evidence anchor is stale",
    )?;
    let now = Instant::now();
    let grant_expiry = browser_control_grant_expiry(
        &state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        now,
    )?;
    let snapshot = current_snapshot(&state)?;
    if !browser_surface_matches_visibility_policy(state, snapshot.visible, visibility_policy)?
        || snapshot.url.as_deref() != Some(anchor.url.as_str())
    {
        return Err("browser evidence anchor is stale".to_string());
    }
    let scope = scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
    let (expired, active_capture_exists) = {
        let mut captures = state
            .evidence_captures
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let expired_ids = captures
            .iter()
            .filter_map(|(capture_id, capture)| {
                (capture.expires_at <= now).then(|| capture_id.clone())
            })
            .collect::<Vec<_>>();
        let expired = expired_ids
            .into_iter()
            .filter_map(|capture_id| captures.remove(&capture_id))
            .collect::<Vec<_>>();
        let active_capture_exists = captures
            .values()
            .any(|capture| capture.scope == scope && capture.expires_at > now);
        (expired, active_capture_exists)
    };
    for record in expired {
        schedule_browser_evidence_cleanup(state, record);
    }
    if active_capture_exists {
        return Err("browser evidence capture is already active".to_string());
    }
    let document_identity = consume_browser_evidence_anchor(&state, &anchor, visibility_policy)?;
    let capture_id = new_browser_evidence_credential("c");
    let capture_token = new_browser_evidence_credential("t");
    let capture_now = Instant::now();
    let expires_at = (capture_now + BROWSER_EVIDENCE_TTL).min(grant_expiry);
    let record = BrowserEvidenceCaptureRecord {
        scope,
        capture_token: capture_token.clone(),
        lifecycle_token: anchor.lifecycle_token,
        url: anchor.url.clone(),
        page_load_revision: anchor.page_load_revision,
        document_identity,
        expires_at,
    };
    state
        .evidence_captures
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .insert(capture_id.clone(), record);

    let script = match browser_evidence_start_script(
        &capture_token,
        &anchor.url,
        document_identity,
        expires_at
            .saturating_duration_since(capture_now)
            .as_millis()
            .min(u64::MAX as u128) as u64,
    ) {
        Ok(script) => script,
        Err(error) => {
            // No page-side wrapper exists when script construction failed.
            let _ = remove_browser_evidence_capture_with_token(state, &capture_id, &capture_token);
            return Err(error);
        }
    };
    if let Err(error) = require_browser_control_grant(
        &state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        Instant::now(),
    ) {
        // This is before `eval_with_callback`, so no wrapper could be live.
        let _ = remove_browser_evidence_capture_with_token(state, &capture_id, &capture_token);
        return Err(error);
    }
    let started = eval_browser_evidence_with_callback(&state, script)
        .await
        .and_then(|raw| parse_browser_evidence_callback(&raw).map(|_| ()));
    if let Err(error) = started {
        // Evaluation may have installed wrappers before its callback failed or
        // timed out. Remove the capability first, then attempt a fixed drain.
        expire_browser_evidence_capture(state, &capture_id, &capture_token);
        return Err(error);
    }

    let remaining_ms = expires_at
        .duration_since(Instant::now())
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let expiry_state = state.clone();
    let expiry_id = capture_id.clone();
    let expiry_token = capture_token.clone();
    let delay = expires_at.saturating_duration_since(Instant::now());
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        expire_browser_evidence_capture(&expiry_state, &expiry_id, &expiry_token);
    });
    Ok(BrowserEvidenceCaptureHandle {
        capture_id,
        remaining_ms,
    })
}

/// Consumes and drains one evidence capture. The backend record is removed
/// before the native callback, so timeout, page changes, or malformed remote
/// data cannot be retried with the same capability.
#[tauri::command]
pub async fn browser_read_evidence_capture(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    capture_id: String,
) -> Result<BrowserEvidenceResult, String> {
    let audit_workspace_id = workspace_id.clone();
    let audit_session_id = session_id.clone();
    let grant_state =
        browser_audit_active_grant_state(&state, &audit_workspace_id, audit_session_id.as_deref());
    let result =
        read_browser_evidence_capture(&state, &sessions, workspace_id, session_id, capture_id)
            .await;
    append_browser_audit(
        &state,
        BrowserAuditOrigin::Ui,
        None,
        None,
        &audit_workspace_id,
        audit_session_id.as_deref(),
        BrowserAuditTool::EvidenceRead,
        grant_state,
        browser_audit_outcome(result.as_ref().err().map(String::as_str)),
    );
    result
}

/// Shared one-shot evidence drain. The binding owns workspace/session; callers
/// deliberately cannot provide a lifecycle token for this operation.
pub(crate) async fn read_browser_evidence_capture(
    state: &BrowserState,
    sessions: &SessionCommandState,
    workspace_id: String,
    session_id: Option<String>,
    capture_id: String,
) -> Result<BrowserEvidenceResult, String> {
    let _operation = state.operation_lock.lock().await;
    validate_browser_evidence_capture_id(&capture_id)?;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    let snapshot = current_snapshot(&state)?;
    let lifecycle_token = snapshot.lifecycle_token;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser evidence capture is stale",
    )?;
    require_browser_control_grant(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        Instant::now(),
    )?;
    let scope = scope_key(&workspace_id, session_id.as_deref());
    let record = {
        let mut captures = state
            .evidence_captures
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let Some(record) = captures.get(&capture_id).cloned() else {
            return Err("browser evidence capture is unavailable".to_string());
        };
        if record.scope != scope
            || record.lifecycle_token != lifecycle_token
            || record.expires_at <= Instant::now()
        {
            captures.remove(&capture_id);
            return Err("browser evidence capture is stale".to_string());
        }
        if !snapshot.visible || snapshot.url.as_deref() != Some(record.url.as_str()) {
            captures.remove(&capture_id);
            return Err("browser evidence capture is stale".to_string());
        }
        let current_revision = state
            .contexts
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?
            .get(&scope)
            .filter(|context| context_is_ready_for_url(context, &record.url))
            .map(|context| context.page_load_revision);
        if current_revision != Some(record.page_load_revision) {
            captures.remove(&capture_id);
            return Err("browser evidence capture is stale".to_string());
        }
        // One-shot consume happens before the native drain callback.
        captures.remove(&capture_id);
        record
    };

    let result = async {
        // Recheck the grant at the effect boundary. Expiry after consumption is
        // a safe failure and deliberately cannot restore the consumed capability.
        require_browser_control_grant(
            &state,
            &workspace_id,
            session_id.as_deref(),
            lifecycle_token,
            Instant::now(),
        )?;
        let script = browser_evidence_read_script(
            &record.capture_token,
            &record.url,
            record.document_identity,
        )?;
        let raw = eval_browser_evidence_with_callback(&state, script).await?;
        let callback = parse_browser_evidence_callback(&raw)?;
        let (events, truncated) =
            normalize_browser_evidence_events(callback.events, callback.truncated);
        Ok(BrowserEvidenceResult {
            events,
            truncated,
            untrusted: true,
        })
    }
    .await;
    if result.is_err() {
        // The page-side wrapper may still be installed after a construction,
        // callback, parse, timeout, or grant-boundary failure. The one-shot
        // record is already gone, so cleanup cannot re-expose its events.
        schedule_browser_evidence_cleanup(state, record);
    }
    result
}

/// Best-effort cleanup for an evidence capture that could not be handed to an
/// authenticated projection lease. It immediately removes the backend record
/// and schedules a short fixed-script drain to restore page wrappers without
/// returning their buffered events.
pub(crate) fn discard_browser_evidence_capture(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    capture_id: &str,
) {
    let scope = scope_key(workspace_id, session_id);
    let record = state
        .evidence_captures
        .lock()
        .ok()
        .and_then(|mut captures| {
            let belongs_to_scope = captures
                .get(capture_id)
                .is_some_and(|capture| capture.scope == scope);
            belongs_to_scope
                .then(|| captures.remove(capture_id))
                .flatten()
        });
    let Some(record) = record else {
        return;
    };
    schedule_browser_evidence_cleanup(state, record);
}

#[tauri::command]
pub async fn browser_extract_context(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserAgentContext, String> {
    let audit_workspace_id = workspace_id.clone();
    let audit_session_id = session_id.clone();
    let result = extract_browser_context_for_scope(
        &state,
        &sessions,
        workspace_id,
        session_id,
        lifecycle_token,
    )
    .await;
    append_browser_audit(
        &state,
        BrowserAuditOrigin::Ui,
        None,
        None,
        &audit_workspace_id,
        audit_session_id.as_deref(),
        BrowserAuditTool::Context,
        BrowserAuditGrantState::NotApplicable,
        browser_audit_outcome(result.as_ref().err().map(String::as_str)),
    );
    result
}

pub(crate) async fn extract_browser_context_for_scope(
    state: &BrowserState,
    sessions: &SessionCommandState,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserAgentContext, String> {
    let _operation = state.operation_lock.lock().await;
    extract_browser_context_locked(
        state,
        sessions,
        workspace_id,
        session_id,
        lifecycle_token,
        BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
    )
    .await
}

/// Context endpoint for the app-owned MCP projection. The caller never gets
/// to supply a lifecycle token: it is taken from the active native snapshot
/// while holding the same operation lock used for extraction.
pub(crate) async fn extract_browser_control_context(
    state: &BrowserState,
    sessions: &SessionCommandState,
    workspace_id: String,
    session_id: Option<String>,
) -> Result<BrowserControlContext, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(sessions, &workspace_id, session_id.as_deref()).await?;
    let snapshot = current_snapshot(state)?;
    if !snapshot.visible {
        return Err("browser is not visible".to_string());
    }
    let lifecycle_token = snapshot.lifecycle_token;
    require_current_lifecycle(
        state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser context lifecycle is stale",
    )?;
    require_browser_control_grant(
        state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        Instant::now(),
    )?;
    let exact_url = snapshot
        .url
        .ok_or_else(|| "browser page URL is unavailable".to_string())?;
    let context = extract_browser_context_locked(
        state,
        sessions,
        workspace_id.clone(),
        session_id.clone(),
        lifecycle_token,
        BrowserVisibilityPolicy::RequireVisible,
    )
    .await?;
    if let Err(error) = require_browser_control_grant(
        state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        Instant::now(),
    ) {
        // A grant can expire during the bounded native callback. The map was
        // just issued for this controlled read, so revoke it before reporting
        // the expired capability.
        invalidate_semantic_map_for_scope(state, &workspace_id, session_id.as_deref());
        return Err(error);
    }
    Ok(BrowserControlContext {
        anchor: BrowserActionAnchor {
            workspace_id,
            session_id,
            lifecycle_token,
            map_id: context.semantic_map.map_id.clone(),
            generation: context.semantic_map.generation,
            url: exact_url,
            page_load_revision: context.semantic_map.page_load_revision,
        },
        context,
    })
}

async fn extract_browser_context_locked(
    state: &BrowserState,
    sessions: &SessionCommandState,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    visibility_policy: BrowserVisibilityPolicy,
) -> Result<BrowserAgentContext, String> {
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser context lifecycle is stale",
    )?;
    let initial_snapshot = current_snapshot(&state)?;
    if !browser_surface_matches_visibility_policy(
        state,
        initial_snapshot.visible,
        visibility_policy,
    )? {
        return Err("browser is not visible".to_string());
    }
    let initial_url = initial_snapshot
        .url
        .ok_or_else(|| "browser page URL is unavailable".to_string())?;
    if !context_ready_for_scope(&state, &workspace_id, session_id.as_deref(), &initial_url)? {
        return Err("browser page is still loading".to_string());
    }
    let initial_page_load_revision =
        current_page_load_revision(&state, &workspace_id, session_id.as_deref(), &initial_url)?;

    let raw = eval_page(state, browser_context_script(), BROWSER_CONTEXT_TIMEOUT)
        .await
        .map_err(|error| match error {
            PageEvalError::NotOpen(message) => message,
            PageEvalError::Start(error) => format!("failed to read browser page context: {error}"),
            PageEvalError::Timeout => "browser page context timed out".to_string(),
            PageEvalError::Cancelled => "browser page context response was cancelled".to_string(),
        })?;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser context lifecycle is stale",
    )?;
    let snapshot = current_snapshot(&state)?;
    if !browser_surface_matches_visibility_policy(state, snapshot.visible, visibility_policy)?
        || snapshot.url.as_deref() != Some(initial_url.as_str())
    {
        return Err("browser page changed while reading context".to_string());
    }
    if !context_ready_for_scope(&state, &workspace_id, session_id.as_deref(), &initial_url)? {
        return Err("browser page changed while reading context".to_string());
    }
    // The trust boundary over what the page answered, shared with the
    // native app (`dcc_core::domain::browser_context`).
    let bounded = bound_context(&raw, &initial_url, snapshot.title.as_deref())?;

    // Page-load callbacks do not take the operation lock. Repeat the complete
    // validity check after parsing untrusted JSON and immediately before the
    // backend issues the map id/generation.
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser context lifecycle is stale",
    )?;
    let final_snapshot = current_snapshot(&state)?;
    if !browser_surface_matches_visibility_policy(state, final_snapshot.visible, visibility_policy)?
        || final_snapshot.url.as_deref() != Some(initial_url.as_str())
    {
        return Err("browser page changed while building semantic map".to_string());
    }
    if !context_ready_for_scope(&state, &workspace_id, session_id.as_deref(), &initial_url)? {
        return Err("browser page changed while building semantic map".to_string());
    }
    if !page_load_revision_is_current(
        current_page_load_revision(&state, &workspace_id, session_id.as_deref(), &initial_url)?,
        initial_page_load_revision,
    ) {
        return Err("browser page changed while building semantic map".to_string());
    }
    let semantic_map = issue_semantic_map(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        &initial_url,
        initial_page_load_revision,
        bounded.document_identity,
        bounded.items,
        bounded.targets,
        bounded.semantic_truncated,
    )?;

    Ok(BrowserAgentContext {
        workspace_id,
        session_id,
        url: bounded.url,
        title: bounded.title,
        text: bounded.text,
        selection_only: bounded.selection_only,
        truncated: bounded.truncated || semantic_map.truncated,
        semantic_map,
    })
}

pub fn shutdown(state: &BrowserState) {
    clear_browser_evidence_captures(state);
}

// ---- The native app's Browser (`http_api::browser_bridge`) ----------------

/// Page operations go to the native app's Browser through the bridge; the
/// scope, lifecycle, grant, maps, evidence and audit around them stay here.
pub(crate) enum PageDriver {
    Remote,
}

pub(crate) enum PageEvalError {
    /// Neither Browser is there ("browser is not open").
    NotOpen(String),
    Start(String),
    Timeout,
    Cancelled,
}

fn remote_attached(state: &BrowserState) -> bool {
    state.remote.lock().map(|remote| *remote).unwrap_or(false)
}

pub(crate) fn page_driver(state: &BrowserState) -> Result<PageDriver, String> {
    if remote_attached(state) {
        return Ok(PageDriver::Remote);
    }
    Err("browser is not open".to_string())
}

impl PageDriver {
    fn navigate(&self, url: &Url) -> Result<(), String> {
        match self {
            Self::Remote => {
                dcc_http::native_browser_bridge().fire(dcc_http::BridgeOpKind::Navigate {
                    url: url.to_string(),
                })
            }
        }
    }

    fn reload(&self) -> Result<(), String> {
        match self {
            Self::Remote => dcc_http::native_browser_bridge().fire(dcc_http::BridgeOpKind::Reload),
        }
    }

    fn eval_detached(&self, script: String) -> Result<(), String> {
        match self {
            Self::Remote => dcc_http::native_browser_bridge()
                .fire(dcc_http::BridgeOpKind::EvalDetached { script }),
        }
    }
}

/// An input or a capture in the native app's Browser, its answer within
/// `wait`; anything but a delivered answer is `failure`.
async fn remote_input(
    kind: dcc_http::BridgeOpKind,
    wait: Duration,
    failure: &str,
) -> Result<Option<String>, String> {
    let receiver = dcc_http::native_browser_bridge().request(kind)?;
    match timeout(wait, receiver).await {
        Ok(Ok(reply)) if reply.ok => Ok(reply.raw),
        _ => Err(failure.to_string()),
    }
}

/// `browser_input::click`: a real click at the fractions of the viewport.
async fn deliver_click(state: &BrowserState, x: f64, y: f64) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() || !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
        return Err("browser click point is invalid".into());
    }
    match page_driver(state)? {
        PageDriver::Remote => remote_input(
            dcc_http::BridgeOpKind::Click { x, y },
            Duration::from_secs(3),
            "browser click could not be delivered",
        )
        .await
        .map(|_| ()),
    }
}

/// `browser_input::press`: one allowed key to the focused page.
async fn deliver_press(state: &BrowserState, key: &str) -> Result<(), String> {
    match page_driver(state)? {
        PageDriver::Remote => {
            if !crate::browser_input::valid_key(key) {
                return Err("browser key is invalid".into());
            }
            remote_input(
                dcc_http::BridgeOpKind::Press {
                    key: key.to_string(),
                },
                Duration::from_secs(3),
                "browser keyboard input could not be delivered",
            )
            .await
            .map(|_| ())
        }
    }
}

/// The native Browser's viewport as PNG (`browser_capture::capture`'s bounds).
async fn remote_capture() -> Result<Vec<u8>, String> {
    use base64::Engine;
    let encoded = remote_input(
        dcc_http::BridgeOpKind::Snapshot,
        Duration::from_secs(8),
        "Browser snapshot could not be captured",
    )
    .await?
    .ok_or("Browser snapshot could not be captured")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Browser snapshot could not be captured")?;
    if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err("Browser snapshot could not be captured".into());
    }
    Ok(bytes)
}

/// A script in the page, its JSON answer within `wait`.
async fn eval_page(
    state: &BrowserState,
    script: String,
    wait: Duration,
) -> Result<String, PageEvalError> {
    match page_driver(state).map_err(PageEvalError::NotOpen)? {
        PageDriver::Remote => {
            let receiver = dcc_http::native_browser_bridge()
                .request(dcc_http::BridgeOpKind::Eval { script })
                .map_err(PageEvalError::NotOpen)?;
            let reply = timeout(wait, receiver)
                .await
                .map_err(|_| PageEvalError::Timeout)?
                .map_err(|_| PageEvalError::Cancelled)?;
            if reply.ok {
                Ok(reply.raw.unwrap_or_default())
            } else {
                Err(PageEvalError::Start(reply.error.unwrap_or_else(|| {
                    "browser page script failed".to_string()
                })))
            }
        }
    }
}

fn remote_token_is_current(state: &BrowserState, lifecycle_token: u64) -> Result<String, String> {
    let current = *state
        .lifecycle_token
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let open = *state
        .lifecycle_open
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    if !remote_attached(state) || !open || current != lifecycle_token {
        return Err("browser lifecycle is stale".to_string());
    }
    state
        .active_scope
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "browser lifecycle is stale".to_string())
}

/// `browser_open`'s bookkeeping for the native app's Browser, opened on
/// `url` for a task's conversation: it becomes the surface (the desktop's
/// own view, if any, goes), a new lifecycle starts — no grant, map or
/// evidence carried over — and the URL is the one asked for.
pub(crate) async fn remote_browser_open(
    state: &BrowserState,
    sessions: &SessionCommandState,
    workspace_id: &str,
    session_id: Option<&str>,
    url: &str,
) -> Result<u64, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(sessions, workspace_id, session_id).await?;
    let url = validate_browser_url(url)?.to_string();
    set_active_scope(state, workspace_id, session_id)?;
    let token = {
        let mut token = state
            .lifecycle_token
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        advance_lifecycle_token(&mut token);
        *token
    };
    invalidate_semantic_map_for_scope(state, workspace_id, session_id);
    clear_browser_evidence_captures(state);
    clear_browser_control_grant(state);
    *state
        .remote
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = true;
    *state
        .lifecycle_open
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = true;
    *state
        .occluded
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = false;
    *state
        .visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = true;
    if let Ok(mut contexts) = state.contexts.lock() {
        mark_context_open_request(
            contexts
                .entry(scope_key(workspace_id, session_id))
                .or_default(),
            url,
            None,
        );
    }
    Ok(token)
}

/// What the native Browser's page did, in the desktop's page context.
pub(crate) async fn remote_browser_report(
    state: &BrowserState,
    lifecycle_token: u64,
    report: dcc_http::PageReport,
) -> Result<(), String> {
    let _operation = state.operation_lock.lock().await;
    let key = remote_token_is_current(state, lifecycle_token)?;
    use dcc_http::PageReport;
    if let PageReport::Visibility { visible } = report {
        *state
            .visible
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())? = visible;
        *state
            .occluded
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())? = !visible;
        return Ok(());
    }
    let mut changed_page = false;
    {
        let mut contexts = state
            .contexts
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let context = contexts.entry(key.clone()).or_default();
        match report {
            PageReport::Loading { url } => {
                mark_context_loading(context, url);
                context.main_load_started = true;
                changed_page = true;
            }
            PageReport::Loaded { url } => {
                context.expected_url = Some(url.clone());
                context.url = Some(url.clone());
                context.ready_url = Some(url);
                context.main_load_started = false;
            }
            // A single-page app moved: a new document for the agent.
            PageReport::Url { url } => {
                if context.ready_url.is_some() && context.url.as_deref() != Some(url.as_str()) {
                    mark_context_loading(context, url.clone());
                    context.url = Some(url.clone());
                    context.ready_url = Some(url);
                    changed_page = true;
                } else if context.ready_url.is_none() {
                    context.expected_url = Some(url);
                }
            }
            PageReport::Title { title } => {
                context.title = (!title.trim().is_empty()).then_some(title);
            }
            PageReport::Visibility { .. } => {}
        }
    }
    if changed_page {
        invalidate_browser_evidence_for_scope_key(state, &key);
    }
    Ok(())
}

/// `browser_hide`'s bookkeeping: the native Browser closed (or its app
/// went away): no grant, no evidence, a new lifecycle, no surface.
pub(crate) async fn remote_browser_close(
    state: &BrowserState,
    lifecycle_token: Option<u64>,
) -> Result<(), String> {
    let _operation = state.operation_lock.lock().await;
    let key = match lifecycle_token {
        Some(token) => remote_token_is_current(state, token)?,
        None if remote_attached(state) => state
            .active_scope
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?
            .clone()
            .unwrap_or_default(),
        None => return Ok(()),
    };
    clear_browser_control_grant(state);
    invalidate_browser_evidence_for_scope_key(state, &key);
    if let Ok(mut contexts) = state.contexts.lock() {
        if let Some(context) = contexts.get_mut(&key) {
            invalidate_semantic_map(context);
        }
    }
    *state
        .remote
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = false;
    *state
        .visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = false;
    *state
        .lifecycle_open
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = false;
    *state
        .occluded
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = false;
    let mut token = state
        .lifecycle_token
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    advance_lifecycle_token(&mut token);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn the_native_browser_runs_the_scripts_and_reports_its_page() {
        use super::*;
        let state = BrowserState::default();
        let scope = scope_key("w-native", Some("s-native"));
        *state.active_scope.lock().unwrap() = Some(scope.clone());
        *state.lifecycle_token.lock().unwrap() = 41;
        *state.lifecycle_open.lock().unwrap() = true;
        *state.remote.lock().unwrap() = true;
        // The bridge carries the script; the answer comes back by its id.
        let mut native = dcc_http::native_browser_bridge().subscribe();
        let answering = tokio::spawn(async move {
            let op = native.recv().await.unwrap();
            assert_eq!(
                op.kind,
                dcc_http::BridgeOpKind::Eval {
                    script: "document.title".into()
                }
            );
            dcc_http::native_browser_bridge().reply(
                op.op_id,
                dcc_http::BridgeReply {
                    ok: true,
                    raw: Some("\"Docs\"".into()),
                    error: None,
                },
            );
        });
        let raw = eval_page(&state, "document.title".into(), Duration::from_secs(2)).await;
        answering.await.unwrap();
        assert_eq!(raw.ok().as_deref(), Some("\"Docs\""));
        // Its page reports move the desktop's context like WebKit's do.
        use dcc_http::PageReport;
        remote_browser_report(
            &state,
            41,
            PageReport::Loading {
                url: "https://ex.com/a".into(),
            },
        )
        .await
        .unwrap();
        remote_browser_report(
            &state,
            41,
            PageReport::Loaded {
                url: "https://ex.com/a".into(),
            },
        )
        .await
        .unwrap();
        let revision = {
            let contexts = state.contexts.lock().unwrap();
            let context = &contexts[&scope];
            assert!(context_is_ready_for_url(context, "https://ex.com/a"));
            context.page_load_revision
        };
        remote_browser_report(
            &state,
            41,
            PageReport::Url {
                url: "https://ex.com/b".into(),
            },
        )
        .await
        .unwrap();
        {
            let contexts = state.contexts.lock().unwrap();
            let context = &contexts[&scope];
            assert!(context_is_ready_for_url(context, "https://ex.com/b"));
            assert!(context.page_load_revision > revision, "a new document");
        }
        assert!(
            remote_browser_report(&state, 40, PageReport::Title { title: "x".into() })
                .await
                .is_err(),
            "an old lifecycle"
        );
        // A real click, and the viewport's PNG, from the native Browser.
        let mut native = dcc_http::native_browser_bridge().subscribe();
        let answering = tokio::spawn(async move {
            use base64::Engine;
            for _ in 0..2 {
                let op = native.recv().await.unwrap();
                let raw = match op.kind {
                    dcc_http::BridgeOpKind::Click { x, y } => {
                        assert_eq!((x, y), (0.25, 0.5));
                        None
                    }
                    dcc_http::BridgeOpKind::Snapshot => {
                        Some(base64::engine::general_purpose::STANDARD.encode(b"png"))
                    }
                    other => panic!("unexpected {other:?}"),
                };
                dcc_http::native_browser_bridge().reply(
                    op.op_id,
                    dcc_http::BridgeReply {
                        ok: true,
                        raw,
                        error: None,
                    },
                );
            }
        });
        deliver_click(&state, 0.25, 0.5).await.unwrap();
        assert_eq!(remote_capture().await.unwrap(), b"png");
        answering.await.unwrap();
        assert!(
            deliver_click(&state, 1.5, 0.5).await.is_err(),
            "off the viewport"
        );
        assert_eq!(
            deliver_press(&state, "Meta+Q").await.unwrap_err(),
            "browser key is invalid"
        );
        remote_browser_close(&state, Some(41)).await.unwrap();
        assert!(!*state.lifecycle_open.lock().unwrap());
        assert!(matches!(page_driver(&state), Err(error) if error == "browser is not open"));
    }

    use std::time::{Duration as StdDuration, Instant};

    use super::{
        advance_lifecycle_token, append_browser_audit, arm_browser_control_grant,
        browser_action_anchor_matches_context, browser_action_url_check_script,
        browser_audit_outcome, browser_evidence_cleanup_script, browser_evidence_read_script,
        browser_evidence_start_script, browser_scroll_script, browser_semantic_action_script,
        browser_surface_matches_visibility_policy, clear_browser_control_grant,
        click_target_is_compatible, consume_browser_evidence_anchor,
        consume_semantic_map_for_anchor, control_grant_is_current, control_grant_status,
        fill_target_is_compatible, mark_context_loading, normalize_browser_context_text,
        normalize_browser_evidence_events, normalize_browser_evidence_resource_initiator_type,
        normalize_browser_evidence_resource_status, normalize_browser_fill_text,
        normalize_browser_scroll_delta, normalize_browser_semantic_map,
        occlusion_request_is_current, page_load_revision_is_current, parse_browser_action_callback,
        parse_browser_evidence_callback, parse_browser_evidence_cleanup_callback,
        prepare_browser_control_action, read_browser_audit, require_browser_control_grant,
        require_current_lifecycle, sanitize_browser_evidence_url,
        sanitize_browser_link_destination, semantic_item_serialized_chars,
        semantic_map_record_is_current, should_persist_context_url, tracked_browser_url,
        validate_browser_document_identity, validate_browser_evidence_capture_id,
        validate_browser_reference, validate_browser_url, BrowserActionAnchor, BrowserActionResult,
        BrowserAuditGrantState, BrowserAuditOrigin, BrowserAuditOutcome, BrowserAuditRecord,
        BrowserAuditTool, BrowserAuditViewRecord, BrowserContext, BrowserControlAction,
        BrowserControlGrant, BrowserControlStatus, BrowserEvidenceCaptureRecord,
        BrowserEvidenceEventRaw, BrowserSemanticItemExtraction, BrowserSemanticMap,
        BrowserSemanticMapExtraction, BrowserSemanticMapRecord, BrowserSemanticTargetRecord,
        BrowserState, BrowserVisibilityPolicy, MAX_BROWSER_AUDIT_ENTRIES,
        MAX_BROWSER_AUDIT_LEASE_FINGERPRINT_CHARS, MAX_BROWSER_AUDIT_PROVIDER_CHARS,
        MAX_BROWSER_AUDIT_SCOPE_CHARS, MAX_BROWSER_EVIDENCE_CHARS, MAX_BROWSER_EVIDENCE_EVENTS,
        MAX_BROWSER_EVIDENCE_RESOURCE_DURATION_MS, MAX_BROWSER_SEMANTIC_ITEMS,
    };

    #[test]
    fn allows_https_and_local_http_only() {
        assert!(validate_browser_url("https://example.com/docs").is_ok());
        assert!(validate_browser_url("HTTPS://EXAMPLE.COM/docs").is_ok());
        assert!(validate_browser_url("http://localhost:3000").is_ok());
        assert!(validate_browser_url("http://127.0.0.1:5173").is_ok());
        assert!(validate_browser_url("http://[::1]:8080").is_ok());
        assert!(validate_browser_url("http://127.0.0.2:5173").is_ok());
        assert!(validate_browser_url("http://example.com").is_err());
        assert!(validate_browser_url("http://localhost.evil.test").is_err());
        assert!(validate_browser_url("http://[::2]:8080").is_err());
        assert!(validate_browser_url("https://").is_err());
        assert!(validate_browser_url("https:///path").is_err());
        assert!(validate_browser_url("ftp://example.com").is_err());
        assert!(validate_browser_url("javascript:alert(1)").is_err());
        assert!(validate_browser_url("data:text/html,hello").is_err());
        assert!(validate_browser_url("https://user:pass@example.com").is_err());
        assert!(validate_browser_url("https://user@example.com").is_err());
        assert!(validate_browser_url("https://example.com:443@evil.test").is_err());
    }

    #[test]
    fn bounds_page_context_and_removes_control_sequences() {
        let (text, truncated) = normalize_browser_context_text("one\r\ntwo\u{1b}[31m", 6);
        assert_eq!(text, "one\ntw");
        assert!(truncated);

        let (text, truncated) = normalize_browser_context_text("\u{0000} safe\ttext ", 32);
        assert_eq!(text, "safe\ttext");
        assert!(!truncated);
    }

    #[test]
    fn semantic_extraction_script_uses_only_bounded_incremental_walks() {
        let script = super::browser_context_script();
        assert!(script.contains("createTreeWalker"));
        assert!(script.contains("maxScanNodes"));
        assert!(script.contains("maxTextNodes"));
        assert!(script.contains("maxShadowRoots"));
        assert!(script.contains("maxCandidates"));
        assert!(script.contains("maxMapChars"));
        assert!(script.contains("const textNodeSafety"));
        assert!(script.contains("const composedParent"));
        assert!(script.contains("element?.shadowRoot"));
        assert!(script.contains("const composedVisibleText"));
        assert!(script.contains("[\"input\", \"textarea\", \"select\", \"option\"].includes(tag)"));
        assert!(script.contains("element.isContentEditable"));
        assert!(script.contains("const safety = textNodeSafety(node)"));
        assert!(script.contains("range.startContainer !== range.endContainer"));
        assert!(script.contains("const selectionSafety = textNodeSafety(range.startContainer)"));
        assert!(!script.contains("querySelectorAll"));
        assert!(!script.contains(".innerText"));
        assert!(!script.contains(".outerHTML"));
        assert!(!script.contains("document.cookie"));
        assert!(!script.contains("localStorage"));
        assert!(!script.contains("sessionStorage"));
        assert!(!script.contains(".value"));
        assert!(script.contains("performance.timeOrigin"));
        assert!(script.contains("documentIdentity"));
    }

    #[test]
    fn document_identity_is_private_finite_and_distinguishes_same_url_documents() {
        assert_eq!(validate_browser_document_identity(Some(42.5)), Ok(42.5));
        assert!(validate_browser_document_identity(Some(0.0)).is_err());
        assert!(validate_browser_document_identity(Some(f64::NAN)).is_err());
        assert!(validate_browser_document_identity(None).is_err());

        let old_document = 42.5;
        let new_document = 43.5;
        assert_ne!(old_document, new_document);
        let script = browser_semantic_action_script(
            &target("button", "button", None),
            None,
            "https://example.test/page",
            old_document,
        )
        .unwrap();
        assert!(script.contains("expectedDocumentIdentity"));
        assert!(script.contains("performance.timeOrigin !== expectedDocumentIdentity"));
        assert!(!script.contains("documentIdentity:"));
    }

    fn action_anchor_and_context() -> (BrowserActionAnchor, BrowserContext) {
        let scope = "workspace\u{1f}|session".to_string();
        let anchor = BrowserActionAnchor {
            workspace_id: "workspace".to_string(),
            session_id: Some("session".to_string()),
            lifecycle_token: 8,
            map_id: "m-8-2".to_string(),
            generation: 2,
            url: "https://example.test/page".to_string(),
            page_load_revision: 4,
        };
        let context = BrowserContext {
            url: Some(anchor.url.clone()),
            expected_url: Some(anchor.url.clone()),
            ready_url: Some(anchor.url.clone()),
            page_load_revision: anchor.page_load_revision,
            semantic_map: Some(BrowserSemanticMapRecord {
                scope,
                map_id: anchor.map_id.clone(),
                generation: anchor.generation,
                lifecycle_token: anchor.lifecycle_token,
                url: anchor.url.clone(),
                page_load_revision: anchor.page_load_revision,
                document_identity: 1.0,
                targets: vec![BrowserSemanticTargetRecord {
                    reference: "e1".to_string(),
                    ordinal: 2,
                    role: "button".to_string(),
                    name: "Save".to_string(),
                    destination: None,
                    disabled: Some(false),
                    checked: None,
                    selected: None,
                    expanded: None,
                    pressed: None,
                    tag: "button".to_string(),
                    input_type: None,
                    content_editable: false,
                }],
            }),
            ..BrowserContext::default()
        };
        (anchor, context)
    }

    #[test]
    fn action_anchor_requires_every_current_identity_dimension_and_is_consumed() {
        let (anchor, context) = action_anchor_and_context();
        let scope = super::scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
        assert!(browser_action_anchor_matches_context(
            &context, &anchor, &scope
        ));

        let mut stale = anchor.clone();
        stale.workspace_id = "other-workspace".to_string();
        assert!(!browser_action_anchor_matches_context(
            &context,
            &stale,
            &super::scope_key(&stale.workspace_id, stale.session_id.as_deref()),
        ));
        let mut stale = anchor.clone();
        stale.session_id = Some("other-session".to_string());
        assert!(!browser_action_anchor_matches_context(
            &context,
            &stale,
            &super::scope_key(&stale.workspace_id, stale.session_id.as_deref()),
        ));
        for stale in [
            BrowserActionAnchor {
                lifecycle_token: 9,
                ..anchor.clone()
            },
            BrowserActionAnchor {
                map_id: "m-8-3".to_string(),
                ..anchor.clone()
            },
            BrowserActionAnchor {
                generation: 3,
                ..anchor.clone()
            },
            BrowserActionAnchor {
                url: "https://example.test/other".to_string(),
                ..anchor.clone()
            },
            BrowserActionAnchor {
                page_load_revision: 5,
                ..anchor.clone()
            },
        ] {
            assert!(!browser_action_anchor_matches_context(
                &context, &stale, &scope
            ));
        }

        let mut consumable = context.clone();
        assert!(consume_semantic_map_for_anchor(
            &mut consumable,
            &anchor,
            &scope
        ));
        assert!(consumable.semantic_map.is_none());
        assert!(!consume_semantic_map_for_anchor(
            &mut consumable,
            &anchor,
            &scope
        ));

        let invalid_payload_keeps_map = context.clone();
        assert!(
            prepare_browser_control_action(BrowserControlAction::Navigate {
                url: "javascript:alert(1)".to_string(),
            })
            .is_err()
        );
        assert!(invalid_payload_keeps_map.semantic_map.is_some());
    }

    #[test]
    fn unknown_reference_is_rejected_without_target_lookup_or_side_effect() {
        let (anchor, context) = action_anchor_and_context();
        let state = BrowserState::default();
        let scope = super::scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
        state.contexts.lock().unwrap().insert(scope, context);
        assert_eq!(
            super::browser_action_target(&state, &anchor, "e1")
                .unwrap()
                .0
                .reference,
            "e1"
        );
        assert!(super::browser_action_target(&state, &anchor, "e2").is_err());
        assert!(state
            .contexts
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .semantic_map
            .is_some());
    }

    #[test]
    fn tracked_url_prefers_the_requested_page_without_reading_the_native_webview() {
        let context = BrowserContext {
            url: Some("https://example.test/previous".to_string()),
            expected_url: Some("http://localhost:5173".to_string()),
            ..BrowserContext::default()
        };
        assert_eq!(
            tracked_browser_url(&context).as_deref(),
            Some("http://localhost:5173")
        );

        let committed_only = BrowserContext {
            url: Some("https://example.test/ready".to_string()),
            ..BrowserContext::default()
        };
        assert_eq!(
            tracked_browser_url(&committed_only).as_deref(),
            Some("https://example.test/ready")
        );
    }

    #[test]
    fn failed_localhost_snapshot_and_persist_do_not_require_a_native_url() {
        let state = BrowserState::default();
        *state.active_workspace.lock().unwrap() = Some("workspace".to_string());
        *state.active_session.lock().unwrap() = Some("session".to_string());
        *state.visible.lock().unwrap() = false;
        *state.lifecycle_token.lock().unwrap() = 9;
        state.contexts.lock().unwrap().insert(
            super::scope_key("workspace", Some("session")),
            BrowserContext {
                url: Some("https://example.test/previous".to_string()),
                expected_url: Some("http://localhost:5173".to_string()),
                ready_url: None,
                ..BrowserContext::default()
            },
        );

        super::persist_current_context(&state);
        let snapshot = super::current_snapshot(&state).unwrap();
        assert!(!snapshot.visible);
        assert_eq!(snapshot.lifecycle_token, 9);
        assert_eq!(snapshot.url.as_deref(), Some("http://localhost:5173"));
        assert_eq!(
            state
                .contexts
                .lock()
                .unwrap()
                .get(&super::scope_key("workspace", Some("session")))
                .and_then(|context| context.url.as_deref()),
            Some("https://example.test/previous")
        );
    }

    #[test]
    fn control_grants_are_scoped_lifecycle_bound_expiring_and_clearable() {
        let now = Instant::now();
        let grant = BrowserControlGrant {
            scope: "workspace\u{1f}|session".to_string(),
            lifecycle_token: 8,
            lease_id: None,
            expires_at: now + StdDuration::from_secs(60),
        };
        assert!(control_grant_is_current(
            Some(&grant),
            "workspace\u{1f}|session",
            8,
            now,
        ));
        assert!(!control_grant_is_current(
            Some(&grant),
            "workspace\u{1f}|other-session",
            8,
            now,
        ));
        assert!(!control_grant_is_current(
            Some(&grant),
            "workspace\u{1f}|session",
            9,
            now,
        ));
        assert!(!control_grant_is_current(
            Some(&grant),
            "workspace\u{1f}|session",
            8,
            grant.expires_at,
        ));

        let state = BrowserState::default();
        arm_browser_control_grant(&state, "workspace", Some("session"), 8, now).unwrap();
        assert!(control_grant_is_current(
            state.control_grant.lock().unwrap().as_ref(),
            "workspace\u{1f}|session",
            8,
            now,
        ));
        // browser_open and browser_hide call this same helper before their
        // lifecycle transitions, so neither can retain a prior arm.
        clear_browser_control_grant(&state);
        assert!(state.control_grant.lock().unwrap().is_none());
    }

    #[test]
    fn explicit_ui_actions_accept_only_visible_or_temporarily_occluded_open_surfaces() {
        let state = BrowserState::default();
        assert!(!browser_surface_matches_visibility_policy(
            &state,
            false,
            BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
        )
        .unwrap());

        *state.lifecycle_open.lock().unwrap() = true;
        assert!(browser_surface_matches_visibility_policy(
            &state,
            true,
            BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
        )
        .unwrap());

        *state.occluded.lock().unwrap() = true;
        assert!(browser_surface_matches_visibility_policy(
            &state,
            false,
            BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
        )
        .unwrap());
        assert!(!browser_surface_matches_visibility_policy(
            &state,
            false,
            BrowserVisibilityPolicy::RequireVisible,
        )
        .unwrap());

        *state.occluded.lock().unwrap() = false;
        assert!(!browser_surface_matches_visibility_policy(
            &state,
            false,
            BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
        )
        .unwrap());

        *state.lifecycle_open.lock().unwrap() = false;
        assert!(!browser_surface_matches_visibility_policy(
            &state,
            true,
            BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
        )
        .unwrap());
    }

    #[test]
    fn evidence_anchor_consumption_keeps_mcp_strict_but_allows_its_ui_menu() {
        let (anchor, context) = action_anchor_and_context();
        let state = BrowserState::default();
        *state.active_scope.lock().unwrap() = Some("workspace\u{1f}|session".to_string());
        *state.active_workspace.lock().unwrap() = Some("workspace".to_string());
        *state.active_session.lock().unwrap() = Some("session".to_string());
        *state.lifecycle_token.lock().unwrap() = anchor.lifecycle_token;
        *state.lifecycle_open.lock().unwrap() = true;
        *state.visible.lock().unwrap() = false;
        *state.occluded.lock().unwrap() = true;
        state
            .contexts
            .lock()
            .unwrap()
            .insert("workspace\u{1f}|session".to_string(), context);

        assert!(consume_browser_evidence_anchor(
            &state,
            &anchor,
            BrowserVisibilityPolicy::RequireVisible,
        )
        .is_err());
        assert!(state
            .contexts
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .semantic_map
            .is_some());

        assert!(consume_browser_evidence_anchor(
            &state,
            &anchor,
            BrowserVisibilityPolicy::AllowTemporaryUiOcclusion,
        )
        .is_ok());
        assert!(state
            .contexts
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .semantic_map
            .is_none());
    }

    #[test]
    fn control_grant_status_reports_remaining_and_clears_expired_grants() {
        let now = Instant::now();
        let mut grant = Some(BrowserControlGrant {
            scope: "workspace\u{1f}|session".to_string(),
            lifecycle_token: 8,
            lease_id: None,
            expires_at: now + StdDuration::from_secs(60),
        });
        let status = control_grant_status(&mut grant, "workspace\u{1f}|session", 8, now);
        assert!(status.armed);
        assert!(status.remaining_ms > 59_000);
        assert!(status.remaining_ms <= 60_000);

        let wrong_scope = control_grant_status(&mut grant, "workspace\u{1f}|other-session", 8, now);
        assert_eq!(
            wrong_scope,
            BrowserControlStatus {
                armed: false,
                remaining_ms: 0,
            }
        );
        assert!(
            grant.is_some(),
            "a different scope must not clear a live grant"
        );

        let expired = control_grant_status(
            &mut grant,
            "workspace\u{1f}|session",
            8,
            now + StdDuration::from_secs(60),
        );
        assert_eq!(
            expired,
            BrowserControlStatus {
                armed: false,
                remaining_ms: 0,
            }
        );
        assert!(grant.is_none());
    }

    #[test]
    fn grant_is_rechecked_after_one_shot_consumption_boundary() {
        let state = BrowserState::default();
        let now = Instant::now();
        *state.control_grant.lock().unwrap() = Some(BrowserControlGrant {
            scope: "workspace\u{1f}|session".to_string(),
            lifecycle_token: 8,
            lease_id: None,
            expires_at: now,
        });
        assert!(
            require_browser_control_grant(&state, "workspace", Some("session"), 8, now,).is_err()
        );
        assert!(state.control_grant.lock().unwrap().is_none());
        // The action map is consumed before this final check; expiry therefore
        // fails closed and requires a fresh context rather than retrying it.
    }

    #[test]
    fn scroll_actions_are_finite_bounded_and_use_a_fixed_literal_script() {
        assert_eq!(
            normalize_browser_scroll_delta(2_000.0, -2_000.0),
            Ok((2_000.0, -2_000.0))
        );
        assert!(normalize_browser_scroll_delta(2_000.1, 0.0).is_err());
        assert!(normalize_browser_scroll_delta(f64::NAN, 0.0).is_err());
        assert!(normalize_browser_scroll_delta(0.0, f64::INFINITY).is_err());
        let script = browser_scroll_script(12.5, -40.0);
        assert!(script.contains("window.scrollBy(12.5, -40)"));
        assert!(!script.contains("eval("));
        assert!(!script.contains("querySelector"));
        assert!(!script.contains("document."));
        assert!(!script.contains(".value"));

        let action: BrowserControlAction =
            serde_json::from_str(r#"{"kind":"scroll","deltaX":12.5,"deltaY":-40}"#).unwrap();
        assert_eq!(
            action,
            BrowserControlAction::Scroll {
                delta_x: 12.5,
                delta_y: -40.0,
            }
        );
        assert!(serde_json::from_str::<BrowserControlAction>(
            r#"{"kind":"scroll","delta_x":12.5,"delta_y":-40}"#,
        )
        .is_err());
    }

    #[test]
    fn action_anchor_and_semantic_map_serialize_in_camel_case() {
        let (anchor, _) = action_anchor_and_context();
        let anchor = serde_json::to_value(anchor).unwrap();
        assert_eq!(anchor["workspaceId"], "workspace");
        assert_eq!(anchor["pageLoadRevision"], 4);
        assert!(anchor.get("workspace_id").is_none());
        let mut anchor_with_extra = anchor.clone();
        anchor_with_extra["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<BrowserActionAnchor>(anchor_with_extra).is_err());

        let map = serde_json::to_value(BrowserSemanticMap {
            map_id: "m-8-2".to_string(),
            generation: 2,
            page_load_revision: 4,
            items: Vec::new(),
            truncated: false,
        })
        .unwrap();
        assert_eq!(map["pageLoadRevision"], 4);
        assert!(map.get("page_load_revision").is_none());
    }

    #[test]
    fn action_result_never_echoes_reference_or_fill_text() {
        let result = serde_json::to_string(&BrowserActionResult {
            action: "fill".to_string(),
            status: "executed".to_string(),
            requires_context_refresh: true,
        })
        .unwrap();
        assert!(!result.contains("e1"));
        assert!(!result.contains("secret fill value"));
    }

    #[test]
    fn persists_only_the_native_url_ready_for_the_active_scope() {
        let mut context = BrowserContext {
            url: Some("https://example.test/previous".to_string()),
            ..BrowserContext::default()
        };
        mark_context_loading(&mut context, "https://example.test/b".to_string());
        assert!(!should_persist_context_url(
            &context,
            Some("https://example.test/a"),
        ));
        assert_eq!(
            context.url.as_deref(),
            Some("https://example.test/previous")
        );

        context.ready_url = Some("https://example.test/b".to_string());
        assert!(should_persist_context_url(
            &context,
            Some("https://example.test/b"),
        ));
    }

    #[test]
    fn sanitizes_semantic_link_destinations_at_the_backend_boundary() {
        let (destination, truncated) = sanitize_browser_link_destination(
            "https://user:secret@example.test:8443/a/path?token=secret#private",
        );
        assert_eq!(
            destination.as_deref(),
            Some("https://example.test:8443/a/path")
        );
        assert!(!truncated);
        assert_eq!(
            sanitize_browser_link_destination("javascript:alert(1)"),
            (None, true)
        );
        assert_eq!(
            sanitize_browser_link_destination("mailto:person@example.test"),
            (None, true)
        );
        let prefix = "https://example.test/";
        let exact = format!("{prefix}{}", "a".repeat(180 - prefix.len()));
        let (destination, truncated) = sanitize_browser_link_destination(&exact);
        assert_eq!(destination.as_deref().map(str::len), Some(180));
        assert!(!truncated);
    }

    #[test]
    fn normalizes_hostile_semantic_payloads_with_budgets_and_boolean_states() {
        let extracted = BrowserSemanticMapExtraction {
            truncated: false,
            items: vec![
                BrowserSemanticItemExtraction {
                    role: "checkbox".to_string(),
                    level: None,
                    name: format!("Name {}", "x".repeat(500)),
                    destination: Some(
                        "https://user:secret@example.test/a?secret=1#fragment".to_string(),
                    ),
                    disabled: Some(false),
                    checked: Some(false),
                    selected: Some(true),
                    expanded: Some(false),
                    pressed: Some(true),
                    ordinal: Some(1),
                    tag: Some("input".to_string()),
                    input_type: Some("checkbox".to_string()),
                    content_editable: Some(false),
                    options: Vec::new(),
                },
                BrowserSemanticItemExtraction {
                    role: "<script>evil</script>".to_string(),
                    name: "attempted schema injection".to_string(),
                    ..Default::default()
                },
            ],
        };
        let (items, _, truncated) = normalize_browser_semantic_map(extracted, 500);
        assert!(truncated);
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.reference, "e1");
        assert_eq!(item.role, "checkbox");
        assert_eq!(item.disabled, Some(false));
        assert_eq!(item.checked, Some(false));
        assert_eq!(item.selected, None);
        assert_eq!(item.pressed, None);
        assert_eq!(item.destination, None);
        assert!(semantic_item_serialized_chars(&items) <= 500);
        let (_, targets, _) = normalize_browser_semantic_map(
            BrowserSemanticMapExtraction {
                items: vec![BrowserSemanticItemExtraction {
                    role: "button".to_string(),
                    name: "Save".to_string(),
                    ordinal: Some(4),
                    tag: Some("button".to_string()),
                    content_editable: Some(false),
                    ..Default::default()
                }],
                ..Default::default()
            },
            5_000,
        );
        assert_eq!(targets[0].reference, "e1");
        assert_eq!(targets[0].ordinal, 4);
        assert_eq!(targets[0].tag, "button");

        let oversized = BrowserSemanticMapExtraction {
            items: (0..MAX_BROWSER_SEMANTIC_ITEMS + 1)
                .map(|_| BrowserSemanticItemExtraction {
                    role: "button".to_string(),
                    name: "safe".to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let (_, _, truncated) = normalize_browser_semantic_map(oversized, 5_000);
        assert!(truncated);
    }

    #[test]
    fn retains_only_valid_heading_levels() {
        let extracted = BrowserSemanticMapExtraction {
            items: vec![
                BrowserSemanticItemExtraction {
                    role: "heading".to_string(),
                    level: Some(2),
                    name: "Section".to_string(),
                    ordinal: Some(1),
                    tag: Some("h2".to_string()),
                    content_editable: Some(false),
                    ..Default::default()
                },
                BrowserSemanticItemExtraction {
                    role: "heading".to_string(),
                    level: Some(9),
                    name: "Invalid".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let (items, _, truncated) = normalize_browser_semantic_map(extracted, 5_000);
        assert!(truncated);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].role, "heading");
        assert_eq!(items[0].level, Some(2));
    }

    fn target(role: &str, tag: &str, input_type: Option<&str>) -> BrowserSemanticTargetRecord {
        BrowserSemanticTargetRecord {
            reference: "e1".to_string(),
            ordinal: 2,
            role: role.to_string(),
            name: "Accessible name".to_string(),
            destination: (role == "link").then(|| "https://example.test".to_string()),
            disabled: Some(false),
            checked: None,
            selected: None,
            expanded: None,
            pressed: None,
            tag: tag.to_string(),
            input_type: input_type.map(str::to_string),
            content_editable: false,
        }
    }

    #[test]
    fn click_and_fill_targets_are_narrowly_compatible() {
        for role in [
            "button", "checkbox", "switch", "select", "option", "tab", "menuitem",
        ] {
            assert!(click_target_is_compatible(&target(role, "div", None)));
        }
        assert!(fill_target_is_compatible(&target(
            "spinbutton",
            "input",
            Some("number")
        )));
        assert!(!click_target_is_compatible(&target(
            "button",
            "input",
            Some("file")
        )));
        assert!(!click_target_is_compatible(&target(
            "button",
            "input",
            Some("password")
        )));
        assert!(click_target_is_compatible(&target(
            "button", "button", None
        )));
        assert!(click_target_is_compatible(&target("link", "a", None)));
        assert!(click_target_is_compatible(&target(
            "checkbox",
            "input",
            Some("checkbox")
        )));
        assert!(click_target_is_compatible(&target(
            "radio",
            "input",
            Some("radio")
        )));
        assert!(click_target_is_compatible(&target(
            "switch", "button", None
        )));
        assert!(!click_target_is_compatible(&target("heading", "h2", None)));
        assert!(!click_target_is_compatible(&target(
            "slider",
            "input",
            Some("range")
        )));
        assert!(!click_target_is_compatible(&target(
            "select", "select", None
        )));
        let mut disabled_button = target("button", "button", None);
        disabled_button.disabled = Some(true);
        assert!(!click_target_is_compatible(&disabled_button));

        assert!(fill_target_is_compatible(&target(
            "textbox",
            "input",
            Some("text")
        )));
        assert!(fill_target_is_compatible(&target(
            "textbox", "textarea", None
        )));
        assert!(!fill_target_is_compatible(&target(
            "textbox",
            "input",
            Some("password")
        )));
        assert!(!fill_target_is_compatible(&target(
            "textbox",
            "input",
            Some("file")
        )));
        assert!(!fill_target_is_compatible(&target("textbox", "div", None)));
        assert!(!fill_target_is_compatible(&target(
            "textbox",
            "input",
            Some("hidden")
        )));
        let mut editable = target("textbox", "div", None);
        editable.content_editable = true;
        assert!(fill_target_is_compatible(&editable));
        editable.disabled = Some(true);
        assert!(!fill_target_is_compatible(&editable));
    }

    #[test]
    fn references_and_fill_text_are_bounded_before_map_consumption() {
        assert!(validate_browser_reference("e1").is_ok());
        assert!(validate_browser_reference("e80").is_ok());
        assert!(validate_browser_reference("e0").is_err());
        assert!(validate_browser_reference("e01").is_err());
        assert!(validate_browser_reference("button").is_err());
        assert!(validate_browser_reference("e81").is_err());
        assert!(normalize_browser_fill_text("safe text").is_ok());
        assert!(normalize_browser_fill_text(&"x".repeat(2_001)).is_err());
        assert_eq!(
            normalize_browser_fill_text("line\r\nfeed"),
            Ok("line\nfeed".to_string())
        );
        assert!(normalize_browser_fill_text("bad\u{0000}value").is_err());
        assert!(normalize_browser_fill_text("bad\u{000b}value").is_err());
    }

    #[test]
    fn action_script_uses_escaped_data_and_callback_only_confirms_status() {
        let mut item = target("textbox", "input", Some("text"));
        item.name = "quote \" and </script>".to_string();
        let script = browser_semantic_action_script(
            &item,
            Some("safe \" text"),
            "https://example.test/page",
            1.0,
        )
        .unwrap();
        assert!(script.contains("expected"));
        assert!(script.contains("window.location.href !== expectedUrl"));
        assert!(script.contains("const result = ("));
        assert!(script.contains("ok, reason"));
        assert!(!script.contains("const result = JSON.stringify"));
        assert!(script.contains("dispatchEvent"));
        assert!(script.contains("Object.getOwnPropertyDescriptor"));
        assert!(script.contains("checked:"));
        assert!(script.contains("selected:"));
        assert!(script.contains("expanded:"));
        assert!(script.contains("pressed:"));
        assert!(script.contains("return clean(url.toString(), maxDestinationChars);"));
        assert!(!script.contains("value.length >= maxDestinationChars"));
        assert!(!script.contains("querySelector"));
        assert!(!script.contains("innerHTML"));
        assert!(!script.contains("document.cookie"));
        assert!(!script.contains("eval("));
        assert!(parse_browser_action_callback(r#"{"ok":true,"reason":"ok"}"#).is_ok());
        assert_eq!(
            parse_browser_action_callback(r#"{"ok":false,"reason":"stale"}"#),
            Err("browser action anchor is stale".to_string())
        );
        assert!(parse_browser_action_callback("not-json").is_err());

        let url_check = browser_action_url_check_script(
            "https://example.test/path?value=\"quoted\"&next=</script>",
        )
        .unwrap();
        assert!(url_check.contains("window.location.href ==="));
        assert!(url_check.contains(r#"\"quoted\""#));
        assert!(url_check.contains("{ ok: false, reason: \"stale\" }"));
        assert!(!url_check.contains("document.cookie"));
    }

    #[test]
    fn semantic_map_records_bind_scope_lifecycle_url_and_page_revision() {
        let record = BrowserSemanticMapRecord {
            scope: "workspace\u{1f}|session".to_string(),
            map_id: "m-8-2".to_string(),
            generation: 2,
            lifecycle_token: 8,
            url: "https://example.test/page".to_string(),
            page_load_revision: 4,
            document_identity: 1.0,
            targets: Vec::new(),
        };
        assert!(semantic_map_record_is_current(
            Some(&record),
            "workspace\u{1f}|session",
            8,
            "https://example.test/page",
            4,
            "m-8-2",
            2,
        ));
        assert!(!semantic_map_record_is_current(
            Some(&record),
            "workspace\u{1f}|session",
            8,
            "https://example.test/page",
            5,
            "m-8-2",
            2,
        ));
        assert!(!semantic_map_record_is_current(
            Some(&record),
            "workspace\u{1f}|other-session",
            8,
            "https://example.test/page",
            4,
            "m-8-2",
            2,
        ));
    }

    #[test]
    fn rejects_a_same_url_response_after_page_load_revision_changes() {
        let url = "https://example.test/same-url";
        assert!(page_load_revision_is_current(4, 4));
        // Reloading or navigating back to an identical URL still invalidates
        // the old eval callback because PageLoad Started advanced 4 -> 5.
        assert!(!page_load_revision_is_current(5, 4));
        let record = BrowserSemanticMapRecord {
            scope: "workspace\u{1f}|session".to_string(),
            map_id: "m-3-1".to_string(),
            generation: 1,
            lifecycle_token: 3,
            url: url.to_string(),
            page_load_revision: 4,
            document_identity: 1.0,
            targets: Vec::new(),
        };
        assert!(!semantic_map_record_is_current(
            Some(&record),
            "workspace\u{1f}|session",
            3,
            url,
            5,
            "m-3-1",
            1,
        ));
    }

    #[test]
    fn loading_invalidates_semantic_maps_and_advances_page_revision() {
        let mut context = BrowserContext {
            semantic_map: Some(BrowserSemanticMapRecord {
                scope: "workspace\u{1f}|".to_string(),
                map_id: "m-1-1".to_string(),
                generation: 1,
                lifecycle_token: 1,
                url: "https://example.test/old".to_string(),
                page_load_revision: 1,
                document_identity: 1.0,
                targets: Vec::new(),
            }),
            page_load_revision: 1,
            ..BrowserContext::default()
        };
        mark_context_loading(&mut context, "https://example.test/new".to_string());
        assert_eq!(context.page_load_revision, 2);
        assert!(context.semantic_map.is_none());
    }

    #[test]
    fn rejects_occlusion_restores_from_closed_or_previous_lifecycles() {
        assert!(occlusion_request_is_current(true, 7, 7));
        assert!(!occlusion_request_is_current(false, 7, 7));
        assert!(!occlusion_request_is_current(true, 8, 7));
    }

    #[test]
    fn advances_token_for_each_open_even_when_scope_is_reused() {
        let mut token = 1;
        advance_lifecycle_token(&mut token);
        assert_eq!(token, 2);
        advance_lifecycle_token(&mut token);
        assert_eq!(token, 3);
    }

    #[test]
    fn evidence_scripts_are_fixed_bounded_and_document_bound() {
        let start =
            browser_evidence_start_script("t-test-secret", "https://example.test/page", 42.5, 900)
                .unwrap();
        assert!(start.find("let stop").unwrap() < start.find("try").unwrap());
        assert!(start.contains("stop = () =>"));
        assert!(start.contains("try { stop(); } catch (_) {}"));
        assert!(start.contains("window.setTimeout(stop, 900)"));
        assert!(start.contains("maxEvents"));
        assert!(start.contains("events.shift()"));
        assert!(start.contains("window.location.origin !== expectedOrigin"));
        assert!(!start.contains("https://example.test/page"));
        assert!(start.contains("performance.timeOrigin !== expectedIdentity"));
        assert!(start.contains("window.addEventListener(\"error\", onError, true)"));
        assert!(start.contains("window.removeEventListener(\"error\", onError, true)"));
        assert!(start.contains("window.addEventListener(key, drainListener, false)"));
        assert!(start.contains("window.removeEventListener(key, drainListener, false)"));
        assert!(start.contains("new PerformanceObserver(onResources)"));
        assert!(start.contains("resourceObserver.observe({ type: \"resource\" })"));
        assert!(start.contains("resourceObserver.disconnect()"));
        assert!(start.contains("const maxResourceEntries = 33"));
        assert!(start.contains("if (processed >= maxResourceEntries) { truncated = true; break; }"));
        assert!(
            start.find("resourceObserver.disconnect()").unwrap()
                < start.find("clearTimeout(timer)").unwrap()
        );
        assert!(start.contains("try { if (installedWarn"));
        assert!(start.contains("try { if (installedErrorListener)"));
        // `entries.getEntries()` is the observer-delivered batch, not a query
        // of the global Performance timeline.
        assert!(!start.contains("performance.getEntries"));
        assert!(!start.contains("buffered"));
        assert!(!start.contains("setInterval"));
        assert!(!start.contains("JSON.stringify"));
        assert!(!start.contains("document.cookie"));
        assert!(!start.contains("localStorage"));
        assert!(!start.contains("sessionStorage"));
        assert!(!start.contains("fetch("));
        assert!(!start.contains("XMLHttpRequest"));
        assert!(!start.contains("WebSocket"));
        assert!(!start.contains("querySelector"));
        assert!(!start.contains(".value"));

        let read = browser_evidence_read_script("t-test-secret", "https://example.test/page", 42.5)
            .unwrap();
        assert!(read.contains("state.drain()"));
        assert!(read.contains("window.dispatchEvent(new CustomEvent(key"));
        assert!(read.contains("reason: \"collector_missing\""));
        assert!(read.contains("window.location.origin !=="));
        assert!(!read.contains("https://example.test/page"));
        assert!(read.contains("performance.timeOrigin !=="));
        assert!(!read.contains("JSON.stringify"));

        let cleanup =
            browser_evidence_cleanup_script("t-test-secret", "https://example.test/page", 42.5)
                .unwrap();
        assert!(cleanup.contains("window.dispatchEvent(new CustomEvent(key"));
        assert!(cleanup.contains("reason: \"collector_missing\""));
        assert!(cleanup.contains("window.location.origin !=="));
        assert!(!cleanup.contains("https://example.test/page"));
    }

    #[test]
    fn evidence_url_sanitization_preserves_port_but_drops_secrets() {
        assert_eq!(
            sanitize_browser_evidence_url(Some("https://localhost:3000/path?q=private#fragment",)),
            Some("https://localhost:3000/path".to_string())
        );
        assert_eq!(
            sanitize_browser_evidence_url(Some("https://user:pass@example.test/a")),
            Some("https://example.test/a".to_string())
        );
        assert_eq!(
            sanitize_browser_evidence_url(Some("https://example.test/token/reset")),
            Some("[redacted sensitive browser evidence URL]".to_string())
        );
        assert!(sanitize_browser_evidence_url(Some("javascript:alert(1)")).is_none());
    }

    #[test]
    fn evidence_events_are_allowlisted_redacted_and_budgeted() {
        let raw = (0..MAX_BROWSER_EVIDENCE_EVENTS + 2)
            .map(|sequence| BrowserEvidenceEventRaw {
                kind: if sequence == 0 {
                    "unknown".to_string()
                } else {
                    "consoleWarn".to_string()
                },
                sequence: sequence as u64,
                message: if sequence == 1 {
                    "authorization token leaked".to_string()
                } else {
                    "safe message".to_string()
                },
                url: Some("https://localhost:3000/app?q=hidden".to_string()),
                line: Some(1_000_001),
                column: Some(12),
                initiator_type: None,
                duration_ms: None,
                status: None,
            })
            .collect();
        let (events, truncated) = normalize_browser_evidence_events(raw, false);
        assert!(truncated);
        assert!(events.len() <= MAX_BROWSER_EVIDENCE_EVENTS);
        assert_eq!(events[0].message, "[redacted sensitive browser evidence]");
        assert_eq!(events[0].url.as_deref(), Some("https://localhost:3000/app"));
        assert!(events[0].line.is_none());
        assert_eq!(events[0].column, Some(12));
        assert!(
            serde_json::to_string(&events).unwrap().chars().count() <= MAX_BROWSER_EVIDENCE_CHARS
        );
    }

    #[test]
    fn resource_timing_evidence_is_allowlisted_clamped_and_content_free() {
        let raw = vec![
            BrowserEvidenceEventRaw {
                kind: "resource".to_string(),
                sequence: 1,
                message: "authorization: never use this".to_string(),
                url: Some(
                    "https://user:password@example.test:8443/assets/app.js?token=private#hash"
                        .to_string(),
                ),
                line: Some(7),
                column: Some(8),
                initiator_type: Some("fetch".to_string()),
                duration_ms: Some(u64::from(MAX_BROWSER_EVIDENCE_RESOURCE_DURATION_MS) + 1),
                status: Some(503),
            },
            BrowserEvidenceEventRaw {
                kind: "resource".to_string(),
                sequence: 2,
                message: "ignored".to_string(),
                url: Some("https://example.test/image.png".to_string()),
                line: None,
                column: None,
                initiator_type: Some("worker".to_string()),
                duration_ms: Some(12),
                status: Some(99),
            },
            BrowserEvidenceEventRaw {
                kind: "resource".to_string(),
                sequence: 3,
                message: "ignored".to_string(),
                url: Some("javascript:alert(1)".to_string()),
                line: None,
                column: None,
                initiator_type: Some("script".to_string()),
                duration_ms: Some(1),
                status: Some(200),
            },
        ];
        let (events, truncated) = normalize_browser_evidence_events(raw, false);
        assert!(truncated);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "resource");
        assert_eq!(events[0].message, "resource timing observed");
        assert_eq!(
            events[0].url.as_deref(),
            Some("https://example.test:8443/assets/app.js")
        );
        assert_eq!(events[0].initiator_type.as_deref(), Some("fetch"));
        assert_eq!(
            events[0].duration_ms,
            Some(MAX_BROWSER_EVIDENCE_RESOURCE_DURATION_MS)
        );
        assert_eq!(events[0].status, Some(503));
        assert!(events[0].line.is_none());
        assert!(events[0].column.is_none());
        assert!(events[1].initiator_type.is_none());
        assert_eq!(events[1].duration_ms, Some(12));
        assert!(events[1].status.is_none());
        assert_eq!(
            normalize_browser_evidence_resource_initiator_type(Some("script")).as_deref(),
            Some("script")
        );
        assert!(normalize_browser_evidence_resource_initiator_type(Some("beacon")).is_none());
        assert_eq!(
            normalize_browser_evidence_resource_status(Some(100)),
            Some(100)
        );
        assert_eq!(
            normalize_browser_evidence_resource_status(Some(599)),
            Some(599)
        );
        assert!(normalize_browser_evidence_resource_status(Some(600)).is_none());
        assert!(
            serde_json::to_string(&events).unwrap().chars().count() <= MAX_BROWSER_EVIDENCE_CHARS
        );
    }

    #[test]
    fn evidence_callback_is_bounded_and_requires_ok() {
        let callback =
            parse_browser_evidence_callback(r#"{"ok":true,"events":[],"truncated":false}"#)
                .unwrap();
        assert!(callback.ok);
        assert!(
            parse_browser_evidence_callback(r#"{"ok":false,"events":[],"truncated":true}"#,)
                .is_err()
        );
        assert_eq!(
            parse_browser_evidence_callback(
                r#"{"ok":false,"reason":"url_changed","events":[],"truncated":true}"#,
            )
            .unwrap_err(),
            "browser page changed during evidence capture"
        );
        assert_eq!(
            parse_browser_evidence_callback(
                r#"{"ok":false,"reason":"origin_changed","events":[],"truncated":true}"#,
            )
            .unwrap_err(),
            "browser page changed during evidence capture"
        );
        assert_eq!(
            parse_browser_evidence_callback(
                r#"{"ok":false,"reason":"document_changed","events":[],"truncated":true}"#,
            )
            .unwrap_err(),
            "browser page reloaded during evidence capture"
        );
        assert_eq!(
            parse_browser_evidence_callback(
                r#"{"ok":false,"reason":"collector_missing","events":[],"truncated":true}"#,
            )
            .unwrap_err(),
            "browser evidence collector ended before collection"
        );
        assert!(parse_browser_evidence_callback(&format!(
            "{{\"ok\":true,\"events\":[],\"padding\":\"{}\"}}",
            "x".repeat(super::MAX_BROWSER_EVIDENCE_CALLBACK_CHARS)
        ))
        .is_err());
    }

    #[test]
    fn navigation_and_page_load_scope_invalidation_removes_evidence() {
        let state = BrowserState::default();
        state.evidence_captures.lock().unwrap().insert(
            "c-test".to_string(),
            BrowserEvidenceCaptureRecord {
                scope: "workspace\u{1f}|session".to_string(),
                capture_token: "t-test".to_string(),
                lifecycle_token: 8,
                url: "https://example.test/page".to_string(),
                page_load_revision: 4,
                document_identity: 42.5,
                expires_at: Instant::now() + StdDuration::from_secs(60),
            },
        );
        super::invalidate_browser_evidence_for_scope(&state, "other", Some("session"));
        assert!(state
            .evidence_captures
            .lock()
            .unwrap()
            .contains_key("c-test"));
        super::invalidate_browser_evidence_for_scope(&state, "workspace", Some("session"));
        assert!(state.evidence_captures.lock().unwrap().is_empty());

        state.evidence_captures.lock().unwrap().insert(
            "c-other".to_string(),
            BrowserEvidenceCaptureRecord {
                scope: "other\u{1f}|session".to_string(),
                capture_token: "t-other".to_string(),
                lifecycle_token: 9,
                url: "https://example.test/other".to_string(),
                page_load_revision: 5,
                document_identity: 43.5,
                expires_at: Instant::now() + StdDuration::from_secs(60),
            },
        );
        super::clear_browser_evidence_captures(&state);
        // The registry is drained before cleanup is scheduled, so no command
        // can read the capture while its page wrapper is being drained.
        assert!(state.evidence_captures.lock().unwrap().is_empty());
    }

    #[test]
    fn evidence_expiry_is_token_matched_and_one_shot() {
        let state = BrowserState::default();
        state.evidence_captures.lock().unwrap().insert(
            "c-test".to_string(),
            BrowserEvidenceCaptureRecord {
                scope: "workspace\u{1f}|session".to_string(),
                capture_token: "t-test".to_string(),
                lifecycle_token: 8,
                url: "https://example.test/page".to_string(),
                page_load_revision: 4,
                document_identity: 42.5,
                expires_at: Instant::now() + StdDuration::from_secs(60),
            },
        );
        super::expire_browser_evidence_capture(&state, "c-test", "wrong-token");
        assert!(state
            .evidence_captures
            .lock()
            .unwrap()
            .contains_key("c-test"));
        super::expire_browser_evidence_capture(&state, "c-test", "t-test");
        assert!(state.evidence_captures.lock().unwrap().is_empty());
        super::expire_browser_evidence_capture(&state, "c-test", "t-test");
    }

    #[test]
    fn evidence_capture_take_requires_the_current_private_token() {
        let mut captures = std::collections::HashMap::new();
        captures.insert(
            "c-test".to_string(),
            BrowserEvidenceCaptureRecord {
                scope: "workspace\u{1f}|session".to_string(),
                capture_token: "t-current".to_string(),
                lifecycle_token: 8,
                url: "https://example.test/page".to_string(),
                page_load_revision: 4,
                document_identity: 42.5,
                expires_at: Instant::now() + StdDuration::from_secs(60),
            },
        );
        assert!(
            super::take_browser_evidence_capture_with_token(&mut captures, "c-test", "t-old",)
                .is_none()
        );
        assert!(captures.contains_key("c-test"));
        let taken =
            super::take_browser_evidence_capture_with_token(&mut captures, "c-test", "t-current");
        assert_eq!(
            taken.as_ref().map(|record| record.capture_token.as_str()),
            Some("t-current")
        );
        assert!(captures.is_empty());
    }

    #[test]
    fn capture_handle_and_result_never_serialize_page_token_or_capture_id() {
        let handle = super::BrowserEvidenceCaptureHandle {
            capture_id: "c-public".to_string(),
            remaining_ms: 1_000,
        };
        let handle_json = serde_json::to_string(&handle).unwrap();
        assert!(handle_json.contains("c-public"));
        assert!(!handle_json.contains("t-private"));

        let result = super::BrowserEvidenceResult {
            events: Vec::new(),
            truncated: false,
            untrusted: true,
        };
        let result_json = serde_json::to_string(&result).unwrap();
        assert!(!result_json.contains("captureId"));
        assert!(!result_json.contains("t-private"));
        assert!(result_json.contains("untrusted"));
    }

    #[test]
    fn evidence_capture_ids_are_small_opaque_backend_handles() {
        assert!(validate_browser_evidence_capture_id("c-0123456789abcdef0123456789abcdef").is_ok());
        assert!(validate_browser_evidence_capture_id("").is_err());
        assert!(validate_browser_evidence_capture_id("c/secret").is_err());
        assert!(
            validate_browser_evidence_capture_id("c-0123456789abcdef0123456789abcdeF").is_err()
        );
        assert!(validate_browser_evidence_capture_id("c-0123456789abcdef").is_err());
    }

    #[test]
    fn evidence_cleanup_script_drains_without_returning_remote_events() {
        let script =
            browser_evidence_cleanup_script("t-private-token", "https://example.test/page", 42.5)
                .unwrap();
        assert!(script.contains("typeof state.drain === \"function\" ? state.drain() : null"));
        assert!(script.contains("window.dispatchEvent(new CustomEvent(key"));
        assert!(!script.contains("events"));
        assert!(parse_browser_evidence_cleanup_callback(r#"{"ok":true}"#).is_ok());
        assert!(parse_browser_evidence_cleanup_callback(
            r#"{"ok":true,"events":[{"message":"must not return"}]}"#
        )
        .is_err());
    }

    #[test]
    fn discard_evidence_capture_removes_backend_record_before_best_effort_cleanup() {
        let state = BrowserState::default();
        state.evidence_captures.lock().unwrap().insert(
            "c-0123456789abcdef0123456789abcdef".to_string(),
            BrowserEvidenceCaptureRecord {
                scope: "workspace\u{1f}|session".to_string(),
                capture_token: "t-private-token".to_string(),
                lifecycle_token: 8,
                url: "https://example.test/page".to_string(),
                page_load_revision: 4,
                document_identity: 42.5,
                expires_at: Instant::now() + StdDuration::from_secs(60),
            },
        );
        super::discard_browser_evidence_capture(
            &state,
            "workspace",
            Some("session"),
            "c-0123456789abcdef0123456789abcdef",
        );
        assert!(state.evidence_captures.lock().unwrap().is_empty());
    }

    #[test]
    fn browser_audit_is_closed_bounded_ordered_and_scope_filtered() {
        let state = BrowserState::default();
        for index in 0..(MAX_BROWSER_AUDIT_ENTRIES + 4) {
            append_browser_audit(
                &state,
                BrowserAuditOrigin::Ui,
                None,
                None,
                "workspace",
                Some("session"),
                BrowserAuditTool::Context,
                BrowserAuditGrantState::Armed,
                if index + 1 == MAX_BROWSER_AUDIT_ENTRIES + 4 {
                    BrowserAuditOutcome::Failed
                } else {
                    BrowserAuditOutcome::Executed
                },
            );
        }
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Ui,
            None,
            None,
            "other-workspace",
            Some("session"),
            BrowserAuditTool::Navigate,
            BrowserAuditGrantState::Missing,
            BrowserAuditOutcome::Rejected,
        );

        assert_eq!(
            state.browser_audit.lock().unwrap().len(),
            MAX_BROWSER_AUDIT_ENTRIES
        );
        let newest = read_browser_audit(&state, "workspace", Some("session"), 1).unwrap();
        assert_eq!(newest.len(), 1);
        assert_eq!(newest[0].outcome, BrowserAuditOutcome::Failed);
        assert_eq!(
            read_browser_audit(&state, "other-workspace", Some("session"), 1)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            read_browser_audit(&state, "workspace", Some("session"), 100)
                .unwrap()
                .len(),
            100
        );
        assert!(read_browser_audit(&state, "workspace", Some("session"), 0).is_err());
        assert!(read_browser_audit(&state, "workspace", Some("session"), 101).is_err());
    }

    #[test]
    fn browser_audit_viewer_guard_rejects_stale_scope_and_closed_lifecycle() {
        let state = BrowserState::default();

        assert!(
            require_current_lifecycle(&state, "workspace", Some("session"), 7, "stale",).is_err()
        );

        *state.active_scope.lock().unwrap() = Some("workspace\u{1f}|session".to_string());
        *state.lifecycle_token.lock().unwrap() = 7;
        *state.lifecycle_open.lock().unwrap() = true;

        assert!(
            require_current_lifecycle(&state, "other-workspace", Some("session"), 7, "stale",)
                .is_err()
        );
        assert!(
            require_current_lifecycle(&state, "workspace", Some("session"), 6, "stale",).is_err()
        );
        assert!(
            require_current_lifecycle(&state, "workspace", Some("session"), 7, "stale",).is_ok()
        );

        *state.lifecycle_open.lock().unwrap() = false;
        assert!(
            require_current_lifecycle(&state, "workspace", Some("session"), 7, "stale",).is_err()
        );
    }

    #[test]
    fn browser_audit_outcome_classifies_disabled_target_as_rejected() {
        assert_eq!(
            browser_audit_outcome(Some("browser action target is disabled")),
            BrowserAuditOutcome::Rejected
        );
    }

    #[test]
    fn browser_audit_rejects_unsafe_provenance_and_bounds_provider_identity() {
        let state = BrowserState::default();
        let long = "x".repeat(MAX_BROWSER_AUDIT_SCOPE_CHARS + 1);
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Ui,
            Some("provider"),
            None,
            "workspace",
            Some("session"),
            BrowserAuditTool::Fill,
            BrowserAuditGrantState::Armed,
            BrowserAuditOutcome::Executed,
        );
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Mcp,
            None,
            Some("0123456789abcdef"),
            "workspace",
            Some("session"),
            BrowserAuditTool::EvidenceRead,
            BrowserAuditGrantState::Armed,
            BrowserAuditOutcome::Executed,
        );
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Mcp,
            Some("provider"),
            Some("not-a-fingerprint"),
            "workspace",
            Some("session"),
            BrowserAuditTool::EvidenceRead,
            BrowserAuditGrantState::Armed,
            BrowserAuditOutcome::Executed,
        );
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Ui,
            None,
            None,
            &long,
            Some("session"),
            BrowserAuditTool::Context,
            BrowserAuditGrantState::Missing,
            BrowserAuditOutcome::Rejected,
        );
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Mcp,
            Some(&"p".repeat(MAX_BROWSER_AUDIT_PROVIDER_CHARS + 1)),
            Some("0123456789abcdef"),
            "workspace",
            Some("session"),
            BrowserAuditTool::Navigate,
            BrowserAuditGrantState::Armed,
            BrowserAuditOutcome::Executed,
        );
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Mcp,
            Some("provider"),
            Some(&"a".repeat(MAX_BROWSER_AUDIT_LEASE_FINGERPRINT_CHARS + 1)),
            "workspace",
            Some("session"),
            BrowserAuditTool::Navigate,
            BrowserAuditGrantState::Armed,
            BrowserAuditOutcome::Executed,
        );
        let audit = state.browser_audit.lock().unwrap();
        assert_eq!(audit.len(), 1);
        assert_eq!(
            audit
                .front()
                .and_then(|record| record.provider_id.as_ref())
                .unwrap(),
            &"p".repeat(MAX_BROWSER_AUDIT_PROVIDER_CHARS)
        );
    }

    #[test]
    fn browser_audit_serialization_has_only_bounded_safe_fields() {
        let record = BrowserAuditRecord {
            origin: BrowserAuditOrigin::Mcp,
            provider_id: Some("provider".to_string()),
            lease_fingerprint: Some("0123456789abcdef".to_string()),
            workspace_id: "workspace".to_string(),
            session_id: Some("session".to_string()),
            tool: BrowserAuditTool::Fill,
            grant_state: BrowserAuditGrantState::NotApplicable,
            outcome: BrowserAuditOutcome::NotArmed,
            timestamp_ms: 123,
        };
        let serialized = serde_json::to_string(&record).unwrap();
        for forbidden in ["url", "ref", "text", "message", "events", "token", "error"] {
            assert!(
                !serialized.contains(forbidden),
                "unexpected field: {forbidden}"
            );
        }
        assert!(serialized.contains("leaseFingerprint"));
        assert!(serialized.contains("notArmed"));
        assert!(serialized.contains("dcc_browser_fill"));
        assert!(serialized.contains("mcp"));
    }

    #[test]
    fn browser_audit_view_serialization_excludes_scope_and_internal_identity() {
        let record = BrowserAuditRecord {
            origin: BrowserAuditOrigin::Mcp,
            provider_id: Some("provider".to_string()),
            lease_fingerprint: Some("0123456789abcdef".to_string()),
            workspace_id: "workspace".to_string(),
            session_id: Some("session".to_string()),
            tool: BrowserAuditTool::EvidenceRead,
            grant_state: BrowserAuditGrantState::Armed,
            outcome: BrowserAuditOutcome::Executed,
            timestamp_ms: 123,
        };
        let serialized = serde_json::to_value(BrowserAuditViewRecord::from(record)).unwrap();
        let object = serialized.as_object().unwrap();

        assert!(object.contains_key("origin"));
        assert!(object.contains_key("providerId"));
        assert!(object.contains_key("tool"));
        assert!(object.contains_key("grantState"));
        assert!(object.contains_key("outcome"));
        assert!(object.contains_key("timestampMs"));
        for forbidden in [
            "workspaceId",
            "sessionId",
            "leaseFingerprint",
            "url",
            "ref",
            "text",
            "message",
            "error",
            "token",
            "captureId",
            "event",
        ] {
            assert!(
                !object.contains_key(forbidden),
                "unexpected field: {forbidden}"
            );
        }
    }

    #[test]
    fn browser_audit_append_is_best_effort_when_its_lock_is_poisoned() {
        let state = BrowserState::default();
        let audit = state.browser_audit.clone();
        let poisoned = std::thread::spawn(move || {
            let _guard = audit.lock().unwrap();
            panic!("poison audit lock for test");
        })
        .join();
        assert!(poisoned.is_err());
        append_browser_audit(
            &state,
            BrowserAuditOrigin::Ui,
            None,
            None,
            "workspace",
            None,
            BrowserAuditTool::DisarmControl,
            BrowserAuditGrantState::NotApplicable,
            BrowserAuditOutcome::Failed,
        );
        assert!(read_browser_audit(&state, "workspace", None, 1).is_err());
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "browser_webkit_tests.rs"]
mod browser_webkit_tests;
