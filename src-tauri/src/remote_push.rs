//! Push notifications to paired phones, through the Expo push service.
//!
//! The phone registers its Expo push token over the encrypted relay channel
//! (`PUT /api/v1/remote/push-token`, handled in `remote_connector`), with a
//! 48 h lease it renews on every connection — the lease is Paseo's
//! (`packages/server/src/server/push/index.ts`, `PUSH_TOKEN_LEASE_MS`). The
//! token lives next to the phone in `paired_devices` and goes away when the
//! phone is revoked.
//!
//! What notifies is what puts a task under "Precisa de você" on the desktop
//! and has an event of its own: a permission request, a question from the
//! agent, and a turn that finished or was interrupted (the sidebar's
//! `permission`, `input`, `completed` and `aborted`), and a task stopped by
//! its plan's usage limit with nothing that resumes it by itself
//! (`usageLimit`). A task that waits for its limit to reset and continues by
//! itself does not notify — not even as "interrupted". Besides those, the
//! HTTP backend's watcher (`http_api::attention`) hands over a Browser or
//! computer-use request from an agent (`agent_requests`) and a task that
//! newly has a git conflict (local or in its PR) or failing checks — each
//! once per appearance. Delegated edits to review, a snooze that ended and a
//! pending setup do not notify: they are not urgent.
//!
//! The text is generic on purpose — "DCC" and "<project> precisa de você" —
//! because it travels in the clear through Expo, Apple and Google. Never the
//! conversation, a command or a path: the phone fetches the rest over the
//! encrypted channel when the person opens it. The payload carries only the
//! session id (to open the thread) and the kind.
//!
//! Who gets one: every paired phone with a live token, except a phone whose
//! app is in the foreground on a connected channel (it shows the request
//! itself) and nobody while the person is using the desktop. Both rules are
//! Paseo's presence rule (`agent-attention-policy.ts`: a present client
//! suppresses the push) with Happy's "fail open" (`pushDispatch.ts`: without
//! proof the person is looking, send).
//!
//! Sending: events that arrive within a short window go in one request
//! (Expo takes up to 100 messages per call), each attention event once
//! (dedupe by request or turn), at most `rate_max` per phone per
//! `rate_window`, retried with exponential backoff on network errors, 429
//! and 5xx (Happy, `pushNotifications.ts`). Tickets and, later, receipts
//! that say `DeviceNotRegistered` forget the token (Paseo's `push-service.ts`
//! does the tickets; the receipts follow Expo's documentation).
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use dcc_core::ports::events::CoreEvent;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::pairing::{self, PushTarget};

/// Expo's push API; `DCC_PUSH_API_URL` points it elsewhere (tests, a fake
/// service to try notifications without an Expo project).
pub const EXPO_PUSH_API: &str = "https://exp.host/--/api/v2/push";
pub const PUSH_API_ENV: &str = "DCC_PUSH_API_URL";
/// How long a registered token is used without the phone coming back.
pub const TOKEN_LEASE: Duration = Duration::from_secs(48 * 60 * 60);
/// Android channel the app creates before it asks for a token.
pub const ANDROID_CHANNEL: &str = "attention";

const MAX_MESSAGES_PER_REQUEST: usize = 100;
const MAX_RECEIPTS_PER_REQUEST: usize = 1000;
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const SEEN_CAPACITY: usize = 2048;
const PROJECT_NAME_MAX: usize = 40;
const TITLE: &str = "DCC";

#[derive(Clone, Debug)]
pub struct PushConfig {
    /// Base of the push API: `<base>/send` and `<base>/getReceipts`.
    pub api_base: String,
    /// Events arriving this close together go out in one request.
    pub batch_window: Duration,
    pub max_attempts: u32,
    pub retry_base: Duration,
    /// Expo keeps receipts for a day and has them ready within ~15 minutes.
    pub receipt_delay: Duration,
    pub rate_window: Duration,
    pub rate_max: usize,
    pub request_timeout: Duration,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            api_base: EXPO_PUSH_API.to_string(),
            batch_window: Duration::from_millis(250),
            max_attempts: 4,
            retry_base: Duration::from_secs(1),
            receipt_delay: Duration::from_secs(15 * 60),
            rate_window: Duration::from_secs(60),
            rate_max: 5,
            request_timeout: Duration::from_secs(15),
        }
    }
}

impl PushConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(base) = std::env::var(PUSH_API_ENV) {
            let base = base.trim().trim_end_matches('/');
            if !base.is_empty() {
                config.api_base = base.to_string();
            }
        }
        config
    }
}

