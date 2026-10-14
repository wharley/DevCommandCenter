//! What waits on the person, for the phone: the agent requests outside the
//! permission path (`agent_requests`: open the Browser, computer use).
//!
//! - `GET /api/v1/mobile/agent-requests` lists them, with the apps a
//!   computer-use approval can pick from.
//! - `POST /api/v1/mobile/agent-requests/:id/respond` answers one; the first
//!   answer wins (409 with the outcome otherwise), the same ledger the
//!   desktop's dialogs answer through.
//! - A watcher polls the source and, when the list changes, sends
//!   `event: agent-requests` on `/api/v1/events/stream` and hands each new
//!   request to `remote_push` (generic "<projeto> precisa de você").
//!
//! Task proposals (`dcc_propose_task`) wait as durable records, not as
//! agent requests (those time out in minutes):
//!
//! - `GET /api/v1/mobile/task-proposals` lists the ones nobody started.
//! - `POST /api/v1/mobile/task-proposals/:id/start` and `/:id/dismiss` answer
//!   one; a proposal already started or dismissed answers 409.
//!
//! And every task's reason to be under "Precisa de você":
//!
//! - `GET /api/v1/mobile/attention` — the desktop sidebar's rule, ported to
//!   `dcc_tauri::commands::workspace_attention::attention_items` and pinned
//!   to the desktop's by shared test vectors.
//! - A second watcher pushes when a task newly has a git conflict (local or
//!   in its PR) or failing checks — once per appearance, primed on its first
//!   pass so blockers older than the app never notify.
use super::*;
use crate::agent_requests::{
    AgentRequestAnswer, AgentRequestError, AgentRequestSource, AnswerOrigin,
    InMemoryAgentRequests, PendingAgentRequest,
};
use crate::remote_push::Attention;
use std::future::Future;

const WATCH_INTERVAL: Duration = Duration::from_millis(500);
/// Blockers run git for every task; the phone's list reuses them this long.
const BLOCKERS_MAX_AGE: Duration = Duration::from_secs(30);
const BLOCKER_WATCH_INTERVAL: Duration = Duration::from_secs(60);

/// Runs on the caller's Tokio runtime, or Tauri's when there is none (the
/// desktop attaches from its setup hook).
pub(super) fn spawn_background<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(future);
        }
        Err(_) => {
            tauri::async_runtime::spawn(future);
        }
    }
}

impl HttpBackend {
    /// Serves these agent requests to phones and notifies about new ones.
    /// The desktop attaches its real queues; once only.
    pub fn attach_agent_requests(&self, source: Arc<dyn AgentRequestSource>) {
        if self.agent_requests.set(source.clone()).is_err() {
            return;
        }
        spawn_background(watch_agent_requests(
            Arc::downgrade(&source),
            self.events.clone(),
            self.attention.clone(),
        ));
    }

    /// `dccd-http` with `DCC_FIXTURE_AGENT_REQUESTS=1` (the simulator
    /// fixture): an in-memory queue the operator fills through
    /// `POST /api/v1/remote/fixture/agent-requests`, never a phone.
    pub fn attach_fixture_agent_requests(&self) -> Arc<InMemoryAgentRequests> {
        let queue = InMemoryAgentRequests::new();
        if self.fixture_agent_requests.set(queue.clone()).is_ok() {
            self.attach_agent_requests(queue.clone());
        }
        self.fixture_agent_requests.get().cloned().unwrap_or(queue)
    }
}

async fn watch_agent_requests(
    source: std::sync::Weak<dyn AgentRequestSource>,
    events: tokio::sync::broadcast::Sender<HttpStreamItem>,
    attention: tokio::sync::broadcast::Sender<Attention>,
) {
    let mut known: Vec<String> = Vec::new();
    let mut interval = tokio::time::interval(WATCH_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(source) = source.upgrade() else {
            return;
        };
        let pending = source.pending();
        drop(source);
        let ids: Vec<String> = pending.iter().map(|r| r.request_id.clone()).collect();
        if ids == known {
            continue;
        }
        for request in &pending {
            if !known.contains(&request.request_id) {
                let _ = attention.send(Attention::agent_request(
                    &request.session_id,
                    &request.request_id,
                ));
            }
        }
        known = ids;
        let _ = events.send(HttpStreamItem {
            payload: HttpStreamPayload::AgentRequests(json!({ "requests": pending })),
            durable: None,
        });
    }
}

pub(super) async fn list_agent_requests(
    State(backend): State<Arc<HttpBackend>>,
) -> Json<Value> {
    let requests = match backend.agent_requests.get() {
        Some(source) => source.pending_detailed().await,
        None => Vec::new(),
    };
    Json(json!({ "requests": requests }))
}

