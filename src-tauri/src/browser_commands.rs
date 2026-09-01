//! Native, human-facing browser surface.
//!
//! This intentionally owns the child WebView in Rust. The renderer receives no
//! permission to create WebViews and remote pages never get access to DCC IPC.
//! The MVP keeps context in memory; durable browser metadata and automation are
//! separate follow-up features.

use std::collections::{HashMap, VecDeque};
use std::net::Ipv6Addr;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, Manager, State, Webview, WebviewUrl, Wry};
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tokio::time::{timeout, Duration};
use url::{Host, Url};

use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;

const BROWSER_LABEL: &str = "dcc-browser";
const DEFAULT_URL: &str = "https://www.google.com/";
const BROWSER_LOCATION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
const MAX_BROWSER_LOCATION_URL_CHARS: usize = 2_048;
const MAX_BROWSER_LOCATION_CACHE_ENTRIES: usize = 256;
const MAX_BROWSER_DIMENSION: f64 = 16_384.0;
const MAX_BROWSER_CONTEXT_CHARS: usize = 6_000;
const MAX_BROWSER_TITLE_CHARS: usize = 300;
/// Bounds for the deliberately small, human-requested semantic page map.
/// These constants are interpolated into the fixed page-side extraction script
/// and enforced again at the Rust trust boundary.
const MAX_BROWSER_SEMANTIC_CANDIDATES: usize = 300;
const MAX_BROWSER_SEMANTIC_SCAN_NODES: usize = 1_500;
const MAX_BROWSER_SEMANTIC_ANCESTORS: usize = 64;
const MAX_BROWSER_CONTEXT_TEXT_NODES: usize = 600;
const MAX_BROWSER_SEMANTIC_NAME_TEXT_NODES: usize = 24;
const MAX_BROWSER_SEMANTIC_ITEMS: usize = 80;
const MAX_BROWSER_SEMANTIC_NAME_CHARS: usize = 120;
const MAX_BROWSER_SEMANTIC_DESTINATION_CHARS: usize = 180;
const MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS: usize = 5_000;
const MAX_BROWSER_CONTEXT_ENVELOPE_CHARS: usize = 14_000;
const MAX_BROWSER_CONTEXT_URL_CHARS: usize = 2_048;
const BROWSER_CONTEXT_TIMEOUT: Duration = Duration::from_secs(2);
/// A deliberate, short-lived consent window. It is in-memory only and must be
/// armed again after expiry, a new Browser lifecycle, or any close; a future UI
/// may renew it only through another explicit user gesture.
const BROWSER_CONTROL_GRANT_TTL: Duration = Duration::from_secs(60);
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
    pub url: Option<String>,
    pub title: Option<String>,
}

/// A deliberately small, provider-neutral page excerpt requested by a human
/// from the browser toolbar. This is not an automation or general DOM API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentContext {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub selection_only: bool,
    pub truncated: bool,
    pub semantic_map: BrowserSemanticMap,
}

/// A page-local, opaque reference map intended to make a later, separately
/// permissioned interaction contract possible. The references are not CSS,
/// XPath, DOM ids, or locators; this command exposes no automation API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSemanticMap {
    pub map_id: String,
    pub generation: u64,
    pub page_load_revision: u64,
    pub items: Vec<BrowserSemanticItem>,
    pub truncated: bool,
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
}

enum PreparedBrowserControlAction {
    Navigate(Url),
    Reload,
    Scroll { delta_x: f64, delta_y: f64 },
    Click { reference: String },
    Fill { reference: String, text: String },
}