/// Timestamps for the token lease, fixed width so SQLite compares them as text.
pub fn lease_time(at: chrono::DateTime<chrono::Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// An Expo push token as the phone hands it over.
pub fn valid_push_token(token: &str) -> bool {
    let inner = token
        .strip_prefix("ExponentPushToken[")
        .or_else(|| token.strip_prefix("ExpoPushToken["))
        .and_then(|rest| rest.strip_suffix(']'));
    matches!(inner, Some(inner) if !inner.is_empty()
        && inner.len() <= 200
        && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AttentionKind {
    Permission,
    Question,
    Completed,
    Interrupted,
    /// `dcc_browser_open` or `dcc_computer_request_control` waiting.
    AgentRequest,
    /// Local merge conflicts, or the PR conflicts with its base.
    Conflict,
    ChecksFailing,
    /// Stopped by the usage limit, waiting for the person.
    UsageLimit,
    /// Stopped by the usage limit, continuing by itself after the reset.
    /// Never sent: it only claims the turn's key so the "interrupted" push
    /// for the same turn does not go out.
    LimitWait,
}

/// One "needs you" moment, from the event that starts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attention {
    pub kind: AttentionKind,
    pub session_id: String,
    /// Same request or same turn → same key: notified once.
    pub key: String,
}

impl Attention {
    pub fn agent_request(session_id: &str, request_id: &str) -> Self {
        Self {
            kind: AttentionKind::AgentRequest,
            session_id: session_id.to_string(),
            key: format!("agent:{request_id}"),
        }
    }
}

impl Attention {
    pub fn from_event(event: &CoreEvent) -> Option<Self> {
        let (kind, session_id, key) = match event {
            CoreEvent::SessionTurnPermissionRequested {
                session_id,
                request_id,
                ..
            } => (
                AttentionKind::Permission,
                session_id,
                format!("request:{session_id}:{request_id}"),
            ),
            CoreEvent::SessionTurnUserInputRequested {
                session_id,
                request_id,
                ..
            } => (
                AttentionKind::Question,
                session_id,
                format!("request:{session_id}:{request_id}"),
            ),
            CoreEvent::SessionTurnCompleted {
                session_id,
                turn_id,
            } => (
                AttentionKind::Completed,
                session_id,
                format!("turn:{session_id}:{turn_id}"),
            ),
            CoreEvent::SessionTurnAborted {
                session_id,
                turn_id,
                ..
            } => (
                AttentionKind::Interrupted,
                session_id,
                format!("turn:{session_id}:{turn_id}"),
            ),
            // Recorded just before the turn's abort, under the same key.
            CoreEvent::SessionUsageLimitReached {
                session_id,
                turn_id,
                paused,
                ..
            } => (
                if *paused {
                    AttentionKind::UsageLimit
                } else {
                    AttentionKind::LimitWait
                },
                session_id,
                format!("turn:{session_id}:{turn_id}"),
            ),
            _ => return None,
        };
        Some(Self {
            kind,
            session_id: session_id.clone(),
            key,
        })
    }
}

/// Title and body. `project` is the project's display name; anything that
/// looks like a path is dropped rather than sent.
pub fn notification_text(kind: AttentionKind, project: Option<&str>) -> (String, String) {
    let project = project.and_then(display_project);
    let body = match (kind, project) {
        (
            AttentionKind::Permission | AttentionKind::Question | AttentionKind::AgentRequest,
            Some(project),
        ) => format!("{project} precisa de você"),
        (
            AttentionKind::Permission | AttentionKind::Question | AttentionKind::AgentRequest,
            None,
        ) => "Uma tarefa precisa de você".to_string(),
        (AttentionKind::Conflict, Some(project)) => format!("{project}: conflito para resolver"),
        (AttentionKind::Conflict, None) => "Uma tarefa tem conflito para resolver".to_string(),
        (AttentionKind::ChecksFailing, Some(project)) => format!("{project}: checks falharam"),
        (AttentionKind::ChecksFailing, None) => "Os checks de uma tarefa falharam".to_string(),
        (AttentionKind::Completed, Some(project)) => format!("{project}: tarefa concluída"),
        (AttentionKind::Completed, None) => "Tarefa concluída".to_string(),
        (AttentionKind::Interrupted, Some(project)) => format!("{project}: tarefa interrompida"),
        (AttentionKind::Interrupted, None) => "Tarefa interrompida".to_string(),
        (AttentionKind::UsageLimit | AttentionKind::LimitWait, Some(project)) => {
            format!("{project}: parou no limite de uso")
        }
        (AttentionKind::UsageLimit | AttentionKind::LimitWait, None) => {
            "Uma tarefa parou no limite de uso".to_string()
        }
    };
    (TITLE.to_string(), body)
}

fn display_project(name: &str) -> Option<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.contains(['/', '\\', '~']) {
        return None;
    }
    if name.chars().count() <= PROJECT_NAME_MAX {
        return Some(name);
    }
    let short: String = name.chars().take(PROJECT_NAME_MAX - 1).collect();
    Some(format!("{}…", short.trim_end()))
}

/// One message of Expo's `/send` request.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushMessage {
    pub to: String,
    pub title: String,
    pub body: String,
    pub data: Value,
    pub sound: &'static str,
    pub priority: &'static str,
    pub channel_id: &'static str,
    #[serde(skip)]
    pub device_id: String,
}

/// Whether the person is already looking: at a phone's app (reported over
/// its channel) or at the desktop.
pub trait PushPresence: Send + Sync {
    fn device_in_foreground(&self, device_id: &str) -> bool;
    fn desktop_in_use(&self) -> bool;
}

#[derive(Default)]
struct Limits {
    seen_order: VecDeque<String>,
    seen: HashSet<String>,
    sent: HashMap<String, VecDeque<Instant>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Delivery {
    pub sent: usize,
    pub failed: usize,
    /// Tokens forgotten because Expo said the app is gone.
    pub forgotten: usize,
}

#[derive(Deserialize)]
struct Ticket {
    status: String,
    id: Option<String>,
    message: Option<String>,
    details: Option<TicketDetails>,
}

#[derive(Deserialize)]
struct TicketDetails {
    error: Option<String>,
}

impl Ticket {
    fn device_gone(&self) -> bool {
        self.status == "error"
            && self.details.as_ref().and_then(|d| d.error.as_deref()) == Some("DeviceNotRegistered")
    }
}

pub struct PushNotifier {
    config: PushConfig,
    db_path: PathBuf,
    http: reqwest::Client,
    presence: Arc<dyn PushPresence>,
    limits: StdMutex<Limits>,
}

impl PushNotifier {
    pub fn new(config: PushConfig, db_path: PathBuf, presence: Arc<dyn PushPresence>) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .unwrap_or_default();
        Arc::new(Self {
            config,
            db_path,
            http,
            presence,
            limits: StdMutex::new(Limits::default()),
        })
    }

