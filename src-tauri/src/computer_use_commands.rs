//! App-owned, target-scoped desktop computer use for macOS.
//!
//! This is intentionally separate from Browser control. A provider lease only
//! proves that DCC can offer tools to a session; it never grants OS input or
//! screen access. Each session must be explicitly armed with a short-lived,
//! in-memory allowlist of bundle identifiers.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{Emitter, State};
use tokio::sync::oneshot;
use tokio::time::timeout;

/// A grant ends after this long without a capture or action...
const GRANT_IDLE_TTL: Duration = Duration::from_secs(10 * 60);
/// ...and never outlives this, however active the agent is.
const GRANT_MAX_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_ALLOWED_APPS: usize = 16;
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_PREVIEW_SESSIONS: usize = 8;
const MAX_TYPE_CHARS: usize = 2_000;
/// Scroll deltas are pixels of content movement.
pub(crate) const MAX_SCROLL_DELTA: i32 = 5_000;
/// Mirrors the native element-map bound (`DCCTreeMaxRefs`).
pub(crate) const MAX_ELEMENT_REFS: usize = 800;
/// After an action the UI needs a moment to react before it is observed;
/// web content that reports itself busy gets more, up to the cap.
const ACTION_SETTLE: Duration = Duration::from_millis(350);
const ACTION_SETTLE_POLL: Duration = Duration::from_millis(400);
const ACTION_SETTLE_MAX: Duration = Duration::from_secs(3);
/// Chromium/Electron populate web content asynchronously after its
/// accessibility is first enabled.
const WEB_ACCESSIBILITY_WARMUP_POLL: Duration = Duration::from_millis(200);
const WEB_ACCESSIBILITY_WARMUP_ATTEMPTS: usize = 6;
const WEB_ACCESSIBILITY_RECONCILE_INTERVAL: Duration = Duration::from_secs(15);
const CONTROL_REQUEST_TTL: Duration = Duration::from_secs(45);
/// The MCP listener permits eight requests total. Four long-lived consent
/// waits leave capacity for normal Browser and Computer tools.
const MAX_PENDING_CONTROL_REQUESTS: usize = 4;
const MAX_CONTROL_REASON_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComputerTarget {
    pub bundle_id: String,
    pub name: String,
    pub pid: i32,
    pub window_id: u32,
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComputerTargetSnapshot {
    pub bundle_id: String,
    pub name: String,
    pub window_id: u32,
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub generation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseStatus {
    pub platform: &'static str,
    pub supported: bool,
    pub unsupported_reason: Option<&'static str>,
    pub minimum_macos_version: Option<u8>,
    pub accessibility: ComputerPermissionStatus,
    pub screen_recording: ComputerPermissionStatus,
    pub runtime_attached: bool,
    pub grant: ComputerGrantStatus,
    pub targets: Vec<ComputerTarget>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerPermissionStatus {
    pub granted: bool,
    pub can_request: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerGrantStatus {
    pub session_id: Option<String>,
    pub armed: bool,
    pub remaining_ms: u64,
    pub allowed_bundle_ids: Vec<String>,
}

/// Accessibility elements observed in the captured window. `text` lists one
/// element per line, `e<ref> role "name" … @x,y wxh`, in the screenshot's
/// window-local pixels. Refs are valid only with this capture's generation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerElements {
    pub available: bool,
    pub count: usize,
    pub truncated: bool,
    pub loading: bool,
    #[serde(skip)]
    pub text: String,
}

impl ComputerElements {
    fn unavailable() -> Self {
        Self {
            available: false,
            count: 0,
            truncated: false,
            loading: false,
            text: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerCapture {
    pub target: ComputerTargetSnapshot,
    pub mime_type: &'static str,
    pub image_base64: String,
    pub elements: ComputerElements,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointerButton {
    #[default]
    Left,
    Right,
}

/// Where a pointer action lands: an element observed by the capture, or a
/// window-local point in the screenshot.
#[derive(Debug, Clone, PartialEq)]
pub enum ComputerActionTarget {
    Element(String),
    Point { x: f64, y: f64 },
}

/// How an action reached the app, reported so the agent knows whether the
/// app was brought forward and the pointer moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComputerInputMethod {
    Accessibility,
    Pointer,
}

/// Latest locally captured frame for an explicitly authorized provider
/// session. It is separate from the MCP result so the renderer can preview it
/// without depending on provider-specific tool-call UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUsePreviewTarget {
    pub bundle_id: String,
    pub name: String,
    pub window_id: u32,
    pub title: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUsePreviewFrame {
    pub session_id: String,
    pub target: ComputerUsePreviewTarget,
    pub mime_type: &'static str,
    pub image_base64: String,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone)]
struct StoredPreviewFrame {
    frame: ComputerUsePreviewFrame,
    pid: i32,
    lease_id: String,
    revision: String,
}

struct PreviewRefreshGuard {
    sessions: Arc<Mutex<HashSet<String>>>,
    session_id: String,
}

impl PreviewRefreshGuard {
    fn acquire(sessions: &Arc<Mutex<HashSet<String>>>, session_id: &str) -> Option<Self> {
        if !sessions.lock().ok()?.insert(session_id.to_string()) {
            return None;
        }
        Some(Self {
            sessions: Arc::clone(sessions),
            session_id: session_id.to_string(),
        })
    }
}

impl Drop for PreviewRefreshGuard {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(&self.session_id);
        }
    }
}

/// A renderer-visible request created by an authenticated provider tool. This
/// intentionally contains no window metadata; the renderer discovers targets
/// locally only after the human decides to allow the request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingComputerControlRequest {
    pub request_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub provider_id: String,
    pub reason: String,
    pub remaining_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUsePendingRequestsOutput {
    pub requests: Vec<PendingComputerControlRequest>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseRespondControlRequestOutput {
    pub grant: ComputerGrantStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComputerAccessKind {
    Accessibility,
    ScreenRecording,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerUseStatusInput {
    pub session_id: Option<String>,
    #[serde(default)]
    pub include_targets: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerUseArmInput {
    pub session_id: String,
    pub allowed_bundle_ids: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerUseDisarmInput {
    pub session_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerUseRequestAccessInput {
    pub kind: ComputerAccessKind,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerUseRespondControlRequestInput {
    pub request_id: String,
    pub allowed: bool,
    #[serde(default)]
    pub allowed_bundle_ids: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerUsePreviewFrameInput {
    pub session_id: String,
}

/// Owns one native element map. The map is released when the last snapshot
/// referring to it is consumed, replaced, or dropped with its grant.
struct ElementMap {
    native: Arc<dyn ComputerNative>,
    handle: u64,
    count: usize,
}

impl std::fmt::Debug for ElementMap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ElementMap")
            .field("handle", &self.handle)
            .field("count", &self.count)
            .finish()
    }
}

impl Drop for ElementMap {
    fn drop(&mut self) {
        self.native.release_tree(self.handle);
    }
}

#[derive(Debug, Clone)]
struct CapturedSnapshot {
    visible: ComputerTargetSnapshot,
    identity: String,
    elements: Option<Arc<ElementMap>>,
}
#[derive(Debug, Clone)]
struct ComputerGrant {
    allowed_bundle_ids: HashSet<String>,
    granted_at: Instant,
    expires_at: Instant,
    lease_id: String,
    nonce: String,
    snapshots: HashMap<u32, CapturedSnapshot>,
}

impl ComputerGrant {
    /// Activity keeps a grant alive up to its hard limit; an idle agent
    /// loses it after `GRANT_IDLE_TTL`.
    fn touch(&mut self, now: Instant) {
        self.expires_at = (now + GRANT_IDLE_TTL).min(self.granted_at + GRANT_MAX_TTL);
    }
}

struct PendingControlRequest {
    request_id: String,
    session_id: String,
    workspace_id: String,
    provider_id: String,
    lease_id: String,
    reason: String,
    expires_at: Instant,
    decision: oneshot::Sender<ControlDecision>,
}

enum ControlDecision {
    Allowed {
        allowed_bundle_ids: Vec<String>,
        completion: oneshot::Sender<Result<ComputerGrantStatus, String>>,
    },
    Denied,
}

/// Removes a pending request if its long-polling MCP call is cancelled by the
/// provider or its HTTP connection. Dropping the sender wakes a concurrent UI
/// response without creating a grant.
struct PendingControlCleanup {
    requests: Arc<Mutex<HashMap<String, PendingControlRequest>>>,
    request_id: Option<String>,
}

impl PendingControlCleanup {
    fn new(
        requests: Arc<Mutex<HashMap<String, PendingControlRequest>>>,
        request_id: String,
    ) -> Self {
        Self {
            requests,
            request_id: Some(request_id),
        }
    }

    fn complete(&mut self) {
        self.request_id = None;
    }
}

impl Drop for PendingControlCleanup {
    fn drop(&mut self) {
        let Some(request_id) = self.request_id.take() else {
            return;
        };
        if let Ok(mut requests) = self.requests.lock() {
            requests.remove(&request_id);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NativeStatus {
    accessibility: bool,
    screen_recording: bool,
    major_version: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct NativeTree {
    available: bool,
    handle: u64,
    count: usize,
    text: String,
    truncated: bool,
    busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebAccessibility {
    Unsupported,
    AlreadyEnabled,
    Changed,
}

/// A resolved pointer location for the native bridge.
#[derive(Debug, Clone, Copy, PartialEq)]
enum NativeAt {
    Point { x: f64, y: f64 },
    Element { handle: u64, index: u32 },
}

/// Native effect failures, mirroring `DCC_COMPUTER_ERR_*` in the bridge header.
/// Every variant tells the agent what to do next instead of a bare failure.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NativeEffectError {
    Permission,
    Invalid,
    TargetUnavailable,
    Focus,
    Occluded(Option<String>),
    OutsideTarget,
    ElementStale,
    ElementOutside,
    Unsupported,
    Event,
    Other(String),
}

impl NativeEffectError {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn from_code(code: i32, detail: Option<String>) -> Self {
        match code {
            1 => Self::Permission,
            2 => Self::Invalid,
            3 => Self::TargetUnavailable,
            4 => Self::Focus,
            5 => Self::Occluded(detail.filter(|name| !name.trim().is_empty())),
            6 => Self::OutsideTarget,
            7 => Self::ElementStale,
            8 => Self::ElementOutside,
            9 => Self::Unsupported,
            10 => Self::Event,
            _ => Self::Other(format!("desktop input failed with native status {code}")),
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Permission => "DCC lacks the macOS Accessibility permission; ask the user to enable it in System Settings > Privacy & Security > Accessibility".to_string(),
            Self::Invalid => "the native input bridge rejected these arguments".to_string(),
            Self::TargetUnavailable => "the target window moved, resized, or closed; capture it again".to_string(),
            Self::Focus => "macOS did not bring the target window to the front with keyboard focus; a modal dialog, a locked screen, or a full-screen app may be in the way, so tell the user what you need".to_string(),
            Self::Occluded(owner) => format!(
                "{} covers that point of the target window; pick a visible point or ask the user to move it",
                owner.as_deref().unwrap_or("another window")
            ),
            Self::OutsideTarget => "that point is outside the target window".to_string(),
            Self::ElementStale => "that element no longer exists; use a ref from the latest capture".to_string(),
            Self::ElementOutside => "that element is not visible inside the target window; scroll it into view first".to_string(),
            Self::Unsupported => "that element does not accept this action; click it and type instead".to_string(),
            Self::Event => "macOS rejected the synthesized input event".to_string(),
            Self::Other(message) => message.clone(),
        }
    }
}

trait ComputerNative: Send + Sync {
    fn status(&self) -> NativeStatus;
    fn request_access(&self, kind: ComputerAccessKind) -> bool;
    fn targets(&self) -> Result<Vec<ComputerTarget>, String>;
    fn capture(&self, target: &ComputerTarget) -> Result<Vec<u8>, String>;
    fn accessibility_tree(&self, target: &ComputerTarget) -> Result<NativeTree, String>;
    fn release_tree(&self, handle: u64);
    fn set_web_accessibility(&self, pid: i32, enabled: bool) -> WebAccessibility;
    fn click(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        button: PointerButton,
        click_count: u8,
    ) -> Result<ComputerInputMethod, NativeEffectError>;
    fn scroll(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), NativeEffectError>;
    fn drag(
        &self,
        target: &ComputerTarget,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), NativeEffectError>;
    fn set_value(
        &self,
        target: &ComputerTarget,
        handle: u64,
        index: u32,
        value: &str,
    ) -> Result<(), NativeEffectError>;
    fn type_text(&self, target: &ComputerTarget, text: &str) -> Result<(), NativeEffectError>;
    fn key(
        &self,
        target: &ComputerTarget,
        key_code: u16,
        flags: u64,
    ) -> Result<(), NativeEffectError>;
}

#[derive(Clone)]
pub struct ComputerUseState {
    native: Arc<dyn ComputerNative>,
    grants: Arc<Mutex<HashMap<String, ComputerGrant>>>,
    projected_sessions: Arc<Mutex<HashMap<String, String>>>,
    pending_control_requests: Arc<Mutex<HashMap<String, PendingControlRequest>>>,
    preview_frames: Arc<Mutex<HashMap<String, StoredPreviewFrame>>>,
    preview_refreshing: Arc<Mutex<HashSet<String>>>,
    /// Processes whose web accessibility DCC switched on, by bundle id. It is
    /// switched off again once no live grant allows that bundle.
    web_accessibility: Arc<Mutex<HashMap<i32, WebAccessibilityEntry>>>,
    web_accessibility_watcher: Arc<AtomicBool>,
}

impl Default for ComputerUseState {
    fn default() -> Self {
        Self::with_native(Arc::new(PlatformComputerNative))
    }
}

impl ComputerUseState {
    fn with_native(native: Arc<dyn ComputerNative>) -> Self {
        Self {
            native,
            grants: Arc::new(Mutex::new(HashMap::new())),
            projected_sessions: Arc::new(Mutex::new(HashMap::new())),
            pending_control_requests: Arc::new(Mutex::new(HashMap::new())),
            preview_frames: Arc::new(Mutex::new(HashMap::new())),
            preview_refreshing: Arc::new(Mutex::new(HashSet::new())),
            web_accessibility: Arc::new(Mutex::new(HashMap::new())),
            web_accessibility_watcher: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(test)]
    pub(crate) fn supported_test_state() -> Self {
        Self::with_native(Arc::new(SupportedTestNative))
    }

    /// Called only by the authenticated MCP projector once it issued a
    /// provider lease. It is deliberately not a renderer-callable operation.
    pub(crate) fn record_projection(&self, session_id: &str, lease_id: &str) {
        if let Ok(mut sessions) = self.projected_sessions.lock() {
            sessions.insert(session_id.to_string(), lease_id.to_string());
        }
        if let Ok(mut grants) = self.grants.lock() {
            grants.remove(session_id);
        }
        self.clear_preview_frame(session_id);
        self.cancel_control_requests_for_session(session_id, None);
        self.reconcile_web_accessibility();
    }
    pub(crate) fn revoke_projection(&self, session_id: &str, lease_id: &str) {
        let revoked = self.projected_sessions.lock().is_ok_and(|mut sessions| {
            if sessions
                .get(session_id)
                .is_some_and(|current| current == lease_id)
            {
                sessions.remove(session_id);
                true
            } else {
                false
            }
        });
        if revoked {
            if let Ok(mut grants) = self.grants.lock() {
                grants.remove(session_id);
            }
            self.cancel_control_requests_for_session(session_id, Some(lease_id));
            self.clear_preview_frame(session_id);
            self.reconcile_web_accessibility();
        }
    }
    pub(crate) fn revoke_all(&self) {
        if let Ok(mut sessions) = self.projected_sessions.lock() {
            sessions.clear();
        }
        if let Ok(mut grants) = self.grants.lock() {
            grants.clear();
        }
        if let Ok(mut requests) = self.pending_control_requests.lock() {
            requests.clear();
        }
        if let Ok(mut frames) = self.preview_frames.lock() {
            frames.clear();
        }
        self.reconcile_web_accessibility();
    }
    fn is_projected(&self, session_id: &str) -> bool {
        self.projected_sessions
            .lock()
            .is_ok_and(|sessions| sessions.contains_key(session_id))
    }
    fn projection_lease(&self, session_id: &str) -> Option<String> {
        self.projected_sessions
            .lock()
            .ok()?
            .get(session_id)
            .cloned()
    }

    fn clear_preview_frame(&self, session_id: &str) {
        if let Ok(mut frames) = self.preview_frames.lock() {
            frames.remove(session_id);
        }
    }

    pub(crate) fn preview_frame(&self, session_id: &str) -> Option<ComputerUsePreviewFrame> {
        self.authorized_preview_frame(session_id)
            .map(|stored| stored.frame)
    }

    fn authorized_preview_frame(&self, session_id: &str) -> Option<StoredPreviewFrame> {
        let current_lease = self.projection_lease(session_id);
        let now = Instant::now();
        let authorized = current_lease.as_ref().is_some_and(|lease_id| {
            self.grants.lock().is_ok_and(|mut grants| {
                prune_expired(&mut grants);
                grants
                    .get(session_id)
                    .is_some_and(|grant| grant.lease_id == *lease_id && grant.expires_at > now)
            })
        });
        if !authorized {
            self.clear_preview_frame(session_id);
            return None;
        }
        let stored = self.preview_frames.lock().ok()?.get(session_id).cloned()?;
        let still_allowed = self.grants.lock().is_ok_and(|mut grants| {
            prune_expired(&mut grants);
            grants.get(session_id).is_some_and(|grant| {
                current_lease.as_ref() == Some(&grant.lease_id)
                    && grant.lease_id == stored.lease_id
                    && grant
                        .allowed_bundle_ids
                        .contains(&stored.frame.target.bundle_id)
            })
        });
        if !still_allowed {
            self.clear_preview_frame(session_id);
            return None;
        }
        Some(stored)
    }

    /// Refreshes only the window the provider already captured. This local
    /// preview does not mint an action generation or send an image to the model.
    pub(crate) fn refresh_preview_frame(
        &self,
        session_id: &str,
    ) -> Option<ComputerUsePreviewFrame> {
        let _in_flight = PreviewRefreshGuard::acquire(&self.preview_refreshing, session_id)?;
        let stored = self.authorized_preview_frame(session_id)?;
        let lease_id = self.projection_lease(session_id)?;
        let nonce = self
            .capture_grant(session_id, &lease_id, &stored.frame.target.bundle_id)
            .ok()?;
        let target = self.native.targets().ok()?.into_iter().find(|item| {
            item.window_id == stored.frame.target.window_id
                && item.bundle_id == stored.frame.target.bundle_id
                && item.pid == stored.pid
        });
        let Some(target) = target else {
            if let Ok(mut frames) = self.preview_frames.lock() {
                if frames
                    .get(session_id)
                    .is_some_and(|current| current.revision == stored.revision)
                {
                    frames.remove(session_id);
                }
            }
            return None;
        };
        let bytes = self.native.capture(&target).ok()?;
        if bytes.len() > MAX_CAPTURE_BYTES || !self.grant_is_current(session_id, &lease_id, &nonce)
        {
            return None;
        }
        let mut frames = self.preview_frames.lock().ok()?;
        let current = frames.get_mut(session_id)?;
        if current.lease_id != lease_id
            || current.pid != stored.pid
            || current.revision != stored.revision
        {
            return None;
        }
        current.frame.image_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        current.frame.captured_at_ms = preview_timestamp_ms();
        current.frame.target.name = target.name;
        current.frame.target.title = target.title;
        current.frame.target.width = target.width;
        current.frame.target.height = target.height;
        current.revision = random_token();
        drop(frames);
        self.preview_frame(session_id)
    }
    pub(crate) fn lease_matches(&self, session_id: &str, lease_id: &str) -> bool {
        self.projected_sessions.lock().is_ok_and(|sessions| {
            sessions
                .get(session_id)
                .is_some_and(|current| current == lease_id)
        })
    }

    fn supported(&self) -> bool {
        self.native.status().major_version >= 14
    }
    fn permission_status(&self) -> (ComputerPermissionStatus, ComputerPermissionStatus) {
        let native = self.native.status();
        let requestable = self.supported();
        (
            ComputerPermissionStatus {
                granted: native.accessibility,
                can_request: requestable && !native.accessibility,
            },
            ComputerPermissionStatus {
                granted: native.screen_recording,
                can_request: requestable && !native.screen_recording,
            },
        )
    }
    fn grant_status(&self, session_id: Option<&str>) -> ComputerGrantStatus {
        let now = Instant::now();
        let mut grants = match self.grants.lock() {
            Ok(grants) => grants,
            Err(_) => {
                return ComputerGrantStatus {
                    session_id: None,
                    armed: false,
                    remaining_ms: 0,
                    allowed_bundle_ids: vec![],
                }
            }
        };
        grants.retain(|_, grant| grant.expires_at > now);
        let Some(session_id) = session_id else {
            return ComputerGrantStatus {
                session_id: None,
                armed: false,
                remaining_ms: 0,
                allowed_bundle_ids: vec![],
            };
        };
        let Some(grant) = grants.get(session_id) else {
            return ComputerGrantStatus {
                session_id: Some(session_id.to_string()),
                armed: false,
                remaining_ms: 0,
                allowed_bundle_ids: vec![],
            };
        };
        if self.projection_lease(session_id).as_deref() != Some(grant.lease_id.as_str()) {
            grants.remove(session_id);
            return ComputerGrantStatus {
                session_id: Some(session_id.to_string()),
                armed: false,
                remaining_ms: 0,
                allowed_bundle_ids: vec![],
            };
        }
        let mut allowed_bundle_ids = grant.allowed_bundle_ids.iter().cloned().collect::<Vec<_>>();
        allowed_bundle_ids.sort();
        ComputerGrantStatus {
            session_id: Some(session_id.to_string()),
            armed: true,
            remaining_ms: u64::try_from(
                grant.expires_at.saturating_duration_since(now).as_millis(),
            )
            .unwrap_or(u64::MAX),
            allowed_bundle_ids,
        }
    }
    pub(crate) fn status(
        &self,
        session_id: Option<&str>,
        include_targets: bool,
    ) -> ComputerUseStatus {
        let (accessibility, screen_recording) = self.permission_status();
        let supported = self.supported();
        let runtime_attached = session_id.is_some_and(|id| self.is_projected(id));
        let grant = self.grant_status(session_id);
        let targets = if include_targets && supported {
            self.native.targets().unwrap_or_default()
        } else {
            vec![]
        };
        ComputerUseStatus {
            platform: if cfg!(target_os = "macos") {
                "macos"
            } else {
                "unsupported"
            },
            supported,
            unsupported_reason: (!supported).then_some(if cfg!(target_os = "macos") {
                "macOS 14 or later is required"
            } else {
                "Desktop computer use is currently available only on macOS"
            }),
            minimum_macos_version: cfg!(target_os = "macos").then_some(14),
            accessibility,
            screen_recording,
            runtime_attached,
            grant,
            targets,
        }
    }
    /// The MCP view must never enumerate arbitrary applications. Once the
    /// human enables a session grant it exposes only windows owned by the
    /// explicit allowlist, which gives the provider usable capture targets.
    pub(crate) fn agent_status(&self, session_id: &str, lease_id: &str) -> ComputerUseStatus {
        if !self.lease_matches(session_id, lease_id) {
            return self.status(Some(session_id), false);
        }
        let mut status = self.status(Some(session_id), false);
        if status.grant.armed {
            let allowed = status
                .grant
                .allowed_bundle_ids
                .iter()
                .collect::<HashSet<_>>();
            status.targets = self
                .native
                .targets()
                .unwrap_or_default()
                .into_iter()
                .filter(|target| allowed.contains(&target.bundle_id))
                .collect();
        }
        status
    }
    pub(crate) fn arm(
        &self,
        session_id: &str,
        bundle_ids: &[String],
    ) -> Result<ComputerGrantStatus, String> {
        let lease_id = self.projection_lease(session_id).ok_or_else(|| {
            "desktop computer use is unavailable for this provider session".to_string()
        })?;
        self.arm_for_lease(session_id, &lease_id, bundle_ids)
    }

    /// The request-control responder owns an already authenticated lease.
    /// Keeping that lease explicit prevents an old dialog response from
    /// granting a replacement provider session.
    fn arm_for_lease(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        bundle_ids: &[String],
    ) -> Result<ComputerGrantStatus, String> {
        if !self.supported() {
            return Err(
                "desktop computer use is unsupported on this platform or macOS version".to_string(),
            );
        }
        if !self.lease_matches(session_id, expected_lease_id) {
            return Err("desktop computer use provider session changed while enabling".to_string());
        }
        let (accessibility, screen_recording) = self.permission_status();
        if !accessibility.granted || !screen_recording.granted {
            return Err("Accessibility and Screen Recording permission are required before enabling desktop computer use".to_string());
        }
        let allowed = normalized_bundle_ids(bundle_ids)?;
        let available = self.native.targets()?;
        if allowed
            .iter()
            .any(|bundle| !available.iter().any(|target| &target.bundle_id == bundle))
        {
            return Err("an allowed target application is not currently available".to_string());
        }
        if !self.lease_matches(session_id, expected_lease_id) {
            return Err("desktop computer use provider session changed while enabling".to_string());
        }
        let mut grants = self
            .grants
            .lock()
            .map_err(|_| "desktop computer use state is unavailable".to_string())?;
        if !self.lease_matches(session_id, expected_lease_id) {
            return Err("desktop computer use provider session changed while enabling".to_string());
        }
        let now = Instant::now();
        grants.insert(
            session_id.to_string(),
            ComputerGrant {
                allowed_bundle_ids: allowed,
                granted_at: now,
                expires_at: now + GRANT_IDLE_TTL,
                lease_id: expected_lease_id.to_string(),
                nonce: random_token(),
                snapshots: HashMap::new(),
            },
        );
        drop(grants);
        self.clear_preview_frame(session_id);
        self.reconcile_web_accessibility();
        Ok(self.grant_status(Some(session_id)))
    }
    pub(crate) fn disarm(&self, session_id: &str) -> ComputerGrantStatus {
        if let Ok(mut grants) = self.grants.lock() {
            grants.remove(session_id);
        }
        self.clear_preview_frame(session_id);
        self.reconcile_web_accessibility();
        self.grant_status(Some(session_id))
    }
    pub(crate) fn request_access(
        &self,
        kind: ComputerAccessKind,
    ) -> Result<ComputerUseStatus, String> {
        if !self.supported() {
            return Err(
                "desktop computer use is unsupported on this platform or macOS version".to_string(),
            );
        }
        if !self.native.request_access(kind) {
            return Err(match kind {
                ComputerAccessKind::Accessibility => {
                    "could not initiate the macOS Accessibility permission request".to_string()
                }
                ComputerAccessKind::ScreenRecording => {
                    "could not initiate the macOS Screen Recording permission request".to_string()
                }
            });
        }
        Ok(self.status(None, false))
    }

    /// Creates one bounded, lease-bound approval request for an agent. A
    /// grant is created only after the renderer allows it and this same MCP
    /// call is still alive to receive that decision.
    pub(crate) async fn request_control(
        &self,
        session_id: &str,
        workspace_id: &str,
        provider_id: &str,
        expected_lease_id: &str,
        reason: &str,
    ) -> Result<ComputerUseStatus, String> {
        if !self.supported() {
            return Err(
                "desktop computer use is unsupported on this platform or macOS version".to_string(),
            );
        }
        if !self.lease_matches(session_id, expected_lease_id) {
            return Err(
                "desktop computer use is unavailable for this provider session".to_string(),
            );
        }
        if self.agent_status(session_id, expected_lease_id).grant.armed {
            return Ok(self.agent_status(session_id, expected_lease_id));
        }
        let reason = normalized_control_reason(reason)?;
        let request_id = random_token();
        let (decision_tx, decision_rx) = oneshot::channel();
        {
            let mut requests = self
                .pending_control_requests
                .lock()
                .map_err(|_| "desktop computer use state is unavailable".to_string())?;
            prune_expired_control_requests(&mut requests);
            if !self.lease_matches(session_id, expected_lease_id) {
                return Err(
                    "desktop computer use is unavailable for this provider session".to_string(),
                );
            }
            if requests.values().any(|request| {
                request.session_id == session_id && request.lease_id == expected_lease_id
            }) {
                return Err(
                    "a desktop computer-use approval request is already pending".to_string()
                );
            }
            if requests.len() >= MAX_PENDING_CONTROL_REQUESTS {
                return Err(
                    "too many desktop computer-use approval requests are pending".to_string(),
                );
            }
            requests.insert(
                request_id.clone(),
                PendingControlRequest {
                    request_id: request_id.clone(),
                    session_id: session_id.to_string(),
                    workspace_id: workspace_id.to_string(),
                    provider_id: provider_id.to_string(),
                    lease_id: expected_lease_id.to_string(),
                    reason,
                    expires_at: Instant::now() + CONTROL_REQUEST_TTL,
                    decision: decision_tx,
                },
            );
        }
        let mut cleanup = PendingControlCleanup::new(
            Arc::clone(&self.pending_control_requests),
            request_id.clone(),
        );
        let decision = match timeout(CONTROL_REQUEST_TTL, decision_rx).await {
            Ok(Ok(decision)) => decision,
            Ok(Err(_)) => {
                return Err(
                    "desktop computer-use approval request is no longer available".to_string(),
                )
            }
            Err(_) => return Err("desktop computer-use approval request timed out".to_string()),
        };
        cleanup.complete();
        match decision {
            ControlDecision::Denied => {
                Err("desktop computer-use approval request was denied".to_string())
            }
            ControlDecision::Allowed {
                allowed_bundle_ids,
                completion,
            } => {
                let result = self.arm_for_lease(session_id, expected_lease_id, &allowed_bundle_ids);
                let _ = completion.send(result.clone());
                result?;
                if !self.lease_matches(session_id, expected_lease_id) {
                    self.disarm_for_lease(session_id, expected_lease_id);
                    return Err(
                        "desktop computer use provider session changed while enabling".to_string(),
                    );
                }
                Ok(self.agent_status(session_id, expected_lease_id))
            }
        }
    }

    pub(crate) fn pending_control_requests(&self) -> ComputerUsePendingRequestsOutput {
        let now = Instant::now();
        let mut requests = match self.pending_control_requests.lock() {
            Ok(requests) => requests,
            Err(_) => return ComputerUsePendingRequestsOutput { requests: vec![] },
        };
        prune_expired_control_requests(&mut requests);
        let mut visible = requests
            .values()
            .map(|request| PendingComputerControlRequest {
                request_id: request.request_id.clone(),
                session_id: request.session_id.clone(),
                workspace_id: request.workspace_id.clone(),
                provider_id: request.provider_id.clone(),
                reason: request.reason.clone(),
                remaining_ms: u64::try_from(
                    request
                        .expires_at
                        .saturating_duration_since(now)
                        .as_millis(),
                )
                .unwrap_or(u64::MAX),
            })
            .collect::<Vec<_>>();
        visible.sort_by(|left, right| {
            let left_remaining = left.remaining_ms;
            let right_remaining = right.remaining_ms;
            left_remaining
                .cmp(&right_remaining)
                .then_with(|| left.request_id.cmp(&right.request_id))
        });
        ComputerUsePendingRequestsOutput { requests: visible }
    }

    pub(crate) async fn respond_control_request(
        &self,
        request_id: &str,
        allowed: bool,
        allowed_bundle_ids: &[String],
    ) -> Result<ComputerUseRespondControlRequestOutput, String> {
        if request_id.len() != 32 || !request_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("desktop computer-use approval request is invalid".to_string());
        }
        let request = self
            .pending_control_requests
            .lock()
            .map_err(|_| "desktop computer use state is unavailable".to_string())?
            .remove(request_id)
            .ok_or_else(|| {
                "desktop computer-use approval request is no longer available".to_string()
            })?;
        if request.expires_at <= Instant::now() {
            return Err("desktop computer-use approval request timed out".to_string());
        }
        if !self.lease_matches(&request.session_id, &request.lease_id) {
            return Err("desktop computer use provider session changed".to_string());
        }
        if !allowed {
            let _ = request.decision.send(ControlDecision::Denied);
            return Ok(ComputerUseRespondControlRequestOutput {
                grant: self.grant_status(Some(&request.session_id)),
            });
        }
        let allowed_bundle_ids = normalized_bundle_ids(allowed_bundle_ids)?
            .into_iter()
            .collect::<Vec<_>>();
        let (completion_tx, completion_rx) = oneshot::channel();
        request
            .decision
            .send(ControlDecision::Allowed {
                allowed_bundle_ids,
                completion: completion_tx,
            })
            .map_err(|_| {
                "desktop computer-use approval request is no longer available".to_string()
            })?;
        let grant = timeout(Duration::from_secs(5), completion_rx)
            .await
            .map_err(|_| "desktop computer-use approval did not complete".to_string())?
            .map_err(|_| {
                "desktop computer-use approval request is no longer available".to_string()
            })??;
        if !self.lease_matches(&request.session_id, &request.lease_id) {
            self.disarm_for_lease(&request.session_id, &request.lease_id);
            return Err("desktop computer use provider session changed while enabling".to_string());
        }
        Ok(ComputerUseRespondControlRequestOutput { grant })
    }

    fn cancel_control_requests_for_session(&self, session_id: &str, lease_id: Option<&str>) {
        if let Ok(mut requests) = self.pending_control_requests.lock() {
            requests.retain(|_, request| {
                request.session_id != session_id
                    || lease_id.is_some_and(|lease_id| request.lease_id != lease_id)
            });
        }
    }

    fn disarm_for_lease(&self, session_id: &str, expected_lease_id: &str) {
        if let Ok(mut grants) = self.grants.lock() {
            if grants
                .get(session_id)
                .is_some_and(|grant| grant.lease_id == expected_lease_id)
            {
                grants.remove(session_id);
            }
        }
        self.reconcile_web_accessibility();
    }

    /// Consumes the one-shot snapshot an action names and proves the window
    /// still has the captured identity. The returned element map stays alive
    /// until the action finishes.
    fn require_target(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
    ) -> Result<(ComputerTarget, Option<Arc<ElementMap>>), String> {
        if !self.supported() {
            return Err(
                "desktop computer use is unsupported on this platform or macOS version".to_string(),
            );
        }
        let (nonce, expected) = self.snapshot_for_action(session_id, expected_lease_id, target)?;
        let current = self
            .native
            .targets()?
            .into_iter()
            .find(|item| item.window_id == target.window_id && item.bundle_id == target.bundle_id)
            .ok_or_else(|| "target window is no longer available".to_string())?;
        if target_identity(&current) != expected.identity {
            return Err(
                "target window moved, resized, or was replaced; capture it again before acting"
                    .to_string(),
            );
        }
        // Do not hold the grant lock while enumerating windows. This second
        // read closes disarm/re-arm races before the native driver receives
        // an event; the native bridge performs one final target check too.
        let mut elements = None;
        let valid = self.grants.lock().is_ok_and(|mut grants| {
            prune_expired(&mut grants);
            grants.get_mut(session_id).is_some_and(|grant| {
                if grant.nonce != nonce
                    || grant.lease_id != expected_lease_id
                    || self.projection_lease(session_id).as_deref() != Some(expected_lease_id)
                {
                    return false;
                }
                match grant.snapshots.remove(&target.window_id) {
                    Some(snapshot) if snapshot.visible.generation == target.generation => {
                        elements = snapshot.elements;
                        true
                    }
                    _ => false,
                }
            })
        });
        if !valid {
            return Err(
                "desktop computer use grant changed; capture again before acting".to_string(),
            );
        }
        Ok((current, elements))
    }

    pub(crate) fn capture(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        bundle_id: &str,
        window_id: u32,
    ) -> Result<ComputerCapture, String> {
        let nonce = self.capture_grant(session_id, expected_lease_id, bundle_id)?;
        let target = self
            .native
            .targets()?
            .into_iter()
            .find(|item| item.window_id == window_id && item.bundle_id == bundle_id)
            .ok_or_else(|| "target window is no longer available".to_string())?;
        if !self.grant_is_current(session_id, expected_lease_id, &nonce) {
            return Err(
                "desktop computer use grant changed while resolving the target".to_string(),
            );
        }
        let bytes = self.native.capture(&target)?;
        if bytes.len() > MAX_CAPTURE_BYTES {
            return Err("captured image exceeds the desktop computer-use size limit".to_string());
        }
        // Observe elements right after the pixels so refs and screenshot
        // describe the same moment.
        let (elements, element_map) = self.observe_elements(&target);
        let snapshot = ComputerTargetSnapshot {
            bundle_id: target.bundle_id.clone(),
            name: target.name.clone(),
            window_id: target.window_id,
            title: target.title.clone(),
            width: target.width,
            height: target.height,
            generation: random_token(),
        };
        let accepted = self.grants.lock().is_ok_and(|mut grants| {
            prune_expired(&mut grants);
            let Some(grant) = grants.get_mut(session_id) else {
                return false;
            };
            if grant.nonce != nonce
                || grant.lease_id != expected_lease_id
                || !grant.allowed_bundle_ids.contains(bundle_id)
                || self.projection_lease(session_id).as_deref() != Some(expected_lease_id)
            {
                return false;
            }
            grant.snapshots.insert(
                window_id,
                CapturedSnapshot {
                    visible: snapshot.clone(),
                    identity: target_identity(&target),
                    elements: element_map,
                },
            );
            grant.touch(Instant::now());
            true
        });
        if !accepted {
            return Err("desktop computer use grant changed while capturing".to_string());
        }
        let image_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        let preview_frame = ComputerUsePreviewFrame {
            session_id: session_id.to_string(),
            target: ComputerUsePreviewTarget {
                bundle_id: snapshot.bundle_id.clone(),
                name: snapshot.name.clone(),
                window_id: snapshot.window_id,
                title: snapshot.title.clone(),
                width: snapshot.width,
                height: snapshot.height,
            },
            mime_type: "image/png",
            image_base64: image_base64.clone(),
            captured_at_ms: preview_timestamp_ms(),
        };
        if let Ok(mut frames) = self.preview_frames.lock() {
            if frames.len() >= MAX_PREVIEW_SESSIONS && !frames.contains_key(session_id) {
                if let Some(oldest_session) = frames
                    .iter()
                    .min_by_key(|(_, stored)| stored.frame.captured_at_ms)
                    .map(|(id, _)| id.clone())
                {
                    frames.remove(&oldest_session);
                }
            }
            frames.insert(
                session_id.to_string(),
                StoredPreviewFrame {
                    frame: preview_frame,
                    pid: target.pid,
                    lease_id: expected_lease_id.to_string(),
                    revision: random_token(),
                },
            );
        }
        Ok(ComputerCapture {
            target: snapshot,
            mime_type: "image/png",
            image_base64,
            elements,
        })
    }

    /// Observes the acted-on window again, so the agent sees the effect and
    /// gets a fresh generation in the same round trip. Web content that
    /// reports itself busy is given a bounded extra moment to settle.
    pub(crate) fn observe_after_action(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        bundle_id: &str,
        window_id: u32,
    ) -> Result<ComputerCapture, String> {
        std::thread::sleep(ACTION_SETTLE);
        let started = Instant::now();
        loop {
            let capture = self.capture(session_id, expected_lease_id, bundle_id, window_id)?;
            if !capture.elements.loading || started.elapsed() + ACTION_SETTLE_POLL > ACTION_SETTLE_MAX
            {
                return Ok(capture);
            }
            std::thread::sleep(ACTION_SETTLE_POLL);
        }
    }

    /// A failed tree read never fails the capture: the screenshot alone still
    /// supports pointer actions by coordinates.
    fn observe_elements(
        &self,
        target: &ComputerTarget,
    ) -> (ComputerElements, Option<Arc<ElementMap>>) {
        let warming_up = self.enable_web_accessibility(target);
        let Ok(mut tree) = self.native.accessibility_tree(target) else {
            return (ComputerElements::unavailable(), None);
        };
        let mut map = self.element_map(&tree);
        if warming_up {
            for _ in 0..WEB_ACCESSIBILITY_WARMUP_ATTEMPTS {
                std::thread::sleep(WEB_ACCESSIBILITY_WARMUP_POLL);
                let Ok(next) = self.native.accessibility_tree(target) else {
                    break;
                };
                let grew = next.count > tree.count;
                map = self.element_map(&next);
                tree = next;
                if !grew && tree.count > 0 {
                    break;
                }
            }
        }
        (
            ComputerElements {
                available: tree.available,
                count: map.as_ref().map_or(0, |map| map.count),
                truncated: tree.truncated,
                loading: tree.busy,
                text: tree.text,
            },
            map,
        )
    }

    /// Takes ownership of the tree's native handle; an empty map is released
    /// at once, so refs only ever exist for listed elements.
    fn element_map(&self, tree: &NativeTree) -> Option<Arc<ElementMap>> {
        if tree.handle == 0 {
            return None;
        }
        let map = Arc::new(ElementMap {
            native: Arc::clone(&self.native),
            handle: tree.handle,
            count: tree.count.min(MAX_ELEMENT_REFS),
        });
        (map.count > 0).then_some(map)
    }

    /// Returns true only the first time DCC switches a process's web
    /// accessibility on, when its tree still needs to populate.
    fn enable_web_accessibility(&self, target: &ComputerTarget) -> bool {
        let Ok(mut entries) = self.web_accessibility.lock() else {
            return false;
        };
        if entries
            .get(&target.pid)
            .is_some_and(|entry| entry.bundle_id == target.bundle_id)
        {
            return false;
        }
        let owned = self.native.set_web_accessibility(target.pid, true) == WebAccessibility::Changed;
        entries.insert(
            target.pid,
            WebAccessibilityEntry {
                bundle_id: target.bundle_id.clone(),
                owned,
            },
        );
        drop(entries);
        if owned {
            self.start_web_accessibility_watcher();
        }
        owned
    }

    /// Switches web accessibility back off for every process DCC switched on
    /// whose bundle no live grant allows any more.
    fn reconcile_web_accessibility(&self) {
        let live = self
            .grants
            .lock()
            .map(|mut grants| {
                prune_expired(&mut grants);
                grants
                    .values()
                    .flat_map(|grant| grant.allowed_bundle_ids.iter().cloned())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let mut released = Vec::new();
        if let Ok(mut entries) = self.web_accessibility.lock() {
            entries.retain(|pid, entry| {
                let keep = live.contains(&entry.bundle_id);
                if !keep && entry.owned {
                    released.push(*pid);
                }
                keep
            });
        }
        for pid in released {
            let _ = self.native.set_web_accessibility(pid, false);
        }
    }

    fn owns_web_accessibility(&self) -> bool {
        self.web_accessibility
            .lock()
            .is_ok_and(|entries| entries.values().any(|entry| entry.owned))
    }

    /// Grants also end by expiry, which nothing else observes. One background
    /// thread reconciles while DCC owns any process's web accessibility.
    fn start_web_accessibility_watcher(&self) {
        if self.web_accessibility_watcher.swap(true, Ordering::AcqRel) {
            return;
        }
        let state = self.clone();
        let spawned = std::thread::Builder::new()
            .name("dcc-computer-use-accessibility".to_string())
            .spawn(move || loop {
                std::thread::sleep(WEB_ACCESSIBILITY_RECONCILE_INTERVAL);
                state.reconcile_web_accessibility();
                if state.owns_web_accessibility() {
                    continue;
                }
                state
                    .web_accessibility_watcher
                    .store(false, Ordering::Release);
                // An entry added between the check and the store must not be
                // left without a watcher.
                if !state.owns_web_accessibility()
                    || state.web_accessibility_watcher.swap(true, Ordering::AcqRel)
                {
                    break;
                }
            });
        if spawned.is_err() {
            self.web_accessibility_watcher
                .store(false, Ordering::Release);
        }
    }

    fn touch_grant(&self, session_id: &str, expected_lease_id: &str) {
        if let Ok(mut grants) = self.grants.lock() {
            prune_expired(&mut grants);
            if let Some(grant) = grants
                .get_mut(session_id)
                .filter(|grant| grant.lease_id == expected_lease_id)
            {
                grant.touch(Instant::now());
            }
        }
    }

    pub(crate) fn click(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        at: &ComputerActionTarget,
        button: PointerButton,
        click_count: u8,
    ) -> Result<ComputerInputMethod, ComputerActionError> {
        if !(1..=2).contains(&click_count) {
            return Err(ComputerActionError::Rejected(
                "click count must be 1 or 2".to_string(),
            ));
        }
        validate_action_target(target, at)?;
        let (current, elements) = self
            .require_target(session_id, expected_lease_id, target)
            .map_err(ComputerActionError::Rejected)?;
        let at = resolve_action_target(at, elements.as_deref())?;
        let method = self
            .native
            .click(&current, at, button, click_count)
            .map_err(|error| ComputerActionError::Failed(error.message()))?;
        self.touch_grant(session_id, expected_lease_id);
        Ok(method)
    }

    pub(crate) fn scroll(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        at: &ComputerActionTarget,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), ComputerActionError> {
        if (delta_x == 0 && delta_y == 0)
            || delta_x.unsigned_abs() > MAX_SCROLL_DELTA as u32
            || delta_y.unsigned_abs() > MAX_SCROLL_DELTA as u32
        {
            return Err(ComputerActionError::Rejected(
                "scroll delta is invalid".to_string(),
            ));
        }
        validate_action_target(target, at)?;
        let (current, elements) = self
            .require_target(session_id, expected_lease_id, target)
            .map_err(ComputerActionError::Rejected)?;
        let at = resolve_action_target(at, elements.as_deref())?;
        self.native
            .scroll(&current, at, delta_x, delta_y)
            .map_err(|error| ComputerActionError::Failed(error.message()))?;
        self.touch_grant(session_id, expected_lease_id);
        Ok(())
    }

    pub(crate) fn drag(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ComputerActionError> {
        if !point_is_inside(target, from.0, from.1) || !point_is_inside(target, to.0, to.1) {
            return Err(ComputerActionError::Rejected(
                "drag points must be inside the captured target window".to_string(),
            ));
        }
        let (current, _) = self
            .require_target(session_id, expected_lease_id, target)
            .map_err(ComputerActionError::Rejected)?;
        self.native
            .drag(&current, from, to)
            .map_err(|error| ComputerActionError::Failed(error.message()))?;
        self.touch_grant(session_id, expected_lease_id);
        Ok(())
    }

    pub(crate) fn set_value(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        reference: &str,
        value: &str,
    ) -> Result<(), ComputerActionError> {
        if !valid_input_text(value) {
            return Err(ComputerActionError::Rejected(
                "desktop text input is invalid".to_string(),
            ));
        }
        let at = ComputerActionTarget::Element(reference.to_string());
        validate_action_target(target, &at)?;
        let (current, elements) = self
            .require_target(session_id, expected_lease_id, target)
            .map_err(ComputerActionError::Rejected)?;
        let NativeAt::Element { handle, index } = resolve_action_target(&at, elements.as_deref())?
        else {
            unreachable!("an element ref resolves to an element");
        };
        self.native
            .set_value(&current, handle, index, value)
            .map_err(|error| ComputerActionError::Failed(error.message()))?;
        self.touch_grant(session_id, expected_lease_id);
        Ok(())
    }

    pub(crate) fn type_text(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        text: &str,
    ) -> Result<(), ComputerActionError> {
        if text.is_empty() || !valid_input_text(text) {
            return Err(ComputerActionError::Rejected(
                "desktop text input is invalid".to_string(),
            ));
        }
        let (current, _) = self
            .require_target(session_id, expected_lease_id, target)
            .map_err(ComputerActionError::Rejected)?;
        self.native
            .type_text(&current, text)
            .map_err(|error| ComputerActionError::Failed(error.message()))?;
        self.touch_grant(session_id, expected_lease_id);
        Ok(())
    }

    pub(crate) fn key(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        key: &str,
        modifiers: &[String],
    ) -> Result<(), ComputerActionError> {
        let code = key_code(key)
            .ok_or_else(|| ComputerActionError::Rejected("desktop key is unsupported".to_string()))?;
        let flags = modifier_flags(modifiers).map_err(ComputerActionError::Rejected)?;
        let (current, _) = self
            .require_target(session_id, expected_lease_id, target)
            .map_err(ComputerActionError::Rejected)?;
        self.native
            .key(&current, code, flags)
            .map_err(|error| ComputerActionError::Failed(error.message()))?;
        self.touch_grant(session_id, expected_lease_id);
        Ok(())
    }
}

/// Why an action did not happen. `Rejected` never reached the window: the
/// capture the agent holds is still the latest one it was given. `Failed`
/// consumed that capture at the window, which may have changed, so the
/// caller observes it again before reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComputerActionError {
    Rejected(String),
    Failed(String),
}

impl ComputerActionError {
    #[cfg(test)]
    fn message(&self) -> &str {
        match self {
            Self::Rejected(message) | Self::Failed(message) => message,
        }
    }
}

#[derive(Debug, Clone)]
struct WebAccessibilityEntry {
    bundle_id: String,
    /// DCC switched it on and must switch it off again.
    owned: bool,
}

fn point_is_inside(target: &ComputerTargetSnapshot, x: f64, y: f64) -> bool {
    x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0 && x < target.width && y < target.height
}

fn valid_input_text(text: &str) -> bool {
    text.chars().count() <= MAX_TYPE_CHARS
        && !text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

/// Element refs are `e1`…`e800`, one-based, as printed by the capture.
pub(crate) fn parse_element_ref(reference: &str) -> Option<usize> {
    let digits = reference.strip_prefix('e')?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let number = digits.parse::<usize>().ok()?;
    (1..=MAX_ELEMENT_REFS).contains(&number).then(|| number - 1)
}

/// Checks what can be checked before the one-shot capture is consumed.
fn validate_action_target(
    target: &ComputerTargetSnapshot,
    at: &ComputerActionTarget,
) -> Result<(), ComputerActionError> {
    match at {
        ComputerActionTarget::Point { x, y } if !point_is_inside(target, *x, *y) => Err(
            ComputerActionError::Rejected(
                "coordinates are outside the captured target window".to_string(),
            ),
        ),
        ComputerActionTarget::Element(reference) if parse_element_ref(reference).is_none() => {
            Err(ComputerActionError::Rejected(
                "element refs look like e12, as listed by the capture".to_string(),
            ))
        }
        _ => Ok(()),
    }
}

fn resolve_action_target(
    at: &ComputerActionTarget,
    elements: Option<&ElementMap>,
) -> Result<NativeAt, ComputerActionError> {
    match at {
        ComputerActionTarget::Point { x, y } => Ok(NativeAt::Point { x: *x, y: *y }),
        ComputerActionTarget::Element(reference) => {
            let index = parse_element_ref(reference).ok_or_else(|| {
                ComputerActionError::Failed("element refs look like e12".to_string())
            })?;
            let map = elements.ok_or_else(|| {
                ComputerActionError::Failed(
                    "that capture listed no elements; use x/y from its screenshot".to_string(),
                )
            })?;
            if index >= map.count {
                return Err(ComputerActionError::Failed(format!(
                    "{reference} is not in that capture, which listed e1-e{}",
                    map.count
                )));
            }
            Ok(NativeAt::Element {
                handle: map.handle,
                index: index as u32,
            })
        }
    }
}

impl ComputerUseState {
    fn capture_grant(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        bundle_id: &str,
    ) -> Result<String, String> {
        let mut grants = self
            .grants
            .lock()
            .map_err(|_| "desktop computer use state is unavailable".to_string())?;
        prune_expired(&mut grants);
        let grant = grants
            .get(session_id)
            .ok_or_else(|| "desktop computer use is not enabled for this session; call dcc_computer_request_control to ask the user for access".to_string())?;
        if !grant.allowed_bundle_ids.contains(bundle_id) {
            return Err("target application is not allowed for this session".to_string());
        }
        if grant.lease_id != expected_lease_id
            || self.projection_lease(session_id).as_deref() != Some(expected_lease_id)
        {
            return Err("desktop computer use provider session changed".to_string());
        }
        Ok(grant.nonce.clone())
    }
    fn grant_is_current(&self, session_id: &str, expected_lease_id: &str, nonce: &str) -> bool {
        self.grants.lock().is_ok_and(|mut grants| {
            prune_expired(&mut grants);
            grants
                .get(session_id)
                .is_some_and(|grant| grant.lease_id == expected_lease_id && grant.nonce == nonce)
        }) && self.projection_lease(session_id).as_deref() == Some(expected_lease_id)
    }
    fn snapshot_for_action(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
    ) -> Result<(String, CapturedSnapshot), String> {
        let mut grants = self
            .grants
            .lock()
            .map_err(|_| "desktop computer use state is unavailable".to_string())?;
        prune_expired(&mut grants);
        let grant = grants
            .get(session_id)
            .ok_or_else(|| "desktop computer use is not enabled for this session; call dcc_computer_request_control to ask the user for access".to_string())?;
        if !grant.allowed_bundle_ids.contains(&target.bundle_id) {
            return Err("target application is not allowed for this session".to_string());
        }
        if grant.lease_id != expected_lease_id
            || self.projection_lease(session_id).as_deref() != Some(expected_lease_id)
        {
            return Err("desktop computer use provider session changed".to_string());
        }
        let snapshot = grant
            .snapshots
            .get(&target.window_id)
            .filter(|snapshot| {
                snapshot.visible.generation == target.generation
                    && snapshot.visible.bundle_id == target.bundle_id
            })
            .cloned()
            .ok_or_else(|| "use the generation from the latest capture or action result; capture the target window if you have none".to_string())?;
        Ok((grant.nonce.clone(), snapshot))
    }
}

fn normalized_bundle_ids(values: &[String]) -> Result<HashSet<String>, String> {
    if values.is_empty() || values.len() > MAX_ALLOWED_APPS {
        return Err("choose between one and sixteen target applications".to_string());
    }
    let normalized = values.iter().map(|value| value.trim()).collect::<Vec<_>>();
    let values = normalized.iter().copied().collect::<HashSet<_>>();
    if values.len() != normalized.len()
        || values.iter().any(|value| {
            value.is_empty() || value.len() > 255 || value.chars().any(char::is_control)
        })
    {
        return Err("target application identifiers are invalid".to_string());
    }
    Ok(values.into_iter().map(ToString::to_string).collect())
}
fn normalized_control_reason(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_CONTROL_REASON_CHARS
        || value.chars().any(char::is_control)
    {
        return Err("desktop computer-use request reason is invalid".to_string());
    }
    Ok(value.to_string())
}
/// The window a capture was taken of: process, window-server id and
/// geometry. The title is deliberately excluded: a browser retitles its
/// window on every navigation, which is the window's content changing, not
/// the window being replaced. Coordinates stay valid while geometry holds.
fn target_identity(target: &ComputerTarget) -> String {
    let material = format!(
        "{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}",
        target.bundle_id,
        target.pid,
        target.window_id,
        target.x,
        target.y,
        target.width,
        target.height
    );
    hex::encode(&Sha256::digest(material.as_bytes())[..16])
}
fn random_token() -> String {
    let mut bytes = [0_u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn preview_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
fn prune_expired(grants: &mut HashMap<String, ComputerGrant>) {
    let now = Instant::now();
    grants.retain(|_, grant| grant.expires_at > now);
}
fn prune_expired_control_requests(requests: &mut HashMap<String, PendingControlRequest>) {
    let now = Instant::now();
    requests.retain(|_, request| request.expires_at > now);
}
fn key_code(key: &str) -> Option<u16> {
    match key {
        "ENTER" => Some(36),
        "TAB" => Some(48),
        "ESCAPE" => Some(53),
        "SPACE" => Some(49),
        "ARROW_UP" => Some(126),
        "ARROW_DOWN" => Some(125),
        "ARROW_LEFT" => Some(123),
        "ARROW_RIGHT" => Some(124),
        "BACKSPACE" => Some(51),
        "DELETE" => Some(117),
        "HOME" => Some(115),
        "END" => Some(119),
        "PAGE_UP" => Some(116),
        "PAGE_DOWN" => Some(121),
        "A" => Some(0),
        "B" => Some(11),
        "C" => Some(8),
        "D" => Some(2),
        "E" => Some(14),
        "F" => Some(3),
        "G" => Some(5),
        "H" => Some(4),
        "I" => Some(34),
        "J" => Some(38),
        "K" => Some(40),
        "L" => Some(37),
        "M" => Some(46),
        "N" => Some(45),
        "O" => Some(31),
        "P" => Some(35),
        "Q" => Some(12),
        "R" => Some(15),
        "S" => Some(1),
        "T" => Some(17),
        "U" => Some(32),
        "V" => Some(9),
        "W" => Some(13),
        "X" => Some(7),
        "Y" => Some(16),
        "Z" => Some(6),
        "0" => Some(29),
        "1" => Some(18),
        "2" => Some(19),
        "3" => Some(20),
        "4" => Some(21),
        "5" => Some(23),
        "6" => Some(22),
        "7" => Some(26),
        "8" => Some(28),
        "9" => Some(25),
        "F1" => Some(122),
        "F2" => Some(120),
        "F3" => Some(99),
        "F4" => Some(118),
        "F5" => Some(96),
        "F6" => Some(97),
        "F7" => Some(98),
        "F8" => Some(100),
        "F9" => Some(101),
        "F10" => Some(109),
        "F11" => Some(103),
        "F12" => Some(111),
        _ => None,
    }
}
fn modifier_flags(modifiers: &[String]) -> Result<u64, String> {
    modifiers
        .iter()
        .try_fold(0_u64, |flags, modifier| match modifier.as_str() {
            "SHIFT" => Ok(flags | (1 << 17)),
            "CONTROL" => Ok(flags | (1 << 18)),
            "OPTION" => Ok(flags | (1 << 19)),
            "COMMAND" => Ok(flags | (1 << 20)),
            _ => Err("desktop key modifier is unsupported".to_string()),
        })
}

#[tauri::command]
pub fn computer_use_status(
    state: State<'_, ComputerUseState>,
    input: ComputerUseStatusInput,
) -> ComputerUseStatus {
    state.status(input.session_id.as_deref(), input.include_targets)
}
#[tauri::command]
pub fn computer_use_arm(
    app: tauri::AppHandle,
    state: State<'_, ComputerUseState>,
    input: ComputerUseArmInput,
) -> Result<ComputerGrantStatus, String> {
    let grant = state.arm(&input.session_id, &input.allowed_bundle_ids)?;
    let _ = app.emit("computer-use-preview-cleared", input.session_id);
    Ok(grant)
}
#[tauri::command]
pub fn computer_use_disarm(
    app: tauri::AppHandle,
    state: State<'_, ComputerUseState>,
    input: ComputerUseDisarmInput,
) -> ComputerGrantStatus {
    let grant = state.disarm(&input.session_id);
    let _ = app.emit("computer-use-preview-cleared", input.session_id);
    grant
}
#[tauri::command]
pub fn computer_use_request_access(
    state: State<'_, ComputerUseState>,
    input: ComputerUseRequestAccessInput,
) -> Result<ComputerUseStatus, String> {
    state.request_access(input.kind)
}
#[tauri::command]
pub fn computer_use_list_pending_requests(
    state: State<'_, ComputerUseState>,
) -> ComputerUsePendingRequestsOutput {
    state.pending_control_requests()
}
/// Answers through the shared ledger, so the desktop and a phone cannot both
/// answer (`desktop_agent_requests`).
#[tauri::command]
pub async fn computer_use_respond_control_request(
    requests: State<'_, crate::desktop_agent_requests::DesktopAgentRequests>,
    input: ComputerUseRespondControlRequestInput,
) -> Result<ComputerUseRespondControlRequestOutput, String> {
    if input.request_id.len() != 32 || !input.request_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("desktop computer-use approval request is invalid".to_string());
    }
    requests
        .respond_computer(
            &input.request_id,
            input.allowed,
            &input.allowed_bundle_ids,
            dev_command_center_tauri::agent_requests::AnswerOrigin::Desktop,
        )
        .await
        .map(|(output, _)| output)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn computer_use_preview_frame(
    state: State<'_, ComputerUseState>,
    input: ComputerUsePreviewFrameInput,
) -> Option<ComputerUsePreviewFrame> {
    state.preview_frame(&input.session_id)
}

#[tauri::command]
pub async fn computer_use_refresh_preview_frame(
    state: State<'_, ComputerUseState>,
    input: ComputerUsePreviewFrameInput,
) -> Result<Option<ComputerUsePreviewFrame>, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.refresh_preview_frame(&input.session_id))
        .await
        .map_err(|_| "desktop computer-use preview refresh failed".to_string())
}

// Human-initiated Appshots reuse the native capture primitives without granting
// agent control. A screenshot needs Screen Recording, not Accessibility.
pub(crate) fn appshot_status() -> (bool, bool) {
    let status = PlatformComputerNative.status();
    (
        cfg!(target_os = "macos") && status.major_version >= 14,
        status.screen_recording,
    )
}

pub(crate) fn appshot_request_access() {
    PlatformComputerNative.request_access(ComputerAccessKind::ScreenRecording);
}

pub(crate) fn appshot_targets() -> Result<Vec<ComputerTarget>, String> {
    let (supported, granted) = appshot_status();
    if !supported {
        return Err("unsupported".into());
    }
    if !granted {
        return Err("permission".into());
    }
    Ok(PlatformComputerNative
        .targets()?
        .into_iter()
        .filter(|target| target.pid != std::process::id() as i32)
        .collect())
}

pub(crate) fn appshot_capture(target: &ComputerTarget) -> Result<Vec<u8>, String> {
    let current = appshot_targets()?
        .into_iter()
        .find(|current| {
            current.pid == target.pid
                && current.window_id == target.window_id
                && current.bundle_id == target.bundle_id
        })
        .ok_or("windowUnavailable")?;
    PlatformComputerNative
        .capture(&current)
        .map_err(|_| "captureFailed".into())
}

#[cfg(target_os = "macos")]
pub(crate) fn appshot_frontmost_target() -> Result<ComputerTarget, String> {
    unsafe extern "C" {
        fn dcc_appshot_frontmost_pid() -> i32;
    }
    let pid = unsafe { dcc_appshot_frontmost_pid() };
    appshot_targets()?
        .into_iter()
        .find(|target| target.pid == pid)
        .ok_or("windowUnavailable".into())
}

#[cfg(target_os = "macos")]
struct PlatformComputerNative;
#[cfg(not(target_os = "macos"))]
struct PlatformComputerNative;

#[cfg(target_os = "macos")]
impl ComputerNative for PlatformComputerNative {
    fn status(&self) -> NativeStatus {
        unsafe {
            let raw = take_string(dcc_computer_status_json());
            let value: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
            NativeStatus {
                accessibility: value
                    .get("accessibility")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                screen_recording: value
                    .get("screenRecording")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                major_version: value
                    .get("majorVersion")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u8::try_from(value).ok())
                    .unwrap_or(0),
            }
        }
    }
    fn request_access(&self, kind: ComputerAccessKind) -> bool {
        unsafe {
            dcc_computer_request_access(match kind {
                ComputerAccessKind::Accessibility => 1,
                ComputerAccessKind::ScreenRecording => 2,
            })
        }
    }
    fn targets(&self) -> Result<Vec<ComputerTarget>, String> {
        unsafe {
            serde_json::from_str(&take_string(dcc_computer_targets_json()))
                .map_err(|_| "desktop target inventory is unavailable".to_string())
        }
    }
    fn capture(&self, target: &ComputerTarget) -> Result<Vec<u8>, String> {
        let bundle = std::ffi::CString::new(target.bundle_id.as_str())
            .map_err(|_| "target application is invalid".to_string())?;
        unsafe {
            let mut bytes = std::ptr::null_mut();
            let mut len = 0;
            if !dcc_computer_capture_png(
                bundle.as_ptr(),
                target.pid,
                target.window_id,
                target.x,
                target.y,
                target.width,
                target.height,
                &mut bytes,
                &mut len,
            ) || bytes.is_null()
                || len == 0
            {
                return Err(
                    "target window capture failed; grant Screen Recording and capture again"
                        .to_string(),
                );
            }
            let output = std::slice::from_raw_parts(bytes, len).to_vec();
            dcc_computer_free_bytes(bytes);
            Ok(output)
        }
    }
    fn accessibility_tree(&self, target: &ComputerTarget) -> Result<NativeTree, String> {
        let bundle = bundle_cstring(target).map_err(|error| error.message())?;
        let raw = unsafe {
            take_string(dcc_computer_ax_tree_json(
                bundle.as_ptr(),
                target.pid,
                target.window_id,
                target.x,
                target.y,
                target.width,
                target.height,
            ))
        };
        let value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|_| "the accessibility tree is unavailable".to_string())?;
        let status = value
            .get("status")
            .and_then(serde_json::Value::as_i64)
            .and_then(|status| i32::try_from(status).ok())
            .unwrap_or(-1);
        if status != 0 {
            return Err(NativeEffectError::from_code(status, None).message());
        }
        let flag = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        Ok(NativeTree {
            available: flag("available"),
            handle: value
                .get("handle")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            count: value
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
                .unwrap_or(0),
            text: value
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            truncated: flag("truncated"),
            busy: flag("busy"),
        })
    }
    fn release_tree(&self, handle: u64) {
        if handle != 0 {
            unsafe { dcc_computer_ax_tree_release(handle) }
        }
    }
    fn set_web_accessibility(&self, pid: i32, enabled: bool) -> WebAccessibility {
        match unsafe { dcc_computer_web_accessibility(pid, enabled) } {
            2 => WebAccessibility::Changed,
            1 => WebAccessibility::AlreadyEnabled,
            _ => WebAccessibility::Unsupported,
        }
    }
    fn click(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        button: PointerButton,
        click_count: u8,
    ) -> Result<ComputerInputMethod, NativeEffectError> {
        let bundle = bundle_cstring(target)?;
        let button = pointer_button_code(button);
        let mut detail = std::ptr::null_mut();
        let mut method = NATIVE_METHOD_POINTER;
        let status = unsafe {
            match at {
                NativeAt::Point { x, y } => dcc_computer_click_point(
                    bundle.as_ptr(),
                    target.pid,
                    target.window_id,
                    target.x,
                    target.y,
                    target.width,
                    target.height,
                    x,
                    y,
                    button,
                    click_count,
                    &mut detail,
                ),
                NativeAt::Element { handle, index } => dcc_computer_click_element(
                    bundle.as_ptr(),
                    target.pid,
                    target.window_id,
                    target.x,
                    target.y,
                    target.width,
                    target.height,
                    handle,
                    index,
                    button,
                    click_count,
                    &mut method,
                    &mut detail,
                ),
            }
        };
        effect_result(status, detail)?;
        Ok(if method == NATIVE_METHOD_ACCESSIBILITY {
            ComputerInputMethod::Accessibility
        } else {
            ComputerInputMethod::Pointer
        })
    }
    fn scroll(
        &self,
        target: &ComputerTarget,
        at: NativeAt,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), NativeEffectError> {
        let bundle = bundle_cstring(target)?;
        let mut detail = std::ptr::null_mut();
        let status = unsafe {
            match at {
                NativeAt::Point { x, y } => dcc_computer_scroll_point(
                    bundle.as_ptr(),
                    target.pid,
                    target.window_id,
                    target.x,
                    target.y,
                    target.width,
                    target.height,
                    x,
                    y,
                    delta_x,
                    delta_y,
                    &mut detail,
                ),
                NativeAt::Element { handle, index } => dcc_computer_scroll_element(
                    bundle.as_ptr(),
                    target.pid,
                    target.window_id,
                    target.x,
                    target.y,
                    target.width,
                    target.height,
                    handle,
                    index,
                    delta_x,
                    delta_y,
                    &mut detail,
                ),
            }
        };
        effect_result(status, detail)
    }
    fn drag(
        &self,
        target: &ComputerTarget,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), NativeEffectError> {
        let bundle = bundle_cstring(target)?;
        let mut detail = std::ptr::null_mut();
        let status = unsafe {
            dcc_computer_drag(
                bundle.as_ptr(),
                target.pid,
                target.window_id,
                target.x,
                target.y,
                target.width,
                target.height,
                from.0,
                from.1,
                to.0,
                to.1,
                &mut detail,
            )
        };
        effect_result(status, detail)
    }
    fn set_value(
        &self,
        target: &ComputerTarget,
        handle: u64,
        index: u32,
        value: &str,
    ) -> Result<(), NativeEffectError> {
        let bundle = bundle_cstring(target)?;
        let value = std::ffi::CString::new(value).map_err(|_| NativeEffectError::Invalid)?;
        let mut detail = std::ptr::null_mut();
        let status = unsafe {
            dcc_computer_set_value(
                bundle.as_ptr(),
                target.pid,
                target.window_id,
                target.x,
                target.y,
                target.width,
                target.height,
                handle,
                index,
                value.as_ptr(),
                &mut detail,
            )
        };
        effect_result(status, detail)
    }
    fn type_text(&self, target: &ComputerTarget, text: &str) -> Result<(), NativeEffectError> {
        let bundle = bundle_cstring(target)?;
        let text = std::ffi::CString::new(text).map_err(|_| NativeEffectError::Invalid)?;
        let status = unsafe {
            dcc_computer_type(
                bundle.as_ptr(),
                target.pid,
                target.window_id,
                target.x,
                target.y,
                target.width,
                target.height,
                text.as_ptr(),
            )
        };
        effect_result(status, std::ptr::null_mut())
    }
    fn key(
        &self,
        target: &ComputerTarget,
        key_code: u16,
        flags: u64,
    ) -> Result<(), NativeEffectError> {
        let bundle = bundle_cstring(target)?;
        let status = unsafe {
            dcc_computer_key(
                bundle.as_ptr(),
                target.pid,
                target.window_id,
                target.x,
                target.y,
                target.width,
                target.height,
                key_code,
                flags,
            )
        };
        effect_result(status, std::ptr::null_mut())
    }
}

#[cfg(target_os = "macos")]
const NATIVE_METHOD_ACCESSIBILITY: i32 = 1;
#[cfg(target_os = "macos")]
const NATIVE_METHOD_POINTER: i32 = 2;

#[cfg(target_os = "macos")]
fn bundle_cstring(target: &ComputerTarget) -> Result<std::ffi::CString, NativeEffectError> {
    std::ffi::CString::new(target.bundle_id.as_str()).map_err(|_| NativeEffectError::Invalid)
}

#[cfg(target_os = "macos")]
fn pointer_button_code(button: PointerButton) -> i32 {
    match button {
        PointerButton::Left => 0,
        PointerButton::Right => 1,
    }
}

/// Takes ownership of the bridge-allocated detail string, if any.
#[cfg(target_os = "macos")]
fn effect_result(status: i32, detail: *mut std::ffi::c_char) -> Result<(), NativeEffectError> {
    let detail = (!detail.is_null()).then(|| unsafe { take_string(detail) });
    if status == 0 {
        Ok(())
    } else {
        Err(NativeEffectError::from_code(status, detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeNative {
        targets: Mutex<Vec<ComputerTarget>>,
        effects: Mutex<usize>,
        captures: Mutex<usize>,
        capture_bytes: Mutex<usize>,
        after_targets: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
        status: Mutex<NativeStatus>,
        request_result: Mutex<bool>,
        tree_counts: Mutex<Vec<usize>>,
        tree_busy: Mutex<bool>,
        next_handle: Mutex<u64>,
        released: Mutex<Vec<u64>>,
        web_accessibility: Mutex<WebAccessibility>,
        web_calls: Mutex<Vec<(i32, bool)>>,
        last_at: Mutex<Option<NativeAt>>,
        effect_error: Mutex<Option<NativeEffectError>>,
    }
    impl FakeNative {
        fn with_target() -> Self {
            Self {
                targets: Mutex::new(vec![ComputerTarget {
                    bundle_id: "com.example.Editor".to_string(),
                    name: "Editor".to_string(),
                    pid: 42,
                    window_id: 7,
                    title: "file.rs".to_string(),
                    x: 1.0,
                    y: 2.0,
                    width: 800.0,
                    height: 600.0,
                }]),
                effects: Mutex::new(0),
                captures: Mutex::new(0),
                capture_bytes: Mutex::new(4),
                after_targets: Mutex::new(None),
                status: Mutex::new(NativeStatus {
                    accessibility: true,
                    screen_recording: true,
                    major_version: 14,
                }),
                request_result: Mutex::new(true),
                tree_counts: Mutex::new(vec![3]),
                tree_busy: Mutex::new(false),
                next_handle: Mutex::new(0),
                released: Mutex::new(Vec::new()),
                web_accessibility: Mutex::new(WebAccessibility::Unsupported),
                web_calls: Mutex::new(Vec::new()),
                last_at: Mutex::new(None),
                effect_error: Mutex::new(None),
            }
        }
        fn effect(&self) -> Result<(), NativeEffectError> {
            if let Some(error) = self.effect_error.lock().unwrap().clone() {
                return Err(error);
            }
            *self.effects.lock().unwrap() += 1;
            Ok(())
        }
        fn live_handles(&self) -> Vec<u64> {
            let released = self.released.lock().unwrap();
            (1..=*self.next_handle.lock().unwrap())
                .filter(|handle| !released.contains(handle))
                .collect()
        }
    }
    impl ComputerNative for FakeNative {
        fn status(&self) -> NativeStatus {
            *self.status.lock().unwrap()
        }
        fn request_access(&self, _: ComputerAccessKind) -> bool {
            *self.request_result.lock().unwrap()
        }
        fn targets(&self) -> Result<Vec<ComputerTarget>, String> {
            let targets = self.targets.lock().unwrap().clone();
            if let Some(callback) = self.after_targets.lock().unwrap().take() {
                callback();
            }
            Ok(targets)
        }
        fn capture(&self, _: &ComputerTarget) -> Result<Vec<u8>, String> {
            *self.captures.lock().unwrap() += 1;
            Ok(vec![0; *self.capture_bytes.lock().unwrap()])
        }
        fn accessibility_tree(&self, _: &ComputerTarget) -> Result<NativeTree, String> {
            let mut counts = self.tree_counts.lock().unwrap();
            let count = if counts.len() > 1 { counts.remove(0) } else { counts[0] };
            let mut next = self.next_handle.lock().unwrap();
            *next += 1;
            Ok(NativeTree {
                available: true,
                handle: *next,
                count,
                text: (1..=count).map(|n| format!("e{n} button\n")).collect(),
                truncated: false,
                busy: *self.tree_busy.lock().unwrap(),
            })
        }
        fn release_tree(&self, handle: u64) {
            self.released.lock().unwrap().push(handle);
        }
        fn set_web_accessibility(&self, pid: i32, enabled: bool) -> WebAccessibility {
            self.web_calls.lock().unwrap().push((pid, enabled));
            *self.web_accessibility.lock().unwrap()
        }
        fn click(
            &self,
            _: &ComputerTarget,
            at: NativeAt,
            _: PointerButton,
            _: u8,
        ) -> Result<ComputerInputMethod, NativeEffectError> {
            *self.last_at.lock().unwrap() = Some(at);
            self.effect()?;
            Ok(match at {
                NativeAt::Element { .. } => ComputerInputMethod::Accessibility,
                NativeAt::Point { .. } => ComputerInputMethod::Pointer,
            })
        }
        fn scroll(&self, _: &ComputerTarget, at: NativeAt, _: i32, _: i32) -> Result<(), NativeEffectError> {
            *self.last_at.lock().unwrap() = Some(at);
            self.effect()
        }
        fn drag(&self, _: &ComputerTarget, _: (f64, f64), _: (f64, f64)) -> Result<(), NativeEffectError> {
            self.effect()
        }
        fn set_value(&self, _: &ComputerTarget, handle: u64, index: u32, _: &str) -> Result<(), NativeEffectError> {
            *self.last_at.lock().unwrap() = Some(NativeAt::Element { handle, index });
            self.effect()
        }
        fn type_text(&self, _: &ComputerTarget, _: &str) -> Result<(), NativeEffectError> {
            self.effect()
        }
        fn key(&self, _: &ComputerTarget, _: u16, _: u64) -> Result<(), NativeEffectError> {
            self.effect()
        }
    }
    fn point(x: f64, y: f64) -> ComputerActionTarget {
        ComputerActionTarget::Point { x, y }
    }
    fn element(reference: &str) -> ComputerActionTarget {
        ComputerActionTarget::Element(reference.to_string())
    }
    fn armed() -> (ComputerUseState, Arc<FakeNative>) {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native.clone());
        state.record_projection("session", "lease");
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        (state, native)
    }

    async fn next_pending_request(state: &ComputerUseState) -> PendingComputerControlRequest {
        for _ in 0..32 {
            if let Some(request) = state.pending_control_requests().requests.into_iter().next() {
                return request;
            }
            tokio::task::yield_now().await;
        }
        panic!("control request was not published")
    }

    #[tokio::test]
    async fn control_request_allows_only_the_exact_waiting_lease() {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native);
        state.record_projection("session", "lease");
        let waiting = {
            let state = state.clone();
            tokio::spawn(async move {
                state
                    .request_control(
                        "session",
                        "workspace",
                        "provider",
                        "lease",
                        "edit the selected document",
                    )
                    .await
            })
        };
        let request = next_pending_request(&state).await;
        assert_eq!(request.session_id, "session");
        assert_eq!(request.workspace_id, "workspace");
        assert_eq!(request.provider_id, "provider");
        assert_eq!(request.reason, "edit the selected document");
        let response = state
            .respond_control_request(
                &request.request_id,
                true,
                &["com.example.Editor".to_string()],
            )
            .await
            .unwrap();
        assert!(response.grant.armed);
        assert!(waiting.await.unwrap().unwrap().grant.armed);
        assert!(state.pending_control_requests().requests.is_empty());
    }

    #[tokio::test]
    async fn denied_control_request_never_creates_a_grant() {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native);
        state.record_projection("session", "lease");
        let waiting = {
            let state = state.clone();
            tokio::spawn(async move {
                state
                    .request_control("session", "workspace", "provider", "lease", "inspect")
                    .await
            })
        };
        let request = next_pending_request(&state).await;
        let response = state
            .respond_control_request(&request.request_id, false, &[])
            .await
            .unwrap();
        assert!(!response.grant.armed);
        assert!(waiting.await.unwrap().unwrap_err().contains("denied"));
        assert!(!state.grant_status(Some("session")).armed);
    }

    #[tokio::test]
    async fn revoking_or_replacing_a_lease_cancels_its_pending_control_request() {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native);
        state.record_projection("session", "old-lease");
        let waiting = {
            let state = state.clone();
            tokio::spawn(async move {
                state
                    .request_control("session", "workspace", "provider", "old-lease", "inspect")
                    .await
            })
        };
        let request = next_pending_request(&state).await;
        state.record_projection("session", "new-lease");
        assert!(waiting
            .await
            .unwrap()
            .unwrap_err()
            .contains("no longer available"));
        assert!(state.pending_control_requests().requests.is_empty());
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        assert!(state
            .respond_control_request(
                &request.request_id,
                true,
                &["com.example.Editor".to_string()],
            )
            .await
            .is_err());
        assert!(state.grant_status(Some("session")).armed);
    }

    #[tokio::test]
    async fn cancelled_mcp_wait_removes_its_request_without_creating_a_grant() {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native);
        state.record_projection("session", "lease");
        let waiting = {
            let state = state.clone();
            tokio::spawn(async move {
                state
                    .request_control("session", "workspace", "provider", "lease", "inspect")
                    .await
            })
        };
        let request = next_pending_request(&state).await;
        waiting.abort();
        assert!(waiting.await.unwrap_err().is_cancelled());
        tokio::task::yield_now().await;
        assert!(state.pending_control_requests().requests.is_empty());
        assert!(state
            .respond_control_request(
                &request.request_id,
                true,
                &["com.example.Editor".to_string()],
            )
            .await
            .is_err());
        assert!(!state.grant_status(Some("session")).armed);
    }

    #[tokio::test(start_paused = true)]
    async fn unanswered_control_request_times_out_without_a_grant() {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native);
        state.record_projection("session", "lease");
        let waiting = {
            let state = state.clone();
            tokio::spawn(async move {
                state
                    .request_control("session", "workspace", "provider", "lease", "inspect")
                    .await
            })
        };
        let _ = next_pending_request(&state).await;
        tokio::time::advance(CONTROL_REQUEST_TTL + Duration::from_secs(1)).await;
        assert!(waiting.await.unwrap().unwrap_err().contains("timed out"));
        assert!(state.pending_control_requests().requests.is_empty());
        assert!(!state.grant_status(Some("session")).armed);
    }

    #[test]
    fn capture_and_effect_require_a_grant_and_one_shot_snapshot() {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native.clone());
        state.record_projection("session", "lease");
        assert!(state
            .capture("session", "lease", "com.example.Editor", 7)
            .is_err());
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1)
            .unwrap();
        assert!(state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1)
            .is_err());
        assert_eq!(*native.effects.lock().unwrap(), 1);
    }

    #[test]
    fn scroll_requires_a_fresh_snapshot_and_a_bounded_nonzero_delta() {
        let (state, native) = armed();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state
            .scroll("session", "lease", &capture.target, &point(20.0, 20.0), 0, -120)
            .unwrap();
        assert_eq!(*native.effects.lock().unwrap(), 1);

        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert!(state
            .scroll("session", "lease", &capture.target, &point(20.0, 20.0), 0, 0)
            .is_err());
        assert_eq!(*native.effects.lock().unwrap(), 1);

        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert!(state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 3)
            .is_err());
        assert_eq!(*native.effects.lock().unwrap(), 1);
    }

    #[test]
    fn changed_window_or_rearm_invalidates_a_capture() {
        let (state, native) = armed();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        native.targets.lock().unwrap()[0].width = 801.0;
        assert!(state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1)
            .is_err());
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        assert!(state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1)
            .is_err());
    }

    #[test]
    fn stale_lease_revoke_does_not_clear_new_projection_or_grant() {
        let (state, _) = armed();
        state.record_projection("session", "new-lease");
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        state.revoke_projection("session", "lease");
        assert!(state.lease_matches("session", "new-lease"));
        assert!(state.grant_status(Some("session")).armed);
    }

    #[test]
    fn agent_status_filters_targets_to_the_explicit_allowlist() {
        let (state, _) = armed();
        let status = state.agent_status("session", "lease");
        assert_eq!(status.targets.len(), 1);
        state.disarm("session");
        assert!(state.agent_status("session", "lease").targets.is_empty());
    }

    #[test]
    fn cross_session_unknown_app_expired_or_revoked_grants_never_reach_the_driver() {
        let (state, native) = armed();
        assert!(state
            .arm("session", &["com.example.Unknown".to_string()])
            .is_err());
        assert!(state
            .capture("other-session", "lease", "com.example.Editor", 7)
            .is_err());
        state
            .grants
            .lock()
            .unwrap()
            .get_mut("session")
            .unwrap()
            .expires_at = Instant::now() - Duration::from_secs(1);
        assert!(state
            .capture("session", "lease", "com.example.Editor", 7)
            .is_err());
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        state.revoke_projection("session", "lease");
        assert!(state
            .capture("session", "lease", "com.example.Editor", 7)
            .is_err());
        assert_eq!(*native.effects.lock().unwrap(), 0);
    }

    #[test]
    fn unsupported_or_unprojected_session_cannot_arm() {
        let native = Arc::new(FakeNative::with_target());
        let state = ComputerUseState::with_native(native);
        assert!(state
            .arm("session", &["com.example.Editor".to_string()])
            .is_err());
    }

    #[test]
    fn opened_permission_request_can_remain_ungranted_without_a_session_or_grant() {
        let native = Arc::new(FakeNative::with_target());
        *native.status.lock().unwrap() = NativeStatus {
            accessibility: false,
            screen_recording: false,
            major_version: 14,
        };
        let state = ComputerUseState::with_native(native);
        let status = state
            .request_access(ComputerAccessKind::Accessibility)
            .unwrap();
        assert!(!status.accessibility.granted);
        assert!(status.accessibility.can_request);
        assert!(!status.grant.armed);
        assert!(!status.runtime_attached);
    }

    #[test]
    fn failed_permission_request_is_an_explicit_error() {
        let native = Arc::new(FakeNative::with_target());
        *native.request_result.lock().unwrap() = false;
        let state = ComputerUseState::with_native(native);
        let error = state
            .request_access(ComputerAccessKind::ScreenRecording)
            .unwrap_err();
        assert!(error.contains("could not initiate the macOS Screen Recording permission request"));
    }

    #[test]
    fn queued_old_lease_cannot_use_a_replacement_sessions_new_grant() {
        let (state, native) = armed();
        state.record_projection("session", "new-lease");
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        assert!(state
            .capture("session", "lease", "com.example.Editor", 7)
            .is_err());
        assert_eq!(*native.captures.lock().unwrap(), 0);
        assert!(state
            .capture("session", "new-lease", "com.example.Editor", 7)
            .is_ok());
        assert_eq!(*native.captures.lock().unwrap(), 1);
    }

    #[test]
    fn revocation_during_target_discovery_prevents_capture_and_oversized_output_is_rejected() {
        let (state, native) = armed();
        let revoke_state = state.clone();
        *native.after_targets.lock().unwrap() = Some(Arc::new(move || {
            revoke_state.revoke_projection("session", "lease")
        }));
        assert!(state
            .capture("session", "lease", "com.example.Editor", 7)
            .is_err());
        assert_eq!(*native.captures.lock().unwrap(), 0);

        state.record_projection("session", "lease-two");
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        *native.capture_bytes.lock().unwrap() = MAX_CAPTURE_BYTES + 1;
        assert!(state
            .capture("session", "lease-two", "com.example.Editor", 7)
            .is_err());
    }

    #[test]
    fn element_refs_resolve_against_the_capture_that_listed_them() {
        let (state, native) = armed();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert!(capture.elements.available);
        assert_eq!(capture.elements.count, 3);
        let method = state
            .click("session", "lease", &capture.target, &element("e2"), PointerButton::Left, 1)
            .unwrap();
        assert_eq!(method, ComputerInputMethod::Accessibility);
        assert_eq!(
            *native.last_at.lock().unwrap(),
            Some(NativeAt::Element { handle: 1, index: 1 })
        );
        // The action consumed the capture and released its element map.
        assert_eq!(*native.released.lock().unwrap(), vec![1]);
    }

    #[test]
    fn unknown_or_malformed_refs_never_reach_the_driver() {
        let (state, native) = armed();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        // Malformed refs are rejected before the capture is consumed...
        for reference in ["2", "e0", "e02", "e801", "ex"] {
            assert!(matches!(
                state.click("session", "lease", &capture.target, &element(reference), PointerButton::Left, 1),
                Err(ComputerActionError::Rejected(_))
            ));
        }
        // ...so the same capture still works, while a ref it did not list
        // consumes it and asks for the fresh observation.
        assert!(matches!(
            state.click("session", "lease", &capture.target, &element("e4"), PointerButton::Left, 1),
            Err(ComputerActionError::Failed(message)) if message.contains("e1-e3")
        ));
        assert_eq!(*native.effects.lock().unwrap(), 0);
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state
            .set_value("session", "lease", &capture.target, "e3", "hello")
            .unwrap();
        assert_eq!(
            *native.last_at.lock().unwrap(),
            Some(NativeAt::Element { handle: 2, index: 2 })
        );
    }

    #[test]
    fn a_capture_without_elements_still_supports_coordinates() {
        let (state, native) = armed();
        *native.tree_counts.lock().unwrap() = vec![0];
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert_eq!(capture.elements.count, 0);
        assert!(state
            .click("session", "lease", &capture.target, &element("e1"), PointerButton::Left, 1)
            .unwrap_err()
            .message()
            .contains("use x/y"));
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert_eq!(
            state
                .click("session", "lease", &capture.target, &point(10.0, 10.0), PointerButton::Right, 2)
                .unwrap(),
            ComputerInputMethod::Pointer
        );
    }

    #[test]
    fn a_new_title_does_not_invalidate_the_window_but_new_geometry_does() {
        let (state, native) = armed();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        native.targets.lock().unwrap()[0].title = "another page".to_string();
        state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1)
            .unwrap();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        native.targets.lock().unwrap()[0].x = 40.0;
        assert!(matches!(
            state.click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1),
            Err(ComputerActionError::Rejected(message)) if message.contains("capture it again")
        ));
    }

    #[test]
    fn replacing_consuming_or_ending_a_capture_releases_its_element_map() {
        let (state, native) = armed();
        state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert_eq!(native.live_handles(), vec![2]);
        state.disarm("session");
        assert!(native.live_handles().is_empty());
    }

    #[test]
    fn activity_extends_the_grant_but_never_past_its_hard_limit() {
        let (state, _) = armed();
        let now = Instant::now();
        {
            let mut grants = state.grants.lock().unwrap();
            let grant = grants.get_mut("session").unwrap();
            grant.granted_at = now - GRANT_MAX_TTL + Duration::from_secs(60);
            grant.expires_at = now + Duration::from_secs(5);
        }
        state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        let expires_at = state.grants.lock().unwrap()["session"].expires_at;
        assert!(expires_at > now + Duration::from_secs(30));
        assert!(expires_at <= now + Duration::from_secs(60));

        let mut fresh = state.grants.lock().unwrap()["session"].clone();
        fresh.granted_at = now;
        fresh.touch(now + Duration::from_secs(120));
        assert_eq!(fresh.expires_at, now + Duration::from_secs(120) + GRANT_IDLE_TTL);
    }

    #[test]
    fn web_accessibility_is_switched_on_once_and_off_when_no_grant_allows_the_app() {
        let (state, native) = armed();
        *native.web_accessibility.lock().unwrap() = WebAccessibility::Changed;
        *native.tree_counts.lock().unwrap() = vec![2, 9, 9];
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        // The first capture waits for the web tree to stop growing.
        assert_eq!(capture.elements.count, 9);
        state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert_eq!(*native.web_calls.lock().unwrap(), vec![(42, true)]);
        state.disarm("session");
        assert_eq!(*native.web_calls.lock().unwrap(), vec![(42, true), (42, false)]);
    }

    #[test]
    fn web_accessibility_dcc_did_not_switch_on_is_left_alone() {
        let (state, native) = armed();
        *native.web_accessibility.lock().unwrap() = WebAccessibility::AlreadyEnabled;
        state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state.disarm("session");
        assert_eq!(*native.web_calls.lock().unwrap(), vec![(42, true)]);
    }

    #[test]
    fn native_failures_consume_the_capture_and_say_what_to_do() {
        let (state, native) = armed();
        *native.effect_error.lock().unwrap() =
            Some(NativeEffectError::Occluded(Some("Slack".to_string())));
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        let error = state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1)
            .unwrap_err();
        assert!(matches!(error, ComputerActionError::Failed(_)));
        assert!(error.message().starts_with("Slack covers that point"));
        assert!(state
            .click("session", "lease", &capture.target, &point(20.0, 20.0), PointerButton::Left, 1)
            .is_err());
        for code in 1..=10 {
            assert!(!NativeEffectError::from_code(code, None).message().is_empty());
        }
        assert_eq!(
            NativeEffectError::from_code(5, Some("  ".to_string())),
            NativeEffectError::Occluded(None)
        );
    }

    #[test]
    fn drag_and_scroll_bounds_are_checked_before_consuming_the_capture() {
        let (state, native) = armed();
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert!(matches!(
            state.drag("session", "lease", &capture.target, (10.0, 10.0), (800.0, 10.0)),
            Err(ComputerActionError::Rejected(_))
        ));
        assert!(matches!(
            state.scroll("session", "lease", &capture.target, &point(10.0, 10.0), 0, MAX_SCROLL_DELTA + 1),
            Err(ComputerActionError::Rejected(_))
        ));
        state
            .drag("session", "lease", &capture.target, (10.0, 10.0), (799.0, 599.0))
            .unwrap();
        assert_eq!(*native.effects.lock().unwrap(), 1);
    }

    #[test]
    fn element_refs_are_one_based_and_bounded() {
        assert_eq!(parse_element_ref("e1"), Some(0));
        assert_eq!(parse_element_ref("e800"), Some(799));
        for invalid in ["", "e", "e0", "e01", "e801", "E1", "e1x", "1"] {
            assert_eq!(parse_element_ref(invalid), None, "{invalid}");
        }
    }
}
#[cfg(target_os = "macos")]
unsafe fn take_string(raw: *mut std::ffi::c_char) -> String {
    if raw.is_null() {
        return "{}".to_string();
    }
    let value = std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned();
    dcc_computer_free_string(raw);
    value
}
#[cfg(target_os = "macos")]
extern "C" {
    fn dcc_computer_status_json() -> *mut std::ffi::c_char;
    fn dcc_computer_targets_json() -> *mut std::ffi::c_char;
    fn dcc_computer_request_access(kind: i32) -> bool;
    fn dcc_computer_capture_png(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        bytes: *mut *mut u8,
        length: *mut usize,
    ) -> bool;
    fn dcc_computer_ax_tree_json(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> *mut std::ffi::c_char;
    fn dcc_computer_ax_tree_release(handle: u64);
    fn dcc_computer_web_accessibility(pid: i32, enabled: bool) -> i32;
    fn dcc_computer_click_point(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        click_x: f64,
        click_y: f64,
        button: i32,
        click_count: u8,
        detail: *mut *mut std::ffi::c_char,
    ) -> i32;
    fn dcc_computer_click_element(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        handle: u64,
        index: u32,
        button: i32,
        click_count: u8,
        method: *mut i32,
        detail: *mut *mut std::ffi::c_char,
    ) -> i32;
    fn dcc_computer_scroll_point(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        scroll_x: f64,
        scroll_y: f64,
        delta_x: i32,
        delta_y: i32,
        detail: *mut *mut std::ffi::c_char,
    ) -> i32;
    fn dcc_computer_scroll_element(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        handle: u64,
        index: u32,
        delta_x: i32,
        delta_y: i32,
        detail: *mut *mut std::ffi::c_char,
    ) -> i32;
    fn dcc_computer_drag(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        detail: *mut *mut std::ffi::c_char,
    ) -> i32;
    fn dcc_computer_set_value(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        handle: u64,
        index: u32,
        value: *const std::ffi::c_char,
        detail: *mut *mut std::ffi::c_char,
    ) -> i32;
    fn dcc_computer_type(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        text: *const std::ffi::c_char,
    ) -> i32;
    fn dcc_computer_key(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        key_code: u16,
        flags: u64,
    ) -> i32;
    fn dcc_computer_free_string(value: *mut std::ffi::c_char);
    fn dcc_computer_free_bytes(value: *mut u8);
}

/// Stand-in for platforms without desktop computer use, and for tests that
/// need a supported platform which never touches the desktop.
#[cfg(any(test, not(target_os = "macos")))]
macro_rules! inert_effects {
    ($message:expr) => {
        fn accessibility_tree(&self, _: &ComputerTarget) -> Result<NativeTree, String> {
            Err($message.to_string())
        }
        fn release_tree(&self, _: u64) {}
        fn set_web_accessibility(&self, _: i32, _: bool) -> WebAccessibility {
            WebAccessibility::Unsupported
        }
        fn click(
            &self,
            _: &ComputerTarget,
            _: NativeAt,
            _: PointerButton,
            _: u8,
        ) -> Result<ComputerInputMethod, NativeEffectError> {
            Err(NativeEffectError::Other($message.to_string()))
        }
        fn scroll(&self, _: &ComputerTarget, _: NativeAt, _: i32, _: i32) -> Result<(), NativeEffectError> {
            Err(NativeEffectError::Other($message.to_string()))
        }
        fn drag(&self, _: &ComputerTarget, _: (f64, f64), _: (f64, f64)) -> Result<(), NativeEffectError> {
            Err(NativeEffectError::Other($message.to_string()))
        }
        fn set_value(&self, _: &ComputerTarget, _: u64, _: u32, _: &str) -> Result<(), NativeEffectError> {
            Err(NativeEffectError::Other($message.to_string()))
        }
        fn type_text(&self, _: &ComputerTarget, _: &str) -> Result<(), NativeEffectError> {
            Err(NativeEffectError::Other($message.to_string()))
        }
        fn key(&self, _: &ComputerTarget, _: u16, _: u64) -> Result<(), NativeEffectError> {
            Err(NativeEffectError::Other($message.to_string()))
        }
    };
}

#[cfg(not(target_os = "macos"))]
impl ComputerNative for PlatformComputerNative {
    fn status(&self) -> NativeStatus {
        NativeStatus {
            accessibility: false,
            screen_recording: false,
            major_version: 0,
        }
    }
    fn request_access(&self, _: ComputerAccessKind) -> bool {
        false
    }
    fn targets(&self) -> Result<Vec<ComputerTarget>, String> {
        Ok(vec![])
    }
    fn capture(&self, _: &ComputerTarget) -> Result<Vec<u8>, String> {
        Err("desktop computer use is unsupported on this platform".to_string())
    }
    inert_effects!("desktop computer use is unsupported on this platform");
}

#[cfg(test)]
struct SupportedTestNative;
#[cfg(test)]
impl ComputerNative for SupportedTestNative {
    fn status(&self) -> NativeStatus {
        NativeStatus {
            accessibility: true,
            screen_recording: true,
            major_version: 14,
        }
    }
    fn request_access(&self, _: ComputerAccessKind) -> bool {
        true
    }
    fn targets(&self) -> Result<Vec<ComputerTarget>, String> {
        Ok(vec![])
    }
    fn capture(&self, _: &ComputerTarget) -> Result<Vec<u8>, String> {
        Err("test native does not capture".to_string())
    }
    inert_effects!("test native does not act");
}
