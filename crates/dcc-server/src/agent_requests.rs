//! Agent requests that wait on the person outside the provider permission
//! path: `dcc_browser_open` (open the in-app Browser at a URL) and
//! `dcc_computer_request_control` (let the agent drive apps on the Mac).
//!
//! The desktop keeps its own queues for them (`browser_agent_requests`,
//! `computer_use_commands`) and its own dialogs. This module is the shape
//! both the desktop and the phone answer through, so the rule is one:
//!
//! - [`AgentRequestSource`] lists what is open and answers it. The desktop
//!   implements it over its real queues (`desktop_agent_requests`, in the app
//!   crate); `dccd-http` and the tests use [`InMemoryAgentRequests`].
//! - [`AnswerLedger`] makes the first answer win, from either side: an answer
//!   claims the request before it reaches the queue, a second one gets the
//!   first one's outcome (HTTP 409), and an answer that never reached the
//!   queue (it failed before) releases the claim so it can be given again —
//!   the same rule as `respond_to_permission_request_with_state`.
//!
//! What a phone may do differs by kind and is decided by the person, not
//! here: opening a URL can be approved remotely; a sign-in, MFA, CAPTCHA or
//! payment inside that page still needs the Mac's screen (the phone only
//! says so). Computer use can be approved remotely too, with an explicit
//! warning on the phone that nobody is in front of the Mac.
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

/// Answers remembered for the 409 of a late second answer.
const LEDGER_CAPACITY: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequestApp {
    pub bundle_id: String,
    pub name: String,
}

/// What the agent asks for. Serialized flat into [`PendingAgentRequest`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentRequestDetail {
    /// Open the in-app Browser at `url`.
    #[serde(rename_all = "camelCase")]
    BrowserOpen { url: String },
    /// Drive apps on the Mac. `apps` are the apps with a window open now
    /// (the person picks which ones); `blocker` says why approving cannot
    /// work yet (macOS permissions, conversation not attached), in pt-BR.
    #[serde(rename_all = "camelCase")]
    ComputerControl {
        apps: Vec<AgentRequestApp>,
        blocker: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAgentRequest {
    pub request_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub provider_id: String,
    pub reason: String,
    /// Unix ms; the agent gives up after it.
    pub expires_at_ms: u64,
    #[serde(flatten)]
    pub detail: AgentRequestDetail,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentRequestAnswer {
    pub allow: bool,
    /// Computer use: the apps the agent may drive (bundle ids from `apps`).
    #[serde(default)]
    pub allowed_bundle_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnswerOrigin {
    Desktop,
    Phone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentRequestDecision {
    Allowed,
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequestOutcome {
    pub request_id: String,
    pub decision: AgentRequestDecision,
    pub by: AnswerOrigin,
    /// RFC 3339.
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRequestError {
    /// Someone answered first; carries their answer.
    AlreadyAnswered(AgentRequestOutcome),
    /// Not open anymore (expired, cancelled by the agent) or never existed.
    NotFound,
    /// The answer itself is wrong (no app picked, unknown kind…).
    Invalid(String),
    Failed(String),
}

impl std::fmt::Display for AgentRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAnswered(outcome) => match (outcome.decision, outcome.by) {
                (AgentRequestDecision::Allowed, AnswerOrigin::Desktop) => {
                    f.write_str("Esse pedido já foi aprovado no computador.")
                }
                (AgentRequestDecision::Denied, AnswerOrigin::Desktop) => {
                    f.write_str("Esse pedido já foi negado no computador.")
                }
                (AgentRequestDecision::Allowed, AnswerOrigin::Phone) => {
                    f.write_str("Esse pedido já foi aprovado pelo celular.")
                }
                (AgentRequestDecision::Denied, AnswerOrigin::Phone) => {
                    f.write_str("Esse pedido já foi negado pelo celular.")
                }
            },
            Self::NotFound => f.write_str("Esse pedido não está mais aberto."),
            Self::Invalid(message) | Self::Failed(message) => f.write_str(message),
        }
    }
}

#[async_trait]
pub trait AgentRequestSource: Send + Sync {
    /// What is open now, cheap enough to poll: computer-use requests come
    /// without `apps` and `blocker`.
    fn pending(&self) -> Vec<PendingAgentRequest>;
    /// The same with the computer-use details (lists the Mac's windows).
    async fn pending_detailed(&self) -> Vec<PendingAgentRequest>;
    async fn respond(
        &self,
        request_id: &str,
        answer: AgentRequestAnswer,
        by: AnswerOrigin,
    ) -> Result<AgentRequestOutcome, AgentRequestError>;
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| u64::try_from(time.as_millis()).ok())
        .unwrap_or(u64::MAX)
}

/// Who answered what, so the first answer wins and a late one learns it.
#[derive(Default)]
pub struct AnswerLedger {
    inner: Mutex<LedgerInner>,
}

#[derive(Default)]
struct LedgerInner {
    answers: HashMap<String, AgentRequestOutcome>,
    order: VecDeque<String>,
}

/// A claimed answer. Dropped without [`AnswerClaim::settle`] it is released,
/// so an answer that never reached the queue can be given again.
pub struct AnswerClaim<'a> {
    ledger: &'a AnswerLedger,
    request_id: Option<String>,
    pub outcome: AgentRequestOutcome,
}

impl AnswerClaim<'_> {
    pub fn settle(mut self) -> AgentRequestOutcome {
        self.request_id = None;
        self.outcome.clone()
    }
}