    /// Follows the attention moments for as long as they flow.
    pub async fn run(self: Arc<Self>, mut events: broadcast::Receiver<Attention>) {
        use broadcast::error::RecvError;
        loop {
            let first = match events.recv().await {
                Ok(event) => event,
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            };
            let mut batch = vec![first];
            let deadline = tokio::time::Instant::now() + self.config.batch_window;
            loop {
                match tokio::time::timeout_at(deadline, events.recv()).await {
                    Ok(Ok(event)) => batch.push(event),
                    Ok(Err(RecvError::Lagged(_))) => continue,
                    Ok(Err(RecvError::Closed)) | Err(_) => break,
                }
            }
            let messages = self.plan_attentions(batch).await;
            if !messages.is_empty() {
                let notifier = self.clone();
                tokio::spawn(async move {
                    notifier.deliver(messages).await;
                });
            }
        }
    }

    /// The messages these events call for, after dedupe, presence and the
    /// per-phone rate limit.
    pub async fn plan(&self, events: &[CoreEvent]) -> Vec<PushMessage> {
        self.plan_attentions(events.iter().filter_map(Attention::from_event).collect())
            .await
    }

    pub async fn plan_attentions(&self, attentions: Vec<Attention>) -> Vec<PushMessage> {
        let attentions: Vec<Attention> = attentions
            .into_iter()
            .filter(|attention| self.first_time(&attention.key))
            .filter(|attention| attention.kind != AttentionKind::LimitWait)
            .collect();
        if attentions.is_empty() {
            return Vec::new();
        }
        if self.presence.desktop_in_use() {
            return Vec::new();
        }
        let sessions: Vec<String> = attentions
            .iter()
            .map(|attention| attention.session_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let db_path = self.db_path.clone();
        let lookup = tokio::task::spawn_blocking(move || lookup(&db_path, &sessions)).await;
        let (targets, sessions) = match lookup {
            Ok(Ok(found)) => found,
            Ok(Err(error)) => {
                eprintln!("[DCC Push] could not read push targets: {error}");
                return Vec::new();
            }
            Err(error) => {
                eprintln!("[DCC Push] could not read push targets: {error}");
                return Vec::new();
            }
        };
        if targets.is_empty() {
            return Vec::new();
        }

        let mut messages = Vec::new();
        for attention in attentions {
            let info = sessions.get(&attention.session_id);
            let delegated = info.is_some_and(|info| info.delegated);
            // A delegated task's result goes back to the agent that asked
            // for it, not to the person; its requests still need them.
            if delegated
                && matches!(
                    attention.kind,
                    AttentionKind::Completed | AttentionKind::Interrupted
                )
            {
                continue;
            }
            let (title, body) = notification_text(
                attention.kind,
                info.and_then(|info| info.project.as_deref()),
            );
            for target in &targets {
                if self.presence.device_in_foreground(&target.device_id) {
                    continue;
                }
                if !self.within_rate(&target.device_id) {
                    eprintln!(
                        "[DCC Push] rate limit: skipped a notification for device {}",
                        target.device_id
                    );
                    continue;
                }
                messages.push(PushMessage {
                    to: target.token.clone(),
                    title: title.clone(),
                    body: body.clone(),
                    data: json!({
                        "sessionId": attention.session_id,
                        "kind": attention.kind,
                    }),
                    sound: "default",
                    priority: "high",
                    channel_id: ANDROID_CHANNEL,
                    device_id: target.device_id.clone(),
                });
            }
        }
        messages
    }

    /// Sends in requests of up to 100, then checks the receipts later.
    pub async fn deliver(self: &Arc<Self>, messages: Vec<PushMessage>) -> Delivery {
        let mut delivery = Delivery::default();
        let mut receipts = Vec::new();
        for chunk in messages.chunks(MAX_MESSAGES_PER_REQUEST) {
            let tickets = match self.post("send", &chunk).await.and_then(|body| {
                serde_json::from_value::<Vec<Ticket>>(body["data"].clone())
                    .map_err(|error| format!("unexpected answer: {error}"))
            }) {
                Ok(tickets) if tickets.len() == chunk.len() => tickets,
                Ok(_) => {
                    eprintln!("[DCC Push] Expo answered a different number of tickets");
                    delivery.failed += chunk.len();
                    continue;
                }
                Err(error) => {
                    eprintln!(
                        "[DCC Push] sending {} notification(s) failed: {error}",
                        chunk.len()
                    );
                    delivery.failed += chunk.len();
                    continue;
                }
            };
            for (message, ticket) in chunk.iter().zip(tickets) {
                if ticket.status == "ok" {
                    delivery.sent += 1;
                    if let Some(id) = ticket.id {
                        receipts.push((id, message.to.clone()));
                    }
                    continue;
                }
                delivery.failed += 1;
                eprintln!(
                    "[DCC Push] not delivered to device {}: {}",
                    message.device_id,
                    ticket.message.as_deref().unwrap_or("error")
                );
                if ticket.device_gone() && self.forget(message.to.clone()).await {
                    delivery.forgotten += 1;
                }
            }
        }
        if !receipts.is_empty() {
            let notifier = self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(notifier.config.receipt_delay).await;
                notifier.check_receipts(receipts).await;
            });
        }
        delivery
    }

    /// Reads Expo's receipts for `(ticket id, token)` and forgets the tokens
    /// of apps that are gone. Returns how many were forgotten.
    pub async fn check_receipts(&self, tickets: Vec<(String, String)>) -> usize {
        let mut forgotten = 0;
        for chunk in tickets.chunks(MAX_RECEIPTS_PER_REQUEST) {
            let ids: Vec<&str> = chunk.iter().map(|(id, _)| id.as_str()).collect();
            let receipts = match self.post("getReceipts", &json!({ "ids": ids })).await {
                Ok(body) => body["data"].clone(),
                Err(error) => {
                    eprintln!("[DCC Push] reading receipts failed: {error}");
                    continue;
                }
            };
            for (id, token) in chunk {
                let Ok(receipt) = serde_json::from_value::<Ticket>(receipts[id].clone()) else {
                    continue;
                };
                if receipt.device_gone() && self.forget(token.clone()).await {
                    forgotten += 1;
                }
            }
        }
        forgotten
    }

    async fn forget(&self, token: String) -> bool {
        let db_path = self.db_path.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
            let conn = open(&db_path)?;
            let devices = pairing::forget_push_token(&conn, &token).map_err(|e| e.to_string())?;
            for device in &devices {
                pairing::record_audit(
                    &conn,
                    "push_token_forgotten",
                    Some(device),
                    None,
                    None,
                    Some("{\"reason\":\"DeviceNotRegistered\"}"),
                );
            }
            Ok(devices)
        })
        .await;
        match result {
            Ok(Ok(devices)) => !devices.is_empty(),
            Ok(Err(error)) => {
                eprintln!("[DCC Push] could not forget a token: {error}");
                false
            }
            Err(error) => {
                eprintln!("[DCC Push] could not forget a token: {error}");
                false
            }
        }
    }

    /// POSTs JSON to the push API, retrying what may pass on a second try.
    async fn post(&self, path: &str, body: &impl Serialize) -> Result<Value, String> {
        let url = format!("{}/{path}", self.config.api_base);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let outcome = self
                .http
                .post(&url)
                .header("accept", "application/json")
                .json(body)
                .send()
                .await;
            let retryable = match outcome {
                Ok(response) if response.status().is_success() => {
                    return response.json::<Value>().await.map_err(|e| e.to_string());
                }
                Ok(response)
                    if response.status().as_u16() == 429 || response.status().is_server_error() =>
                {
                    format!("HTTP {}", response.status())
                }
                // Our request is wrong; sending it again will not help.
                Ok(response) => return Err(format!("HTTP {}", response.status())),
                Err(error) => error.to_string(),
            };
            if attempt >= self.config.max_attempts {
                return Err(format!("{retryable} after {attempt} attempts"));
            }
            tokio::time::sleep(self.backoff(attempt)).await;
        }
    }

    fn backoff(&self, attempt: u32) -> Duration {
        let exp = self
            .config
            .retry_base
            .saturating_mul(1 << (attempt - 1).min(16))
            .min(MAX_BACKOFF);
        let jitter = exp.mul_f64(rand::random::<f64>() * 0.25);
        exp + jitter
    }

    fn first_time(&self, key: &str) -> bool {
        let mut limits = self.limits.lock().unwrap_or_else(|p| p.into_inner());
        if !limits.seen.insert(key.to_string()) {
            return false;
        }
        limits.seen_order.push_back(key.to_string());
        if limits.seen_order.len() > SEEN_CAPACITY {
            if let Some(old) = limits.seen_order.pop_front() {
                limits.seen.remove(&old);
            }
        }
        true
    }

    fn within_rate(&self, device_id: &str) -> bool {
        let now = Instant::now();
        let mut limits = self.limits.lock().unwrap_or_else(|p| p.into_inner());
        let sent = limits.sent.entry(device_id.to_string()).or_default();
        while sent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= self.config.rate_window)
        {
            sent.pop_front();
        }
        if sent.len() >= self.config.rate_max {
            return false;
        }
        sent.push_back(now);
        true
    }
}

