//! Native, human-facing browser surface.
//!
//! This intentionally owns the child WebView in Rust. The renderer receives no
//! permission to create WebViews and remote pages never get access to DCC IPC.
//! The MVP keeps context in memory; durable browser metadata and automation are
//! separate follow-up features.

use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::sync::{Arc, Mutex};

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
const BROWSER_CONTEXT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshot {
    pub workspace_id: String,
    pub session_id: Option<String>,
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserContextExtraction {
    text: String,
    selection_only: bool,
    truncated: bool,
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

fn browser_context_script() -> String {
    // This code is initiated by the Rust command after scope validation. It
    // does not expose an IPC bridge or a callable DCC API to the remote page.
    format!(
        r#"(() => {{
  try {{
    const maxChars = {MAX_BROWSER_CONTEXT_CHARS};
    const clean = (value) => String(value ?? "").replace(/\u0000/g, "").trim();
    const selection = clean(window.getSelection?.().toString());
    const visibleText = selection || clean(document.body?.innerText);
    return {{
      text: visibleText.slice(0, maxChars),
      selectionOnly: selection.length > 0,
      truncated: visibleText.length > maxChars,
    }};
  }} catch (_) {{
    return {{ text: "", selectionOnly: false, truncated: false }};
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
}

fn context_is_ready_for_url(context: &BrowserContext, url: &str) -> bool {
    context.expected_url.as_deref() == Some(url) && context.ready_url.as_deref() == Some(url)
}

fn should_persist_context_url(context: &BrowserContext, native_url: Option<&str>) -> bool {
    native_url.is_some_and(|url| context_is_ready_for_url(context, url))
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
    *state
        .webview
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = Some(webview);
    *state_visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = true;
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
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    let bounds = normalize_browser_bounds(bounds)?;
    let key = scope_key(&workspace_id, session_id.as_deref());
    let requested_url = initial_url.is_some();
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
    set_active_scope(&state, &workspace_id, session_id.as_deref())?;
    let desired = initial_url
        .or_else(|| context_url(&state, &workspace_id, session_id.as_deref()))
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let desired = validate_browser_url(&desired)?;
    let should_navigate = requested_url || previous_scope.as_deref() != Some(key.as_str());
    if should_navigate || !existing {
        if let Ok(mut contexts) = state.contexts.lock() {
            mark_context_loading(
                contexts.entry(key.clone()).or_default(),
                desired.to_string(),
            );
        }
    }
    if !existing {
        build_browser(&app, &state, desired, bounds)?;
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
            webview.show().map_err(|error| error.to_string())?;
        }
    }
    *state
        .visible
        .lock()
        .map_err(|_| "browser state lock poisoned".to_string())? = true;
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
    url: String,
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    let url = validate_browser_url(&url)?;
    require_active_scope(&state, &workspace_id, session_id.as_deref())?;
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
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    require_active_scope(&state, &workspace_id, session_id.as_deref())?;
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

#[tauri::command]
pub async fn browser_extract_context(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
) -> Result<BrowserAgentContext, String> {
    let _operation = state.operation_lock.lock().await;
    validate_scope(&sessions, &workspace_id, session_id.as_deref()).await?;
    require_active_scope(&state, &workspace_id, session_id.as_deref())?;
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
    require_active_scope(&state, &workspace_id, session_id.as_deref())?;
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
    let title = snapshot
        .title
        .map(|title| normalize_browser_context_text(&title, MAX_BROWSER_TITLE_CHARS).0);

    Ok(BrowserAgentContext {
        workspace_id,
        session_id,
        url: initial_url,
        title: title.filter(|title| !title.is_empty()),
        text,
        selection_only: extracted.selection_only,
        truncated: extracted.truncated || text_truncated,
    })
}

#[tauri::command]
pub async fn browser_set_bounds(
    state: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
    bounds: BrowserBounds,
) -> Result<(), String> {
    let _operation = state.operation_lock.lock().await;
    require_active_scope(&state, &workspace_id, session_id.as_deref())?;
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

#[tauri::command]
pub async fn browser_hide(
    app: AppHandle<Wry>,
    state: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
) -> Result<BrowserSnapshot, String> {
    let _operation = state.operation_lock.lock().await;
    require_active_scope(&state, &workspace_id, session_id.as_deref())?;
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
    use super::{
        context_is_ready_for_url, mark_context_loading, normalize_browser_bounds,
        normalize_browser_context_text, page_load_matches_expected, should_persist_context_url,
        validate_browser_url, BrowserBounds, BrowserContext,
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
}