impl Drop for AnswerClaim<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.request_id.take() {
            if let Ok(mut inner) = self.ledger.inner.lock() {
                inner.answers.remove(&id);
                inner.order.retain(|other| other != &id);
            }
        }
    }
}

impl AnswerLedger {
    /// Takes the request for this answer, or returns the answer that took it.
    pub fn claim(
        &self,
        request_id: &str,
        decision: AgentRequestDecision,
        by: AnswerOrigin,
    ) -> Result<AnswerClaim<'_>, AgentRequestOutcome> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = inner.answers.get(request_id) {
            return Err(existing.clone());
        }
        let outcome = AgentRequestOutcome {
            request_id: request_id.to_string(),
            decision,
            by,
            at: chrono::Utc::now().to_rfc3339(),
        };
        inner.answers.insert(request_id.to_string(), outcome.clone());
        inner.order.push_back(request_id.to_string());
        while inner.order.len() > LEDGER_CAPACITY {
            if let Some(old) = inner.order.pop_front() {
                inner.answers.remove(&old);
            }
        }
        Ok(AnswerClaim {
            ledger: self,
            request_id: Some(request_id.to_string()),
            outcome,
        })
    }

    pub fn outcome(&self, request_id: &str) -> Option<AgentRequestOutcome> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.answers.get(request_id).cloned())
    }
}

/// A queue with no desktop behind it: the agent side is whoever calls
/// [`InMemoryAgentRequests::enqueue`] (the tests, the simulator fixture).
#[derive(Default)]
pub struct InMemoryAgentRequests {
    pending: Mutex<HashMap<String, (PendingAgentRequest, oneshot::Sender<AgentRequestOutcome>)>>,
    ledger: AnswerLedger,
}

impl InMemoryAgentRequests {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Opens a request; the receiver gets the winning answer.
    pub fn enqueue(&self, request: PendingAgentRequest) -> oneshot::Receiver<AgentRequestOutcome> {
        let (sender, receiver) = oneshot::channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(request.request_id.clone(), (request, sender));
        }
        receiver
    }

    /// The agent gave up (timeout, cancelled call).
    pub fn cancel(&self, request_id: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(request_id);
        }
    }
}

#[async_trait]
impl AgentRequestSource for InMemoryAgentRequests {
    fn pending(&self) -> Vec<PendingAgentRequest> {
        let now = now_ms();
        let mut requests: Vec<_> = self
            .pending
            .lock()
            .map(|mut pending| {
                pending.retain(|_, (request, _)| request.expires_at_ms > now);
                pending.values().map(|(request, _)| request.clone()).collect()
            })
            .unwrap_or_default();
        requests.sort_by(|a, b| {
            (a.expires_at_ms, &a.request_id).cmp(&(b.expires_at_ms, &b.request_id))
        });
        requests
    }

    async fn pending_detailed(&self) -> Vec<PendingAgentRequest> {
        self.pending()
    }