struct SessionInfo {
    project: Option<String>,
    delegated: bool,
}

fn open(db_path: &std::path::Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn lookup(
    db_path: &std::path::Path,
    sessions: &[String],
) -> Result<(Vec<PushTarget>, HashMap<String, SessionInfo>), String> {
    let conn = open(db_path)?;
    let targets =
        pairing::push_targets(&conn, &lease_time(chrono::Utc::now())).map_err(|e| e.to_string())?;
    let mut info = HashMap::new();
    if targets.is_empty() {
        return Ok((targets, info));
    }
    for session in sessions {
        info.insert(
            session.clone(),
            SessionInfo {
                project: project_name(&conn, session),
                delegated: is_delegated(&conn, session),
            },
        );
    }
    Ok((targets, info))
}

/// The project's display name. A missing table (a bare `dccd-http`
/// database) or row just means the generic text.
fn project_name(conn: &Connection, session_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT COALESCE(NULLIF(TRIM(r.display_name), ''), r.name)
         FROM dcc_sessions s JOIN dcc_repositories r ON r.project_id = s.project_id
         WHERE s.id = ?1
         ORDER BY r.display_name IS NULL, r.created_at
         LIMIT 1",
        params![session_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .ok()
    .flatten()
    .flatten()
}

fn is_delegated(conn: &Connection, session_id: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM dcc_delegations WHERE child_session_id = ?1)",
        params![session_id],
        |row| row.get(0),
    )
    .unwrap_or(false)
}

