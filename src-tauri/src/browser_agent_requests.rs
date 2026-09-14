//! Broker for an MCP request to open the human-owned in-app Browser.
//!
//! The MCP listener never opens a WebView. It creates a short-lived request
//! for the matching workspace/session surface and waits for a UI decision.
//! An allow is incomplete until the native Browser lifecycle is independently
//! checked by [`BrowserState`].

use std::{collections::{HashMap, HashSet, VecDeque}, sync::{Arc, Mutex}, time::{Duration, SystemTime, UNIX_EPOCH}};

use serde::{Deserialize, Serialize};
use tokio::{sync::oneshot, time::timeout};
use uuid::Uuid;
use tauri::State;

use crate::browser_commands::{
    arm_browser_control_for_approved_open, browser_open_lifecycle_matches_request,
    revoke_browser_approved_lease, validate_browser_url, BrowserControlStatus, BrowserState,
};

pub const BROWSER_AGENT_REQUEST_TTL: Duration = Duration::from_secs(45);
const MAX_REASON_CHARS: usize = 500;
const MAX_PENDING_REQUESTS: usize = 64;
/// The bridge admits at most 128 live leases. Retain a small bounded tail so
/// a revoke which races a just-starting open request still wins without
/// accumulating one tombstone for every historical provider session.
const MAX_REVOKED_LEASES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAgentRequestScope {
    pub workspace_id: String,
    pub session_id: String,
    pub provider_id: String,
    pub lease_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentRequestView {
    pub request_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub provider_id: String,
    pub url: String,
    pub reason: String,
    pub expires_at_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAgentPendingInput {
    pub workspace_id: String,
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAgentResolveInput {
    pub request_id: String,
    pub workspace_id: String,
    pub session_id: String,
    pub decision: BrowserAgentRequestDecision,
    pub lifecycle_token: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserAgentCancelInput {
    pub request_id: String,
    pub workspace_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BrowserAgentRequestDecision { Allow, Deny }

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentOpenResult {
    pub status: BrowserAgentOpenStatus,
    pub lifecycle_token: Option<u64>,
    pub remaining_ms: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BrowserAgentOpenStatus { Approved, Denied, TimedOut, Cancelled, Failed }

struct PendingRequest {
    scope: BrowserAgentRequestScope,
    view: BrowserAgentRequestView,
    completion: oneshot::Sender<BrowserAgentOpenResult>,
}

#[derive(Clone, Default)]
pub struct BrowserAgentRequestBroker {
    pending: Arc<Mutex<HashMap<String, PendingRequest>>>,
    revoked_leases: Arc<Mutex<HashSet<(String, String)>>>,
    revoked_lease_order: Arc<Mutex<VecDeque<(String, String)>>>,
}

/// Dropping an HTTP/MCP future is cancellation, not an abandoned approval.
/// This guard removes the pending entry and wakes no detached task; the UI can
/// never approve an orphan after the client disconnects.
struct PendingRequestGuard {
    broker: BrowserAgentRequestBroker,
    request_id: String,
}

impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        self.broker.complete_if_pending(&self.request_id, cancelled());
    }
}

impl BrowserAgentRequestBroker {
    pub async fn request_open(
        &self,
        scope: BrowserAgentRequestScope,
        url: String,
        reason: String,
    ) -> BrowserAgentOpenResult {
        if !valid_scope(&scope) || validate_browser_url(&url).is_err() || !valid_text(&reason, MAX_REASON_CHARS) {
            return failed("browser open request is invalid");
        }
        if self.lease_is_revoked(&scope.session_id, &scope.lease_id) {
            return cancelled();
        }
        let request_id = format!("bo-{}", Uuid::new_v4().simple());
        let expires_at_ms = now_ms().saturating_add(BROWSER_AGENT_REQUEST_TTL.as_millis().min(u64::MAX as u128) as u64);
        let view = BrowserAgentRequestView {
            request_id: request_id.clone(), workspace_id: scope.workspace_id.clone(),
            session_id: scope.session_id.clone(), provider_id: scope.provider_id.clone(),
            url, reason, expires_at_ms,
        };
        let (completion, receiver) = oneshot::channel();
        {
            let Ok(mut pending) = self.pending.lock() else { return failed("browser open broker is unavailable"); };
            if pending.len() >= MAX_PENDING_REQUESTS { return failed("browser open request queue is full"); }
            pending.insert(request_id.clone(), PendingRequest { scope, view, completion });
        }
        let _guard = PendingRequestGuard { broker: self.clone(), request_id: request_id.clone() };
        match timeout(BROWSER_AGENT_REQUEST_TTL, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => failed("browser open request was cancelled"),
            Err(_) => {
                self.complete_if_pending(&request_id, timed_out());
                timed_out()
            }
        }
    }

    pub fn pending_for(&self, input: &BrowserAgentPendingInput) -> Vec<BrowserAgentRequestView> {
        self.pending.lock().map(|pending| pending.values()
            .filter(|request| request.scope.workspace_id == input.workspace_id && request.scope.session_id == input.session_id)
            .map(|request| request.view.clone()).collect()).unwrap_or_default()
    }

    pub fn pending_all(&self) -> Vec<BrowserAgentRequestView> {
        let mut requests: Vec<BrowserAgentRequestView> = self.pending.lock().map(|pending| pending.values().map(|request| request.view.clone()).collect()).unwrap_or_default();
        requests.sort_by(|left, right| (left.expires_at_ms, &left.request_id).cmp(&(right.expires_at_ms, &right.request_id)));
        requests
    }

    /// Resolving allow checks the actual native Browser state. A renderer
    /// cannot acknowledge a request for a different scope or invent a token.
    pub fn resolve(
        &self,
        browser: &BrowserState,
        input: BrowserAgentResolveInput,
    ) -> BrowserAgentOpenResult {
        let request = self.take_matching(&input.request_id, &input.workspace_id, &input.session_id);
        let Some(request) = request else { return failed("browser open request is unavailable"); };
        if request.view.expires_at_ms <= now_ms() {
            let result = timed_out();
            let _ = request.completion.send(result.clone());
            return result;
        }
        if self.lease_is_revoked(&request.scope.session_id, &request.scope.lease_id) {
            let result = cancelled();
            let _ = request.completion.send(result.clone());
            return result;
        }
        let result = match input.decision {
            BrowserAgentRequestDecision::Deny => BrowserAgentOpenResult { status: BrowserAgentOpenStatus::Denied, lifecycle_token: None, remaining_ms: 0, message: None },
            BrowserAgentRequestDecision::Allow => match input.lifecycle_token {
                Some(token) => match browser_open_lifecycle_matches_request(browser, &request.scope.workspace_id, Some(&request.scope.session_id), token, &request.view.url)
                    .and_then(|_| arm_browser_control_for_approved_open(browser, &request.scope.workspace_id, Some(&request.scope.session_id), token, &request.scope.lease_id)) {
                    Ok(BrowserControlStatus { remaining_ms, .. }) if !self.lease_is_revoked(&request.scope.session_id, &request.scope.lease_id) => BrowserAgentOpenResult { status: BrowserAgentOpenStatus::Approved, lifecycle_token: Some(token), remaining_ms, message: None },
                    Ok(_) => { revoke_browser_approved_lease(browser, &request.scope.lease_id); cancelled() },
                    Err(error) => failed(&error),
                },
                None => failed("browser open lifecycle is required"),
            },
        };
        let _ = request.completion.send(result.clone());
        result
    }

    pub fn cancel_matching(&self, input: &BrowserAgentCancelInput) -> bool {
        let request = self.take_matching(&input.request_id, &input.workspace_id, &input.session_id);
        if let Some(request) = request {
            let _ = request.completion.send(cancelled());
            true
        } else { false }
    }

    pub fn revoke_lease(&self, session_id: &str, lease_id: &str, browser: &BrowserState) {
        let lease = (session_id.to_string(), lease_id.to_string());
        if let (Ok(mut revoked), Ok(mut order)) =
            (self.revoked_leases.lock(), self.revoked_lease_order.lock())
        {
            if revoked.insert(lease.clone()) {
                order.push_back(lease);
            }
            while order.len() > MAX_REVOKED_LEASES {
                if let Some(expired) = order.pop_front() {
                    revoked.remove(&expired);
                }
            }
        }
        revoke_browser_approved_lease(browser, lease_id);
        let ids = self.pending.lock().map(|pending| pending.iter()
            .filter(|(_, request)| request.scope.session_id == session_id && request.scope.lease_id == lease_id)
            .map(|(id, _)| id.clone()).collect::<Vec<_>>()).unwrap_or_default();
        for id in ids { self.complete_if_pending(&id, cancelled()); }
    }

    pub fn revoke_all(&self, browser: &BrowserState) {
        let requests = self.pending.lock().map(|mut pending| pending.drain().map(|(_, request)| request).collect::<Vec<_>>()).unwrap_or_default();
        for request in requests {
            revoke_browser_approved_lease(browser, &request.scope.lease_id);
            let _ = request.completion.send(cancelled());
        }
        if let Ok(mut revoked) = self.revoked_leases.lock() {
            revoked.clear();
        }
        if let Ok(mut order) = self.revoked_lease_order.lock() {
            order.clear();
        }
    }

    fn take_matching(&self, request_id: &str, workspace_id: &str, session_id: &str) -> Option<PendingRequest> {
        let Ok(mut pending) = self.pending.lock() else { return None; };
        let matches = pending.get(request_id).is_some_and(|request| request.scope.workspace_id == workspace_id && request.scope.session_id == session_id);
        matches.then(|| pending.remove(request_id)).flatten()
    }

    fn complete_if_pending(&self, request_id: &str, result: BrowserAgentOpenResult) {
        if let Ok(mut pending) = self.pending.lock() {
            if let Some(request) = pending.remove(request_id) { let _ = request.completion.send(result); }
        }
    }

    fn lease_is_revoked(&self, session_id: &str, lease_id: &str) -> bool {
        self.revoked_leases.lock().is_ok_and(|revoked| revoked.contains(&(session_id.to_string(), lease_id.to_string())))
    }
}

fn valid_text(value: &str, max: usize) -> bool { !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control) }
fn valid_scope(scope: &BrowserAgentRequestScope) -> bool { valid_text(&scope.workspace_id, 128) && valid_text(&scope.session_id, 128) && valid_text(&scope.provider_id, 128) && valid_text(&scope.lease_id, 128) }
fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|time| u64::try_from(time.as_millis()).ok()).unwrap_or(u64::MAX) }
fn failed(message: &str) -> BrowserAgentOpenResult { BrowserAgentOpenResult { status: BrowserAgentOpenStatus::Failed, lifecycle_token: None, remaining_ms: 0, message: Some(message.to_string()) } }
fn timed_out() -> BrowserAgentOpenResult { BrowserAgentOpenResult { status: BrowserAgentOpenStatus::TimedOut, lifecycle_token: None, remaining_ms: 0, message: None } }
fn cancelled() -> BrowserAgentOpenResult { BrowserAgentOpenResult { status: BrowserAgentOpenStatus::Cancelled, lifecycle_token: None, remaining_ms: 0, message: None } }

#[tauri::command]
pub fn browser_agent_pending(
    broker: State<'_, BrowserAgentRequestBroker>,
    input: BrowserAgentPendingInput,
) -> Vec<BrowserAgentRequestView> {
    broker.pending_for(&input)
}

#[tauri::command]
pub fn browser_agent_pending_all(
    broker: State<'_, BrowserAgentRequestBroker>,
) -> Vec<BrowserAgentRequestView> {
    broker.pending_all()
}

#[tauri::command]
pub fn browser_agent_resolve(
    broker: State<'_, BrowserAgentRequestBroker>,
    browser: State<'_, BrowserState>,
    input: BrowserAgentResolveInput,
) -> BrowserAgentOpenResult {
    broker.resolve(&browser, input)
}

#[tauri::command]
pub fn browser_agent_cancel(
    broker: State<'_, BrowserAgentRequestBroker>,
    input: BrowserAgentCancelInput,
) -> bool {
    broker.cancel_matching(&input)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope(lease: &str) -> BrowserAgentRequestScope { BrowserAgentRequestScope { workspace_id: "workspace".into(), session_id: "session".into(), provider_id: "provider".into(), lease_id: lease.into() } }

    #[tokio::test]
    async fn cross_scope_resolve_cannot_consume_pending_request() {
        let broker = BrowserAgentRequestBroker::default(); let waiting = broker.clone();
        let task = tokio::spawn(async move { waiting.request_open(scope("lease"), "https://example.com".into(), "open site".into()).await });
        tokio::task::yield_now().await;
        let result = broker.resolve(&BrowserState::default(), BrowserAgentResolveInput { request_id: broker.pending_for(&BrowserAgentPendingInput {workspace_id:"workspace".into(),session_id:"session".into()})[0].request_id.clone(), workspace_id:"other".into(), session_id:"session".into(), decision: BrowserAgentRequestDecision::Deny, lifecycle_token: None });
        assert_eq!(result.status, BrowserAgentOpenStatus::Failed);
        assert_eq!(broker.pending_for(&BrowserAgentPendingInput {workspace_id:"workspace".into(),session_id:"session".into()}).len(), 1);
        broker.revoke_lease("session", "lease", &BrowserState::default());
        assert_eq!(task.await.unwrap().status, BrowserAgentOpenStatus::Cancelled);
    }

    #[tokio::test]
    async fn revoke_cancels_only_the_exact_lease() {
        let broker = BrowserAgentRequestBroker::default(); let one = broker.clone(); let two = broker.clone();
        let first = tokio::spawn(async move { one.request_open(scope("one"), "https://one.example".into(), "one".into()).await });
        let second = tokio::spawn(async move { two.request_open(scope("two"), "https://two.example".into(), "two".into()).await });
        tokio::task::yield_now().await; let browser = BrowserState::default(); broker.revoke_lease("session", "one", &browser);
        assert_eq!(first.await.unwrap().status, BrowserAgentOpenStatus::Cancelled);
        assert_eq!(broker.pending_for(&BrowserAgentPendingInput {workspace_id:"workspace".into(),session_id:"session".into()}).len(), 1);
        broker.revoke_lease("session", "two", &browser); assert_eq!(second.await.unwrap().status, BrowserAgentOpenStatus::Cancelled);
    }

    #[tokio::test]
    async fn dropping_the_mcp_wait_removes_its_pending_request() {
        let broker = BrowserAgentRequestBroker::default();
        let waiting = broker.clone();
        let task = tokio::spawn(async move {
            waiting.request_open(scope("lease"), "https://example.com".into(), "open".into()).await
        });
        tokio::task::yield_now().await;
        assert_eq!(broker.pending_all().len(), 1);
        task.abort();
        let _ = task.await;
        assert!(broker.pending_all().is_empty());
    }

    #[tokio::test]
    async fn revoked_lease_cannot_enqueue_or_later_reacquire_a_grant() {
        let broker = BrowserAgentRequestBroker::default();
        let browser = BrowserState::default();
        broker.revoke_lease("session", "lease", &browser);
        let result = broker
            .request_open(scope("lease"), "https://example.com".into(), "open".into())
            .await;
        assert_eq!(result.status, BrowserAgentOpenStatus::Cancelled);
        assert!(broker.pending_all().is_empty());
    }

    #[test]
    fn expired_request_is_consumed_without_arming_a_browser() {
        let broker = BrowserAgentRequestBroker::default();
        let (completion, receiver) = oneshot::channel();
        broker.pending.lock().unwrap().insert("expired".into(), PendingRequest {
            scope: scope("lease"),
            view: BrowserAgentRequestView { request_id: "expired".into(), workspace_id: "workspace".into(), session_id: "session".into(), provider_id: "provider".into(), url: "https://example.com".into(), reason: "open".into(), expires_at_ms: 0 },
            completion,
        });
        let result = broker.resolve(&BrowserState::default(), BrowserAgentResolveInput { request_id: "expired".into(), workspace_id: "workspace".into(), session_id: "session".into(), decision: BrowserAgentRequestDecision::Allow, lifecycle_token: Some(1) });
        assert_eq!(result.status, BrowserAgentOpenStatus::TimedOut);
        assert_eq!(receiver.blocking_recv().unwrap().status, BrowserAgentOpenStatus::TimedOut);
    }

    #[test]
    fn revoked_lease_tombstones_are_bounded() {
        let broker = BrowserAgentRequestBroker::default();
        let browser = BrowserState::default();
        for index in 0..=MAX_REVOKED_LEASES {
            broker.revoke_lease("session", &format!("lease-{index}"), &browser);
        }
        assert_eq!(broker.revoked_leases.lock().unwrap().len(), MAX_REVOKED_LEASES);
        assert!(!broker.lease_is_revoked("session", "lease-0"));
        assert!(broker.lease_is_revoked(
            "session",
            &format!("lease-{MAX_REVOKED_LEASES}")
        ));
    }
}