    async fn respond(
        &self,
        request_id: &str,
        answer: AgentRequestAnswer,
        by: AnswerOrigin,
    ) -> Result<AgentRequestOutcome, AgentRequestError> {
        let decision = if answer.allow {
            AgentRequestDecision::Allowed
        } else {
            AgentRequestDecision::Denied
        };
        let claim = self
            .ledger
            .claim(request_id, decision, by)
            .map_err(AgentRequestError::AlreadyAnswered)?;
        let Some((request, sender)) = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(request_id))
        else {
            return Err(AgentRequestError::NotFound);
        };
        if answer.allow {
            if let AgentRequestDetail::ComputerControl { apps, .. } = &request.detail {
                if let Err(error) = validate_allowed_apps(apps, &answer.allowed_bundle_ids) {
                    // Put it back: the answer never reached the agent.
                    if let Ok(mut pending) = self.pending.lock() {
                        pending.insert(request_id.to_string(), (request, sender));
                    }
                    return Err(error);
                }
            }
        }
        let outcome = claim.settle();
        let _ = sender.send(outcome.clone());
        Ok(outcome)
    }
}

/// Computer use needs at least one app, and only apps the request listed.
pub fn validate_allowed_apps(
    apps: &[AgentRequestApp],
    allowed: &[String],
) -> Result<(), AgentRequestError> {
    if allowed.is_empty() {
        return Err(AgentRequestError::Invalid(
            "Escolha pelo menos um app que o agente pode controlar.".into(),
        ));
    }
    if allowed.len() > 16 || allowed.iter().any(|id| !apps.iter().any(|app| &app.bundle_id == id)) {
        return Err(AgentRequestError::Invalid(
            "Escolha apps da lista de janelas abertas no computador.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser(id: &str) -> PendingAgentRequest {
        PendingAgentRequest {
            request_id: id.into(),
            session_id: "s1".into(),
            workspace_id: "w1".into(),
            provider_id: "claude".into(),
            reason: "ver a doc".into(),
            expires_at_ms: now_ms() + 45_000,
            detail: AgentRequestDetail::BrowserOpen {
                url: "https://example.com".into(),
            },
        }
    }

    #[tokio::test]
    async fn the_first_answer_wins_and_the_second_learns_it() {
        let queue = InMemoryAgentRequests::new();
        let waiting = queue.enqueue(browser("bo-1"));
        let first = queue
            .respond("bo-1", AgentRequestAnswer { allow: true, ..Default::default() }, AnswerOrigin::Phone)
            .await
            .unwrap();
        assert_eq!(first.decision, AgentRequestDecision::Allowed);
        assert_eq!(waiting.await.unwrap(), first);
        let second = queue
            .respond("bo-1", AgentRequestAnswer::default(), AnswerOrigin::Desktop)
            .await
            .unwrap_err();
        assert_eq!(second, AgentRequestError::AlreadyAnswered(first));
        assert!(queue.pending().is_empty());
    }

    #[tokio::test]
    async fn an_answer_that_never_reached_the_agent_can_be_given_again() {
        let queue = InMemoryAgentRequests::new();
        let mut request = browser("cu-1");
        request.detail = AgentRequestDetail::ComputerControl {
            apps: vec![AgentRequestApp {
                bundle_id: "com.apple.Safari".into(),
                name: "Safari".into(),
            }],
            blocker: None,
        };
        let _waiting = queue.enqueue(request);
        let error = queue
            .respond("cu-1", AgentRequestAnswer { allow: true, allowed_bundle_ids: vec!["com.evil".into()] }, AnswerOrigin::Phone)
            .await
            .unwrap_err();
        assert!(matches!(error, AgentRequestError::Invalid(_)));
        let outcome = queue
            .respond(
                "cu-1",
                AgentRequestAnswer { allow: true, allowed_bundle_ids: vec!["com.apple.Safari".into()] },
                AnswerOrigin::Desktop,
            )
            .await
            .unwrap();
        assert_eq!(outcome.by, AnswerOrigin::Desktop);
    }

    #[test]
    fn a_dropped_claim_is_released() {
        let ledger = AnswerLedger::default();
        {
            let _claim = ledger
                .claim("x", AgentRequestDecision::Denied, AnswerOrigin::Phone)
                .unwrap();
            assert!(ledger.claim("x", AgentRequestDecision::Allowed, AnswerOrigin::Desktop).is_err());
        }
        assert!(ledger.outcome("x").is_none());
        let settled = ledger
            .claim("x", AgentRequestDecision::Allowed, AnswerOrigin::Desktop)
            .unwrap()
            .settle();
        assert_eq!(ledger.outcome("x"), Some(settled));
    }

    #[test]
    fn requests_serialize_flat_with_their_kind() {
        let value = serde_json::to_value(browser("bo-1")).unwrap();
        assert_eq!(value["kind"], "browserOpen");
        assert_eq!(value["url"], "https://example.com");
        assert_eq!(value["requestId"], "bo-1");
    }
}