/// macOS: seconds since the last keyboard or mouse event anywhere.
#[cfg(target_os = "macos")]
pub fn system_idle() -> Option<Duration> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }
    // kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType.
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

#[cfg(not(target_os = "macos"))]
pub fn system_idle() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::atomic::{AtomicBool, Ordering};

    // ---------- a fake Expo push service ----------

    #[derive(Default)]
    struct Fake {
        sends: Vec<Value>,
        receipt_requests: Vec<Value>,
        /// Answer the next N `/send` calls with this status.
        fail_next: usize,
        fail_status: u16,
    }

    type Shared = Arc<StdMutex<Fake>>;

    /// Tokens with `gone` are refused at once (ticket), with `stale` they
    /// are accepted and the receipt says the app is gone.
    async fn send(State(fake): State<Shared>, Json(body): Json<Value>) -> Response {
        let mut fake = fake.lock().unwrap();
        if fake.fail_next > 0 {
            fake.fail_next -= 1;
            let status = StatusCode::from_u16(fake.fail_status).unwrap();
            return (status, Json(json!({ "errors": [{ "code": "TRY_AGAIN" }] }))).into_response();
        }
        fake.sends.push(body.clone());
        let tickets: Vec<Value> = body
            .as_array()
            .unwrap()
            .iter()
            .map(|message| {
                let to = message["to"].as_str().unwrap();
                if to.contains("gone") {
                    json!({
                        "status": "error",
                        "message": "not a registered device",
                        "details": { "error": "DeviceNotRegistered" }
                    })
                } else {
                    json!({ "status": "ok", "id": format!("ticket-{to}") })
                }
            })
            .collect();
        Json(json!({ "data": tickets })).into_response()
    }

    async fn receipts(State(fake): State<Shared>, Json(body): Json<Value>) -> Response {
        let mut fake = fake.lock().unwrap();
        fake.receipt_requests.push(body.clone());
        let mut data = serde_json::Map::new();
        for id in body["ids"].as_array().unwrap() {
            let id = id.as_str().unwrap();
            let receipt = if id.contains("stale") {
                json!({ "status": "error", "details": { "error": "DeviceNotRegistered" } })
            } else {
                json!({ "status": "ok" })
            };
            data.insert(id.to_string(), receipt);
        }
        Json(json!({ "data": data })).into_response()
    }

    async fn fake_expo() -> (String, Shared) {
        let fake: Shared = Arc::default();
        let app = Router::new()
            .route("/send", post(send))
            .route("/getReceipts", post(receipts))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), fake)
    }

    // ---------- the desktop side ----------

    #[derive(Default)]
    struct Presence {
        foreground: StdMutex<HashSet<String>>,
        desktop: AtomicBool,
    }

    impl PushPresence for Presence {
        fn device_in_foreground(&self, device_id: &str) -> bool {
            self.foreground.lock().unwrap().contains(device_id)
        }
        fn desktop_in_use(&self) -> bool {
            self.desktop.load(Ordering::SeqCst)
        }
    }

    struct Desk {
        _dir: tempfile::TempDir,
        db: PathBuf,
        notifier: Arc<PushNotifier>,
        presence: Arc<Presence>,
        fake: Shared,
    }

    fn config(api_base: String) -> PushConfig {
        PushConfig {
            api_base,
            batch_window: Duration::from_millis(50),
            retry_base: Duration::from_millis(5),
            receipt_delay: Duration::from_millis(20),
            ..PushConfig::default()
        }
    }

    async fn desk_with(adjust: impl FnOnce(&mut PushConfig)) -> Desk {
        let (base, fake) = fake_expo().await;
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("state.sqlite");
        let conn = Connection::open(&db).unwrap();
        crate::remote_connector::ensure_remote_schema(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE dcc_sessions (id TEXT PRIMARY KEY, project_id TEXT NOT NULL);
             CREATE TABLE dcc_repositories (id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
                 name TEXT NOT NULL, display_name TEXT, created_at TEXT);
             CREATE TABLE dcc_delegations (id TEXT PRIMARY KEY, child_session_id TEXT);
             INSERT INTO dcc_sessions VALUES ('s1', 'p1'), ('s2', 'p2'), ('child', 'p1');
             INSERT INTO dcc_repositories VALUES
                 ('r1', 'p1', 'acido', ' Projeto   Ácido ', '2026-01-01'),
                 ('r2', 'p2', '/Users/me/secret-repo', NULL, '2026-01-01');
             INSERT INTO dcc_delegations VALUES ('d1', 'child');",
        )
        .unwrap();
        let mut config = config(base);
        adjust(&mut config);
        let presence = Arc::new(Presence::default());
        let notifier = PushNotifier::new(config, db.clone(), presence.clone());
        Desk {
            _dir: dir,
            db,
            notifier,
            presence,
            fake,
        }
    }

    async fn desk() -> Desk {
        desk_with(|_| {}).await
    }

    impl Desk {
        fn conn(&self) -> Connection {
            Connection::open(&self.db).unwrap()
        }

        /// A paired phone with a live token.
        fn phone(&self, key: u8, token: &str) -> String {
            let conn = self.conn();
            let device =
                pairing::register_relay_device(&conn, &[key; 32], "Phone", Some("ios"), "FP")
                    .unwrap();
            let until = lease_time(chrono::Utc::now() + chrono::Duration::hours(48));
            pairing::set_push_token(&conn, &device, token, Some("ios"), &until).unwrap();
            device
        }

        fn sent(&self) -> Vec<Value> {
            self.fake.lock().unwrap().sends.clone()
        }

        fn token_of(&self, device: &str) -> Option<String> {
            self.conn()
                .query_row(
                    "SELECT push_token FROM paired_devices WHERE device_id = ?1",
                    params![device],
                    |row| row.get(0),
                )
                .unwrap()
        }
    }

    fn permission(session: &str, request: &str) -> CoreEvent {
        CoreEvent::SessionTurnPermissionRequested {
            session_id: session.to_string(),
            turn_id: "t1".to_string(),
            request_id: request.to_string(),
            tool_name: "Bash".to_string(),
            title: Some("Apagar a pasta build".to_string()),
            description: Some("O agente quer limpar o cache".to_string()),
            command: Some("rm -rf /Users/me/secret-repo/build".to_string()),
            file: Some("/Users/me/secret-repo/src/main.rs".to_string()),
        }
    }

    fn completed(session: &str, turn: &str) -> CoreEvent {
        CoreEvent::SessionTurnCompleted {
            session_id: session.to_string(),
            turn_id: turn.to_string(),
        }
    }

    async fn notify(desk: &Desk, events: &[CoreEvent]) -> Delivery {
        let messages = desk.notifier.plan(events).await;
        desk.notifier.deliver(messages).await
    }

    // ---------- tests ----------

    #[tokio::test]
    async fn every_phone_in_one_request_with_generic_text_only() {
        let desk = desk().await;
        let a = desk.phone(1, "ExponentPushToken[aaa]");
        let b = desk.phone(2, "ExponentPushToken[bbb]");

        let delivery = notify(&desk, &[permission("s1", "r1")]).await;
        assert_eq!(
            delivery,
            Delivery {
                sent: 2,
                failed: 0,
                forgotten: 0
            }
        );

        let sent = desk.sent();
        assert_eq!(sent.len(), 1, "one request for both phones");
        let messages = sent[0].as_array().unwrap();
        let to: Vec<&str> = messages.iter().map(|m| m["to"].as_str().unwrap()).collect();
        assert_eq!(to, ["ExponentPushToken[aaa]", "ExponentPushToken[bbb]"]);
        for message in messages {
            assert_eq!(message["title"], "DCC");
            assert_eq!(message["body"], "Projeto Ácido precisa de você");
            assert_eq!(
                message["data"],
                json!({ "sessionId": "s1", "kind": "permission" })
            );
            assert_eq!(message["channelId"], ANDROID_CHANNEL);
            assert_eq!(message["sound"], "default");
            assert!(message.get("deviceId").is_none());
        }
        // Nothing from the request itself leaves the Mac.
        let wire = sent[0].to_string();
        for secret in [
            "rm -rf",
            "/Users",
            "secret-repo",
            "Apagar",
            "cache",
            "main.rs",
            "Bash",
            &a,
            &b,
        ] {
            assert!(!wire.contains(secret), "push carried {secret:?}: {wire}");
        }
    }

    fn limited(session: &str, turn: &str, paused: bool) -> Vec<CoreEvent> {
        vec![
            CoreEvent::SessionUsageLimitReached {
                session_id: session.to_string(),
                turn_id: turn.to_string(),
                resets_at: None,
                auto_resume: true,
                paused,
            },
            CoreEvent::SessionTurnAborted {
                session_id: session.to_string(),
                turn_id: turn.to_string(),
                reason: Some("You've hit your usage limit".to_string()),
            },
        ]
    }

    #[tokio::test]
    async fn a_usage_limit_notifies_only_when_it_needs_the_person_and_once() {
        let desk = desk().await;
        desk.phone(1, "ExponentPushToken[aaa]");
        // Waiting for the reset, continuing by itself: no push at all, not
        // even "interrupted".
        let delivery = notify(&desk, &limited("s1", "t1", false)).await;
        assert_eq!(delivery.sent, 0);
        assert!(desk.sent().is_empty());

        // No known reset: one push, and the abort that follows adds none.
        let delivery = notify(&desk, &limited("s1", "t2", true)).await;
        assert_eq!(delivery.sent, 1);
        let sent = desk.sent();
        let message = &sent[0].as_array().unwrap()[0];
        assert_eq!(message["body"], "Projeto Ácido: parou no limite de uso");
        assert_eq!(message["data"], json!({ "sessionId": "s1", "kind": "usageLimit" }));
        let again = notify(&desk, &limited("s1", "t2", true)).await;
        assert_eq!(again.sent, 0);
    }

    #[tokio::test]
    async fn a_project_name_that_looks_like_a_path_falls_back_to_generic_text() {
        let desk = desk().await;
        desk.phone(1, "ExponentPushToken[aaa]");
        notify(
            &desk,
            &[permission("s2", "r1"), completed("unknown-session", "t9")],
        )
        .await;
        let sent = desk.sent();
        let bodies: Vec<&str> = sent[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["body"].as_str().unwrap())
            .collect();
        assert_eq!(bodies, ["Uma tarefa precisa de você", "Tarefa concluída"]);
    }

    #[test]
    fn texts_by_kind() {
        let project = Some("api");
        assert_eq!(
            notification_text(AttentionKind::Question, project).1,
            "api precisa de você"
        );
        assert_eq!(
            notification_text(AttentionKind::Completed, project).1,
            "api: tarefa concluída"
        );
        assert_eq!(
            notification_text(AttentionKind::Interrupted, project).1,
            "api: tarefa interrompida"
        );
        assert_eq!(
            notification_text(AttentionKind::Interrupted, None).1,
            "Tarefa interrompida"
        );
        assert_eq!(
            notification_text(AttentionKind::Permission, Some("~/work")).1,
            "Uma tarefa precisa de você"
        );
        assert_eq!(
            notification_text(AttentionKind::AgentRequest, project).1,
            "api precisa de você"
        );
        assert_eq!(
            notification_text(AttentionKind::Conflict, project).1,
            "api: conflito para resolver"
        );
        assert_eq!(
            notification_text(AttentionKind::ChecksFailing, None).1,
            "Os checks de uma tarefa falharam"
        );
        let long = "x".repeat(80);
        let body = notification_text(AttentionKind::Completed, Some(&long)).1;
        assert!(
            body.starts_with(&"x".repeat(39)) && body.contains('…') && body.chars().count() < 70
        );
    }

    #[test]
    fn only_expo_tokens_are_accepted() {
        assert!(valid_push_token("ExponentPushToken[xXx-123_abc]"));
        assert!(valid_push_token("ExpoPushToken[abc]"));
        for bad in [
            "",
            "ExponentPushToken[]",
            "ExponentPushToken[a b]",
            "ExponentPushToken[abc",
            "fcm:abc",
            "ExponentPushToken[abc]\n",
        ] {
            assert!(!valid_push_token(bad), "{bad:?}");
        }
        assert!(!valid_push_token(&format!(
            "ExponentPushToken[{}]",
            "a".repeat(201)
        )));
    }

    #[tokio::test]
    async fn only_needs_you_events_notify() {
        let other = CoreEvent::SessionTurnAssistantMessageDelta {
            session_id: "s1".to_string(),
            turn_id: "t1".to_string(),
            message_id: "m".to_string(),
            content: "oi".to_string(),
        };
        assert!(Attention::from_event(&other).is_none());
        let question = CoreEvent::SessionTurnUserInputRequested {
            session_id: "s1".to_string(),
            turn_id: "t1".to_string(),
            request_id: "q1".to_string(),
            questions: Vec::new(),
        };
        assert_eq!(
            Attention::from_event(&question).unwrap().kind,
            AttentionKind::Question
        );
        let aborted = CoreEvent::SessionTurnAborted {
            session_id: "s1".to_string(),
            turn_id: "t1".to_string(),
            reason: Some("Stopped".to_string()),
        };
        assert_eq!(
            Attention::from_event(&aborted).unwrap().kind,
            AttentionKind::Interrupted
        );
    }

    #[tokio::test]
    async fn requests_of_more_than_a_hundred_are_split() {
        let desk = desk().await;
        let messages: Vec<PushMessage> = (0..150)
            .map(|n| PushMessage {
                to: format!("ExponentPushToken[t{n}]"),
                title: "DCC".to_string(),
                body: "x".to_string(),
                data: json!({}),
                sound: "default",
                priority: "high",
                channel_id: ANDROID_CHANNEL,
                device_id: format!("d{n}"),
            })
            .collect();
        let delivery = desk.notifier.deliver(messages).await;
        assert_eq!(delivery.sent, 150);
        let sizes: Vec<usize> = desk
            .sent()
            .iter()
            .map(|s| s.as_array().unwrap().len())
            .collect();
        assert_eq!(sizes, [100, 50]);
    }

    #[tokio::test]
    async fn each_request_and_each_turn_notifies_once() {
        let desk = desk().await;
        desk.phone(1, "ExponentPushToken[aaa]");
        let aborted = CoreEvent::SessionTurnAborted {
            session_id: "s1".to_string(),
            turn_id: "t1".to_string(),
            reason: None,
        };
        let first = desk
            .notifier
            .plan(&[
                permission("s1", "r1"),
                permission("s1", "r1"),
                completed("s1", "t1"),
            ])
            .await;
        assert_eq!(first.len(), 2);
        let again = desk
            .notifier
            .plan(&[permission("s1", "r1"), aborted, completed("s1", "t1")])
            .await;
        assert!(again.is_empty(), "already notified: {again:?}");
        // Another request in the same session is news.
        assert_eq!(desk.notifier.plan(&[permission("s1", "r2")]).await.len(), 1);
    }

    #[tokio::test]
    async fn each_phone_has_a_rate_limit() {
        let desk = desk_with(|config| config.rate_max = 3).await;
        desk.phone(1, "ExponentPushToken[aaa]");
        let events: Vec<CoreEvent> = (0..5).map(|n| permission("s1", &format!("r{n}"))).collect();
        assert_eq!(desk.notifier.plan(&events).await.len(), 3);
        assert!(desk
            .notifier
            .plan(&[permission("s1", "r9")])
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn rate_window_slides() {
        let desk = desk_with(|config| {
            config.rate_max = 1;
            config.rate_window = Duration::from_millis(50);
        })
        .await;
        desk.phone(1, "ExponentPushToken[aaa]");
        assert_eq!(desk.notifier.plan(&[permission("s1", "r1")]).await.len(), 1);
        assert!(desk
            .notifier
            .plan(&[permission("s1", "r2")])
            .await
            .is_empty());
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(desk.notifier.plan(&[permission("s1", "r3")]).await.len(), 1);
    }

    #[tokio::test]
    async fn nothing_for_a_phone_in_the_foreground_or_while_the_desktop_is_in_use() {
        let desk = desk().await;
        let a = desk.phone(1, "ExponentPushToken[aaa]");
        desk.phone(2, "ExponentPushToken[bbb]");
        desk.presence.foreground.lock().unwrap().insert(a);
        let messages = desk.notifier.plan(&[permission("s1", "r1")]).await;
        let to: Vec<&str> = messages.iter().map(|m| m.to.as_str()).collect();
        assert_eq!(to, ["ExponentPushToken[bbb]"]);

        desk.presence.desktop.store(true, Ordering::SeqCst);
        assert!(desk
            .notifier
            .plan(&[permission("s1", "r2")])
            .await
            .is_empty());
        assert!(desk.sent().is_empty());
    }

    #[tokio::test]
    async fn no_token_no_push_and_expired_leases_do_not_count() {
        let desk = desk().await;
        assert!(desk
            .notifier
            .plan(&[permission("s1", "r1")])
            .await
            .is_empty());
        let device = desk.phone(1, "ExponentPushToken[aaa]");
        desk.conn()
            .execute(
                "UPDATE paired_devices SET push_token_expires_at = '2020-01-01T00:00:00.000Z'
                 WHERE device_id = ?1",
                params![device],
            )
            .unwrap();
        assert!(desk
            .notifier
            .plan(&[permission("s1", "r2")])
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn a_delegated_task_notifies_for_its_requests_but_not_its_result() {
        let desk = desk().await;
        desk.phone(1, "ExponentPushToken[aaa]");
        let messages = desk
            .notifier
            .plan(&[completed("child", "t1"), permission("child", "r1")])
            .await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].data["kind"], "permission");
    }

    #[tokio::test]
    async fn retries_with_backoff_then_gives_up() {
        let desk = desk().await;
        desk.phone(1, "ExponentPushToken[aaa]");
        {
            let mut fake = desk.fake.lock().unwrap();
            fake.fail_next = 2;
            fake.fail_status = 503;
        }
        let delivery = notify(&desk, &[permission("s1", "r1")]).await;
        assert_eq!(delivery.sent, 1, "third attempt went through");

        {
            let mut fake = desk.fake.lock().unwrap();
            fake.fail_next = 10;
            fake.fail_status = 429;
        }
        let delivery = notify(&desk, &[permission("s1", "r2")]).await;
        assert_eq!(
            delivery,
            Delivery {
                sent: 0,
                failed: 1,
                forgotten: 0
            }
        );
        assert_eq!(
            desk.fake.lock().unwrap().fail_next,
            10 - 4,
            "four attempts in all"
        );

        // A 400 is our mistake: no retry.
        {
            let mut fake = desk.fake.lock().unwrap();
            fake.fail_next = 1;
            fake.fail_status = 400;
        }
        let delivery = notify(&desk, &[permission("s1", "r3")]).await;
        assert_eq!(delivery.failed, 1);
        assert_eq!(desk.fake.lock().unwrap().fail_next, 0);
        assert_eq!(desk.sent().len(), 1, "only the first one got through");
    }

    #[tokio::test]
    async fn tickets_and_receipts_saying_device_not_registered_forget_the_token() {
        let desk = desk().await;
        let gone = desk.phone(1, "ExponentPushToken[gone]");
        let stale = desk.phone(2, "ExponentPushToken[stale]");
        let fine = desk.phone(3, "ExponentPushToken[fine]");

        let delivery = notify(&desk, &[permission("s1", "r1")]).await;
        assert_eq!(
            delivery,
            Delivery {
                sent: 2,
                failed: 1,
                forgotten: 1
            }
        );
        assert_eq!(desk.token_of(&gone), None, "ticket said the app is gone");

        // The receipt check runs after `receipt_delay`.
        for _ in 0..100 {
            if desk.token_of(&stale).is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(desk.token_of(&stale), None, "receipt said the app is gone");
        assert_eq!(
            desk.token_of(&fine).as_deref(),
            Some("ExponentPushToken[fine]")
        );
        let asked = desk.fake.lock().unwrap().receipt_requests.clone();
        assert_eq!(
            asked,
            [
                json!({ "ids": ["ticket-ExponentPushToken[stale]", "ticket-ExponentPushToken[fine]"] })
            ]
        );
        let forgotten: Vec<Option<String>> = pairing::list_audit_log(&desk.conn(), 100)
            .unwrap()
            .into_iter()
            .filter(|entry| entry.event == "push_token_forgotten")
            .map(|entry| entry.device_id)
            .collect();
        assert_eq!(forgotten.len(), 2);
        assert!(forgotten.contains(&Some(gone)) && forgotten.contains(&Some(stale)));
    }

    #[tokio::test]
    async fn events_close_together_share_one_request() {
        let desk = desk().await;
        desk.phone(1, "ExponentPushToken[aaa]");
        let (events, receiver) = broadcast::channel(16);
        let task = tokio::spawn(desk.notifier.clone().run(receiver));
        events.send(Attention::from_event(&permission("s1", "r1")).unwrap()).unwrap();
        events.send(Attention::from_event(&completed("s2", "t1")).unwrap()).unwrap();
        for _ in 0..100 {
            if !desk.sent().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let sent = desk.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].as_array().unwrap().len(), 2);
        drop(events);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }
}
