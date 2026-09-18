//! App-owned, target-scoped desktop computer use for macOS.
//!
//! This is intentionally separate from Browser control. A provider lease only
//! proves that DCC can offer tools to a session; it never grants OS input or
//! screen access. Each session must be explicitly armed with a short-lived,
//! in-memory allowlist of bundle identifiers.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;
use tokio::sync::oneshot;
use tokio::time::timeout;

const GRANT_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ALLOWED_APPS: usize = 16;
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TYPE_CHARS: usize = 2_000;
const MAX_SCROLL_DELTA: i32 = 1_000;
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
    pub provider_supported: bool,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerCapture {
    pub target: ComputerTargetSnapshot,
    pub mime_type: &'static str,
    pub image_base64: String,
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

#[derive(Debug, Clone)]
struct CapturedSnapshot {
    visible: ComputerTargetSnapshot,
    identity: String,
}
#[derive(Debug, Clone)]
struct ComputerGrant {
    allowed_bundle_ids: HashSet<String>,
    expires_at: Instant,
    lease_id: String,
    nonce: String,
    snapshots: HashMap<u32, CapturedSnapshot>,
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

trait ComputerNative: Send + Sync {
    fn status(&self) -> NativeStatus;
    fn request_access(&self, kind: ComputerAccessKind) -> bool;
    fn targets(&self) -> Result<Vec<ComputerTarget>, String>;
    fn capture(&self, target: &ComputerTarget) -> Result<Vec<u8>, String>;
    fn click(&self, target: &ComputerTarget, x: f64, y: f64, click_count: u8)
        -> Result<(), String>;
    fn scroll(
        &self,
        target: &ComputerTarget,
        x: f64,
        y: f64,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), String>;
    fn type_text(&self, target: &ComputerTarget, text: &str) -> Result<(), String>;
    fn key(&self, target: &ComputerTarget, key_code: u16, flags: u64) -> Result<(), String>;
}

#[derive(Clone)]
pub struct ComputerUseState {
    native: Arc<dyn ComputerNative>,
    grants: Arc<Mutex<HashMap<String, ComputerGrant>>>,
    projected_sessions: Arc<Mutex<HashMap<String, String>>>,
    pending_control_requests: Arc<Mutex<HashMap<String, PendingControlRequest>>>,
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
        self.cancel_control_requests_for_session(session_id, None);
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
            provider_supported: runtime_attached,
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
        grants.insert(
            session_id.to_string(),
            ComputerGrant {
                allowed_bundle_ids: allowed,
                expires_at: Instant::now() + GRANT_TTL,
                lease_id: expected_lease_id.to_string(),
                nonce: random_token(),
                snapshots: HashMap::new(),
            },
        );
        drop(grants);
        Ok(self.grant_status(Some(session_id)))
    }
    pub(crate) fn disarm(&self, session_id: &str) -> ComputerGrantStatus {
        if let Ok(mut grants) = self.grants.lock() {
            grants.remove(session_id);
        }
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
    }
    fn require_target(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
    ) -> Result<ComputerTarget, String> {
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
            return Err("target window changed; capture it again before acting".to_string());
        }
        // Do not hold the grant lock while enumerating windows. This second
        // read closes disarm/re-arm races before the native driver receives
        // an event; the native bridge performs one final target check too.
        let valid = self.grants.lock().is_ok_and(|mut grants| {
            prune_expired(&mut grants);
            grants.get_mut(session_id).is_some_and(|grant| {
                grant.nonce == nonce
                    && grant.lease_id == expected_lease_id
                    && self.projection_lease(session_id).as_deref() == Some(expected_lease_id)
                    && grant
                        .snapshots
                        .remove(&target.window_id)
                        .is_some_and(|snapshot| snapshot.visible.generation == target.generation)
            })
        });
        if !valid {
            return Err(
                "desktop computer use grant changed; capture again before acting".to_string(),
            );
        }
        Ok(current)
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
                },
            );
            true
        });
        if !accepted {
            return Err("desktop computer use grant changed while capturing".to_string());
        }
        Ok(ComputerCapture {
            target: snapshot,
            mime_type: "image/png",
            image_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    }
    pub(crate) fn click(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        x: f64,
        y: f64,
        click_count: u8,
    ) -> Result<(), String> {
        let current = self.require_target(session_id, expected_lease_id, target)?;
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= current.width
            || y >= current.height
        {
            return Err("click coordinates are outside the captured target window".to_string());
        }
        // The native accessibility implementation performs one AXPress. It
        // does not emulate a dependable double-click, so accepting `2` would
        // report a stronger action than the platform actually performed.
        if click_count != 1 {
            return Err("click count must be one".to_string());
        }
        self.native.click(&current, x, y, click_count)
    }
    pub(crate) fn scroll(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        x: f64,
        y: f64,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), String> {
        let current = self.require_target(session_id, expected_lease_id, target)?;
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= current.width
            || y >= current.height
        {
            return Err("scroll coordinates are outside the captured target window".to_string());
        }
        if (delta_x == 0 && delta_y == 0)
            || delta_x.unsigned_abs() > MAX_SCROLL_DELTA as u32
            || delta_y.unsigned_abs() > MAX_SCROLL_DELTA as u32
        {
            return Err("scroll delta is invalid".to_string());
        }
        self.native.scroll(&current, x, y, delta_x, delta_y)
    }
    pub(crate) fn type_text(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        text: &str,
    ) -> Result<(), String> {
        if text.chars().count() > MAX_TYPE_CHARS
            || text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("desktop text input is invalid".to_string());
        }
        let current = self.require_target(session_id, expected_lease_id, target)?;
        self.native.type_text(&current, text)
    }
    pub(crate) fn key(
        &self,
        session_id: &str,
        expected_lease_id: &str,
        target: &ComputerTargetSnapshot,
        key: &str,
        modifiers: &[String],
    ) -> Result<(), String> {
        let current = self.require_target(session_id, expected_lease_id, target)?;
        let code = key_code(key).ok_or_else(|| "desktop key is unsupported".to_string())?;
        let flags = modifier_flags(modifiers)?;
        self.native.key(&current, code, flags)
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
            .ok_or_else(|| "capture the target window before acting".to_string())?;
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
fn target_identity(target: &ComputerTarget) -> String {
    let material = format!(
        "{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}",
        target.bundle_id,
        target.pid,
        target.window_id,
        target.title,
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
    state: State<'_, ComputerUseState>,
    input: ComputerUseArmInput,
) -> Result<ComputerGrantStatus, String> {
    state.arm(&input.session_id, &input.allowed_bundle_ids)
}
#[tauri::command]
pub fn computer_use_disarm(
    state: State<'_, ComputerUseState>,
    input: ComputerUseDisarmInput,
) -> ComputerGrantStatus {
    state.disarm(&input.session_id)
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
#[tauri::command]
pub async fn computer_use_respond_control_request(
    state: State<'_, ComputerUseState>,
    input: ComputerUseRespondControlRequestInput,
) -> Result<ComputerUseRespondControlRequestOutput, String> {
    state
        .respond_control_request(&input.request_id, input.allowed, &input.allowed_bundle_ids)
        .await
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
    fn click(
        &self,
        target: &ComputerTarget,
        x: f64,
        y: f64,
        click_count: u8,
    ) -> Result<(), String> {
        let bundle = std::ffi::CString::new(target.bundle_id.as_str())
            .map_err(|_| "target application is invalid".to_string())?;
        unsafe {
            dcc_computer_click(
                bundle.as_ptr(),
                target.pid,
                target.window_id,
                target.x,
                target.y,
                target.width,
                target.height,
                x,
                y,
                click_count,
            )
            .then_some(())
            .ok_or_else(|| "target click failed; capture again before retrying".to_string())
        }
    }
    fn scroll(
        &self,
        target: &ComputerTarget,
        x: f64,
        y: f64,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), String> {
        let bundle = std::ffi::CString::new(target.bundle_id.as_str())
            .map_err(|_| "target application is invalid".to_string())?;
        unsafe {
            dcc_computer_scroll(
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
            )
            .then_some(())
            .ok_or_else(|| "target scroll failed; capture again before retrying".to_string())
        }
    }
    fn type_text(&self, target: &ComputerTarget, text: &str) -> Result<(), String> {
        let bundle = std::ffi::CString::new(target.bundle_id.as_str())
            .map_err(|_| "target application is invalid".to_string())?;
        let text = std::ffi::CString::new(text)
            .map_err(|_| "desktop text input is invalid".to_string())?;
        unsafe {
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
            .then_some(())
            .ok_or_else(|| {
                "target keyboard input failed; capture again before retrying".to_string()
            })
        }
    }
    fn key(&self, target: &ComputerTarget, key_code: u16, flags: u64) -> Result<(), String> {
        let bundle = std::ffi::CString::new(target.bundle_id.as_str())
            .map_err(|_| "target application is invalid".to_string())?;
        unsafe {
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
            .then_some(())
            .ok_or_else(|| {
                "target keyboard input failed; capture again before retrying".to_string()
            })
        }
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
            }
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
        fn click(&self, _: &ComputerTarget, _: f64, _: f64, _: u8) -> Result<(), String> {
            *self.effects.lock().unwrap() += 1;
            Ok(())
        }
        fn scroll(&self, _: &ComputerTarget, _: f64, _: f64, _: i32, _: i32) -> Result<(), String> {
            *self.effects.lock().unwrap() += 1;
            Ok(())
        }
        fn type_text(&self, _: &ComputerTarget, _: &str) -> Result<(), String> {
            *self.effects.lock().unwrap() += 1;
            Ok(())
        }
        fn key(&self, _: &ComputerTarget, _: u16, _: u64) -> Result<(), String> {
            *self.effects.lock().unwrap() += 1;
            Ok(())
        }
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
            .click("session", "lease", &capture.target, 20.0, 20.0, 1)
            .unwrap();
        assert!(state
            .click("session", "lease", &capture.target, 20.0, 20.0, 1)
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
            .scroll("session", "lease", &capture.target, 20.0, 20.0, 0, -120)
            .unwrap();
        assert_eq!(*native.effects.lock().unwrap(), 1);

        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert!(state
            .scroll("session", "lease", &capture.target, 20.0, 20.0, 0, 0)
            .is_err());
        assert_eq!(*native.effects.lock().unwrap(), 1);

        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        assert!(state
            .click("session", "lease", &capture.target, 20.0, 20.0, 3)
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
            .click("session", "lease", &capture.target, 20.0, 20.0, 1)
            .is_err());
        let capture = state
            .capture("session", "lease", "com.example.Editor", 7)
            .unwrap();
        state
            .arm("session", &["com.example.Editor".to_string()])
            .unwrap();
        assert!(state
            .click("session", "lease", &capture.target, 20.0, 20.0, 1)
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
    fn dcc_computer_click(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        click_x: f64,
        click_y: f64,
        click_count: u8,
    ) -> bool;
    fn dcc_computer_scroll(
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
    ) -> bool;
    fn dcc_computer_type(
        bundle_id: *const std::ffi::c_char,
        pid: i32,
        window_id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        text: *const std::ffi::c_char,
    ) -> bool;
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
    ) -> bool;
    fn dcc_computer_free_string(value: *mut std::ffi::c_char);
    fn dcc_computer_free_bytes(value: *mut u8);
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
    fn click(&self, _: &ComputerTarget, _: f64, _: f64, _: u8) -> Result<(), String> {
        Err("desktop computer use is unsupported on this platform".to_string())
    }
    fn scroll(&self, _: &ComputerTarget, _: f64, _: f64, _: i32, _: i32) -> Result<(), String> {
        Err("desktop computer use is unsupported on this platform".to_string())
    }
    fn type_text(&self, _: &ComputerTarget, _: &str) -> Result<(), String> {
        Err("desktop computer use is unsupported on this platform".to_string())
    }
    fn key(&self, _: &ComputerTarget, _: u16, _: u64) -> Result<(), String> {
        Err("desktop computer use is unsupported on this platform".to_string())
    }
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
    fn click(&self, _: &ComputerTarget, _: f64, _: f64, _: u8) -> Result<(), String> {
        Err("test native does not click".to_string())
    }
    fn scroll(&self, _: &ComputerTarget, _: f64, _: f64, _: i32, _: i32) -> Result<(), String> {
        Err("test native does not scroll".to_string())
    }
    fn type_text(&self, _: &ComputerTarget, _: &str) -> Result<(), String> {
        Err("test native does not type".to_string())
    }
    fn key(&self, _: &ComputerTarget, _: u16, _: u64) -> Result<(), String> {
        Err("test native does not type".to_string())
    }
}