fn valid_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub(super) async fn respond_agent_request(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Path(request_id): Path<String>,
    Json(answer): Json<AgentRequestAnswer>,
) -> Result<Json<Value>, HttpApiError> {
    if !valid_request_id(&request_id) {
        return Err(HttpApiError::not_found("Esse pedido não está mais aberto."));
    }
    let Some(source) = backend.agent_requests.get() else {
        return Err(HttpApiError::not_found("Esse pedido não está mais aberto."));
    };
    let by = if device.is_some() {
        AnswerOrigin::Phone
    } else {
        AnswerOrigin::Desktop
    };
    match source.respond(&request_id, answer, by).await {
        Ok(outcome) => Ok(Json(json!({ "ok": true, "outcome": outcome }))),
        Err(error) => Err(classify_agent_request_error(error)),
    }
}

pub(super) async fn list_task_proposals(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    use dcc_core::{
        domain::delegation::{DelegationKind, DelegationMode, DelegationStatus},
        ports::DelegationRepo,
    };
    let drafts =
        DelegationRepo::list_delegations_by_status(&backend.sessions, DelegationStatus::Draft)
            .await
            .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let proposals: Vec<Value> = drafts
        .into_iter()
        .filter(|proposal| proposal.kind == DelegationKind::Task)
        .map(|proposal| {
            json!({
                "id": proposal.id.0,
                "parentSessionId": proposal.parent_session_id.0,
                "workspaceId": proposal.workspace_id.0,
                "title": proposal.title,
                "brief": proposal.instruction.unwrap_or(proposal.prompt),
                "provider": dcc_tauri::delegation_runtime::provider_label(&proposal.target_provider_id.0),
                "model": proposal.target_model_id,
                "edits": matches!(proposal.mode, DelegationMode::Implement)
                    || proposal.budget.allow_file_edits,
                "sharedCheckout": proposal.budget.shared_checkout,
                "createdAt": proposal.created_at,
            })
        })
        .collect();
    Ok(Json(json!({ "proposals": proposals })))
}

