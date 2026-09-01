//! Native, human-facing browser surface.
//!
//! This intentionally owns the child WebView in Rust. The renderer receives no
//! permission to create WebViews and remote pages never get access to DCC IPC.
//! The MVP keeps context in memory; durable browser metadata and automation are
//! separate follow-up features.

use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
#[serde(rename_all = "camelCase")]
pub struct BrowserActionAnchor {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub lifecycle_token: u64,
    pub map_id: String,
    pub generation: u64,
    pub url: String,
    pub page_load_revision: u64,
}

/// This is intentionally narrower than an MCP bridge: no item references,
/// selectors, HTML, form values, or arbitrary page script can be supplied.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BrowserControlAction {
    Navigate { url: String },
    Reload,
    Scroll { delta_x: f64, delta_y: f64 },
}

enum PreparedBrowserControlAction {
    Navigate(Url),
    Reload,
    Scroll { delta_x: f64, delta_y: f64 },
}

impl PreparedBrowserControlAction {
    fn name(&self) -> &'static str {
        match self {
            Self::Navigate(_) => "navigate",
            Self::Reload => "reload",
            Self::Scroll { .. } => "scroll",
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
}

#[derive(Debug, Clone)]
struct BrowserControlGrant {
    scope: String,
    lifecycle_token: u64,
    expires_at: Instant,
}

/// Runtime state for the one browser child WebView. Context is keyed by the
/// active workspace and optional session, while the native view is reused.
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
            operation_lock: Arc::new(AsyncMutex::new(())),
        }
    }
}

fn scope_key(workspace_id: &str, session_id: Option<&str>) -> String {
    format!("{}\u{1f}|{}", workspace_id, session_id.unwrap_or(""))
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
) -> (Vec<BrowserSemanticItem>, bool) {
    let mut truncated = extracted.truncated || extracted.items.len() > MAX_BROWSER_SEMANTIC_ITEMS;
    let mut items = Vec::new();
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
        let mut candidate_items = items.clone();
        candidate_items.push(item);
        if semantic_item_serialized_chars(&candidate_items) > max_serialized_chars {
            truncated = true;
            break;
        }
        items = candidate_items;
    }
    (items, truncated)
}

fn issue_semantic_map(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    url: &str,
    expected_page_load_revision: u64,
    items: Vec<BrowserSemanticItem>,
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

fn arm_browser_control_grant(
    state: &BrowserState,
    workspace_id: &str,
    session_id: Option<&str>,
    lifecycle_token: u64,
    now: Instant,
) -> Result<(), String> {
    let mut grant = state
        .control_grant
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())?;
    *grant = Some(BrowserControlGrant {
        scope: scope_key(workspace_id, session_id),
        lifecycle_token,
        expires_at: now + BROWSER_CONTROL_GRANT_TTL,
    });
    Ok(())
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
    }
}

/// Only finite, bounded numeric literals reach this fixed page-side script.
/// It has no selector, DOM traversal, input value access, or renderer-provided JS.
fn browser_scroll_script(delta_x: f64, delta_y: f64) -> String {
    format!("(() => {{ window.scrollBy({delta_x}, {delta_y}); }})()")
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

fn browser_context_script() -> String {
    // This code is initiated by the Rust command after scope validation. It
    // does not expose an IPC bridge or a callable DCC API to the remote page.
    format!(
        r#"(() => {{
  try {{
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
      const item = {{ role, name: accessibleName(element) }};
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
    let state_visible = state.visible.clone();
    let page_load_app = app.clone();
    let page_load_contexts = state.contexts.clone();
    let page_load_active_scope = state.active_scope.clone();
    let page_load_workspace = state.active_workspace.clone();
    let page_load_session = state.active_session.clone();
    let page_load_visible = state.visible.clone();
    let page_load_token = state.lifecycle_token.clone();
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
                mark_context_loading(contexts.entry(scope).or_default(), url.as_str().to_string());
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
    let desired = initial_url
        .or_else(|| context_url(&state, &workspace_id, session_id.as_deref()))
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let desired = validate_browser_url(&desired)?;
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
        build_browser(&app, &state, desired, bounds, initial_occluded)?;
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
) -> Result<(), String> {
    let _operation = state.operation_lock.lock().await;
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

/// Removes the temporary Browser-control capability without exposing a token.
#[tauri::command]
pub async fn browser_disarm_control(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<(), String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    require_current_lifecycle(
        &state,
        &workspace_id,
        session_id.as_deref(),
        lifecycle_token,
        "browser control lifecycle is stale",
    )?;
    clear_browser_control_grant(&state);
    Ok(())
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
    consume_browser_action_anchor(&state, &anchor)?;

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
            webview
                .eval(browser_scroll_script(delta_x, delta_y))
                .map_err(|_| "browser scroll action failed".to_string())?;
        }
    }

    Ok(BrowserActionResult {
        action: action_name,
        status: "executed".to_string(),
        requires_context_refresh: true,
    })
}

#[tauri::command]
pub async fn browser_extract_context(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserAgentContext, String> {
    let _operation = state.operation_lock.lock().await;
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
    let (semantic_items, semantic_truncated) =
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
        semantic_items,
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
        advance_lifecycle_token, arm_browser_control_grant, browser_action_anchor_matches_context,
        browser_scroll_script, clear_browser_control_grant, consume_semantic_map_for_anchor,
        context_is_ready_for_url, control_grant_is_current, mark_context_loading,
        normalize_browser_bounds, normalize_browser_context_text, normalize_browser_scroll_delta,
        normalize_browser_semantic_map, occlusion_request_is_current, page_load_matches_expected,
        page_load_revision_is_current, prepare_browser_control_action, require_action_native_url,
        sanitize_browser_link_destination, semantic_item_serialized_chars,
        semantic_map_record_is_current, should_persist_context_url, validate_browser_url,
        BrowserActionAnchor, BrowserBounds, BrowserContext, BrowserControlAction,
        BrowserControlGrant, BrowserSemanticItemExtraction, BrowserSemanticMap,
        BrowserSemanticMapExtraction, BrowserSemanticMapRecord, BrowserState,
        MAX_BROWSER_SEMANTIC_ITEMS,
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
                },
                BrowserSemanticItemExtraction {
                    role: "<script>evil</script>".to_string(),
                    name: "attempted schema injection".to_string(),
                    ..Default::default()
                },
            ],
        };
        let (items, truncated) = normalize_browser_semantic_map(extracted, 500);
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
        let (_, truncated) = normalize_browser_semantic_map(oversized, 5_000);
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
        let (items, truncated) = normalize_browser_semantic_map(extracted, 5_000);
        assert!(truncated);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].role, "heading");
        assert_eq!(items[0].level, Some(2));
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
}