impl PreparedBrowserControlAction {
    fn name(&self) -> &'static str {
        match self {
            Self::Navigate(_) => "navigate",
            Self::Reload => "reload",
            Self::Scroll { .. } => "scroll",
            Self::Click { .. } => "click",
            Self::Fill { .. } => "fill",
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSemanticItem {
    /// Opaque only within `map_id`; it is deliberately not a page locator.
    pub reference: String,
    /// Fixed provider-neutral vocabulary such as heading, button or link.
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressed: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BrowserContextExtraction {
    text: String,
    selection_only: bool,
    truncated: bool,
    document_identity: Option<f64>,
    #[serde(default)]
    semantic_map: BrowserSemanticMapExtraction,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BrowserSemanticMapExtraction {
    #[serde(default)]
    items: Vec<BrowserSemanticItemExtraction>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BrowserSemanticItemExtraction {
    role: String,
    level: Option<u8>,
    name: String,
    destination: Option<String>,
    disabled: Option<bool>,
    checked: Option<bool>,
    selected: Option<bool>,
    expanded: Option<bool>,
    pressed: Option<bool>,
    /// Page-side ordinal and element shape are trust-boundary metadata. They
    /// are stripped from the public map and retained only in the backend
    /// record used for the next, separately-consented action.
    ordinal: Option<usize>,
    tag: Option<String>,
    input_type: Option<String>,
    content_editable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Validates and normalizes the logical bounds supplied by the renderer.
///
/// Coordinates are clamped to the window's non-negative coordinate space and
/// dimensions are capped so a malformed renderer payload cannot request an
/// unbounded native view. NaN and infinities are rejected explicitly because
/// `f64::clamp` cannot safely normalize them.
pub fn normalize_browser_bounds(bounds: BrowserBounds) -> Result<BrowserBounds, String> {
    if !bounds.x.is_finite()
        || !bounds.y.is_finite()
        || !bounds.width.is_finite()
        || !bounds.height.is_finite()
    {
        return Err("browser bounds must be finite".to_string());
    }
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Err("browser bounds must have positive dimensions".to_string());
    }
    Ok(BrowserBounds {
        x: bounds.x.clamp(0.0, MAX_BROWSER_DIMENSION),
        y: bounds.y.clamp(0.0, MAX_BROWSER_DIMENSION),
        width: bounds.width.clamp(1.0, MAX_BROWSER_DIMENSION),
        height: bounds.height.clamp(1.0, MAX_BROWSER_DIMENSION),
    })
}

#[derive(Debug, Clone, Default)]
struct BrowserContext {
    url: Option<String>,
    title: Option<String>,
    /// URL requested by the active scope. Native callbacks do not carry a
    /// scope id, so callbacks are accepted only while this URL still matches.
    expected_url: Option<String>,
    /// URL whose finished load was observed for this scope. Context extraction
    /// is unavailable until it matches `expected_url` and the native URL.
    ready_url: Option<String>,
    /// Changes on every navigation/load start, so a map can never survive a
    /// page transition merely because the final URL happens to be identical.
    page_load_revision: u64,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct BrowserSemanticTargetRecord {
    reference: String,
    ordinal: usize,
    role: String,
    name: String,
    destination: Option<String>,
    disabled: Option<bool>,
    checked: Option<bool>,
    selected: Option<bool>,
    expanded: Option<bool>,
    pressed: Option<bool>,
    tag: String,
    input_type: Option<String>,
    content_editable: bool,
}

#[derive(Debug, Clone)]
struct BrowserControlGrant {
    scope: String,
    lifecycle_token: u64,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserLocationCacheEntry {
    safe_url: String,
    expires_at_ms: i64,
}

/// A bounded, process-local write-dedup cache. It is never a restore source:
/// SQLite remains authoritative across process restarts.
#[derive(Default)]
struct BrowserLocationCache {
    entries: HashMap<String, BrowserLocationCacheEntry>,
    oldest_first: VecDeque<String>,
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
    pub(crate) webview: Arc<Mutex<Option<Webview<Wry>>>>,
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
    /// Last successful durable URL per Browser scope. This is deliberately
    /// separate from page context: it only suppresses identical SQLite writes
    /// and is never treated as page, consent, map, or evidence state.
    persisted_locations: Arc<Mutex<BrowserLocationCache>>,
    /// Serializes native child-WebView operations in IPC arrival order, also
    /// across async scope validation. Context extraction keeps it through its
    /// short, bounded native callback wait so a new command cannot change its
    /// scope mid-extraction.
    operation_lock: Arc<AsyncMutex<()>>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            webview: Arc::new(Mutex::new(None)),
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
            persisted_locations: Arc::new(Mutex::new(BrowserLocationCache::default())),
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
    }
}

/// Browser allowlist used both by commands and by the native navigation hook.
/// HTTPS is allowed for normal human browsing; HTTP is restricted to local
/// development servers. Credentials and unsupported schemes are rejected.
pub fn validate_browser_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("browser URL cannot be empty".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https:///") || lower.starts_with("http:///") {
        return Err("browser URL must include a host".to_string());
    }
    let url = Url::parse(trimmed).map_err(|_| "browser URL is invalid".to_string())?;
    if url.username() != "" || url.password().is_some() {
        return Err("browser URLs cannot contain credentials".to_string());
    }
    match url.scheme().to_ascii_lowercase().as_str() {
        "https" => {
            match url.host_str() {
                Some(host) if !host.is_empty() => {}
                _ => return Err("HTTPS browser URL must include a host".to_string()),
            }
            Ok(url)
        }
        "http" => {
            let host = url
                .host()
                .ok_or_else(|| "HTTP browser URL must include a host".to_string())?;
            let local = match host {
                Host::Domain(host) => matches!(
                    host.to_ascii_lowercase().as_str(),
                    "localhost" | "127.0.0.1"
                ),
                Host::Ipv4(host) => host.is_loopback(),
                Host::Ipv6(host) => host == Ipv6Addr::LOCALHOST,
            };
            if local {
                Ok(url)
            } else {
                Err("HTTP browser URLs are only allowed for localhost".to_string())
            }
        }
        _ => Err("browser URL scheme must be HTTPS or local HTTP".to_string()),
    }
}

/// Durable Browser locations retain navigation identity only. Query strings,
/// fragments and credentials can carry secrets, so they never cross the
/// runtime-to-SQLite boundary even when the live Browser currently has them.
fn sanitize_browser_location_url(raw: &str) -> Option<String> {
    if raw.is_empty()
        || raw.chars().count() > MAX_BROWSER_LOCATION_URL_CHARS
        || raw.chars().any(char::is_control)
    {
        return None;
    }
    let mut url = Url::parse(raw).ok()?;
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    let safe_url = url.to_string();
    if safe_url.chars().count() > MAX_BROWSER_LOCATION_URL_CHARS
        || safe_url.chars().any(char::is_control)
    {
        return None;
    }
    validate_browser_url(&safe_url).ok()?;
    Some(safe_url)
}

fn browser_location_now_ms() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

/// Exact open precedence. Runtime context preserves the existing same-process
/// behavior; durable state is considered only after an explicit restore opt-in.
fn select_browser_open_url(
    initial_url: Option<&str>,
    runtime_url: Option<String>,
    stored_url: Option<String>,
    restore_last_url: bool,
) -> Result<Url, String> {
    if let Some(initial_url) = initial_url {
        return validate_browser_url(initial_url);
    }
    if let Some(runtime_url) = runtime_url {
        return validate_browser_url(&runtime_url);
    }
    if restore_last_url {
        if let Some(stored_url) = stored_url.and_then(|url| sanitize_browser_location_url(&url)) {
            return validate_browser_url(&stored_url);
        }
    }
    validate_browser_url(DEFAULT_URL)
}

/// Best-effort durable persistence runs only after the page-load callback has
/// proved active scope, expected URL, native URL and payload URL agree. It
/// intentionally performs synchronous, short local SQLite I/O after all
/// Browser-state locks are dropped; failure never affects navigation.
fn persist_browser_location(
    sessions: &SessionCommandState,
    persisted_locations: &Arc<Mutex<BrowserLocationCache>>,
    workspace_id: &str,
    session_id: Option<&str>,
    raw_url: &str,
) {
    let Some(safe_url) = sanitize_browser_location_url(raw_url) else {
        return;
    };
    let Some(saved_at_ms) = browser_location_now_ms() else {
        return;
    };
    let Some(expires_at_ms) = saved_at_ms.checked_add(BROWSER_LOCATION_TTL_MS) else {
        return;
    };
    let key = scope_key(workspace_id, session_id);
    let already_current = persisted_locations.lock().is_ok_and(|mut cache| {
        browser_location_cache_purge_expired(&mut cache, saved_at_ms);
        browser_location_cache_matches_current(&cache, &key, &safe_url, saved_at_ms)
    });
    if already_current {
        return;
    }
    // Do not hold a Browser/cache mutex over SQLite I/O. A concurrent page
    // callback may write the same URL once, but the database's monotonic
    // saved_at upsert remains authoritative and the bounded cache converges.
    if sessions
        .save_browser_location(
            workspace_id,
            session_id,
            &safe_url,
            saved_at_ms,
            expires_at_ms,
        )
        .is_ok()
    {
        if let Ok(mut cache) = persisted_locations.lock() {
            browser_location_cache_purge_expired(&mut cache, saved_at_ms);
            browser_location_cache_insert(&mut cache, key, safe_url, expires_at_ms);
        }
    }
}

fn browser_location_cache_purge_expired(cache: &mut BrowserLocationCache, now_ms: i64) {
    cache
        .entries
        .retain(|_, entry| entry.expires_at_ms > now_ms);
    cache
        .oldest_first
        .retain(|scope| cache.entries.contains_key(scope));
}

fn browser_location_cache_matches_current(
    cache: &BrowserLocationCache,
    scope: &str,
    safe_url: &str,
    now_ms: i64,
) -> bool {
    cache
        .entries
        .get(scope)
        .is_some_and(|entry| entry.safe_url == safe_url && entry.expires_at_ms > now_ms)
}

fn browser_location_cache_remove(cache: &mut BrowserLocationCache, scope: &str) {
    cache.entries.remove(scope);
    cache.oldest_first.retain(|candidate| candidate != scope);
}

fn browser_location_cache_insert(
    cache: &mut BrowserLocationCache,
    scope: String,
    safe_url: String,
    expires_at_ms: i64,
) {
    browser_location_cache_remove(cache, &scope);
    while cache.entries.len() >= MAX_BROWSER_LOCATION_CACHE_ENTRIES {
        let Some(oldest_scope) = cache.oldest_first.pop_front() else {
            cache.entries.clear();
            break;
        };
        cache.entries.remove(&oldest_scope);
    }
    cache.entries.insert(
        scope.clone(),
        BrowserLocationCacheEntry {
            safe_url,
            expires_at_ms,
        },
    );
    cache.oldest_first.push_back(scope);
}

/// Removes control sequences and bounds text crossing the native WebView
/// boundary. The page-side script also limits output, but this is the final
/// trust boundary because a remote page controls the DOM and JS globals.
pub fn normalize_browser_context_text(raw: &str, max_chars: usize) -> (String, bool) {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut text = String::new();
    let mut chars = 0usize;
    let mut truncated = false;
    for character in normalized.chars() {
        if character.is_control() && character != '\n' && character != '\t' {
            continue;
        }
        if chars == max_chars {
            truncated = true;
            break;
        }
        text.push(character);
        chars += 1;
    }
    (text.trim().to_string(), truncated)
}

fn normalize_browser_semantic_text(raw: &str, max_chars: usize) -> (String, bool) {
    let mut text = String::new();
    let mut chars = 0usize;
    let mut truncated = false;
    let mut previous_space = false;
    for character in raw.chars() {
        let character = if character.is_control() || character.is_whitespace() {
            ' '
        } else {
            character
        };
        if character == ' ' && previous_space {
            continue;
        }
        if chars == max_chars {
            truncated = true;
            break;
        }
        previous_space = character == ' ';
        text.push(character);
        chars += 1;
    }
    (text.trim().to_string(), truncated)
}

fn validate_browser_document_identity(value: Option<f64>) -> Result<f64, String> {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => Ok(value),
        _ => Err("browser document identity is unavailable".to_string()),
    }
}

/// Produces a display-only link destination. The page is untrusted, so the
/// backend removes credentials, query strings and fragments even if the fixed
/// extraction script was modified by page globals or returned hostile JSON.
pub fn sanitize_browser_link_destination(raw: &str) -> (Option<String>, bool) {
    let Ok(mut url) = Url::parse(raw.trim()) else {
        return (None, !raw.trim().is_empty());
    };
    if !matches!(url.scheme().to_ascii_lowercase().as_str(), "https" | "http")
        || url.host_str().is_none()
    {
        return (None, true);
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    let destination = url.to_string();
    let (destination, truncated) =
        normalize_browser_semantic_text(&destination, MAX_BROWSER_SEMANTIC_DESTINATION_CHARS);
    if truncated || destination.is_empty() {
        (None, true)
    } else {
        (Some(destination), false)
    }
}

fn canonical_semantic_role(raw: &str) -> Option<&'static str> {
    match raw {
        "heading" => Some("heading"),
        "button" => Some("button"),
        "link" => Some("link"),
        "textbox" => Some("textbox"),
        "select" => Some("select"),
        "checkbox" => Some("checkbox"),
        "radio" => Some("radio"),
        "switch" => Some("switch"),
        "slider" => Some("slider"),
        "spinbutton" => Some("spinbutton"),
        "option" => Some("option"),
        "file-input" => Some("file-input"),
        _ => None,
    }
}

fn semantic_item_serialized_chars(items: &[BrowserSemanticItem]) -> usize {
    serde_json::to_string(items)
        .map(|serialized| serialized.chars().count())
        .unwrap_or(usize::MAX)
}

fn normalize_browser_semantic_map(
    extracted: BrowserSemanticMapExtraction,
    max_serialized_chars: usize,
) -> (
    Vec<BrowserSemanticItem>,
    Vec<BrowserSemanticTargetRecord>,
    bool,
) {
    let mut truncated = extracted.truncated || extracted.items.len() > MAX_BROWSER_SEMANTIC_ITEMS;
    let mut items = Vec::new();
    let mut targets = Vec::new();
    for extracted in extracted.items.into_iter().take(MAX_BROWSER_SEMANTIC_ITEMS) {
        let Some(role) = canonical_semantic_role(extracted.role.as_str()) else {
            // A malformed page response must not smuggle an arbitrary schema
            // into a future provider/MCP contract.
            truncated = true;
            continue;
        };
        let (name, name_truncated) =
            normalize_browser_semantic_text(&extracted.name, MAX_BROWSER_SEMANTIC_NAME_CHARS);
        let (destination, destination_truncated) = if role == "link" {
            extracted
                .destination
                .as_deref()
                .map(sanitize_browser_link_destination)
                .unwrap_or((None, false))
        } else {
            (None, extracted.destination.is_some())
        };
        let supports_checked = matches!(role, "checkbox" | "radio" | "switch");
        let supports_selected = matches!(role, "select" | "option");
        let level = if role == "heading" {
            match extracted.level {
                Some(level @ 1..=6) => Some(level),
                _ => {
                    truncated = true;
                    continue;
                }
            }
        } else {
            if extracted.level.is_some() {
                truncated = true;
            }
            None
        };
        let item = BrowserSemanticItem {
            reference: format!("e{}", items.len() + 1),
            role: role.to_string(),
            level,
            name,
            destination,
            disabled: extracted.disabled,
            checked: supports_checked.then_some(extracted.checked).flatten(),
            selected: supports_selected.then_some(extracted.selected).flatten(),
            expanded: extracted.expanded,
            pressed: (role == "button").then_some(extracted.pressed).flatten(),
        };
        if name_truncated || destination_truncated {
            truncated = true;
        }
        if item.name.is_empty() && item.destination.is_none() {
            truncated = true;
            continue;
        }
        let Some(ordinal) = extracted.ordinal else {
            truncated = true;
            continue;
        };
        let Some(tag) = extracted.tag.as_deref() else {
            truncated = true;
            continue;
        };
        let tag = tag.to_ascii_lowercase();
        if ordinal == 0
            || ordinal > MAX_BROWSER_SEMANTIC_SCAN_NODES
            || targets
                .last()
                .is_some_and(|previous: &BrowserSemanticTargetRecord| ordinal <= previous.ordinal)
            || tag.is_empty()
            || tag.len() > 32
            || !tag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            truncated = true;
            continue;
        }
        let input_type = extracted
            .input_type
            .map(|input_type| input_type.to_ascii_lowercase());
        let Some(content_editable) = extracted.content_editable else {
            truncated = true;
            continue;
        };
        if input_type.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 32
                || !value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        }) {
            truncated = true;
            continue;
        }
        let mut candidate_items = items.clone();
        candidate_items.push(item);
        if semantic_item_serialized_chars(&candidate_items) > max_serialized_chars {
            truncated = true;
            break;
        }
        let item = candidate_items.last().expect("candidate item was pushed");
        targets.push(BrowserSemanticTargetRecord {
            reference: item.reference.clone(),
            ordinal,
            role: item.role.clone(),
            name: item.name.clone(),
            destination: item.destination.clone(),
            disabled: item.disabled,
            checked: item.checked,
            selected: item.selected,
            expanded: item.expanded,
            pressed: item.pressed,
            tag,
            input_type,
            content_editable,
        });
        items = candidate_items;
    }
    (items, targets, truncated)
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
    let mut grant = state
        .control_grant
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    *grant = Some(BrowserControlGrant {
        scope: scope_key(workspace_id, session_id),
        lifecycle_token,
        expires_at: now + BROWSER_CONTROL_GRANT_TTL,
    });
    Ok(control_grant_status(
        &mut grant,
        &scope_key(workspace_id, session_id),
        lifecycle_token,
        now,
    ))
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
    if let Ok(mut captures) = state.evidence_captures.lock() {
        captures.retain(|_, capture| capture.scope != scope);
    }
}

fn clear_browser_evidence_captures(state: &BrowserState) {
    if let Ok(mut captures) = state.evidence_captures.lock() {
        captures.clear();
    }
}

fn expire_browser_evidence_capture(
    captures: &Arc<Mutex<HashMap<String, BrowserEvidenceCaptureRecord>>>,
    capture_id: &str,
    capture_token: &str,
) {
    if let Ok(mut captures) = captures.lock() {
        if captures
            .get(capture_id)
            .is_some_and(|capture| capture.capture_token == capture_token)
        {
            captures.remove(capture_id);
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserEvidenceCallback {
    ok: bool,
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
        let kind = match raw.kind.as_str() {
            "consoleWarn" | "consoleError" | "error" | "resourceError" | "unhandledRejection" => {
                raw.kind
            }
            _ => {
                truncated = true;
                continue;
            }
        };
        if raw.sequence == 0 {
            truncated = true;
            continue;
        }
        let event = BrowserEvidenceEvent {
            kind,
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
    }
}

/// Only finite, bounded numeric literals reach this fixed page-side script.
/// It has no selector, DOM traversal, input value access, or renderer-provided JS.
fn browser_scroll_script(delta_x: f64, delta_y: f64) -> String {
    format!("(() => {{ window.scrollBy({delta_x}, {delta_y}); }})()")
}

fn browser_semantic_action_script(
    target: &BrowserSemanticTargetRecord,
    fill_text: Option<&str>,
    expected_url: &str,
    expected_document_identity: f64,
) -> Result<String, String> {
    let target_json = serde_json::to_string(target)
        .map_err(|_| "browser action target is invalid".to_string())?;
    let text_json = serde_json::to_string(&fill_text.unwrap_or_default())
        .map_err(|_| "browser fill text is invalid".to_string())?;
    let url_json = serde_json::to_string(expected_url)
        .map_err(|_| "browser action URL is invalid".to_string())?;
    if !expected_document_identity.is_finite() || expected_document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    let mode = if fill_text.is_some() { "fill" } else { "click" };
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
    if (tag === "textarea" || ["textbox", "searchbox"].includes(explicit)) return "textbox";
    if (tag === "select" || ["combobox", "listbox"].includes(explicit)) return "select";
    if (explicit === "checkbox") return "checkbox";
    if (explicit === "radio") return "radio";
    if (explicit === "switch") return "switch";
    if (explicit === "slider") return "slider";
    if (explicit === "spinbutton") return "spinbutton";
    if (explicit === "option") return "option";
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
  const clickCompatible = actual.role === "button" && (actual.tag === "button" || (actual.tag === "input" && ["button", "submit", "reset", "image"].includes(actual.inputType)))
    || actual.role === "link" && actual.tag === "a" && Boolean(actual.destination)
    || ["checkbox", "radio"].includes(actual.role) && actual.tag === "input" && actual.inputType === actual.role
    || actual.role === "switch" && (actual.tag === "button" || (actual.tag === "input" && actual.inputType === "checkbox"));
  const fillCompatible = !actual.contentEditable && actual.role === "textbox" && (actual.tag === "textarea"
    || actual.tag === "input" && ["text", "search", "email", "tel", "url"].includes(actual.inputType));
  if (mode === "click") {{
    if (!clickCompatible) return result(false, "incompatible");
    if (typeof candidate.click !== "function") return result(false, "failed");
    candidate.click();
    return result(true, "ok");
  }}
  if (!fillCompatible || Array.from(fillText).length > {MAX_BROWSER_FILL_CHARS} || /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/.test(fillText)) return result(false, "incompatible");
  const prototype = actual.tag === "textarea" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
  if (typeof setter !== "function") return result(false, "failed");
  setter.call(candidate, fillText);
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
        Err("browser evidence capture was unavailable".to_string())
    }
}

async fn eval_browser_action_with_callback(
    state: &BrowserState,
    script: String,
) -> Result<(), String> {
    let (sender, receiver) = oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let webview = webview
            .as_ref()
            .ok_or_else(|| "browser is not open".to_string())?;
        let sender = sender.clone();
        webview
            .eval_with_callback(script, move |raw| {
                if let Ok(mut sender) = sender.lock() {
                    if let Some(sender) = sender.take() {
                        let _ = sender.send(raw);
                    }
                }
            })
            .map_err(|_| "browser action failed".to_string())?;
    }
    let raw = timeout(BROWSER_CONTEXT_TIMEOUT, receiver)
        .await
        .map_err(|_| "browser action timed out".to_string())?
        .map_err(|_| "browser action confirmation was cancelled".to_string())?;
    parse_browser_action_callback(&raw)
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
    if target.disabled == Some(true) {
        return false;
    }
    match target.role.as_str() {
        "button" => {
            target.tag == "button"
                || (target.tag == "input"
                    && target.input_type.as_deref().is_some_and(|input_type| {
                        matches!(input_type, "button" | "submit" | "reset" | "image")
                    }))
        }
        "link" => target.tag == "a" && target.destination.is_some(),
        "checkbox" => target.tag == "input" && target.input_type.as_deref() == Some("checkbox"),
        "radio" => target.tag == "input" && target.input_type.as_deref() == Some("radio"),
        "switch" => {
            target.tag == "button"
                || (target.tag == "input" && target.input_type.as_deref() == Some("checkbox"))
        }
        _ => false,
    }
}

fn fill_target_is_compatible(target: &BrowserSemanticTargetRecord) -> bool {
    if target.disabled == Some(true) || target.content_editable || target.role != "textbox" {
        return false;
    }
    match target.tag.as_str() {
        "textarea" => true,
        "input" => target.input_type.as_deref().is_some_and(|input_type| {
            matches!(input_type, "text" | "search" | "email" | "tel" | "url")
        }),
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

/// The map is already consumed before this final native check. Reuse it for
/// every page-level action so a navigation that races after anchor validation
/// cannot target a different document.
fn require_action_native_url(
    current_url: &str,
    anchor: &BrowserActionAnchor,
) -> Result<(), String> {
    if current_url == anchor.url {
        Ok(())
    } else {
        Err("browser action anchor is stale".to_string())
    }
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
) -> Result<f64, String> {
    require_current_lifecycle(
        state,
        &anchor.workspace_id,
        anchor.session_id.as_deref(),
        anchor.lifecycle_token,
        "browser evidence anchor is stale",
    )?;
    let snapshot = current_snapshot(state)?;
    if !snapshot.visible || snapshot.url.as_deref() != Some(anchor.url.as_str()) {
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
    let (sender, receiver) = oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let webview = webview
            .as_ref()
            .ok_or_else(|| "browser is not open".to_string())?;
        let sender = sender.clone();
        webview
            .eval_with_callback(script, move |raw| {
                if let Ok(mut sender) = sender.lock() {
                    if let Some(sender) = sender.take() {
                        let _ = sender.send(raw);
                    }
                }
            })
            .map_err(|_| "browser evidence script failed".to_string())?;
    }
    timeout(BROWSER_EVIDENCE_TIMEOUT, receiver)
        .await
        .map_err(|_| "browser evidence script timed out".to_string())?
        .map_err(|_| "browser evidence response was cancelled".to_string())
}

fn browser_evidence_start_script(
    capture_token: &str,
    expected_url: &str,
    document_identity: f64,
    ttl_ms: u64,
) -> Result<String, String> {
    let token_json = serde_json::to_string(capture_token)
        .map_err(|_| "browser evidence token is invalid".to_string())?;
    let url_json = serde_json::to_string(expected_url)
        .map_err(|_| "browser evidence URL is invalid".to_string())?;
    if !document_identity.is_finite() || document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    let ttl_ms = ttl_ms.min(BROWSER_EVIDENCE_TTL.as_millis() as u64).max(1);
    Ok(format!(
        r#"(() => {{
  let stop = () => {{}};
  try {{
    const key = {token_json};
    const expectedUrl = {url_json};
    const expectedIdentity = {document_identity};
    if (window.location.href !== expectedUrl
      || !Number.isFinite(performance.timeOrigin)
      || performance.timeOrigin !== expectedIdentity
      || Object.prototype.hasOwnProperty.call(window, key)) return {{ ok: false }};
    const maxEvents = {MAX_BROWSER_EVIDENCE_EVENTS};
    const maxMessageChars = {MAX_BROWSER_EVIDENCE_MESSAGE_CHARS};
    const maxUrlChars = {MAX_BROWSER_EVIDENCE_URL_CHARS};
    const events = [];
    let sequence = 0;
    let truncated = false;
    let timer = 0;
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
    const add = (kind, message, url, line, column) => {{
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
    let installedWarn = false;
    let installedError = false;
    let installedErrorListener = false;
    let installedRejectionListener = false;
    stop = () => {{
      clearTimeout(timer);
      if (installedWarn && console.warn === warn) console.warn = originalWarn;
      if (installedError && console.error === error) console.error = originalError;
      if (installedErrorListener) window.removeEventListener("error", onError, true);
      if (installedRejectionListener) window.removeEventListener("unhandledrejection", onRejection, false);
      try {{ delete window[key]; }} catch (_) {{}}
    }};
    const drain = () => {{
      const result = {{ events: events.slice(0, maxEvents), truncated }};
      stop();
      return result;
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
    timer = window.setTimeout(stop, {ttl_ms});
    return {{ ok: true }};
  }} catch (_) {{
    try {{ stop(); }} catch (_) {{}}
    return {{ ok: false }};
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
    let url_json = serde_json::to_string(expected_url)
        .map_err(|_| "browser evidence URL is invalid".to_string())?;
    if !document_identity.is_finite() || document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    Ok(format!(
        r#"(() => {{
  try {{
    const key = {token_json};
    if (window.location.href !== {url_json}
      || !Number.isFinite(performance.timeOrigin)
      || performance.timeOrigin !== {document_identity}) return {{ ok: false, events: [], truncated: true }};
    const state = window[key];
    if (!state || typeof state.drain !== "function") return {{ ok: false, events: [], truncated: true }};
    const result = state.drain();
    return {{ ok: true, events: Array.isArray(result.events) ? result.events : [], truncated: Boolean(result.truncated) }};
  }} catch (_) {{ return {{ ok: false, events: [], truncated: true }}; }}
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
    let url_json = serde_json::to_string(expected_url)
        .map_err(|_| "browser evidence URL is invalid".to_string())?;
    if !document_identity.is_finite() || document_identity <= 0.0 {
        return Err("browser document identity is unavailable".to_string());
    }
    Ok(format!(
        r#"(() => {{
  try {{
    if (window.location.href !== {url_json}
      || !Number.isFinite(performance.timeOrigin)
      || performance.timeOrigin !== {document_identity}) return {{ ok: false }};
    const state = window[{token_json}];
    if (!state || typeof state.drain !== "function") return {{ ok: false }};
    state.drain();
    return {{ ok: true }};
  }} catch (_) {{ return {{ ok: false }}; }}
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

fn browser_context_script() -> String {
    // This code is initiated by the Rust command after scope validation. It
    // does not expose an IPC bridge or a callable DCC API to the remote page.
    format!(
        r#"(() => {{
  try {{
    const documentIdentity = (() => {{
      try {{ const value = performance.timeOrigin; return Number.isFinite(value) && value > 0 ? value : null; }}
      catch (_) {{ return null; }}
    }})();
    const maxTextChars = {MAX_BROWSER_CONTEXT_CHARS};
    const maxCandidates = {MAX_BROWSER_SEMANTIC_CANDIDATES};
    const maxScanNodes = {MAX_BROWSER_SEMANTIC_SCAN_NODES};
    const maxAncestors = {MAX_BROWSER_SEMANTIC_ANCESTORS};
    const maxTextNodes = {MAX_BROWSER_CONTEXT_TEXT_NODES};
    const maxNameTextNodes = {MAX_BROWSER_SEMANTIC_NAME_TEXT_NODES};
    const maxItems = {MAX_BROWSER_SEMANTIC_ITEMS};
    const maxNameChars = {MAX_BROWSER_SEMANTIC_NAME_CHARS};
    const maxDestinationChars = {MAX_BROWSER_SEMANTIC_DESTINATION_CHARS};
    const maxMapChars = {MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS};
    const clean = (value, max = maxNameChars) => String(value ?? "").slice(0, Math.max(max * 4, max))
      .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, max);
    let visibilityLimitReached = false;
    const isVisible = (element) => {{
      let node = element;
      for (let depth = 0; node && depth < maxAncestors; depth += 1, node = node.parentElement) {{
        if (node.hidden || node.inert || node.getAttribute("aria-hidden") === "true") return false;
        const style = window.getComputedStyle(node);
        if (style.display === "none" || style.visibility === "hidden" || Number.parseFloat(style.opacity) === 0) return false;
      }}
      if (node) {{ visibilityLimitReached = true; return false; }}
      const rect = element.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0
        && rect.right > 0 && rect.bottom > 0
        && rect.left < window.innerWidth && rect.top < window.innerHeight;
    }};
    let semanticTextTruncated = false;
    const semanticValue = (value, max = maxNameChars) => {{
      const raw = String(value ?? "");
      const text = clean(raw, max);
      if (raw.length > max || text.length >= max) semanticTextTruncated = true;
      return text;
    }};
    // Text nodes can expose authored defaults or live user input from controls.
    // Walk only a bounded ancestor chain and omit the node unless it can be
    // proven outside every value-bearing or editable subtree.
    const textNodeSafety = (node) => {{
      let element = node?.parentElement || null;
      for (let depth = 0; depth < maxAncestors; depth += 1) {{
        if (!element) return {{ safe: false, truncated: true }};
        const tag = element.tagName.toLowerCase();
        if (["input", "textarea", "select", "option"].includes(tag) || element.isContentEditable) {{
          return {{ safe: false, truncated: false }};
        }}
        if (element === document.body) return {{ safe: true, truncated: false }};
        element = element.parentElement;
      }}
      return {{ safe: false, truncated: true }};
    }};
    const boundedText = (root, maxChars, nodeBudget) => {{
      if (!root) return {{ text: "", truncated: false }};
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      const parts = [];
      let used = 0;
      let seen = 0;
      let truncated = false;
      let node;
      while (seen < nodeBudget && (node = walker.nextNode())) {{
        seen += 1;
        const safety = textNodeSafety(node);
        if (!safety.safe) {{
          if (safety.truncated) truncated = true;
          continue;
        }}
        const parent = node.parentElement;
        if (!parent || !isVisible(parent)) continue;
        const remaining = Math.max(0, maxChars - used);
        const raw = String(node.data ?? "");
        const part = clean(raw, remaining);
        if (!part) continue;
        parts.push(part);
        used += part.length + 1;
        if (raw.length > remaining || used >= maxChars) {{ truncated = true; break; }}
      }}
      if (seen >= nodeBudget) truncated = true;
      return {{ text: clean(parts.join(" "), maxChars), truncated }};
    }};
    let selectionPolicyTruncated = false;
    const safeSelection = (() => {{
      try {{
        const selection = window.getSelection?.();
        if (!selection || selection.rangeCount !== 1 || selection.isCollapsed) return null;
        const range = selection.getRangeAt(0);
        // A multi-node range could cross an editable/control subtree even if
        // both endpoints look safe, so only one verified text node is allowed.
        if (range.startContainer !== range.endContainer || range.startContainer.nodeType !== Node.TEXT_NODE) return null;
        const selectionSafety = textNodeSafety(range.startContainer);
        if (!selectionSafety.safe) {{
          if (selectionSafety.truncated) selectionPolicyTruncated = true;
          return null;
        }}
        const raw = selection.toString();
        return {{ text: clean(raw, maxTextChars), truncated: raw.length > maxTextChars }};
      }} catch (_) {{
        selectionPolicyTruncated = true;
        return null;
      }}
    }})();
    // Visible text is bounded independently. The semantic map never reads a
    // body-wide text property and never uses a value-bearing control as text.
    const visibleText = safeSelection
      ? safeSelection
      : boundedText(document.body, maxTextChars, maxTextNodes);
    const labelledByName = (element) => {{
      const ids = (element.getAttribute("aria-labelledby") || "").trim().split(/\s+/).filter(Boolean).slice(0, 4);
      const parts = [];
      for (const id of ids) {{
        const label = document.getElementById(id);
        if (label && label !== element && isVisible(label)) {{
          const text = boundedText(label, maxNameChars, maxNameTextNodes);
          if (text.truncated) semanticTextTruncated = true;
          parts.push(text.text);
        }}
      }}
      return semanticValue(parts.filter(Boolean).join(" "));
    }};
    const accessibleName = (element) => {{
      const ariaLabel = semanticValue(element.getAttribute("aria-label"));
      if (ariaLabel) return ariaLabel;
      const labelledBy = labelledByName(element);
      if (labelledBy) return labelledBy;
      const labels = "labels" in element && element.labels ? Array.from(element.labels).slice(0, 3) : [];
      const associated = semanticValue(labels.map((label) => {{
        const text = boundedText(label, maxNameChars, maxNameTextNodes);
        if (text.truncated) semanticTextTruncated = true;
        return text.text;
      }}).join(" "));
      if (associated) return associated;
      // These are bounded author-provided labels, never runtime control values.
      const alt = semanticValue(element.getAttribute("alt"));
      if (alt) return alt;
      const title = semanticValue(element.getAttribute("title"));
      if (title) return title;
      const placeholder = semanticValue(element.getAttribute("placeholder"));
      if (placeholder) return placeholder;
      if (element.isContentEditable || ["input", "textarea", "select"].includes(element.tagName.toLowerCase())) return "";
      const text = boundedText(element, maxNameChars, maxNameTextNodes);
      if (text.truncated) semanticTextTruncated = true;
      return text.text;
    }};
    const canonicalRole = (element) => {{
      const tag = element.tagName.toLowerCase();
      const explicit = (element.getAttribute("role") || "").toLowerCase();
      if (tag === "input") {{
        const inputType = (element.getAttribute("type") || "text").toLowerCase();
        // Never map value-bearing secrets, even when a page supplies a
        // misleading ARIA role.
        if (inputType === "hidden" || inputType === "password") return null;
      }}
      if (/^h[1-6]$/.test(tag)) return "heading";
      if (explicit === "heading") {{
        const level = Number.parseInt(element.getAttribute("aria-level") || "", 10);
        return level >= 1 && level <= 6 ? "heading" : null;
      }}
      if (tag === "button" || explicit === "button") return "button";
      if ((tag === "a" && element.hasAttribute("href")) || explicit === "link") return "link";
      if (tag === "textarea" || ["textbox", "searchbox"].includes(explicit)) return "textbox";
      if (tag === "select" || ["combobox", "listbox"].includes(explicit)) return "select";
      if (explicit === "checkbox") return "checkbox";
      if (explicit === "radio") return "radio";
      if (explicit === "switch") return "switch";
      if (explicit === "slider") return "slider";
      if (explicit === "spinbutton") return "spinbutton";
      if (explicit === "option") return "option";
      if (tag !== "input") return null;
      const type = (element.getAttribute("type") || "text").toLowerCase();
      if (type === "hidden" || type === "password") return null;
      if (["button", "submit", "reset", "image"].includes(type)) return "button";
      if (type === "checkbox") return "checkbox";
      if (type === "radio") return "radio";
      if (type === "range") return "slider";
      if (type === "number") return "spinbutton";
      if (type === "file") return "file-input";
      return "textbox";
    }};
    const booleanState = (element, property, ariaName) => {{
      try {{
        const aria = element.getAttribute(ariaName);
        if (aria === "true") return true;
        if (aria === "false") return false;
        return typeof element[property] === "boolean" ? element[property] : undefined;
      }} catch (_) {{ return undefined; }}
    }};
    const isCandidate = (element) => {{
      const tag = element.tagName.toLowerCase();
      if (["button", "input", "textarea", "select"].includes(tag) || (tag === "a" && element.hasAttribute("href")) || /^h[1-6]$/.test(tag)) return true;
      return ["heading", "button", "link", "textbox", "searchbox", "combobox", "listbox", "checkbox", "radio", "switch", "slider", "spinbutton", "option"].includes((element.getAttribute("role") || "").toLowerCase());
    }};
    const map = {{ items: [], truncated: false }};
    // This first map intentionally stays in the document tree: closed/open
    // Shadow DOM and iframe documents need their own scoped budgets and are
    // not represented by this human-context extraction.
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
    let scanned = 0;
    let candidates = 0;
    let element;
    while (scanned < maxScanNodes && (element = walker.nextNode())) {{
      scanned += 1;
      if (!isCandidate(element)) continue;
      candidates += 1;
      if (candidates > maxCandidates) {{ map.truncated = true; break; }}
      if (map.items.length >= maxItems) {{ map.truncated = true; break; }}
      if (!isVisible(element)) continue;
      const role = canonicalRole(element);
      if (!role) continue;
      const tag = element.tagName.toLowerCase();
      const inputType = tag === "input"
        ? (element.getAttribute("type") || "text").toLowerCase()
        : undefined;
      const item = {{ role, name: accessibleName(element), ordinal: scanned, tag, inputType, contentEditable: Boolean(element.isContentEditable) }};
      if (role === "heading") {{
        const tag = element.tagName.toLowerCase();
        const level = /^h[1-6]$/.test(tag)
          ? Number.parseInt(tag.slice(1), 10)
          : Number.parseInt(element.getAttribute("aria-level") || "", 10);
        if (level < 1 || level > 6) {{ map.truncated = true; continue; }}
        item.level = level;
      }}
      if (role === "link") {{
        try {{ item.destination = semanticValue(new URL(element.href, window.location.href).toString(), maxDestinationChars); }} catch (_) {{}}
      }}
      const disabled = role === "heading" ? undefined : booleanState(element, "disabled", "aria-disabled");
      const checked = ["checkbox", "radio", "switch"].includes(role) ? booleanState(element, "checked", "aria-checked") : undefined;
      const selected = ["select", "option"].includes(role) ? booleanState(element, "selected", "aria-selected") : undefined;
      const expanded = booleanState(element, "ariaExpanded", "aria-expanded");
      const pressed = role === "button" ? booleanState(element, "ariaPressed", "aria-pressed") : undefined;
      if (disabled !== undefined) item.disabled = disabled;
      if (checked !== undefined) item.checked = checked;
      if (selected !== undefined) item.selected = selected;
      if (expanded !== undefined) item.expanded = expanded;
      if (pressed !== undefined) item.pressed = pressed;
      if (!item.name && !item.destination) continue;
      if (JSON.stringify([...map.items, item]).length > maxMapChars) {{ map.truncated = true; break; }}
      map.items.push(item);
    }}
    if (scanned >= maxScanNodes || visibilityLimitReached || semanticTextTruncated) map.truncated = true;
    return {{
      text: visibleText.text,
      selectionOnly: Boolean(safeSelection),
      truncated: visibleText.truncated || selectionPolicyTruncated,
      documentIdentity,
      semanticMap: map,
    }};
  }} catch (_) {{
    return {{ text: "", selectionOnly: false, truncated: false, semanticMap: {{ items: [], truncated: true }} }};
  }}
}})()"#,
    )
}

fn emit_snapshot(app: &AppHandle<Wry>, snapshot: BrowserSnapshot) {
    let _ = app.emit("browser://state-changed", snapshot);
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
    let url = state
        .webview
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .as_ref()
        .and_then(|webview| webview.url().ok().map(|url| url.to_string()));
    let title = state.contexts.lock().ok().and_then(|contexts| {
        contexts
            .get(&scope_key(&workspace_id, session_id.as_deref()))
            .and_then(|c| c.title.clone())
    });
    Ok(BrowserSnapshot {
        workspace_id,
        session_id,
        lifecycle_token,
        visible,
        url,
        title,
    })
}

fn persist_current_context(state: &BrowserState) {
    let url = state.webview.lock().ok().and_then(|webviews| {
        webviews
            .as_ref()
            .and_then(|webview| webview.url().ok().map(|url| url.to_string()))
    });
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
    if should_persist_context_url(context, url.as_deref()) {
        context.url = url;
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

fn context_url(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
) -> Option<String> {
    state.contexts.lock().ok().and_then(|contexts| {
        contexts
            .get(&scope_key(workspace_id, session_id))
            .and_then(|context| context.url.clone())
    })
}

fn mark_context_loading(context: &mut BrowserContext, expected_url: String) {
    context.expected_url = Some(expected_url);
    context.ready_url = None;
    context.title = None;
    context.page_load_revision = context.page_load_revision.wrapping_add(1).max(1);
    context.semantic_map = None;
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

fn page_load_matches_expected(
    active_scope: Option<&str>,
    scope: &str,
    expected_url: Option<&str>,
    current_url: &str,
    payload_url: &str,
) -> bool {
    active_scope == Some(scope) && expected_url == Some(payload_url) && current_url == payload_url
}

fn callback_matches_scope(
    active_scope: &Arc<Mutex<Option<String>>>,
    contexts: &Arc<Mutex<HashMap<String, BrowserContext>>>,
    workspace_id: &str,
    session_id: Option<&str>,
    observed_url: &str,
) -> bool {
    // Tauri's title/page-load callbacks do not carry our workspace/session
    // token. Matching the active scope and requested URL prevents late events
    // from a previous scope from mutating the new one. Title callbacks remain
    // conservative because they do not expose a payload URL.
    let expected_scope = scope_key(workspace_id, session_id);
    let active_scope = active_scope.lock().ok().and_then(|scope| scope.clone());
    if active_scope.as_deref() != Some(expected_scope.as_str()) {
        return false;
    }
    contexts
        .lock()
        .ok()
        .and_then(|contexts| contexts.get(&expected_scope).cloned())
        .and_then(|context| context.expected_url)
        .is_some_and(|expected| expected == observed_url)
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

fn build_browser(
    app: &AppHandle<Wry>,
    state: &BrowserState,
    sessions: SessionCommandState,
    url: Url,
    bounds: BrowserBounds,
    initial_occluded: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is unavailable".to_string())?;
    let state_contexts = state.contexts.clone();
    let state_workspace = state.active_workspace.clone();
    let state_session = state.active_session.clone();
    let state_active_scope = state.active_scope.clone();
    let navigation_contexts = state.contexts.clone();
    let navigation_active_scope = state.active_scope.clone();
    let navigation_evidence_captures = state.evidence_captures.clone();
    let state_visible = state.visible.clone();
    let page_load_app = app.clone();
    let page_load_contexts = state.contexts.clone();
    let page_load_active_scope = state.active_scope.clone();
    let page_load_workspace = state.active_workspace.clone();
    let page_load_session = state.active_session.clone();
    let page_load_visible = state.visible.clone();
    let page_load_token = state.lifecycle_token.clone();
    let page_load_evidence_captures = state.evidence_captures.clone();
    let page_load_persisted_locations = state.persisted_locations.clone();
    let builder = WebviewBuilder::new(BROWSER_LABEL, WebviewUrl::External(url))
        .on_navigation(move |url| {
            if validate_browser_url(url.as_str()).is_err() {
                return false;
            }
            let Some(scope) = navigation_active_scope
                .lock()
                .ok()
                .and_then(|scope| scope.clone())
            else {
                return false;
            };
            if let Ok(mut contexts) = navigation_contexts.lock() {
                mark_context_loading(
                    contexts.entry(scope.clone()).or_default(),
                    url.as_str().to_string(),
                );
            }
            if let Ok(mut captures) = navigation_evidence_captures.lock() {
                captures.retain(|_, capture| capture.scope != scope);
            }
            true
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, event| {
            if let DownloadEvent::Requested { .. } = event {
                return false;
            }
            false
        })
        .on_document_title_changed(move |webview, title| {
            let (Some(workspace), Ok(session)) = (
                state_workspace.lock().ok().and_then(|w| w.clone()),
                state_session.lock(),
            ) else {
                return;
            };
            let Some(current_url) = webview.url().ok().map(|url| url.to_string()) else {
                return;
            };
            if !callback_matches_scope(
                &state_active_scope,
                &state_contexts,
                &workspace,
                session.as_deref(),
                &current_url,
            ) {
                return;
            }
            if let Ok(mut contexts) = state_contexts.lock() {
                contexts
                    .entry(scope_key(&workspace, session.as_deref()))
                    .or_default()
                    .title = Some(title);
            }
        })
        .on_page_load(move |webview, payload| {
            let Some(workspace_id) = page_load_workspace
                .lock()
                .ok()
                .and_then(|workspace| workspace.clone())
            else {
                return;
            };
            let session_id = page_load_session
                .lock()
                .ok()
                .and_then(|session| session.clone());
            let Ok(current_url) = webview.url() else {
                return;
            };
            let key = scope_key(&workspace_id, session_id.as_deref());
            let payload_url = payload.url().to_string();
            if matches!(payload.event(), PageLoadEvent::Started) {
                let active_scope = page_load_active_scope
                    .lock()
                    .ok()
                    .and_then(|scope| scope.clone());
                if active_scope.as_deref() != Some(key.as_str()) {
                    return;
                }
                if let Ok(mut contexts) = page_load_contexts.lock() {
                    if let Some(context) = contexts.get_mut(&key) {
                        if context.expected_url.as_deref() == Some(payload_url.as_str()) {
                            context.ready_url = None;
                            context.title = None;
                            context.page_load_revision =
                                context.page_load_revision.wrapping_add(1).max(1);
                            invalidate_semantic_map(context);
                        }
                    }
                }
                if let Ok(mut captures) = page_load_evidence_captures.lock() {
                    captures.retain(|_, capture| capture.scope != key);
                }
                return;
            }
            if !matches!(payload.event(), PageLoadEvent::Finished) {
                return;
            }
            // Internal links remain valid because on_navigation updates
            // expected_url for the active scope. A late Finished event from a
            // prior workspace/session cannot satisfy all three URL checks.
            let active_scope = page_load_active_scope
                .lock()
                .ok()
                .and_then(|scope| scope.clone());
            let title = page_load_contexts.lock().ok().and_then(|mut contexts| {
                let context = contexts.get_mut(&key)?;
                if !page_load_matches_expected(
                    active_scope.as_deref(),
                    &key,
                    context.expected_url.as_deref(),
                    current_url.as_str(),
                    &payload_url,
                ) {
                    return None;
                }
                context.url = Some(payload_url.clone());
                context.ready_url = Some(payload_url.clone());
                Some(context.title.clone())
            });
            let Some(title) = title else {
                return;
            };
            persist_browser_location(
                &sessions,
                &page_load_persisted_locations,
                &workspace_id,
                session_id.as_deref(),
                &payload_url,
            );
            emit_snapshot(
                &page_load_app,
                BrowserSnapshot {
                    workspace_id,
                    session_id,
                    lifecycle_token: page_load_token.lock().map(|token| *token).unwrap_or(0),
                    visible: page_load_visible
                        .lock()
                        .map(|visible| *visible)
                        .unwrap_or(true),
                    url: Some(payload_url),
                    title,
                },
            );
        });
    let parent_window = window.as_ref().window();
    let webview = parent_window
        .add_child(
            builder,
            tauri::LogicalPosition::new(bounds.x.max(0.0), bounds.y.max(0.0)),
            tauri::LogicalSize::new(bounds.width.max(1.0), bounds.height.max(1.0)),
        )
        .map_err(|error| format!("failed to create browser WebView: {error}"))?;
    if initial_occluded {
        webview
            .hide()
            .map_err(|error| format!("failed to hide browser during initial occlusion: {error}"))?;
    }
    *state
        .webview
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = Some(webview);
    *state_visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = !initial_occluded;
    Ok(())
}

#[tauri::command]
pub async fn browser_open(
    app: AppHandle<Wry>,
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    initial_url: Option<String>,
    restore_last_url: Option<bool>,
    bounds: BrowserBounds,
    initial_occluded: Option<bool>,
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    let bounds = normalize_browser_bounds(bounds)?;
    let key = scope_key(&workspace_id, session_id.as_deref());
    let requested_url = initial_url.is_some();
    let initial_occluded = initial_occluded.unwrap_or(false);
    let existing = state
        .webview
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .is_some();
    let previous_scope = state
        .active_scope
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?
        .clone();
    let runtime_url = context_url(&state, &workspace_id, session_id.as_deref());
    let restore_last_url = restore_last_url.unwrap_or(false);
    let stored_url = if initial_url.is_none() && runtime_url.is_none() && restore_last_url {
        let now_ms = browser_location_now_ms();
        match now_ms.and_then(|now_ms| {
            sessions
                .load_browser_location(&workspace_id, session_id.as_deref(), now_ms)
                .ok()
                .flatten()
        }) {
            Some(stored_url) => match sanitize_browser_location_url(&stored_url) {
                // A DB read is deliberately not a cache insertion: the first
                // validated PageLoad Finished after restore renews the TTL.
                Some(safe_url) => Some(safe_url),
                None => {
                    let _ = sessions.delete_browser_location(&workspace_id, session_id.as_deref());
                    if let Ok(mut cache) = state.persisted_locations.lock() {
                        browser_location_cache_remove(&mut cache, &key);
                    }
                    None
                }
            },
            // A missing row can mean a prior best-effort deletion (including
            // expiry). It must not leave an in-memory dedup entry behind,
            // otherwise a later PageLoad Finished could skip its TTL renewal.
            None => {
                if let Ok(mut cache) = state.persisted_locations.lock() {
                    browser_location_cache_remove(&mut cache, &key);
                }
                None
            }
        }
    } else {
        None
    };
    let desired = select_browser_open_url(
        initial_url.as_deref(),
        runtime_url,
        stored_url,
        restore_last_url,
    )?;
    set_active_scope(&state, &workspace_id, session_id.as_deref())?;
    let should_navigate = requested_url || previous_scope.as_deref() != Some(key.as_str());
    {
        let mut token = state
            .lifecycle_token
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        advance_lifecycle_token(&mut token);
    }
    // Even a reopen to the same URL is a distinct surface lifecycle.
    invalidate_semantic_map_for_scope(&state, &workspace_id, session_id.as_deref());
    clear_browser_evidence_captures(&state);
    clear_browser_control_grant(&state);
    *state
        .lifecycle_open
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = true;
    *state
        .occluded
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = initial_occluded;
    if should_navigate || !existing {
        if let Ok(mut contexts) = state.contexts.lock() {
            mark_context_loading(
                contexts.entry(key.clone()).or_default(),
                desired.to_string(),
            );
        }
    }
    if !existing {
        build_browser(
            &app,
            &state,
            sessions.inner().clone(),
            desired,
            bounds,
            initial_occluded,
        )?;
    } else {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        if let Some(webview) = webview.as_ref() {
            if should_navigate {
                webview
                    .navigate(desired)
                    .map_err(|error| format!("failed to navigate browser: {error}"))?;
            }
            webview
                .set_position(tauri::LogicalPosition::new(
                    bounds.x.max(0.0),
                    bounds.y.max(0.0),
                ))
                .map_err(|error| error.to_string())?;
            webview
                .set_size(tauri::LogicalSize::new(
                    bounds.width.max(1.0),
                    bounds.height.max(1.0),
                ))
                .map_err(|error| error.to_string())?;
            if initial_occluded {
                webview.hide().map_err(|error| error.to_string())?;
            } else {
                webview.show().map_err(|error| error.to_string())?;
            }
        }
    }
    *state
        .visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = !initial_occluded;
    let snapshot = current_snapshot(&state)?;
    emit_snapshot(&app, snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub async fn browser_navigate(
    app: AppHandle<Wry>,
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    url: String,
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    let url = validate_browser_url(&url)?;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser navigation lifecycle is stale",
    )?;
    if let Ok(mut contexts) = state.contexts.lock() {
        mark_context_loading(
            contexts
                .entry(scope_key(&workspace_id, session_id.as_deref()))
                .or_default(),
            url.to_string(),
        );
    }
    invalidate_browser_evidence_for_scope(&state, &workspace_id, session_id.as_deref());
    {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let webview = webview
            .as_ref()
            .ok_or_else(|| "browser is not open".to_string())?;
        webview
            .navigate(url)
            .map_err(|error| format!("failed to navigate browser: {error}"))?;
    }
    let snapshot = current_snapshot(&state)?;
    emit_snapshot(&app, snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub async fn browser_reload(
    app: AppHandle<Wry>,
    state: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser reload lifecycle is stale",
    )?;
    {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let webview = webview
            .as_ref()
            .ok_or_else(|| "browser is not open".to_string())?;
        let current_url = webview
            .url()
            .map_err(|error| format!("failed to read browser URL: {error}"))?
            .to_string();
        if let Ok(mut contexts) = state.contexts.lock() {
            mark_context_loading(
                contexts
                    .entry(scope_key(&workspace_id, session_id.as_deref()))
                    .or_default(),
                current_url.clone(),
            );
        }
        invalidate_browser_evidence_for_scope(&state, &workspace_id, session_id.as_deref());
        webview
            .reload()
            .map_err(|error| format!("failed to reload browser: {error}"))?;
    }
    let snapshot = current_snapshot(&state)?;
    emit_snapshot(&app, snapshot.clone());
    Ok(snapshot)
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
        if !current_snapshot(&state)?.visible {
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
        _ => None,
    };
    if let Some((target, _document_identity)) = target.as_ref() {
        let reference = &target.reference;
        let _ = consume_browser_action_target(state, &anchor, reference)?;
    } else {
        consume_browser_action_anchor(&state, &anchor)?;
    }

    let action_name = action.name().to_string();
    match action {
        PreparedBrowserControlAction::Navigate(url) => {
            let webview = state
                .webview
                .lock()
                .map_err(|_| "browser state lock poisoned".to_string())?;
            let webview = webview
                .as_ref()
                .ok_or_else(|| "browser is not open".to_string())?;
            let current_url = webview
                .url()
                .map_err(|_| "browser action anchor is stale".to_string())?
                .to_string();
            require_action_native_url(&current_url, &anchor)?;
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
            webview
                .navigate(url)
                .map_err(|_| "browser navigate action failed".to_string())?;
        }
        PreparedBrowserControlAction::Reload => {
            let webview = state
                .webview
                .lock()
                .map_err(|_| "browser state lock poisoned".to_string())?;
            let webview = webview
                .as_ref()
                .ok_or_else(|| "browser is not open".to_string())?;
            let current_url = webview
                .url()
                .map_err(|_| "browser action anchor is stale".to_string())?
                .to_string();
            require_action_native_url(&current_url, &anchor)?;
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
                    current_url,
                );
            }
            invalidate_browser_evidence_for_scope(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
            );
            webview
                .reload()
                .map_err(|_| "browser reload action failed".to_string())?;
        }
        PreparedBrowserControlAction::Scroll { delta_x, delta_y } => {
            let webview = state
                .webview
                .lock()
                .map_err(|_| "browser state lock poisoned".to_string())?;
            let webview = webview
                .as_ref()
                .ok_or_else(|| "browser is not open".to_string())?;
            let current_url = webview
                .url()
                .map_err(|_| "browser action anchor is stale".to_string())?
                .to_string();
            require_action_native_url(&current_url, &anchor)?;
            require_action_page_revision(state, &anchor)?;
            require_browser_control_grant(
                state,
                &anchor.workspace_id,
                anchor.session_id.as_deref(),
                anchor.lifecycle_token,
                Instant::now(),
            )?;
            webview
                .eval(browser_scroll_script(delta_x, delta_y))
                .map_err(|_| "browser scroll action failed".to_string())?;
        }
        PreparedBrowserControlAction::Click { .. } => {
            let (target, document_identity) =
                target.ok_or_else(|| "browser action target is stale".to_string())?;
            let webview_url = {
                let webview = state
                    .webview
                    .lock()
                    .map_err(|_| "browser state lock poisoned".to_string())?;
                let webview = webview
                    .as_ref()
                    .ok_or_else(|| "browser is not open".to_string())?;
                webview
                    .url()
                    .map_err(|_| "browser action anchor is stale".to_string())?
                    .to_string()
            };
            require_action_native_url(&webview_url, &anchor)?;
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
                browser_semantic_action_script(&target, None, &anchor.url, document_identity)?,
            )
            .await?;
        }
        PreparedBrowserControlAction::Fill { text, .. } => {
            let (target, document_identity) =
                target.ok_or_else(|| "browser action target is stale".to_string())?;
            let webview_url = {
                let webview = state
                    .webview
                    .lock()
                    .map_err(|_| "browser state lock poisoned".to_string())?;
                let webview = webview
                    .as_ref()
                    .ok_or_else(|| "browser is not open".to_string())?;
                webview
                    .url()
                    .map_err(|_| "browser action anchor is stale".to_string())?
                    .to_string()
            };
            require_action_native_url(&webview_url, &anchor)?;
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
    let result = start_browser_evidence_capture(&state, &sessions, anchor).await;
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
    if !snapshot.visible || snapshot.url.as_deref() != Some(anchor.url.as_str()) {
        return Err("browser evidence anchor is stale".to_string());
    }
    let scope = scope_key(&anchor.workspace_id, anchor.session_id.as_deref());
    {
        let mut captures = state
            .evidence_captures
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        captures.retain(|_, capture| capture.expires_at > now);
        if captures
            .values()
            .any(|capture| capture.scope == scope && capture.expires_at > now)
        {
            return Err("browser evidence capture is already active".to_string());
        }
    }
    let document_identity = consume_browser_evidence_anchor(&state, &anchor)?;
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
            expire_browser_evidence_capture(&state.evidence_captures, &capture_id, &capture_token);
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
        expire_browser_evidence_capture(&state.evidence_captures, &capture_id, &capture_token);
        return Err(error);
    }
    let started = eval_browser_evidence_with_callback(&state, script)
        .await
        .and_then(|raw| parse_browser_evidence_callback(&raw).map(|_| ()));
    if let Err(error) = started {
        expire_browser_evidence_capture(&state.evidence_captures, &capture_id, &capture_token);
        return Err(error);
    }

    let remaining_ms = expires_at
        .duration_since(Instant::now())
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let captures = state.evidence_captures.clone();
    let expiry_id = capture_id.clone();
    let expiry_token = capture_token.clone();
    let delay = expires_at.saturating_duration_since(Instant::now());
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        expire_browser_evidence_capture(&captures, &expiry_id, &expiry_token);
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

    // Recheck the grant at the effect boundary. Expiry after consumption is a
    // safe failure and deliberately cannot restore the consumed capability.
    require_browser_control_grant(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        Instant::now(),
    )?;
    let script =
        browser_evidence_read_script(&record.capture_token, &record.url, record.document_identity)?;
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

/// Best-effort cleanup for an evidence capture that could not be handed to an
/// authenticated projection lease. It immediately removes the backend record
/// and, when a Tokio runtime is available, schedules a short fixed-script
/// drain to restore page wrappers without returning their buffered events.
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
    let state = state.clone();
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
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
    extract_browser_context_locked(state, sessions, workspace_id, session_id, lifecycle_token).await
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
    if !initial_snapshot.visible {
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

    let (sender, receiver) = oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let webview = webview
            .as_ref()
            .ok_or_else(|| "browser is not open".to_string())?;
        let sender = sender.clone();
        webview
            .eval_with_callback(browser_context_script(), move |raw| {
                if let Ok(mut sender) = sender.lock() {
                    if let Some(sender) = sender.take() {
                        let _ = sender.send(raw);
                    }
                }
            })
            .map_err(|error| format!("failed to read browser page context: {error}"))?;
    }

    let raw = timeout(BROWSER_CONTEXT_TIMEOUT, receiver)
        .await
        .map_err(|_| "browser page context timed out".to_string())?
        .map_err(|_| "browser page context response was cancelled".to_string())?;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser context lifecycle is stale",
    )?;
    let snapshot = current_snapshot(&state)?;
    if !snapshot.visible || snapshot.url.as_deref() != Some(initial_url.as_str()) {
        return Err("browser page changed while reading context".to_string());
    }
    if !context_ready_for_scope(&state, &workspace_id, session_id.as_deref(), &initial_url)? {
        return Err("browser page changed while reading context".to_string());
    }
    let extracted: BrowserContextExtraction = serde_json::from_str(&raw)
        .map_err(|_| "browser page context response was invalid".to_string())?;
    let document_identity = validate_browser_document_identity(extracted.document_identity)?;
    let (text, text_truncated) =
        normalize_browser_context_text(&extracted.text, MAX_BROWSER_CONTEXT_CHARS);
    let (title, title_truncated) = snapshot
        .title
        .as_deref()
        .map(|title| normalize_browser_context_text(title, MAX_BROWSER_TITLE_CHARS))
        .unwrap_or_default();
    let (display_url, url_truncated) =
        normalize_browser_context_text(&initial_url, MAX_BROWSER_CONTEXT_URL_CHARS);
    let fixed_envelope_chars =
        text.chars().count() + title.chars().count() + display_url.chars().count();
    let semantic_budget = MAX_BROWSER_SEMANTIC_SERIALIZED_CHARS
        .min(MAX_BROWSER_CONTEXT_ENVELOPE_CHARS.saturating_sub(fixed_envelope_chars));
    let (semantic_items, semantic_targets, semantic_truncated) =
        normalize_browser_semantic_map(extracted.semantic_map, semantic_budget);

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
    if !final_snapshot.visible || final_snapshot.url.as_deref() != Some(initial_url.as_str()) {
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
        document_identity,
        semantic_items,
        semantic_targets,
        semantic_truncated,
    )?;

    Ok(BrowserAgentContext {
        workspace_id,
        session_id,
        url: display_url,
        title: (!title.is_empty()).then_some(title),
        text,
        selection_only: extracted.selection_only,
        truncated: extracted.truncated
            || text_truncated
            || title_truncated
            || url_truncated
            || semantic_map.truncated,
        semantic_map,
    })
}

#[tauri::command]
pub async fn browser_set_bounds(
    state: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    bounds: BrowserBounds,
) -> Result<(), String> {
    let _operation = state.operation_lock.lock().await;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser layout lifecycle is stale",
    )?;
    let bounds = normalize_browser_bounds(bounds)?;
    let webview = state
        .webview
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let webview = webview
        .as_ref()
        .ok_or_else(|| "browser is not open".to_string())?;
    webview
        .set_position(tauri::LogicalPosition::new(
            bounds.x.max(0.0),
            bounds.y.max(0.0),
        ))
        .and_then(|_| {
            webview.set_size(tauri::LogicalSize::new(
                bounds.width.max(1.0),
                bounds.height.max(1.0),
            ))
        })
        .map_err(|error| error.to_string())
}

/// Hide/show the child WebView for a short-lived DCC surface. This lifecycle
/// is intentionally separate from `browser_hide`, which closes the Browser
/// surface. The token makes a delayed restore from an old mount harmless.
#[tauri::command]
pub async fn browser_set_occluded(
    app: AppHandle<Wry>,
    state: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    occluded: bool,
    bounds: Option<BrowserBounds>,
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser visibility lifecycle is stale",
    )?;
    let currently_occluded = *state
        .occluded
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    let bounds = bounds.map(normalize_browser_bounds).transpose()?;
    {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        let webview = webview
            .as_ref()
            .ok_or_else(|| "browser is not open".to_string())?;
        if let Some(bounds) = bounds {
            webview
                .set_position(tauri::LogicalPosition::new(bounds.x, bounds.y))
                .and_then(|_| {
                    webview.set_size(tauri::LogicalSize::new(bounds.width, bounds.height))
                })
                .map_err(|error| error.to_string())?;
        }
        if currently_occluded != occluded {
            if occluded {
                webview.hide().map_err(|error| error.to_string())?;
            } else {
                webview.show().map_err(|error| error.to_string())?;
            }
        }
    }
    if currently_occluded == occluded {
        return current_snapshot(&state);
    }
    *state
        .occluded
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = occluded;
    *state
        .visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = !occluded;
    let snapshot = current_snapshot(&state)?;
    emit_snapshot(&app, snapshot.clone());
    Ok(snapshot)
}

#[tauri::command]
pub async fn browser_hide(
    app: AppHandle<Wry>,
    state: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser close lifecycle is stale",
    )?;
    clear_browser_control_grant(&state);
    invalidate_browser_evidence_for_scope(&state, &workspace_id, session_id.as_deref());
    {
        let webview = state
            .webview
            .lock()
            .map_err(|_| "browser state lock poisoned".to_string())?;
        webview
            .as_ref()
            .ok_or_else(|| "browser is not open".to_string())?
            .hide()
            .map_err(|error| error.to_string())?;
    }
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
    invalidate_semantic_map_for_scope(&state, &workspace_id, session_id.as_deref());
    state
        .lifecycle_token
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())
        .map(|mut token| advance_lifecycle_token(&mut token))?;
    persist_current_context(&state);
    let snapshot = current_snapshot(&state)?;
    emit_snapshot(&app, snapshot.clone());
    Ok(snapshot)
}

pub fn shutdown(state: &BrowserState) {
    clear_browser_evidence_captures(state);
    if let Ok(mut webview) = state.webview.lock() {
        if let Some(webview) = webview.take() {
            let _ = webview.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration as StdDuration, Instant};

    use super::{
        advance_lifecycle_token, append_browser_audit, arm_browser_control_grant,
        browser_action_anchor_matches_context, browser_audit_outcome,
        browser_evidence_cleanup_script, browser_evidence_read_script,
        browser_evidence_start_script, browser_location_cache_insert,
        browser_location_cache_matches_current, browser_location_cache_purge_expired,
        browser_location_cache_remove, browser_scroll_script, browser_semantic_action_script,
        clear_browser_control_grant, click_target_is_compatible, consume_semantic_map_for_anchor,
        context_is_ready_for_url, control_grant_is_current, control_grant_status,
        fill_target_is_compatible, mark_context_loading, normalize_browser_bounds,
        normalize_browser_context_text, normalize_browser_evidence_events,
        normalize_browser_fill_text, normalize_browser_scroll_delta,
        normalize_browser_semantic_map, occlusion_request_is_current, page_load_matches_expected,
        page_load_revision_is_current, parse_browser_action_callback,
        parse_browser_evidence_callback, parse_browser_evidence_cleanup_callback,
        prepare_browser_control_action, read_browser_audit, require_action_native_url,
        require_browser_control_grant, require_current_lifecycle, sanitize_browser_evidence_url,
        sanitize_browser_link_destination, sanitize_browser_location_url, select_browser_open_url,
        semantic_item_serialized_chars, semantic_map_record_is_current, should_persist_context_url,
        validate_browser_document_identity, validate_browser_evidence_capture_id,
        validate_browser_reference, validate_browser_url, BrowserActionAnchor, BrowserActionResult,
        BrowserAuditGrantState, BrowserAuditOrigin, BrowserAuditOutcome, BrowserAuditRecord,
        BrowserAuditTool, BrowserAuditViewRecord, BrowserBounds, BrowserContext,
        BrowserControlAction, BrowserControlGrant, BrowserControlStatus,
        BrowserEvidenceCaptureRecord, BrowserEvidenceEventRaw, BrowserLocationCache,
        BrowserSemanticItemExtraction, BrowserSemanticMap, BrowserSemanticMapExtraction,
        BrowserSemanticMapRecord, BrowserSemanticTargetRecord, BrowserState,
        MAX_BROWSER_AUDIT_ENTRIES, MAX_BROWSER_AUDIT_LEASE_FINGERPRINT_CHARS,
        MAX_BROWSER_AUDIT_PROVIDER_CHARS, MAX_BROWSER_AUDIT_SCOPE_CHARS,
        MAX_BROWSER_EVIDENCE_CHARS, MAX_BROWSER_EVIDENCE_EVENTS,
        MAX_BROWSER_LOCATION_CACHE_ENTRIES, MAX_BROWSER_SEMANTIC_ITEMS,
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
    fn durable_browser_location_sanitization_removes_secret_components() {
        assert_eq!(
            sanitize_browser_location_url(
                "https://user:secret@example.test:8443/path?q=private#fragment",
            ),
            Some("https://example.test:8443/path".to_string())
        );
        assert_eq!(
            sanitize_browser_location_url("http://localhost:3000/path?token=private#section"),
            Some("http://localhost:3000/path".to_string())
        );
        assert!(sanitize_browser_location_url("https://example.test/path\n").is_none());
        assert!(sanitize_browser_location_url("http://example.test/private").is_none());
        assert!(sanitize_browser_location_url("javascript:alert(1)").is_none());
    }

    #[test]
    fn browser_open_url_precedence_keeps_restore_explicit() {
        let initial = select_browser_open_url(
            Some("https://initial.test/explicit"),
            Some("https://runtime.test/current".to_string()),
            Some("https://stored.test/last?private=yes#fragment".to_string()),
            true,
        )
        .unwrap();
        assert_eq!(initial.as_str(), "https://initial.test/explicit");

        let runtime = select_browser_open_url(
            None,
            Some("https://runtime.test/current".to_string()),
            Some("https://stored.test/last".to_string()),
            true,
        )
        .unwrap();
        assert_eq!(runtime.as_str(), "https://runtime.test/current");

        let opt_in = select_browser_open_url(
            None,
            None,
            Some("https://stored.test/last?private=yes#fragment".to_string()),
            true,
        )
        .unwrap();
        assert_eq!(opt_in.as_str(), "https://stored.test/last");

        let defaulted = select_browser_open_url(
            None,
            None,
            Some("https://stored.test/last".to_string()),
            false,
        )
        .unwrap();
        assert_eq!(defaulted.as_str(), super::DEFAULT_URL);
    }

    #[test]
    fn browser_location_cache_is_bounded_and_evicts_the_oldest_scope() {
        let mut cache = BrowserLocationCache::default();
        for index in 0..MAX_BROWSER_LOCATION_CACHE_ENTRIES {
            browser_location_cache_insert(
                &mut cache,
                format!("scope-{index}"),
                format!("https://example.test/{index}"),
                10_000,
            );
        }
        assert_eq!(cache.entries.len(), MAX_BROWSER_LOCATION_CACHE_ENTRIES);
        browser_location_cache_insert(
            &mut cache,
            "newest".to_string(),
            "https://example.test/newest".to_string(),
            10_000,
        );
        assert_eq!(cache.entries.len(), MAX_BROWSER_LOCATION_CACHE_ENTRIES);
        assert!(!cache.entries.contains_key("scope-0"));
        assert!(cache.entries.contains_key("newest"));
    }

    #[test]
    fn browser_location_cache_deduplicates_only_a_current_matching_url() {
        let mut cache = BrowserLocationCache::default();
        browser_location_cache_insert(
            &mut cache,
            "scope".to_string(),
            "https://example.test/ready".to_string(),
            200,
        );
        assert!(browser_location_cache_matches_current(
            &cache,
            "scope",
            "https://example.test/ready",
            199,
        ));
        browser_location_cache_purge_expired(&mut cache, 200);
        assert!(!browser_location_cache_matches_current(
            &cache,
            "scope",
            "https://example.test/ready",
            200,
        ));
        assert!(cache.entries.is_empty());
        browser_location_cache_insert(
            &mut cache,
            "scope".to_string(),
            "https://example.test/ready".to_string(),
            300,
        );
        assert!(browser_location_cache_matches_current(
            &cache,
            "scope",
            "https://example.test/ready",
            200,
        ));
        browser_location_cache_remove(&mut cache, "scope");
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn loading_a_durable_url_does_not_prewarm_the_write_dedup_cache() {
        let cache = BrowserLocationCache::default();
        let restored = select_browser_open_url(
            None,
            None,
            Some("https://stored.test/last".to_string()),
            true,
        )
        .unwrap();
        assert_eq!(restored.as_str(), "https://stored.test/last");
        // Only a successful PageLoad Finished write can enter this cache, so
        // restored URLs renew their durable TTL on that first confirmed load.
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn only_a_current_finished_page_load_is_persistable() {
        assert!(page_load_matches_expected(
            Some("workspace\u{1f}|session"),
            "workspace\u{1f}|session",
            Some("https://example.test/ready"),
            "https://example.test/ready",
            "https://example.test/ready",
        ));
        assert!(!page_load_matches_expected(
            Some("other\u{1f}|session"),
            "workspace\u{1f}|session",
            Some("https://example.test/ready"),
            "https://example.test/ready",
            "https://example.test/ready",
        ));
        assert!(!page_load_matches_expected(
            Some("workspace\u{1f}|session"),
            "workspace\u{1f}|session",
            Some("https://example.test/ready"),
            "https://example.test/old",
            "https://example.test/ready",
        ));
    }

    #[test]
    fn normalizes_and_rejects_browser_bounds() {
        let normalized = normalize_browser_bounds(BrowserBounds {
            x: -20.0,
            y: 20_000.0,
            width: 20_000.0,
            height: 0.5,
        })
        .expect("valid bounds should normalize");
        assert_eq!(normalized.x, 0.0);
        assert_eq!(normalized.y, 16_384.0);
        assert_eq!(normalized.width, 16_384.0);
        assert_eq!(normalized.height, 1.0);

        for invalid in [
            BrowserBounds {
                x: f64::NAN,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            BrowserBounds {
                x: 0.0,
                y: f64::INFINITY,
                width: 1.0,
                height: 1.0,
            },
            BrowserBounds {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 1.0,
            },
            BrowserBounds {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: -1.0,
            },
        ] {
            assert!(normalize_browser_bounds(invalid).is_err());
        }
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
        assert!(script.contains("maxCandidates"));
        assert!(script.contains("maxMapChars"));
        assert!(script.contains("const textNodeSafety"));
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
    fn navigate_reload_and_scroll_require_the_final_native_url_recheck() {
        let (anchor, _) = action_anchor_and_context();
        for action in ["navigate", "reload", "scroll"] {
            assert!(
                require_action_native_url(&anchor.url, &anchor).is_ok(),
                "{action}"
            );
            assert!(
                require_action_native_url("https://example.test/changed", &anchor).is_err(),
                "{action}"
            );
        }
    }

    #[test]
    fn control_grants_are_scoped_lifecycle_bound_expiring_and_clearable() {
        let now = Instant::now();
        let grant = BrowserControlGrant {
            scope: "workspace\u{1f}|session".to_string(),
            lifecycle_token: 8,
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
    fn control_grant_status_reports_remaining_and_clears_expired_grants() {
        let now = Instant::now();
        let mut grant = Some(BrowserControlGrant {
            scope: "workspace\u{1f}|session".to_string(),
            lifecycle_token: 8,
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
    fn only_accepts_finished_loads_for_the_active_expected_url() {
        let scope = "workspace\u{1f}|session-b";
        let mut context = BrowserContext::default();
        mark_context_loading(&mut context, "https://example.test/b".to_string());
        assert!(!context_is_ready_for_url(
            &context,
            "https://example.test/b"
        ));
        assert!(page_load_matches_expected(
            Some(scope),
            scope,
            context.expected_url.as_deref(),
            "https://example.test/b",
            "https://example.test/b",
        ));
        assert!(!page_load_matches_expected(
            Some(scope),
            scope,
            context.expected_url.as_deref(),
            "https://example.test/a",
            "https://example.test/a",
        ));
        assert!(!page_load_matches_expected(
            Some("workspace\u{1f}|session-a"),
            scope,
            context.expected_url.as_deref(),
            "https://example.test/b",
            "https://example.test/b",
        ));

        context.ready_url = Some("https://example.test/b".to_string());
        assert!(context_is_ready_for_url(&context, "https://example.test/b"));
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
        let mut editable = target("textbox", "textarea", None);
        editable.content_editable = true;
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
        assert!(start.contains("window.location.href !== expectedUrl"));
        assert!(start.contains("performance.timeOrigin !== expectedIdentity"));
        assert!(start.contains("window.addEventListener(\"error\", onError, true)"));
        assert!(start.contains("window.removeEventListener(\"error\", onError, true)"));
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
        assert!(read.contains("window.location.href !=="));
        assert!(read.contains("performance.timeOrigin !=="));
        assert!(!read.contains("JSON.stringify"));
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
    fn evidence_callback_is_bounded_and_requires_ok() {
        let callback =
            parse_browser_evidence_callback(r#"{"ok":true,"events":[],"truncated":false}"#)
                .unwrap();
        assert!(callback.ok);
        assert!(
            parse_browser_evidence_callback(r#"{"ok":false,"events":[],"truncated":true}"#,)
                .is_err()
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
        super::expire_browser_evidence_capture(&state.evidence_captures, "c-test", "wrong-token");
        assert!(state
            .evidence_captures
            .lock()
            .unwrap()
            .contains_key("c-test"));
        super::expire_browser_evidence_capture(&state.evidence_captures, "c-test", "t-test");
        assert!(state.evidence_captures.lock().unwrap().is_empty());
        super::expire_browser_evidence_capture(&state.evidence_captures, "c-test", "t-test");
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
        assert!(script.contains("state.drain();"));
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