pub(super) async fn answer_task_proposal(
    State(backend): State<Arc<HttpBackend>>,
    Path((proposal_id, action)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    use dcc_core::domain::delegation::DelegationId;
    if !valid_request_id(&proposal_id) {
        return Err(HttpApiError::not_found("Essa proposta não está mais aberta."));
    }
    let id = DelegationId(proposal_id);
    let result = match action.as_str() {
        "start" => dcc_tauri::delegation_runtime::start_task_proposal(&backend.sessions, &id, None)
            .await
            .map(|_| ()),
        "dismiss" => dismiss_task_proposal(&backend.sessions, id).await,
        _ => return Err(HttpApiError::not_found("unknown proposal action")),
    };
    match result {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        // Both mean someone answered first (the desktop, or another phone).
        Err(error)
            if error.contains("already started or dismissed")
                || error.contains("already starting")
                || error.contains(" is already ") =>
        {
            Err(HttpApiError::Conflict(
                "Essa proposta já foi respondida.".to_string(),
                Value::Null,
            ))
        }
        Err(error) if error.contains("not found") => {
            Err(HttpApiError::not_found("Essa proposta não está mais aberta."))
        }
        Err(error) => Err(HttpApiError::BadGateway(error)),
    }
}

/// Only a waiting task proposal; this route never cancels running work.
async fn dismiss_task_proposal(
    sessions: &dcc_tauri::state::SessionCommandState,
    id: dcc_core::domain::delegation::DelegationId,
) -> Result<(), String> {
    use dcc_core::{
        domain::delegation::{DelegationKind, DelegationStatus},
        ports::DelegationRepo,
    };
    let proposal = DelegationRepo::get_delegation(sessions, &id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("task proposal not found: {}", id.0))?;
    if proposal.kind != DelegationKind::Task || proposal.status != DelegationStatus::Draft {
        return Err("this task proposal was already started or dismissed".to_string());
    }
    dcc_tauri::commands::delegation_commands::cancel_delegation_with_state(
        sessions,
        dcc_tauri::commands::delegation_commands::CancelDelegationInput {
            delegation_id: id,
            reason: Some("Dismissed by the person.".to_string()),
        },
    )
    .await
    .map(|_| ())
}

fn classify_agent_request_error(error: AgentRequestError) -> HttpApiError {
    let message = error.to_string();
    match error {
        AgentRequestError::AlreadyAnswered(outcome) => HttpApiError::Conflict(
            message,
            serde_json::to_value(outcome).unwrap_or(Value::Null),
        ),
        AgentRequestError::NotFound => HttpApiError::NotFound(message),
        AgentRequestError::Invalid(_) => HttpApiError::BadRequest(message),
        AgentRequestError::Failed(_) => HttpApiError::BadGateway(message),
    }
}

/// Fills the fixture queue (operator only; phones never reach
/// `/api/v1/remote/*`, and a paired identity is refused here anyway).
pub(super) async fn enqueue_fixture_agent_request(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Json(request): Json<PendingAgentRequest>,
) -> Result<Json<Value>, HttpApiError> {
    if device.is_some() {
        return Err(HttpApiError::Forbidden(
            "paired devices cannot create agent requests".into(),
        ));
    }
    let Some(queue) = backend.fixture_agent_requests.get() else {
        return Err(HttpApiError::not_found("fixture agent requests are off"));
    };
    let request_id = request.request_id.clone();
    let answer = queue.enqueue(request);
    // The fixture prints what the "agent" got.
    spawn_background(async move {
        match answer.await {
            Ok(outcome) => eprintln!(
                "[DCC fixture] agent request {request_id}: {:?} by {:?}",
                outcome.decision, outcome.by
            ),
            Err(_) => eprintln!("[DCC fixture] agent request {request_id}: closed"),
        }
    });
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn workspace_attention(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let output = dcc_tauri::commands::workspace_attention::workspace_attention_with_state(
        &backend.workspace,
        BLOCKERS_MAX_AGE,
    )
    .await
    .map_err(HttpApiError::internal)?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

/// Which blockers notify, and how.
fn blocker_attention_kind(
    kind: dcc_tauri::commands::workspace_attention::WorkspaceBlockerKind,
) -> Option<crate::remote_push::AttentionKind> {
    use crate::remote_push::AttentionKind;
    use dcc_tauri::commands::workspace_attention::WorkspaceBlockerKind;
    match kind {
        WorkspaceBlockerKind::Conflicts | WorkspaceBlockerKind::PrConflicts => {
            Some(AttentionKind::Conflict)
        }
        WorkspaceBlockerKind::ChecksFailing => Some(AttentionKind::ChecksFailing),
        // Not urgent: waits for the person to come back.
        WorkspaceBlockerKind::DelegatedEditsReview => None,
    }
}

/// The blockers that are new since the last pass (none on the first), each
/// once while it lasts; one that goes away and comes back is new again.
#[derive(Default)]
pub(super) struct BlockerNews {
    primed: bool,
    known: std::collections::HashSet<(String, crate::remote_push::AttentionKind)>,
    generation: u64,
}

impl BlockerNews {
    pub(super) fn next(
        &mut self,
        blockers: &[dcc_tauri::commands::workspace_attention::WorkspaceBlocker],
    ) -> Vec<(String, crate::remote_push::AttentionKind, String)> {
        self.generation += 1;
        let current: std::collections::HashSet<_> = blockers
            .iter()
            .filter_map(|blocker| {
                blocker_attention_kind(blocker.kind).map(|kind| (blocker.workspace_id.clone(), kind))
            })
            .collect();
        let fresh = if self.primed {
            current
                .difference(&self.known)
                .map(|(workspace, kind)| {
                    (
                        workspace.clone(),
                        *kind,
                        format!("blocker:{workspace}:{kind:?}:{}", self.generation),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        self.known = current;
        self.primed = true;
        fresh
    }
}

/// The task's newest session, which a tapped notification opens.
fn latest_session(db_path: &FsPath, workspace_id: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(db_path).ok()?;
    conn.query_row(
        "SELECT id FROM dcc_sessions WHERE workspace_id = ?1 ORDER BY created_at DESC LIMIT 1",
        rusqlite::params![workspace_id],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn has_push_targets(db_path: &FsPath) -> bool {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return false;
    };
    crate::pairing::push_targets(
        &conn,
        &crate::remote_push::lease_time(chrono::Utc::now()),
    )
    .is_ok_and(|targets| !targets.is_empty())
}

pub(super) fn watch_blockers(
    workspace: dcc_tauri::state::WorkspaceCommandState,
    attention: tokio::sync::broadcast::Sender<Attention>,
    hub: std::sync::Weak<RemoteHub>,
) {
    spawn_background(async move {
        let mut news = BlockerNews::default();
        let mut interval = tokio::time::interval(BLOCKER_WATCH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if hub.upgrade().is_none() {
                return;
            }
            let db_path = workspace.db_path.clone();
            let targets = tokio::task::spawn_blocking(move || has_push_targets(&db_path))
                .await
                .unwrap_or(false);
            if !targets {
                // Nobody to tell: start over, so what is there now is old news.
                news = BlockerNews::default();
                continue;
            }
            let Ok(blockers) = dcc_tauri::commands::workspace_attention::recent_attention_blockers(
                &workspace,
                BLOCKER_WATCH_INTERVAL / 2,
            )
            .await
            else {
                continue;
            };
            for (workspace_id, kind, key) in news.next(&blockers) {
                let db_path = workspace.db_path.clone();
                let Ok(Some(session_id)) =
                    tokio::task::spawn_blocking(move || latest_session(&db_path, &workspace_id)).await
                else {
                    continue;
                };
                let _ = attention.send(Attention {
                    kind,
                    session_id,
                    key,
                });
            }
        }
    });
}
