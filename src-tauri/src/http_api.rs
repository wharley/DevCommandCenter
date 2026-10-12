use async_trait::async_trait;
use axum::{
    extract::{DefaultBodyLimit, FromRef, Path, Query, State},
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, Method, StatusCode},
    middleware,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use dcc_core::{
    application::{
        close_session, restore_session, resume_session, start_thread, AbortRunInput,
        CloseSessionInput, RestoreSessionInput, ResumeSessionInput, SendTurnInput,
        StartThreadInput,
    },
    domain::session::SessionId,
    domain::workspace::WorkspaceId,
    ports::{
        events::{SessionLiveDurableIdentity, SessionLiveEventEnvelope},
        CoreEvent, EventBus,
    },
};
use dcc_infra::db::SqliteSessionRepo;
use dcc_tauri::{
    commands::session_commands::{
        abort_run_with_state, respond_to_permission_request_with_state,
        respond_to_user_input_with_state, send_turn_with_state, start_thread_with_state,
        RespondToPermissionRequestInput, RespondToProviderRequestError, RespondToUserInputInput,
    },
    state::SessionCommandState,
};
use portable_pty::{native_pty_system, Child as PtyChild, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path as FsPath, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::RwLock;
use tokio_stream::{
    wrappers::{errors::BroadcastStreamRecvError, BroadcastStream},
    StreamExt,
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

mod attention;
mod delivery;
mod mobile;

pub fn recover_mobile_tasks(db_path: &FsPath) -> Result<(), String> {
    mobile::recover_interrupted_tasks(db_path)
}

use crate::daemon_client::rpc_with_db_path_timeout;
use crate::http_auth::{auth_middleware, PairedDeviceIdentity};
use crate::http_config::{HttpAuthMode, HttpConfig};
use crate::http_rpc_handler::handle_rpc as handle_json_rpc;
use crate::remote_connector::RemoteHub;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
pub const DCC_HTTP_PROTOCOL_VERSION: &str = "2026-05-15";
static HEADLESS_TERMINALS: OnceLock<Mutex<HashMap<String, HeadlessManagedTerminal>>> =
    OnceLock::new();
static HEADLESS_TERMINAL_EVENT_SENDER: OnceLock<
    tokio::sync::broadcast::Sender<HeadlessTerminalEvent>,
> = OnceLock::new();
static HEADLESS_TERMINAL_SWEEPER: OnceLock<()> = OnceLock::new();

const TERMINAL_OUTPUT_MAX_LINES: usize = 3000;
const PTY_INPUT_CHUNK_BYTES: usize = 8192;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFilterQuery {
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectAndCombFilterQuery {
    project_id: Option<String>,
    comb_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTaskActionQuery {
    project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSessionsQuery {
    workspace_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionSearchQuery {
    #[serde(default = "default_session_search_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseSessionRequest {
    #[serde(default)]
    delete_history: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundleRequest {
    #[serde(default)]
    worktree_paths: Vec<String>,
    #[serde(default)]
    comb_ids: Vec<String>,
}

fn default_session_search_limit() -> usize {
    40
}

/// One event on `/api/v1/events/stream`, with its durable session sequence
/// when it has one.
#[derive(Clone, Debug)]
struct HttpStreamItem {
    payload: HttpStreamPayload,
    durable: Option<SessionLiveDurableIdentity>,
}

#[derive(Clone, Debug)]
enum HttpStreamPayload {
    /// `event: core-event`.
    Core(CoreEvent),
    /// `event: agent-requests`: the open Browser/computer-use requests
    /// changed (see `attention`); the data is the new list.
    AgentRequests(Value),
}

impl HttpStreamItem {
    fn core(event: CoreEvent, durable: Option<SessionLiveDurableIdentity>) -> Self {
        Self {
            payload: HttpStreamPayload::Core(event),
            durable,
        }
    }
}

/// Feeds `/api/v1/events/stream`. Publishing only hands the event to a
/// broadcast channel: a slow phone never holds up the desktop UI's events.
#[derive(Clone)]
struct HttpEventBus {
    sender: tokio::sync::broadcast::Sender<HttpStreamItem>,
    /// Only the events that may notify a phone (`remote_push`), apart from
    /// the stream so a burst of deltas never pushes them out.
    attention: tokio::sync::broadcast::Sender<crate::remote_push::Attention>,
}

#[async_trait]
impl EventBus for HttpEventBus {
    async fn publish(&self, event: CoreEvent) -> dcc_core::Result<()> {
        // The runtime follows every session event with its live envelope,
        // which carries the durable sequence; send those from there.
        if event.session_id().is_none() {
            let _ = self.sender.send(HttpStreamItem::core(event, None));
        }
        Ok(())
    }

    async fn publish_session_live(
        &self,
        envelope: SessionLiveEventEnvelope,
    ) -> dcc_core::Result<()> {
        if let Some(attention) = crate::remote_push::Attention::from_event(&envelope.event) {
            let _ = self.attention.send(attention);
        }
        let _ = self
            .sender
            .send(HttpStreamItem::core(envelope.event, envelope.durable));
        Ok(())
    }
}

/// The sessions the HTTP API acts on. Inside the desktop app it is the app's
/// own `SessionCommandState`, so the phone sees and answers the sessions the
/// desktop runs; `dccd-http` builds a headless one of its own.
pub struct HttpBackend {
    sessions: SessionCommandState,
    /// Git and delivery for the phone; the desktop hands over its own so
    /// both share one delivery-failure store (`with_workspace_state`).
    workspace: dcc_tauri::state::WorkspaceCommandState,
    events: tokio::sync::broadcast::Sender<HttpStreamItem>,
    attention: tokio::sync::broadcast::Sender<crate::remote_push::Attention>,
    /// Browser and computer-use requests from agents (`agent_requests`).
    agent_requests: OnceLock<Arc<dyn crate::agent_requests::AgentRequestSource>>,
    fixture_agent_requests: OnceLock<Arc<crate::agent_requests::InMemoryAgentRequests>>,
    // The runtime keeps only a weak reference to its buses.
    _bus: Arc<dyn EventBus>,
    terminals: bool,
    /// The relay connector serving this router, for the operator routes and
    /// to drop a revoked phone at once. Weak: the hub holds the router.
    remote: OnceLock<std::sync::Weak<RemoteHub>>,
}

impl HttpBackend {
    /// Serves sessions the desktop app owns. Remote shells are left out: the
    /// phone has no use for them yet and they are the riskiest route.
    pub fn embedded(sessions: SessionCommandState) -> Result<Self, String> {
        let (events, _) = tokio::sync::broadcast::channel(256);
        let (attention, _) = tokio::sync::broadcast::channel(64);
        let bus: Arc<dyn EventBus> = Arc::new(HttpEventBus {
            sender: events.clone(),
            attention: attention.clone(),
        });
        sessions
            .register_event_bus(&bus)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            workspace: dcc_tauri::state::WorkspaceCommandState::from_session(&sessions),
            sessions,
            events,
            attention,
            _bus: bus,
            terminals: false,
            remote: OnceLock::new(),
            agent_requests: OnceLock::new(),
            fixture_agent_requests: OnceLock::new(),
        })
    }

    /// The standalone `dccd-http` server, owner of the sessions it starts.
    pub fn headless(db_path: PathBuf, app_data_dir: PathBuf) -> Self {
        let (events, _) = tokio::sync::broadcast::channel(256);
        let (attention, _) = tokio::sync::broadcast::channel(64);
        let bus: Arc<dyn EventBus> = Arc::new(HttpEventBus {
            sender: events.clone(),
            attention: attention.clone(),
        });
        let sessions = SessionCommandState::new_with_event_bus(db_path, app_data_dir, bus.clone());
        Self {
            workspace: dcc_tauri::state::WorkspaceCommandState::from_session(&sessions),
            sessions,
            events,
            attention,
            _bus: bus,
            terminals: true,
            remote: OnceLock::new(),
            agent_requests: OnceLock::new(),
            fixture_agent_requests: OnceLock::new(),
        }
    }

    /// Shares the desktop's workspace state (same delivery-failure store).
    pub fn with_workspace_state(mut self, workspace: dcc_tauri::state::WorkspaceCommandState) -> Self {
        self.workspace = workspace;
        self
    }

    /// Also hands the hub the events that notify phones, and starts
    /// watching git blockers for the ones that notify (`attention`).
    pub fn attach_remote(&self, hub: &Arc<RemoteHub>) {
        let _ = self.remote.set(Arc::downgrade(hub));
        hub.attach_attention_events(self.attention.subscribe());
        attention::watch_blockers(
            self.workspace.clone(),
            self.attention.clone(),
            Arc::downgrade(hub),
        );
    }

    fn remote(&self) -> Option<Arc<RemoteHub>> {
        self.remote.get().and_then(std::sync::Weak::upgrade)
    }
}

#[derive(Clone)]
struct HttpState {
    config: Arc<RwLock<HttpConfig>>,
    backend: Arc<HttpBackend>,
}

impl FromRef<HttpState> for Arc<RwLock<HttpConfig>> {
    fn from_ref(state: &HttpState) -> Self {
        state.config.clone()
    }
}

impl FromRef<HttpState> for Arc<HttpBackend> {
    fn from_ref(state: &HttpState) -> Self {
        state.backend.clone()
    }
}

#[allow(dead_code)]
struct HeadlessManagedTerminal {
    pty_master: Box<dyn MasterPty + Send>,
    child: Box<dyn PtyChild + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pty_owner_key: Option<String>,
    cwd: String,
    command: String,
    args: Vec<String>,
    started_at: String,
    stop_flag: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
    reader_thread: Option<JoinHandle<()>>,
    exit_notified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
enum HeadlessTerminalEvent {
    Output {
        pty_id: String,
        data: String,
        stream: String,
    },
    Exit {
        pty_id: String,
        code: Option<i32>,
    },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalSpawnRequest {
    cwd: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
    #[serde(default)]
    pane_id: Option<String>,
    #[serde(default)]
    pty_owner_key: Option<String>,
    #[serde(default)]
    restart: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalWriteRequest {
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalResizeRequest {
    cols: u16,
    rows: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeadlessTerminalSession {
    pty_id: String,
    cwd: String,
    command: String,
    args: Vec<String>,
    pty_owner_key: Option<String>,
    status: String,
    started_at: String,
    exited_at: Option<String>,
    last_exit_code: Option<i32>,
}

#[derive(Debug)]
enum HttpApiError {
    BadRequest(String),
    Forbidden(String),
    NotFound(String),
    /// The request was already answered; carries the answer that won.
    Conflict(String, Value),
    /// 409 with its own code: the action cannot run in the current state
    /// (stale view, push behind the remote…); `Value` says more.
    Refused(&'static str, String, Value),
    ServiceUnavailable(String),
    GatewayTimeout(String),
    BadGateway(String),
    Internal(String),
}

impl HttpApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self::ServiceUnavailable(message.into())
    }

    fn gateway_timeout(message: impl Into<String>) -> Self {
        Self::GatewayTimeout(message.into())
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self::BadGateway(message.into())
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(..) | Self::Refused(..) => StatusCode::CONFLICT,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::GatewayTimeout(_) => StatusCode::GATEWAY_TIMEOUT,
            Self::BadGateway(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound(_) => "not_found",
            Self::Conflict(..) => "already_resolved",
            Self::Refused(code, ..) => code,
            Self::ServiceUnavailable(_) => "service_unavailable",
            Self::GatewayTimeout(_) => "gateway_timeout",
            Self::BadGateway(_) => "bad_gateway",
            Self::Internal(_) => "internal_error",
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::Forbidden(message)
            | Self::NotFound(message)
            | Self::Conflict(message, _)
            | Self::Refused(_, message, _)
            | Self::ServiceUnavailable(message)
            | Self::GatewayTimeout(message)
            | Self::BadGateway(message)
            | Self::Internal(message) => message,
        }
    }
}

impl IntoResponse for HttpApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let mut error = json!({
            "code": self.code(),
            "message": self.message(),
        });
        if let Self::Conflict(_, outcome) | Self::Refused(_, _, outcome) = &self {
            error["outcome"] = outcome.clone();
        }
        (status, Json(json!({ "ok": false, "error": error }))).into_response()
    }
}

pub fn build_router(config: Arc<RwLock<HttpConfig>>, backend: Arc<HttpBackend>) -> Router {
    let cors_config = config
        .try_read()
        .expect("config lock must be uncontended at router build time")
        .clone();
    let terminal_routes = Router::new()
        .route(
            "/api/v1/terminals/events/stream",
            get(terminals_stream_handler),
        )
        .route("/api/v1/terminals/spawn", post(terminal_spawn_handler))
        .route(
            "/api/v1/terminals/by-owner/:owner_key",
            post(terminal_get_or_create_by_owner_handler),
        )
        .route(
            "/api/v1/terminals/:pty_id/write",
            post(terminal_write_handler),
        )
        .route(
            "/api/v1/terminals/:pty_id/resize",
            post(terminal_resize_handler),
        )
        .route(
            "/api/v1/terminals/:pty_id/kill",
            post(terminal_kill_handler),
        );
    let protected_routes = Router::new()
        .route("/api/v1/mobile/catalog", get(mobile::catalog))
        .route(
            "/api/v1/mobile/workspaces/:workspace_id/patch",
            get(mobile::patch),
        )
        .route(
            "/api/v1/mobile/workspaces/:workspace_id/files",
            get(mobile::workspace_files),
        )
        .route(
            "/api/v1/mobile/repositories/:repository_id/files",
            get(mobile::repository_files),
        )
        .route(
            "/api/v1/mobile/attachments",
            // A 10 MB file is ~13.4 MB of base64; the 2 MB default would refuse it.
            post(mobile::upload_attachment).layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/v1/mobile/sessions/:session_id/attachment",
            get(mobile::attachment_preview),
        )
        .route(
            "/api/v1/mobile/agent-requests",
            get(attention::list_agent_requests),
        )
        .route("/api/v1/mobile/attention", get(attention::workspace_attention))
        .route(
            "/api/v1/mobile/workspaces/:workspace_id/delivery",
            get(delivery::summary),
        )
        .route(
            "/api/v1/mobile/workspaces/:workspace_id/delivery/forge",
            get(delivery::forge),
        )
        .route(
            "/api/v1/mobile/workspaces/:workspace_id/delivery/:action",
            post(delivery::act),
        )
        .route(
            "/api/v1/mobile/agent-requests/:request_id/respond",
            post(attention::respond_agent_request),
        )
        .route(
            "/api/v1/remote/fixture/agent-requests",
            post(attention::enqueue_fixture_agent_request),
        )
        .route("/api/v1/mobile/tasks", post(mobile::create_task))
        .route("/api/v1/mobile/tasks/:request_id", get(mobile::task_status))
        .route("/rpc", post(handle_json_rpc))
        .route("/api/v1/shell/default", get(shell_default_handler))
        .route("/api/v1/events/stream", get(events_stream_handler))
        .route("/api/v1/status", get(status_handler))
        .route("/api/v1/tasks", get(list_tasks_handler))
        .route("/api/v1/tasks/:task_id/run", post(run_task_handler))
        .route(
            "/api/v1/tasks/:task_id/attach",
            post(attach_task_handler).delete(detach_task_handler),
        )
        .route("/api/v1/processes", get(list_processes_handler))
        .route(
            "/api/v1/processes/:process_id/start",
            post(start_process_handler),
        )
        .route(
            "/api/v1/processes/:process_id/stop",
            post(stop_process_handler),
        )
        .route(
            "/api/v1/processes/:process_id/restart",
            post(restart_process_handler),
        )
        .route("/api/v1/combs", get(list_combs_handler))
        .route("/api/v1/panes", get(list_panes_handler))
        .route("/api/v1/diffs/bundle", post(diffs_bundle_handler))
        .route("/api/v1/sessions/start", post(start_thread_handler))
        .route("/api/v1/sessions", get(list_workspace_sessions_handler))
        .route("/api/v1/sessions/search", get(search_sessions_handler))
        .route(
            "/api/v1/sessions/:session_id/events",
            get(list_session_events_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/turns",
            post(send_turn_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/abort",
            post(abort_session_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/resume",
            post(resume_session_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/close",
            post(close_session_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/restore",
            post(restore_session_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/respond-user-input",
            post(respond_to_user_input_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/respond-permission",
            post(respond_to_permission_request_handler),
        )
        .route(
            "/api/v1/auth/bearer/rotate",
            post(rotate_bearer_token_handler),
        )
        .route("/api/v1/remote/status", get(remote_status_handler))
        .route(
            "/api/v1/remote/pairing/offers",
            post(remote_create_offer_handler),
        )
        .route(
            "/api/v1/remote/pairing/approvals",
            get(remote_list_approvals_handler),
        )
        .route(
            "/api/v1/remote/pairing/approvals/:approval_id",
            post(remote_respond_approval_handler),
        );
    let protected_routes = if backend.terminals {
        protected_routes.merge(terminal_routes)
    } else {
        protected_routes
    }
    .route_layer(middleware::from_fn_with_state(
        config.clone(),
        auth_middleware,
    ));

    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .route("/openapi.json", get(openapi_handler))
        .merge(protected_routes)
        .layer(build_cors_layer(&cors_config))
        .layer(TraceLayer::new_for_http())
        .with_state(HttpState { config, backend })
}

pub fn build_cors_layer(config: &HttpConfig) -> CorsLayer {
    let methods = [Method::GET, Method::POST, Method::DELETE, Method::OPTIONS];
    let headers = [
        CONTENT_TYPE,
        HeaderName::from_static("x-api-key"),
        HeaderName::from_static("authorization"),
    ];

    if config.cors_origins.is_empty() || config.cors_origins.iter().any(|origin| origin == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(methods)
            .allow_headers(headers)
    } else {
        let origins = config
            .cors_origins
            .iter()
            .filter_map(|origin| origin.parse::<HeaderValue>().ok())
            .collect::<Vec<_>>();

        if origins.is_empty() {
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(methods)
                .allow_headers(headers)
        } else {
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(origins))
                .allow_methods(methods)
                .allow_headers(headers)
        }
    }
}

async fn rpc_value(
    config: Arc<RwLock<HttpConfig>>,
    method: &'static str,
    params: Value,
    timeout: Duration,
) -> Result<Value, HttpApiError> {
    let db_path = {
        let config = config.read().await;
        config.db_path.clone()
    };
    let method = method.to_string();

    tokio::task::spawn_blocking(move || {
        rpc_with_db_path_timeout(&db_path, &method, params, timeout)
    })
    .await
    .map_err(|error| HttpApiError::internal(format!("failed to join RPC worker: {error}")))?
    .map_err(classify_rpc_error)
}

fn classify_rpc_error(error: String) -> HttpApiError {
    let normalized = error.to_lowercase();

    if normalized.contains("timed out") {
        HttpApiError::gateway_timeout(error)
    } else if normalized.contains("no such table")
        || normalized.contains("unable to open database")
        || normalized.contains("database is locked")
    {
        HttpApiError::service_unavailable(error)
    } else if normalized.contains("not found") {
        HttpApiError::not_found(error)
    } else if normalized.contains("missing ") {
        HttpApiError::bad_request(error)
    } else {
        HttpApiError::bad_gateway(error)
    }
}

fn classify_session_error(error: String) -> HttpApiError {
    let normalized = error.to_lowercase();

    if normalized.contains("not found") {
        HttpApiError::not_found(error)
    } else if normalized.contains("missing ")
        || normalized.contains("must be ")
        || normalized.contains("history is empty")
        || normalized.contains("no active turn")
    {
        HttpApiError::bad_request(error)
    } else {
        HttpApiError::internal(error)
    }
}

async fn session_repo_operation<T, F>(
    config: Arc<RwLock<HttpConfig>>,
    op: F,
) -> Result<T, HttpApiError>
where
    T: Send + 'static,
    F: FnOnce(SqliteSessionRepo) -> Result<T, String> + Send + 'static,
{
    let db_path = {
        let config = config.read().await;
        config.db_path.clone()
    };

    tokio::task::spawn_blocking(move || {
        let repo = SqliteSessionRepo::open(&db_path).map_err(|error| error.to_string())?;
        op(repo)
    })
    .await
    .map_err(|error| HttpApiError::internal(format!("failed to join session worker: {error}")))?
    .map_err(classify_session_error)
}

/// Run a database operation in-process (inside dccd-http), off the async executor,
/// bypassing the daemon RPC queue. The queue is served by a separate, possibly
/// stale daemon process, so the mobile endpoints that need current data read
/// the live tables (`dcc_workspaces`, `dcc_sessions`) directly here.
async fn db_read<T, F>(config: Arc<RwLock<HttpConfig>>, op: F) -> Result<T, HttpApiError>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T, String> + Send + 'static,
{
    let db_path = {
        let config = config.read().await;
        config.db_path.clone()
    };
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        op(&conn)
    })
    .await
    .map_err(|e| HttpApiError::internal(format!("db worker join failed: {e}")))?
    .map_err(HttpApiError::internal)
}

fn root_path_basename(root_path: Option<&str>) -> Option<String> {
    root_path
        .map(|p| p.trim_end_matches('/'))
        .and_then(|p| p.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Live worktrees mapped onto the comb shape the mobile companion expects.
/// Sources `dcc_workspaces` (the legacy `combs` table is empty in current
/// installs); the project group label is the basename of `root_path`.
fn live_combs(conn: &rusqlite::Connection, project_id: Option<String>) -> Result<Value, String> {
    let mut stmt = conn
        .prepare(
            "SELECT w.id, w.project_id, w.name, w.base_branch, w.worktree_path,
                    w.root_path, w.state, w.created_at, w.updated_at,
                    r.display_name, r.icon, r.color, r.logo
             FROM dcc_workspaces w
             LEFT JOIN dcc_repositories r ON r.root_path = w.root_path
             ORDER BY w.updated_at DESC, w.created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let row_project_id: String = row.get(1).map_err(|e| e.to_string())?;
        if let Some(ref filter) = project_id {
            if *filter != row_project_id {
                continue;
            }
        }
        let root_path: Option<String> = row.get(5).map_err(|e| e.to_string())?;
        let display_name: Option<String> = row.get(9).map_err(|e| e.to_string())?;
        // Same name, mascot and color the desktop shows for this project.
        let project_name = display_name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| root_path_basename(root_path.as_deref()))
            .unwrap_or_else(|| row_project_id.clone());
        let base_branch: Option<String> = row.get(3).map_err(|e| e.to_string())?;
        let worktree_path: Option<String> = row.get(4).map_err(|e| e.to_string())?;
        // `base_branch` is what the task started from; a task's worktree has
        // its own branch (none while it is still detached).
        let branch = match worktree_path.as_deref().or(root_path.as_deref()) {
            Some(folder) => mobile::checked_out_branch(FsPath::new(folder)),
            None => base_branch.clone(),
        };
        // A picked logo travels as the icon value itself (a PNG data URI).
        let icon: Option<String> = row.get(10).map_err(|e| e.to_string())?;
        let logo: Option<String> = row.get(12).map_err(|e| e.to_string())?;
        let project_icon = if icon.as_deref() == Some("logo") { logo } else { icon };
        out.push(json!({
            "id": row.get::<_, String>(0).map_err(|e| e.to_string())?,
            "projectId": row_project_id,
            "projectName": project_name,
            "projectPath": root_path,
            "projectIcon": project_icon,
            "projectColor": row.get::<_, Option<String>>(11).map_err(|e| e.to_string())?,
            "name": row.get::<_, String>(2).map_err(|e| e.to_string())?,
            "description": Value::Null,
            "baseBranch": base_branch.clone(),
            "branch": branch,
            "worktreePath": worktree_path,
            "reviewTargets": Value::Null,
            "forgeLink": Value::Null,
            "status": row.get::<_, String>(6).map_err(|e| e.to_string())?,
            "isPinned": false,
            "pinnedAt": Value::Null,
            "lastOpenedAt": row.get::<_, Option<String>>(8).map_err(|e| e.to_string())?,
            "createdAt": row.get::<_, Option<String>>(7).map_err(|e| e.to_string())?,
            "updatedAt": row.get::<_, Option<String>>(8).map_err(|e| e.to_string())?,
            "lastGitActivityAt": Value::Null,
        }));
    }
    Ok(Value::Array(out))
}

/// Live sessions for the mobile companion, mapped onto `SessionSearchResult`.
/// The INNER JOIN to `dcc_workspaces` drops orphaned sessions left in the FTS
/// index by deleted/recreated worktrees.
fn live_sessions(conn: &rusqlite::Connection, limit: i64) -> Result<Value, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, COALESCE(NULLIF(t.title, ''), w.name) AS thread_title,
                    s.workspace_id, w.name AS workspace_name, w.base_branch,
                    w.project_id, w.root_path, s.provider_id, s.model,
                    s.updated_at, t.archived_at
             FROM dcc_sessions s
             JOIN dcc_workspaces w ON w.id = s.workspace_id
             LEFT JOIN dcc_threads t ON t.session_id = s.id
             WHERE w.state != 'archived'
             ORDER BY s.updated_at DESC
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([limit]).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let root_path: Option<String> = row.get(6).map_err(|e| e.to_string())?;
        out.push(json!({
            "sessionId": row.get::<_, String>(0).map_err(|e| e.to_string())?,
            "threadTitle": row.get::<_, Option<String>>(1).map_err(|e| e.to_string())?,
            "snippet": Value::Null,
            "providerId": row.get::<_, Option<String>>(7).map_err(|e| e.to_string())?,
            "model": row.get::<_, Option<String>>(8).map_err(|e| e.to_string())?,
            "workspaceName": row.get::<_, Option<String>>(3).map_err(|e| e.to_string())?,
            "workspaceBranch": row.get::<_, Option<String>>(4).map_err(|e| e.to_string())?,
            "workspaceId": row.get::<_, Option<String>>(2).map_err(|e| e.to_string())?,
            "projectId": row.get::<_, Option<String>>(5).map_err(|e| e.to_string())?,
            "projectName": root_path_basename(root_path.as_deref()),
            "updatedAt": row.get::<_, Option<String>>(9).map_err(|e| e.to_string())?,
            "archivedAt": row.get::<_, Option<String>>(10).map_err(|e| e.to_string())?,
        }));
    }
    Ok(Value::Array(out))
}

fn resolve_worktree_paths(
    conn: &rusqlite::Connection,
    comb_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT worktree_path FROM dcc_workspaces
             WHERE id = ?1 AND worktree_path IS NOT NULL AND worktree_path != ''",
        )
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for id in comb_ids {
        if let Ok(path) = stmt.query_row([id.as_str()], |row| row.get::<_, String>(0)) {
            out.push(path);
        }
    }
    Ok(out)
}

fn headless_terminal_sender() -> tokio::sync::broadcast::Sender<HeadlessTerminalEvent> {
    HEADLESS_TERMINAL_EVENT_SENDER
        .get_or_init(|| {
            let (sender, _) = tokio::sync::broadcast::channel(512);
            sender
        })
        .clone()
}

fn headless_terminals() -> &'static Mutex<HashMap<String, HeadlessManagedTerminal>> {
    HEADLESS_TERMINALS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn headless_terminal_output_buffer() -> Arc<Mutex<VecDeque<String>>> {
    Arc::new(Mutex::new(VecDeque::with_capacity(
        TERMINAL_OUTPUT_MAX_LINES.min(256),
    )))
}

fn append_headless_terminal_output(buffer: &Arc<Mutex<VecDeque<String>>>, chunk: &str) {
    if let Ok(mut guard) = buffer.lock() {
        while guard.len() >= TERMINAL_OUTPUT_MAX_LINES {
            guard.pop_front();
        }
        guard.push_back(chunk.to_string());
    }
}

fn headless_default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(windows) {
            "powershell".to_string()
        } else {
            "/bin/zsh".to_string()
        }
    })
}

fn tmux_session_name(owner_key: &str) -> String {
    let digest = Sha256::digest(owner_key.as_bytes());
    let suffix = hex::encode(&digest[..8]);
    format!("dcc-{suffix}")
}

#[cfg(unix)]
fn tmux_is_available() -> bool {
    std::process::Command::new("sh")
        .args(["-lc", "command -v tmux >/dev/null 2>&1"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn tmux_is_available() -> bool {
    false
}

fn iso_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn pty_write_all_chunked(writer: &mut dyn Write, data: &[u8]) -> std::io::Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    for chunk in data.chunks(PTY_INPUT_CHUNK_BYTES) {
        writer.write_all(chunk)?;
    }
    Ok(())
}

fn spawn_headless_terminal_reader_thread(
    mut reader: Box<dyn Read + Send>,
    pty_id: String,
    sender: tokio::sync::broadcast::Sender<HeadlessTerminalEvent>,
    stop_flag: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<String>>>,
) -> JoinHandle<()> {
    const FLUSH_INTERVAL: Duration = Duration::from_millis(16);
    const MAX_BATCH_BYTES: usize = 128 * 1024;

    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let read_thread = thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buffer[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut pending: Vec<u8> = Vec::new();
        let flush_pending = |pending: &mut Vec<u8>| {
            if pending.is_empty() {
                return;
            }
            let data = String::from_utf8_lossy(pending).to_string();
            pending.clear();
            append_headless_terminal_output(&output_buffer, &data);
            let _ = sender.send(HeadlessTerminalEvent::Output {
                pty_id: pty_id.clone(),
                data,
                stream: "stdout".to_string(),
            });
        };

        loop {
            match rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(chunk) => {
                    pending.extend_from_slice(&chunk);
                    while let Ok(more) = rx.try_recv() {
                        pending.extend_from_slice(&more);
                        if pending.len() >= MAX_BATCH_BYTES {
                            flush_pending(&mut pending);
                        }
                    }
                    if pending.len() >= MAX_BATCH_BYTES {
                        flush_pending(&mut pending);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !pending.is_empty() {
                        flush_pending(&mut pending);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    if !pending.is_empty() {
                        flush_pending(&mut pending);
                    }
                    break;
                }
            }
        }

        let _ = read_thread.join();
    })
}

fn ensure_headless_terminal_sweeper() {
    HEADLESS_TERMINAL_SWEEPER.get_or_init(|| {
        let sender = headless_terminal_sender();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;

                let exited = {
                    let mut guard = match headless_terminals().lock() {
                        Ok(guard) => guard,
                        Err(_) => continue,
                    };
                    let mut exited = Vec::new();
                    for (pty_id, terminal) in guard.iter_mut() {
                        if terminal.exit_notified {
                            continue;
                        }
                        let Some(status) = terminal.child.try_wait().ok().flatten() else {
                            continue;
                        };
                        terminal.exit_notified = true;
                        terminal.stop_flag.store(true, Ordering::Relaxed);
                        let _ = terminal.reader_thread.take();
                        exited.push((pty_id.clone(), status.exit_code()));
                    }
                    exited
                };

                for (pty_id, code) in exited {
                    let _ = sender.send(HeadlessTerminalEvent::Exit {
                        pty_id,
                        code: Some(code as i32),
                    });
                }
            }
        });
    });
}

fn spawn_headless_terminal(options: TerminalSpawnRequest) -> Result<Value, HttpApiError> {
    ensure_headless_terminal_sweeper();

    let TerminalSpawnRequest {
        cwd,
        command,
        args,
        cols,
        rows,
        pane_id: _,
        pty_owner_key,
        restart: _,
    } = options;

    let cwd = cwd.trim();
    if cwd.is_empty() {
        return Err(HttpApiError::bad_request("cwd is required"));
    }

    let shell = headless_default_shell();
    let requested_command = command.unwrap_or_else(|| shell.clone());
    let requested_args = args;
    let use_tmux_persistence = pty_owner_key
        .as_deref()
        .is_some_and(|owner_key| !owner_key.trim().is_empty() && tmux_is_available());
    let (command, args, uses_tmux_transport) = if use_tmux_persistence {
        let owner_key = pty_owner_key.as_deref().unwrap_or_default();
        (
            "tmux".to_string(),
            vec![
                "new-session".to_string(),
                "-A".to_string(),
                "-s".to_string(),
                tmux_session_name(owner_key),
                "-c".to_string(),
                cwd.to_string(),
            ],
            true,
        )
    } else {
        (requested_command, requested_args, false)
    };
    let cols = cols.unwrap_or(120);
    let rows = rows.unwrap_or(32);

    let pty_pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| HttpApiError::internal(error.to_string()))?;

    let mut cmd = CommandBuilder::new(&command);
    cmd.args(&args);
    cmd.cwd(cwd);

    #[cfg(unix)]
    {
        cmd.env("TERM", "xterm-256color");
        if !uses_tmux_transport {
            let git_file_path = FsPath::new(cwd).join(".git");
            if git_file_path.is_file() {
                if let Ok(git_content) = std::fs::read_to_string(&git_file_path) {
                    if let Some(gitdir_line) =
                        git_content.lines().find(|line| line.starts_with("gitdir:"))
                    {
                        let gitdir = gitdir_line.trim_start_matches("gitdir:").trim();
                        cmd.env("GIT_DIR", gitdir);
                        cmd.env("GIT_WORK_TREE", cwd);
                    }
                }
            }
        }
    }

    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|error| HttpApiError::internal(error.to_string()))?;

    let pty_id = format!("pty-{}", Uuid::new_v4().simple());
    let sender = headless_terminal_sender();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let output_buffer = headless_terminal_output_buffer();
    let reader_thread = Some(spawn_headless_terminal_reader_thread(
        reader,
        pty_id.clone(),
        sender,
        stop_flag.clone(),
        output_buffer.clone(),
    ));

    let terminal = HeadlessManagedTerminal {
        pty_master: pty_pair.master,
        child,
        writer: Arc::new(Mutex::new(writer)),
        pty_owner_key,
        cwd: cwd.to_string(),
        command,
        args,
        started_at: iso_now(),
        stop_flag,
        output_buffer,
        reader_thread,
        exit_notified: false,
    };

    headless_terminals()
        .lock()
        .map_err(|error| {
            HttpApiError::internal(format!("terminal runtime cache poisoned: {error}"))
        })?
        .insert(pty_id.clone(), terminal);

    Ok(json!({ "ptyId": pty_id }))
}

fn terminal_event_name(event: &HeadlessTerminalEvent) -> &'static str {
    match event {
        HeadlessTerminalEvent::Output { .. } => "terminal-output",
        HeadlessTerminalEvent::Exit { .. } => "terminal-exit",
    }
}

fn headless_terminal_session_snapshot(
    pty_id: &str,
    terminal: &mut HeadlessManagedTerminal,
) -> HeadlessTerminalSession {
    let wait = terminal.child.try_wait().ok().flatten();
    let (status, exited_at, last_exit_code) = if let Some(status) = wait {
        (
            "exited".to_string(),
            Some(iso_now()),
            Some(status.exit_code() as i32),
        )
    } else {
        ("running".to_string(), None, None)
    };

    HeadlessTerminalSession {
        pty_id: pty_id.to_string(),
        cwd: terminal.cwd.clone(),
        command: terminal.command.clone(),
        args: terminal.args.clone(),
        pty_owner_key: terminal.pty_owner_key.clone(),
        status,
        started_at: terminal.started_at.clone(),
        exited_at,
        last_exit_code,
    }
}

fn filter_by_string_field(
    value: Value,
    field: &str,
    expected: Option<&str>,
) -> Result<Value, HttpApiError> {
    let Some(expected) = expected else {
        return Ok(value);
    };

    match value {
        Value::Array(items) => Ok(Value::Array(
            items
                .into_iter()
                .filter(|item| item.get(field).and_then(Value::as_str) == Some(expected))
                .collect(),
        )),
        other => Err(HttpApiError::internal(format!(
            "expected array response for filter on '{field}', received {other}"
        ))),
    }
}

async fn root_handler(State(config): State<Arc<RwLock<HttpConfig>>>) -> Json<Value> {
    let config = config.read().await;
    Json(json!({
        "name": "DCC HTTP API",
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": DCC_HTTP_PROTOCOL_VERSION,
        "documentation": "/openapi.json",
        "guide": "docs/GUIA_HTTP_API.md",
        "authentication": authentication_descriptor(&config),
        "publicEndpoints": [
            "GET /",
            "GET /health",
            "GET /openapi.json"
        ],
        "restEndpoints": [
            "GET /api/v1/status",
            "GET /api/v1/tasks",
            "POST /api/v1/tasks/:task_id/run",
            "POST /api/v1/tasks/:task_id/attach",
            "DELETE /api/v1/tasks/:task_id/attach",
            "GET /api/v1/processes",
            "POST /api/v1/processes/:process_id/start",
            "POST /api/v1/processes/:process_id/stop",
            "POST /api/v1/processes/:process_id/restart",
            "GET /api/v1/combs",
            "GET /api/v1/panes",
            "POST /api/v1/diffs/bundle",
            "GET /api/v1/shell/default",
            "GET /api/v1/events/stream",
            "GET /api/v1/terminals/events/stream",
            "POST /api/v1/terminals/spawn",
            "POST /api/v1/terminals/by-owner/:owner_key",
            "POST /api/v1/terminals/:pty_id/write",
            "POST /api/v1/terminals/:pty_id/resize",
            "POST /api/v1/terminals/:pty_id/kill",
            "POST /api/v1/sessions/start",
            "GET /api/v1/sessions",
            "GET /api/v1/sessions/search",
            "GET /api/v1/sessions/:session_id/events",
            "POST /api/v1/sessions/:session_id/turns",
            "POST /api/v1/sessions/:session_id/abort",
            "POST /api/v1/sessions/:session_id/resume",
            "POST /api/v1/sessions/:session_id/close",
            "POST /api/v1/sessions/:session_id/restore",
            "POST /api/v1/sessions/:session_id/respond-user-input",
            "POST /api/v1/sessions/:session_id/respond-permission",
            "POST /api/v1/auth/bearer/rotate"
        ],
        "rpcCompatibility": "POST /rpc"
    }))
}

async fn health_handler(State(config): State<Arc<RwLock<HttpConfig>>>) -> Response {
    let database = config.read().await.db_path.to_string_lossy().to_string();

    match rpc_value(config.clone(), "daemon.health", Value::Null, HEALTH_TIMEOUT).await {
        Ok(payload) => (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "daemon": "connected",
                "api": {
                    "name": "DCC HTTP API",
                    "version": env!("CARGO_PKG_VERSION"),
                    "protocolVersion": DCC_HTTP_PROTOCOL_VERSION,
                },
                "database": database,
                "daemonHealth": payload,
            })),
        )
            .into_response(),
        Err(error) => (
            error.status(),
            Json(json!({
                "status": "degraded",
                "daemon": "disconnected",
                "api": {
                    "name": "DCC HTTP API",
                    "version": env!("CARGO_PKG_VERSION"),
                    "protocolVersion": DCC_HTTP_PROTOCOL_VERSION,
                },
                "database": database,
                "error": {
                    "code": error.code(),
                    "message": error.message(),
                }
            })),
        )
            .into_response(),
    }
}

async fn openapi_handler(State(config): State<Arc<RwLock<HttpConfig>>>) -> Json<Value> {
    let config = config.read().await;
    Json(build_openapi_document(&config))
}

async fn shell_default_handler() -> Json<Value> {
    Json(json!({
        "shell": headless_default_shell(),
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventsStreamQuery {
    session_id: Option<String>,
}

/// Turns one broadcast item into an SSE frame. A session event carries its
/// durable sequence as the SSE `id`; falling behind the broadcast becomes an
/// explicit `resync` so the client refetches instead of silently missing
/// events.
fn events_stream_frame(
    message: Result<HttpStreamItem, BroadcastStreamRecvError>,
    session_filter: Option<&str>,
) -> Option<Event> {
    match message {
        Ok(item) => {
            let event = match &item.payload {
                HttpStreamPayload::Core(event) => event,
                // Few and small; a thread picks its own session's out.
                HttpStreamPayload::AgentRequests(requests) => {
                    return Some(
                        Event::default()
                            .event("agent-requests")
                            .data(requests.to_string()),
                    )
                }
            };
            if session_filter.is_some() && event.session_id() != session_filter {
                return None;
            }
            let payload = serde_json::to_string(event).ok()?;
            let frame = Event::default().event("core-event").data(payload);
            Some(match item.durable {
                Some(durable) => frame.id(durable.sequence.to_string()),
                None => frame,
            })
        }
        Err(BroadcastStreamRecvError::Lagged(missed)) => Some(
            Event::default()
                .event("resync")
                .data(json!({ "missed": missed }).to_string()),
        ),
    }
}

async fn events_stream_handler(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<EventsStreamQuery>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    HttpApiError,
> {
    let receiver = backend.events.subscribe();
    let session_filter = query.session_id.filter(|id| !id.is_empty());
    let stream = BroadcastStream::new(receiver)
        .filter_map(move |message| events_stream_frame(message, session_filter.as_deref()).map(Ok));

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

async fn terminals_stream_handler() -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    HttpApiError,
> {
    ensure_headless_terminal_sweeper();
    let sender = headless_terminal_sender();
    let stream = BroadcastStream::new(sender.subscribe()).filter_map(|message| match message {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(payload) => Some(Ok(Event::default()
                .event(terminal_event_name(&event))
                .data(payload))),
            Err(_) => None,
        },
        Err(_) => None,
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

fn build_openapi_document(config: &HttpConfig) -> Value {
    let security = match config.effective_auth_mode() {
        HttpAuthMode::Local => json!([{ "ApiKeyAuth": [] }]),
        HttpAuthMode::Remote => json!([{ "BearerAuth": [] }]),
        HttpAuthMode::Mixed => json!([{ "ApiKeyAuth": [] }, { "BearerAuth": [] }]),
    };

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "DCC HTTP API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "REST facade over the DCC daemon RPC store.",
            "x-protocolVersion": DCC_HTTP_PROTOCOL_VERSION
        },
        "servers": [
            {
                "url": format!("http://{}:{}", config.host, config.port)
            }
        ],
        "components": {
            "securitySchemes": {
                "ApiKeyAuth": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-API-Key"
                },
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                }
            }
        },
        "paths": {
            "/": {
                "get": {
                    "summary": "Server info",
                    "security": [],
                    "responses": {
                        "200": {
                            "description": "Server metadata"
                        }
                    }
                }
            },
            "/health": {
                "get": {
                    "summary": "Health check",
                    "security": [],
                    "responses": {
                        "200": {
                            "description": "Daemon reachable"
                        },
                        "503": {
                            "description": "Daemon unavailable"
                        }
                    }
                }
            },
            "/openapi.json": {
                "get": {
                    "summary": "OpenAPI document",
                    "security": [],
                    "responses": {
                        "200": {
                            "description": "OpenAPI document"
                        }
                    }
                }
            },
            "/rpc": {
                "post": {
                    "summary": "RPC compatibility endpoint",
                    "security": security.clone(),
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["method"],
                                    "properties": {
                                        "method": { "type": "string" },
                                        "params": { "type": "object" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "RPC response"
                        }
                    }
                }
            },
            "/api/v1/status": {
                "get": {
                    "summary": "Daemon status",
                    "security": security.clone(),
                    "responses": {
                        "200": {
                            "description": "Status payload"
                        }
                    }
                }
            },
            "/api/v1/tasks": {
                "get": {
                    "summary": "List daemon tasks",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "projectId",
                            "in": "query",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Task collection"
                        }
                    }
                }
            },
            "/api/v1/tasks/{taskId}/run": {
                "post": {
                    "summary": "Run a task",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "taskId",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "projectId",
                            "in": "query",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Task runtime payload"
                        }
                    }
                }
            },
            "/api/v1/tasks/{taskId}/attach": {
                "post": {
                    "summary": "Attach a task",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "taskId",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "projectId",
                            "in": "query",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Task runtime payload"
                        }
                    }
                },
                "delete": {
                    "summary": "Detach a task",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "taskId",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "projectId",
                            "in": "query",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Task runtime payload"
                        }
                    }
                }
            },
            "/api/v1/processes": {
                "get": {
                    "summary": "List daemon processes",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "projectId",
                            "in": "query",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Process collection"
                        }
                    }
                }
            },
            "/api/v1/processes/{processId}/start": {
                "post": {
                    "summary": "Start a process",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "processId",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "projectId",
                            "in": "query",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Process runtime payload"
                        }
                    }
                }
            },
            "/api/v1/processes/{processId}/stop": {
                "post": {
                    "summary": "Stop a process",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "processId",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "projectId",
                            "in": "query",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Process runtime payload"
                        }
                    }
                }
            },
            "/api/v1/processes/{processId}/restart": {
                "post": {
                    "summary": "Restart a process",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "processId",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "projectId",
                            "in": "query",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Process runtime payload"
                        }
                    }
                }
            },
            "/api/v1/combs": {
                "get": {
                    "summary": "List combs",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "projectId",
                            "in": "query",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Comb collection"
                        }
                    }
                }
            },
            "/api/v1/panes": {
                "get": {
                    "summary": "List panes",
                    "security": security.clone(),
                    "parameters": [
                        {
                            "name": "projectId",
                            "in": "query",
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "combId",
                            "in": "query",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Pane collection"
                        }
                    }
                }
            },
            "/api/v1/diffs/bundle": {
                "post": {
                    "summary": "Build a diff bundle",
                    "security": security.clone(),
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "worktreePaths": {
                                            "type": "array",
                                            "items": { "type": "string" }
                                        },
                                        "combIds": {
                                            "type": "array",
                                            "items": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Diff bundle payload"
                        }
                    }
                }
            },
            "/api/v1/auth/bearer/rotate": {
                "post": {
                    "summary": "Rotate bearer token",
                    "security": security.clone(),
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "ttlSeconds": { "type": "integer", "minimum": 60 },
                                        "graceSeconds": { "type": "integer", "minimum": 0 }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Rotated bearer token"
                        }
                    }
                }
            }
        }
    })
}

async fn list_tasks_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Query(query): Query<ProjectFilterQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(config, "daemon.listTasks", Value::Null, DEFAULT_TIMEOUT).await?;
    let filtered = filter_by_string_field(payload, "projectId", query.project_id.as_deref())?;
    Ok(Json(filtered))
}

async fn run_task_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(task_id): Path<String>,
    Query(query): Query<ProjectTaskActionQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "daemon.runTask",
        json!({
            "projectId": query.project_id,
            "taskId": task_id,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn attach_task_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(task_id): Path<String>,
    Query(query): Query<ProjectTaskActionQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "daemon.attachTask",
        json!({
            "projectId": query.project_id,
            "taskId": task_id,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn detach_task_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(task_id): Path<String>,
    Query(query): Query<ProjectTaskActionQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "daemon.detachTask",
        json!({
            "projectId": query.project_id,
            "taskId": task_id,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn status_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(config, "daemon.getStatus", Value::Null, DEFAULT_TIMEOUT).await?;
    Ok(Json(payload))
}

async fn list_processes_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Query(query): Query<ProjectFilterQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "daemon.listProcesses",
        json!({ "projectId": query.project_id }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn start_process_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(process_id): Path<String>,
    Query(query): Query<ProjectTaskActionQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "daemon.startProcess",
        json!({
            "projectId": query.project_id,
            "processId": process_id,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn stop_process_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(process_id): Path<String>,
    Query(query): Query<ProjectTaskActionQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "daemon.stopProcess",
        json!({
            "projectId": query.project_id,
            "processId": process_id,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn restart_process_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(process_id): Path<String>,
    Query(query): Query<ProjectTaskActionQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "daemon.restartProcess",
        json!({
            "projectId": query.project_id,
            "processId": process_id,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn list_combs_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Query(query): Query<ProjectFilterQuery>,
) -> Result<Json<Value>, HttpApiError> {
    // In-process read of the live worktrees (see `live_combs`).
    let project_id = query.project_id.clone();
    let payload = db_read(config, move |conn| live_combs(conn, project_id)).await?;
    Ok(Json(payload))
}

async fn list_panes_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Query(query): Query<ProjectAndCombFilterQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = rpc_value(
        config,
        "panes.list",
        json!({
            "projectId": query.project_id,
            "combId": query.comb_id,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn diffs_bundle_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Json(body): Json<BundleRequest>,
) -> Result<Json<Value>, HttpApiError> {
    // Resolve comb ids → worktree paths in-process (against the live
    // `dcc_workspaces`), then hand the daemon plain paths. The daemon's git
    // logic is unchanged and works regardless of which table holds the combs.
    let mut paths = body.worktree_paths.clone();
    if !body.comb_ids.is_empty() {
        let comb_ids = body.comb_ids.clone();
        let resolved = db_read(config.clone(), move |conn| {
            resolve_worktree_paths(conn, &comb_ids)
        })
        .await?;
        paths.extend(resolved);
    }
    let payload = rpc_value(
        config,
        "diffs.bundle",
        json!({ "worktreePaths": paths, "combIds": [] }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn terminal_spawn_handler(
    Json(input): Json<TerminalSpawnRequest>,
) -> Result<Json<Value>, HttpApiError> {
    Ok(Json(spawn_headless_terminal(input)?))
}

async fn terminal_get_or_create_by_owner_handler(
    Path(owner_key): Path<String>,
    Json(mut input): Json<TerminalSpawnRequest>,
) -> Result<Json<Value>, HttpApiError> {
    ensure_headless_terminal_sweeper();

    {
        let mut terminals = headless_terminals()
            .lock()
            .map_err(|error| HttpApiError::internal(format!("terminals lock poisoned: {error}")))?;
        if let Some((pty_id, terminal)) = terminals
            .iter_mut()
            .find(|(_, terminal)| terminal.pty_owner_key.as_deref() == Some(owner_key.as_str()))
        {
            let session = headless_terminal_session_snapshot(pty_id, terminal);
            let chunks = terminal
                .output_buffer
                .lock()
                .map(|buffer| buffer.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            return Ok(Json(json!({
                "ptyId": pty_id,
                "existing": true,
                "session": session,
                "chunks": chunks,
                "truncated": false,
            })));
        }
    }

    input.pty_owner_key = Some(owner_key);
    let spawn = spawn_headless_terminal(input)?;
    let pty_id = spawn
        .get("ptyId")
        .and_then(Value::as_str)
        .ok_or_else(|| HttpApiError::internal("spawned terminal missing ptyId"))?
        .to_string();
    let mut terminals = headless_terminals()
        .lock()
        .map_err(|error| HttpApiError::internal(format!("terminals lock poisoned: {error}")))?;
    let terminal = terminals
        .get_mut(&pty_id)
        .ok_or_else(|| HttpApiError::internal("spawned terminal not found in runtime"))?;
    let session = headless_terminal_session_snapshot(&pty_id, terminal);
    let chunks = terminal
        .output_buffer
        .lock()
        .map(|buffer| buffer.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(Json(json!({
        "ptyId": pty_id,
        "existing": false,
        "session": session,
        "chunks": chunks,
        "truncated": false,
    })))
}

async fn terminal_write_handler(
    Path(pty_id): Path<String>,
    Json(input): Json<TerminalWriteRequest>,
) -> Result<Json<Value>, HttpApiError> {
    let terminals = headless_terminals()
        .lock()
        .map_err(|error| HttpApiError::internal(format!("terminals lock poisoned: {error}")))?;
    let Some(terminal) = terminals.get(&pty_id) else {
        return Ok(Json(json!({ "ok": false })));
    };
    let mut writer = terminal
        .writer
        .lock()
        .map_err(|error| HttpApiError::internal(format!("writer lock poisoned: {error}")))?;
    pty_write_all_chunked(&mut *writer, input.data.as_bytes())
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

async fn terminal_resize_handler(
    Path(pty_id): Path<String>,
    Json(input): Json<TerminalResizeRequest>,
) -> Result<Json<Value>, HttpApiError> {
    let terminals = headless_terminals()
        .lock()
        .map_err(|error| HttpApiError::internal(format!("terminals lock poisoned: {error}")))?;
    let Some(terminal) = terminals.get(&pty_id) else {
        return Ok(Json(json!({ "ok": false })));
    };
    terminal
        .pty_master
        .resize(PtySize {
            rows: input.rows,
            cols: input.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

async fn terminal_kill_handler(Path(pty_id): Path<String>) -> Result<Json<Value>, HttpApiError> {
    let terminal = {
        let mut terminals = headless_terminals()
            .lock()
            .map_err(|error| HttpApiError::internal(format!("terminals lock poisoned: {error}")))?;
        terminals.remove(&pty_id)
    };

    let Some(mut terminal) = terminal else {
        return Ok(Json(json!({ "ok": false })));
    };

    terminal.stop_flag.store(true, Ordering::Relaxed);
    let _ = terminal.child.kill();
    drop(terminal.pty_master);
    let _ = terminal.reader_thread.take();
    let _ = headless_terminal_sender().send(HeadlessTerminalEvent::Exit {
        pty_id,
        code: Some(-1),
    });
    Ok(Json(json!({ "ok": true })))
}

async fn start_thread_handler(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<StartThreadInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = start_thread_with_state(&backend.sessions, input)
        .await
        .map_err(classify_session_error)?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn list_workspace_sessions_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Query(query): Query<WorkspaceSessionsQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let workspace_id = query.workspace_id;
    let payload = session_repo_operation(config, move |repo| {
        let items = repo
            .list_workspace_sessions(&WorkspaceId(workspace_id))
            .map_err(|error| error.to_string())?;
        serde_json::to_value(items).map_err(|error| error.to_string())
    })
    .await?;
    Ok(Json(payload))
}

async fn search_sessions_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Query(query): Query<SessionSearchQuery>,
) -> Result<Json<Value>, HttpApiError> {
    // Reads the live `dcc_sessions`/`dcc_workspaces` tables in-process instead
    // of the FTS index (which accumulates orphaned rows for deleted worktrees).
    // The text query is applied client-side by the mobile companion.
    let limit = query.limit.clamp(1, 200) as i64;
    let payload = db_read(config, move |conn| live_sessions(conn, limit)).await?;
    Ok(Json(payload))
}

async fn send_turn_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<SendTurnInput>,
) -> Result<Json<Value>, HttpApiError> {
    input.session_id = SessionId(session_id);
    mobile::resolve_turn_approval_policy(config, &backend, &mut input).await?;
    let output = send_turn_with_state(&backend.sessions, input)
        .await
        .map_err(classify_session_error)?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

const SESSION_EVENTS_DEFAULT_LIMIT: usize = 500;
const SESSION_EVENTS_MAX_LIMIT: usize = 2000;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionEventsQuery {
    /// Durable sequence the client already holds; only later events return.
    after: Option<u64>,
    limit: Option<usize>,
}

/// Without `after`: the whole history as a bare array (the original shape).
/// With `after`: `{events, lastSeq, hasMore}`, where `lastSeq` is the
/// session's highest sequence even when the delta is empty, so the client can
/// tell "up to date" from "history went backwards" (`lastSeq < cursor`).
async fn list_session_events_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
    Query(query): Query<SessionEventsQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = session_repo_operation(config, move |repo| {
        let session_id = SessionId(session_id);
        let Some(after) = query.after else {
            let items = repo
                .list_events_by_session_sync(&session_id)
                .map_err(|error| error.to_string())?;
            return serde_json::to_value(items).map_err(|error| error.to_string());
        };
        let limit = query
            .limit
            .unwrap_or(SESSION_EVENTS_DEFAULT_LIMIT)
            .clamp(1, SESSION_EVENTS_MAX_LIMIT);
        let mut events = repo
            .list_events_by_session_after_sync(&session_id, after, limit + 1)
            .map_err(|error| error.to_string())?;
        let has_more = events.len() > limit;
        events.truncate(limit);
        // Read after the delta so `lastSeq` is never below an event in it.
        let last_seq = repo
            .last_event_sequence_sync(&session_id)
            .map_err(|error| error.to_string())?;
        Ok(json!({ "events": events, "lastSeq": last_seq, "hasMore": has_more }))
    })
    .await?;
    Ok(Json(payload))
}

async fn abort_session_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let output = abort_run_with_state(
        &backend.sessions,
        AbortRunInput {
            session_id: SessionId(session_id),
            reason: Some("Stopped from remote HTTP".to_string()),
        },
    )
    .await
    .map_err(classify_session_error)?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn resume_session_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let state = &backend.sessions;
    let session_id = SessionId(session_id);
    let transition = state
        .acquire_provider_transition(&session_id)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    state
        .validate_provider_resume_preflight_under_transition(&transition, &session_id)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    let output = resume_session(state, state, state, ResumeSessionInput { session_id })
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    state
        .attach_current_provider_session_under_transition(&transition, &output.session)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn close_session_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    body: Option<Json<CloseSessionRequest>>,
) -> Result<Json<Value>, HttpApiError> {
    let delete_history = body.map(|Json(body)| body.delete_history).unwrap_or(false);
    let state = &backend.sessions;
    let session_id = SessionId(session_id);
    let transition = state.acquire_provider_transition(&session_id).await;
    let transition = transition.map_err(|error| classify_session_error(error.to_string()))?;
    state
        .cancel_provider_session_if_attached_under_transition(&transition, &session_id)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    let output = close_session(
        state,
        state,
        state,
        CloseSessionInput {
            session_id,
            delete_history,
        },
    )
    .await
    .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn restore_session_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let state = &backend.sessions;
    let output = restore_session(
        state,
        state,
        RestoreSessionInput {
            session_id: SessionId(session_id),
        },
    )
    .await
    .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

fn classify_respond_error(error: RespondToProviderRequestError) -> HttpApiError {
    match &error {
        RespondToProviderRequestError::AlreadyAnswered(answer) => HttpApiError::Conflict(
            error.to_string(),
            serde_json::to_value(answer).unwrap_or(Value::Null),
        ),
        RespondToProviderRequestError::Failed(message) => classify_session_error(message.clone()),
    }
}

async fn respond_to_user_input_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<RespondToUserInputInput>,
) -> Result<Json<Value>, HttpApiError> {
    input.session_id = session_id;
    let output = respond_to_user_input_with_state(&backend.sessions, input)
        .await
        .map_err(classify_respond_error)?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn respond_to_permission_request_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<RespondToPermissionRequestInput>,
) -> Result<Json<Value>, HttpApiError> {
    input.session_id = session_id;
    let output = respond_to_permission_request_with_state(&backend.sessions, input)
        .await
        .map_err(classify_respond_error)?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RotateBearerTokenRequest {
    #[serde(default)]
    ttl_seconds: Option<i64>,
    #[serde(default)]
    grace_seconds: Option<i64>,
}

async fn rotate_bearer_token_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    body: Option<Json<RotateBearerTokenRequest>>,
) -> Result<Json<Value>, HttpApiError> {
    let mut config = config.write().await;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let rotation = config.rotate_bearer_token(body.ttl_seconds, body.grace_seconds);

    if let Some(path) = crate::http_config::HttpConfig::default_config_path() {
        let _ = config.save(&path);
    }

    let mut response = json!({
        "ok": true,
        "authMode": format!("{:?}", config.effective_auth_mode()).to_lowercase(),
        "bearerToken": rotation.token,
        "expiresAt": rotation.expires_at,
    });

    if let Some(previous_expires_at) = rotation.previous_expires_at {
        response["previousExpiresAt"] = json!(previous_expires_at);
    }

    Ok(Json(response))
}

fn authentication_descriptor(config: &HttpConfig) -> Value {
    json!({
        "mode": format!("{:?}", config.effective_auth_mode()).to_lowercase(),
        "local": {
            "header": "X-API-Key"
        },
        "remote": {
            "header": "Authorization",
            "scheme": "Bearer",
            "bearerTokenExpiresAt": config.bearer_token_expires_at,
            "bearerTokenPreviousExpiresAt": config.bearer_token_previous_expires_at,
        }
    })
}

/// Relay administration is for the operator on this machine (API key), never
/// for a paired device: a phone must not approve phones or mint QR codes.
fn operator_remote(
    backend: &HttpBackend,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
) -> Result<Arc<RemoteHub>, HttpApiError> {
    if device.is_some() {
        return Err(HttpApiError::Forbidden(
            "paired devices cannot manage remote access".into(),
        ));
    }
    backend
        .remote()
        .ok_or_else(|| HttpApiError::service_unavailable("remote access is not running"))
}

async fn remote_status_handler(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
) -> Result<Json<Value>, HttpApiError> {
    let remote = operator_remote(&backend, device)?;
    Ok(Json(json!(remote.status())))
}

async fn remote_create_offer_handler(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
) -> Result<Json<Value>, HttpApiError> {
    let remote = operator_remote(&backend, device)?;
    let offer = remote
        .create_offer()
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(offer)))
}

async fn remote_list_approvals_handler(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
) -> Result<Json<Value>, HttpApiError> {
    let remote = operator_remote(&backend, device)?;
    Ok(Json(json!({ "approvals": remote.pending_approvals() })))
}

#[derive(Deserialize)]
struct RemoteApprovalBody {
    approve: bool,
}

async fn remote_respond_approval_handler(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Path(approval_id): Path<String>,
    Json(body): Json<RemoteApprovalBody>,
) -> Result<Json<Value>, HttpApiError> {
    let remote = operator_remote(&backend, device)?;
    if !remote.respond_approval(&approval_id, body.approve) {
        return Err(HttpApiError::not_found(
            "approval request not found or expired",
        ));
    }
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::{
        domain::{
            project::ProjectId,
            provider::ProviderApprovalPolicy,
            session::{Session, SessionState},
            workspace::WorkspaceId,
        },
        ports::{SessionEventRepo, SessionRepo},
    };

    #[test]
    fn filters_arrays_by_string_field() {
        let value = json!([
            { "projectId": "one", "name": "a" },
            { "projectId": "two", "name": "b" },
            { "projectId": "one", "name": "c" }
        ]);

        let filtered = filter_by_string_field(value, "projectId", Some("one")).unwrap();
        assert_eq!(
            filtered,
            json!([
                { "projectId": "one", "name": "a" },
                { "projectId": "one", "name": "c" }
            ])
        );
    }

    #[tokio::test]
    async fn lan_pairing_and_device_routes_are_gone() {
        use tower::util::ServiceExt;

        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        let db_path = root_path.join("state.sqlite");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(include_str!("../sql/schema.sql"))
            .unwrap();
        crate::pairing::ensure_pairing_schema(&conn).unwrap();
        let config = Arc::new(RwLock::new(HttpConfig {
            db_path: db_path.clone(),
            api_key: "k".to_string(),
            ..HttpConfig::default()
        }));
        let backend = Arc::new(HttpBackend::headless(db_path, root_path.join("app-data")));
        let app = build_router(config, backend);
        let call = |method: &str, uri: &str| {
            axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header("X-API-Key", "k")
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}"))
                .unwrap()
        };

        for (method, uri) in [
            ("POST", "/auth/pair"),
            ("POST", "/auth/pair-init"),
            ("GET", "/m/"),
            ("GET", "/m/pair"),
            ("GET", "/auth/devices"),
            ("DELETE", "/auth/devices/some-device"),
        ] {
            let response = app.clone().oneshot(call(method, uri)).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {uri}");
        }
    }

    #[test]
    fn openapi_document_exposes_api_key_security() {
        let doc = build_openapi_document(&HttpConfig::default());
        assert_eq!(doc["openapi"], "3.0.3");
        assert_eq!(
            doc["components"]["securitySchemes"]["ApiKeyAuth"]["name"],
            "X-API-Key"
        );
        assert_eq!(
            doc["components"]["securitySchemes"]["BearerAuth"]["scheme"],
            "bearer"
        );
        assert!(doc["paths"]["/api/v1/tasks"].is_object());
        assert!(doc["paths"]["/api/v1/processes"].is_object());
        assert!(doc["paths"]["/api/v1/combs"].is_object());
        assert!(doc["paths"]["/api/v1/panes"].is_object());
        assert!(doc["paths"]["/api/v1/diffs/bundle"].is_object());
        assert!(doc["paths"]["/api/v1/auth/bearer/rotate"].is_object());
        assert!(doc["paths"]["/rpc"].is_object());
        assert_eq!(doc["paths"]["/health"]["get"]["security"], json!([]));
        assert_eq!(doc["paths"]["/"]["get"]["security"], json!([]));
    }

    struct HttpFixture {
        _root: tempfile::TempDir,
        state: SessionCommandState,
        backend: Arc<HttpBackend>,
        app: Router,
        session_id: SessionId,
    }

    /// A router on an embedded backend, as the desktop app serves it.
    async fn embedded_fixture() -> HttpFixture {
        let root = tempfile::tempdir().expect("http root");
        let physical = std::fs::canonicalize(root.path()).expect("physical root");
        let db_path = physical.join("state.sqlite");
        let state = SessionCommandState::new_headless(db_path.clone(), physical.join("app-data"));
        let session_id = SessionId("http-respond".to_string());
        SessionRepo::save_session(
            &state,
            &Session {
                id: session_id.clone(),
                project_id: ProjectId("project".to_string()),
                workspace_id: WorkspaceId("workspace".to_string()),
                additional_workspace_ids: Vec::new(),
                provider_id: "codex".to_string(),
                model: None,
                provider_runtime: None,
                working_directory_override: None,
                state: SessionState::Active,
                created_at: "2026-10-08T00:00:00Z".to_string(),
                updated_at: "2026-10-08T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("save session");
        let config = Arc::new(RwLock::new(HttpConfig {
            db_path,
            api_key: "k".to_string(),
            ..HttpConfig::default()
        }));
        let backend = Arc::new(HttpBackend::embedded(state.clone()).expect("embedded backend"));
        HttpFixture {
            _root: root,
            state,
            backend: backend.clone(),
            app: build_router(config, backend),
            session_id,
        }
    }

    async fn post_json(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
        use tower::util::ServiceExt;
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .header("X-API-Key", "k")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn answering_a_resolved_permission_returns_conflict_with_the_outcome() {
        use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, TurnId};
        let fixture = embedded_fixture().await;
        for kind in [
            SessionEventKind::TurnPermissionRequested {
                turn_id: TurnId("t1".to_string()),
                request_id: "p1".to_string(),
                tool_name: "Bash".to_string(),
                title: None,
                description: None,
                command: Some("yarn test".to_string()),
                file: None,
            },
            SessionEventKind::TurnPermissionResolved {
                turn_id: TurnId("t1".to_string()),
                request_id: "p1".to_string(),
                behavior: "allow".to_string(),
            },
        ] {
            SessionEventRepo::append_event(
                &fixture.state,
                &SessionEventRecord {
                    event_id: Uuid::new_v4().to_string(),
                    session_id: fixture.session_id.clone(),
                    sequence: 0,
                    occurred_at: "2026-10-08T00:00:01Z".to_string(),
                    kind,
                },
            )
            .await
            .expect("append event");
        }

        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/respond-permission",
            json!({ "sessionId": "", "requestId": "p1", "behavior": "deny" }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "already_resolved");
        assert_eq!(
            body["error"]["outcome"],
            json!({ "kind": "permission", "behavior": "allow" })
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_second_answer_while_the_first_is_in_flight_is_a_conflict() {
        use dcc_tauri::state::AnsweredProviderRequest;
        let fixture = embedded_fixture().await;
        // The desktop claimed the request and is forwarding its answer.
        assert_eq!(
            fixture
                .state
                .claim_provider_request(
                    &fixture.session_id,
                    "q1",
                    AnsweredProviderRequest::Permission {
                        behavior: "deny".to_string(),
                    },
                )
                .unwrap(),
            None
        );

        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/respond-permission",
            json!({ "sessionId": "", "requestId": "q1", "behavior": "allow" }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["outcome"]["behavior"], "deny");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_answer_that_never_reached_the_provider_can_be_given_again() {
        let fixture = embedded_fixture().await;
        let input = || RespondToPermissionRequestInput {
            session_id: fixture.session_id.0.clone(),
            request_id: "r1".to_string(),
            behavior: "allow".to_string(),
        };
        // No provider is attached in the fixture, so forwarding fails.
        for _ in 0..2 {
            assert!(matches!(
                respond_to_permission_request_with_state(&fixture.state, input()).await,
                Err(RespondToProviderRequestError::Failed(_))
            ));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn embedded_backend_streams_the_desktop_runtime_events() {
        let fixture = embedded_fixture().await;
        let backend = HttpBackend::embedded(fixture.state.clone()).expect("embedded backend");
        let mut receiver = backend.events.subscribe();

        EventBus::publish(
            &fixture.state,
            CoreEvent::SessionResumed {
                session_id: fixture.session_id.0.clone(),
            },
        )
        .await
        .expect("publish");

        assert!(matches!(
            receiver.try_recv(),
            Ok(HttpStreamItem {
                payload: HttpStreamPayload::Core(CoreEvent::SessionResumed { .. }),
                durable: None,
            })
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn durable_session_events_reach_the_stream_with_their_sequence() {
        use dcc_core::domain::session::{SessionEventKind, SessionEventRecord};
        let fixture = embedded_fixture().await;
        let mut receiver = fixture.backend.events.subscribe();
        let record = SessionEventRecord {
            event_id: "evt-42".to_string(),
            session_id: fixture.session_id.clone(),
            sequence: 42,
            occurred_at: "2026-10-08T00:00:00Z".to_string(),
            kind: SessionEventKind::SessionResumed,
        };

        EventBus::publish_durable_session(
            &fixture.state,
            &record,
            CoreEvent::SessionResumed {
                session_id: fixture.session_id.0.clone(),
            },
        )
        .await
        .expect("publish");

        let item = receiver.try_recv().expect("stream item");
        assert_eq!(item.durable.map(|durable| durable.sequence), Some(42));
        // One frame per event, not one from `publish` and one from the envelope.
        assert!(receiver.try_recv().is_err());
    }

    async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
        use tower::util::ServiceExt;
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .header("X-API-Key", "k")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn append_resumed_events(fixture: &HttpFixture, count: usize) {
        use dcc_core::domain::session::{SessionEventKind, SessionEventRecord};
        for _ in 0..count {
            SessionEventRepo::append_event(
                &fixture.state,
                &SessionEventRecord {
                    event_id: Uuid::new_v4().to_string(),
                    session_id: fixture.session_id.clone(),
                    sequence: 0,
                    occurred_at: "2026-10-08T00:00:00Z".to_string(),
                    kind: SessionEventKind::SessionResumed,
                },
            )
            .await
            .expect("append event");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_events_after_a_cursor_return_only_the_delta() {
        let fixture = embedded_fixture().await;
        append_resumed_events(&fixture, 5).await;

        let (status, body) = get_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/events?after=2&limit=2",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let sequences: Vec<u64> = body["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["sequence"].as_u64().unwrap())
            .collect();
        assert_eq!(sequences, vec![3, 4]);
        assert_eq!(body["lastSeq"], 5);
        assert_eq!(body["hasMore"], true);

        let (_, caught_up) =
            get_json(&fixture.app, "/api/v1/sessions/http-respond/events?after=5").await;
        assert_eq!(caught_up["events"], json!([]));
        assert_eq!(caught_up["lastSeq"], 5);
        assert_eq!(caught_up["hasMore"], false);

        // A cursor past the end (history deleted and recreated) still reports
        // the real last sequence, so the client can reset.
        let (_, ahead) = get_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/events?after=99",
        )
        .await;
        assert_eq!(ahead["lastSeq"], 5);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_events_without_a_cursor_keep_the_bare_array() {
        let fixture = embedded_fixture().await;
        append_resumed_events(&fixture, 2).await;

        let (status, body) = get_json(&fixture.app, "/api/v1/sessions/http-respond/events").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().map(Vec::len), Some(2));
    }

    /// Opens the SSE stream and returns its body, ready to be read.
    async fn open_stream(app: &Router, uri: &str) -> axum::body::BodyDataStream {
        use tower::util::ServiceExt;
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .header("X-API-Key", "k")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.into_body().into_data_stream()
    }

    /// Reads SSE frames until one contains `needle` (keep-alives skipped).
    async fn read_until(body: &mut axum::body::BodyDataStream, needle: &str) -> String {
        let mut seen = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !seen.contains(needle) {
            let data = tokio::time::timeout_at(deadline, body.next())
                .await
                .expect("SSE frame in time")
                .expect("stream open")
                .expect("frame");
            seen.push_str(&String::from_utf8_lossy(&data));
        }
        seen
    }

    fn stream_item(session_id: &str, sequence: Option<u64>) -> HttpStreamItem {
        HttpStreamItem {
            payload: HttpStreamPayload::Core(CoreEvent::SessionResumed {
                session_id: session_id.to_string(),
            }),
            durable: sequence.map(|sequence| SessionLiveDurableIdentity {
                session_id: session_id.to_string(),
                event_id: format!("evt-{sequence}"),
                sequence,
            }),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn event_stream_filters_by_session_and_ids_frames_by_sequence() {
        let fixture = embedded_fixture().await;
        let mut body = open_stream(&fixture.app, "/api/v1/events/stream?sessionId=wanted").await;

        fixture
            .backend
            .events
            .send(stream_item("other", Some(3)))
            .unwrap();
        fixture
            .backend
            .events
            .send(stream_item("wanted", Some(7)))
            .unwrap();

        let seen = read_until(&mut body, "wanted").await;
        assert!(!seen.contains("other"), "{seen}");
        assert!(seen.contains("id: 7"), "{seen}");
        assert!(seen.contains("event: core-event"), "{seen}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn falling_behind_the_stream_sends_resync_instead_of_dropping() {
        let fixture = embedded_fixture().await;
        let mut body = open_stream(&fixture.app, "/api/v1/events/stream").await;

        // Overflow the 256-slot broadcast before the client reads anything.
        for sequence in 0..300 {
            let _ = fixture
                .backend
                .events
                .send(stream_item("busy", Some(sequence)));
        }

        let seen = read_until(&mut body, "event: resync").await;
        assert!(seen.contains("missed"), "{seen}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn embedded_backend_serves_no_remote_terminals() {
        let fixture = embedded_fixture().await;
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/terminals/spawn",
            json!({ "cwd": "/tmp" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn http_turn_preflight_rejects_unknown_provider_without_persisting_selection() {
        let root = tempfile::tempdir().expect("http state root");
        let root = std::fs::canonicalize(root.path()).expect("physical http state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session = Session {
            id: SessionId("http-preflight".to_string()),
            project_id: ProjectId("project".to_string()),
            workspace_id: WorkspaceId("workspace".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-09-01T00:00:00Z".to_string(),
            updated_at: "2026-09-01T00:00:00Z".to_string(),
        };
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save HTTP session");
        let input = SendTurnInput {
            session_id: session.id.clone(),
            prompt: "must not become a turn".to_string(),
            tool_instructions: None,
            provider_id: Some("unknown-provider".to_string()),
            model: None,
            provider_runtime: None,
            plan_mode: None,
            effort: None,
            fast_mode: None,
            approval_policy: None,
            evidence: None,
            retry_of_turn_id: None,
            decision_provider_model_route: None,
        };

        assert!(send_turn_with_state(&state, input).await.is_err());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("load HTTP session")
                .expect("HTTP session")
                .provider_id,
            "codex"
        );
        assert!(
            SessionEventRepo::list_events_by_session(&state, &session.id)
                .await
                .expect("HTTP session events")
                .is_empty()
        );
    }

    fn http_turn(
        session_id: &SessionId,
        provider_id: Option<&str>,
        approval_policy: Option<ProviderApprovalPolicy>,
    ) -> SendTurnInput {
        SendTurnInput {
            session_id: session_id.clone(),
            prompt: "continue".to_string(),
            tool_instructions: None,
            provider_id: provider_id.map(str::to_string),
            model: None,
            provider_runtime: None,
            plan_mode: None,
            effort: None,
            fast_mode: None,
            approval_policy,
            evidence: None,
            retry_of_turn_id: None,
            decision_provider_model_route: None,
        }
    }

    async fn append_turn_started(
        fixture: &HttpFixture,
        turn: &str,
        policy: Option<ProviderApprovalPolicy>,
    ) {
        use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, TurnId};
        SessionEventRepo::append_event(
            &fixture.state,
            &SessionEventRecord {
                event_id: Uuid::new_v4().to_string(),
                session_id: fixture.session_id.clone(),
                sequence: 0,
                occurred_at: "2026-10-10T00:00:00Z".to_string(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: TurnId(turn.to_string()),
                    prompt: "p".to_string(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: policy,
                },
            },
        )
        .await
        .expect("append turn");
    }

    fn fixture_config(fixture: &HttpFixture) -> Arc<RwLock<HttpConfig>> {
        let db_path = std::fs::canonicalize(fixture._root.path())
            .expect("physical root")
            .join("state.sqlite");
        Arc::new(RwLock::new(HttpConfig {
            db_path,
            ..HttpConfig::default()
        }))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn phone_turns_keep_the_session_access_level() {
        use ProviderApprovalPolicy::*;
        let fixture = embedded_fixture().await;
        let config = fixture_config(&fixture);

        // Nothing recorded yet: the provider's own default, as before.
        let mut input = http_turn(&fixture.session_id, None, None);
        mobile::resolve_turn_approval_policy(config.clone(), &fixture.backend, &mut input)
            .await
            .unwrap();
        assert_eq!(input.approval_policy, None);

        // The desktop chose full access; a later turn without a level skipped
        // recording one. A continuation from the phone keeps full access.
        append_turn_started(&fixture, "t1", Some(FullAccess)).await;
        append_turn_started(&fixture, "t2", None).await;
        let mut input = http_turn(&fixture.session_id, None, None);
        mobile::resolve_turn_approval_policy(config.clone(), &fixture.backend, &mut input)
            .await
            .unwrap();
        assert_eq!(input.approval_policy, Some(FullAccess));

        // An explicit choice wins over the session's level.
        let mut input = http_turn(&fixture.session_id, None, Some(Ask));
        mobile::resolve_turn_approval_policy(config.clone(), &fixture.backend, &mut input)
            .await
            .unwrap();
        assert_eq!(input.approval_policy, Some(Ask));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn phone_turns_refuse_a_level_the_provider_does_not_enforce() {
        let fixture = embedded_fixture().await;
        let lacking = dcc_providers::PROVIDER_IDS
            .into_iter()
            .find(|id| {
                !mobile::provider_approval_policies(id)
                    .contains(&ProviderApprovalPolicy::FullAccess)
            })
            .expect("a provider without full access");
        let (status, body) = post_json(
            &fixture.app,
            &format!("/api/v1/sessions/{}/turns", fixture.session_id.0),
            json!({
                "sessionId": fixture.session_id.0, "prompt": "go",
                "providerId": lacking, "approvalPolicy": "full_access",
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            SessionEventRepo::list_events_by_session(&fixture.state, &fixture.session_id)
                .await
                .expect("events")
                .is_empty(),
            "a refused level must not start a turn"
        );

        // An inherited level the new provider can't enforce is dropped, not widened.
        append_turn_started(&fixture, "t1", Some(ProviderApprovalPolicy::FullAccess)).await;
        let mut input = http_turn(&fixture.session_id, Some(lacking), None);
        mobile::resolve_turn_approval_policy(
            fixture_config(&fixture),
            &fixture.backend,
            &mut input,
        )
        .await
        .unwrap();
        assert_ne!(
            input.approval_policy,
            Some(ProviderApprovalPolicy::FullAccess)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn phone_attachments_travel_as_prompt_paths_and_preview_only_for_their_session() {
        use base64::Engine as _;
        use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, TurnId};
        let fixture = embedded_fixture().await;
        let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);

        // A photo is stored like a pasted image, by its content (not its name).
        let png = b"\x89PNG\r\n\x1a\nphone-photo";
        let (status, photo) = post_json(
            &fixture.app,
            "/api/v1/mobile/attachments",
            json!({ "name": "IMG_0001.HEIC", "data": encode(png) }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{photo}");
        assert_eq!(photo["kind"], "image");
        let photo_path = photo["path"].as_str().unwrap().to_string();
        assert!(photo_path.ends_with(".png"), "{photo_path}");
        assert_eq!(std::fs::read(&photo_path).unwrap(), png);

        // A document keeps a readable, single-segment name.
        let (status, doc) = post_json(
            &fixture.app,
            "/api/v1/mobile/attachments",
            json!({ "name": "../Relatório final.pdf", "data": encode(b"%PDF-1.7") }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{doc}");
        assert_eq!(doc["kind"], "file");
        assert_eq!(doc["name"], "Relatório-final.pdf");

        // Over the limit: refused with the limit in the message, before any file.
        let big = vec![0u8; crate::prompt_attachments::MAX_PHONE_ATTACHMENT_BYTES + 1];
        let (status, refused) = post_json(
            &fixture.app,
            "/api/v1/mobile/attachments",
            json!({ "name": "big.bin", "data": encode(&big) }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(refused["error"]["message"], "O anexo passa de 10 MB.");
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/mobile/attachments",
            json!({ "name": "x.png", "data": "not base64!" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // The preview reads only what a prompt of this session attached.
        let preview_uri = |path: &str| {
            format!(
                "/api/v1/mobile/sessions/http-respond/attachment?path={}",
                url_encode(path)
            )
        };
        let (status, _) = get_json(&fixture.app, &preview_uri(&photo_path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "not referenced yet");
        SessionEventRepo::append_event(
            &fixture.state,
            &SessionEventRecord {
                event_id: Uuid::new_v4().to_string(),
                session_id: fixture.session_id.clone(),
                sequence: 0,
                occurred_at: "2026-10-10T00:00:00Z".to_string(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: TurnId("with-photo".to_string()),
                    prompt: format!("o que tem aqui? @{photo_path}"),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: None,
                },
            },
        )
        .await
        .expect("append turn");
        let (status, body) = get_json(&fixture.app, &preview_uri(&photo_path)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["dataUrl"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        let other = crate::prompt_attachments::save_temp_image(png, "png").unwrap();
        for path in [other.to_string_lossy().to_string(), "relative.png".into()] {
            let (status, _) = get_json(&fixture.app, &preview_uri(&path)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
        let (status, _) = get_json(
            &fixture.app,
            "/api/v1/mobile/sessions/someone-else/attachment?path=%2Ftmp%2Fx.png",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        for path in [photo_path, doc["path"].as_str().unwrap().to_string()] {
            let _ = std::fs::remove_file(&path);
        }
        let _ = std::fs::remove_file(other);
    }

    fn url_encode(value: &str) -> String {
        value
            .bytes()
            .map(|byte| match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                    (byte as char).to_string()
                }
                _ => format!("%{byte:02X}"),
            })
            .collect()
    }

    #[test]
    fn workspaces_report_the_branch_their_folder_has_checked_out() {
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str], dir: &FsPath| {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .status()
                .unwrap()
                .success());
        };
        git(&["init", "-q", "-b", "main"], repo.path());
        git(
            &["commit", "-q", "--allow-empty", "-m", "init"],
            repo.path(),
        );
        assert_eq!(
            mobile::checked_out_branch(repo.path()).as_deref(),
            Some("main")
        );
        let worktree = repo.path().join("wt");
        git(
            &["worktree", "add", "-q", "--detach", "wt", "main"],
            repo.path(),
        );
        assert_eq!(mobile::checked_out_branch(&worktree), None, "detached");
        git(&["switch", "-q", "-c", "dcc/tarefa-1"], &worktree);
        assert_eq!(
            mobile::checked_out_branch(&worktree).as_deref(),
            Some("dcc/tarefa-1")
        );
        assert_eq!(
            mobile::checked_out_branch(repo.path()).as_deref(),
            Some("main")
        );
        assert_eq!(mobile::checked_out_branch(&repo.path().join("nope")), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn phone_file_search_resolves_workspaces_by_id_only() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        use dcc_core::ports::WorkspaceRepo;
        let fixture = embedded_fixture().await;
        let tree = tempfile::tempdir().unwrap();
        let tree_path = std::fs::canonicalize(tree.path()).unwrap();
        for args in [vec!["init", "-q"], vec!["add", "."]] {
            if args[0] == "add" {
                std::fs::create_dir_all(tree_path.join("src/ui")).unwrap();
                for file in [
                    "README.md",
                    "src/main.rs",
                    "src/ui/Button.tsx",
                    "src/ui/button.css",
                ] {
                    std::fs::write(tree_path.join(file), "x").unwrap();
                }
                std::fs::write(tree_path.join("untracked.txt"), "x").unwrap();
                let status = std::process::Command::new("git")
                    .args(["add", "README.md", "src"])
                    .current_dir(&tree_path)
                    .status()
                    .unwrap();
                assert!(status.success());
                continue;
            }
            assert!(std::process::Command::new("git")
                .args(&args)
                .current_dir(&tree_path)
                .status()
                .unwrap()
                .success());
        }
        let db_path = std::fs::canonicalize(fixture._root.path())
            .unwrap()
            .join("state.sqlite");
        let repo = dcc_infra::db::SqliteWorkspaceRepo::open(&db_path).unwrap();
        repo.save_workspace(&Workspace {
            id: WorkspaceId("ws-files".to_string()),
            project_id: ProjectId("project".to_string()),
            name: None,
            root_path: tree_path.to_string_lossy().into_owned(),
            base_branch: "main".to_string(),
            worktree_path: None,
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            snoozed_until: None,
            created_at: "2026-10-10T00:00:00Z".to_string(),
            updated_at: "2026-10-10T00:00:00Z".to_string(),
        })
        .await
        .unwrap();

        let (status, body) = get_json(
            &fixture.app,
            "/api/v1/mobile/workspaces/ws-files/files?q=BUTTON",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let paths: Vec<&str> = body["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["src/ui/Button.tsx", "src/ui/button.css"]);
        let (_, body) = get_json(
            &fixture.app,
            "/api/v1/mobile/workspaces/ws-files/files?q=untracked",
        )
        .await;
        assert!(
            body["files"].as_array().unwrap().is_empty(),
            "only tracked files: {body}"
        );
        let (_, body) = get_json(
            &fixture.app,
            "/api/v1/mobile/workspaces/ws-files/files?limit=2",
        )
        .await;
        assert_eq!(body["files"].as_array().unwrap().len(), 2);
        assert_eq!(body["total"], 4);

        for uri in [
            "/api/v1/mobile/workspaces/missing/files?q=a",
            "/api/v1/mobile/workspaces/..%2F..%2Fetc/files?q=passwd",
            "/api/v1/mobile/workspaces/%2Fetc/files",
            "/api/v1/mobile/repositories/missing/files",
        ] {
            let (status, _) = get_json(&fixture.app, uri).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        }
        let (status, _) = get_json(
            &fixture.app,
            "/api/v1/mobile/workspaces/ws-files/files?q=a%00b",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// A request as the relay connector runs it for a paired phone (or with
    /// no credentials at all when `device` is `None` and `api_key` false).
    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
        device: Option<&str>,
        api_key: bool,
    ) -> (StatusCode, Value) {
        use tower::util::ServiceExt;
        let mut builder = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if api_key {
            builder = builder.header("X-API-Key", "k");
        }
        let mut request = builder
            .body(axum::body::Body::from(
                body.map(|body| body.to_string()).unwrap_or_default(),
            ))
            .unwrap();
        if let Some(device) = device {
            request
                .extensions_mut()
                .insert(PairedDeviceIdentity(device.to_string()));
        }
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    fn agent_request(id: &str, detail: crate::agent_requests::AgentRequestDetail) -> crate::agent_requests::PendingAgentRequest {
        crate::agent_requests::PendingAgentRequest {
            request_id: id.to_string(),
            session_id: "http-respond".to_string(),
            workspace_id: "workspace".to_string(),
            provider_id: "claude".to_string(),
            reason: "Abrir a documentação".to_string(),
            expires_at_ms: crate::agent_requests::now_ms() + 45_000,
            detail,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_phone_answers_a_browser_request_once_and_the_desktop_gets_409() {
        use crate::agent_requests::{AgentRequestDecision, AgentRequestDetail, AnswerOrigin};
        let fixture = embedded_fixture().await;
        let queue = crate::agent_requests::InMemoryAgentRequests::new();
        fixture.backend.attach_agent_requests(queue.clone());
        let waiting = queue.enqueue(agent_request(
            "bo-1",
            AgentRequestDetail::BrowserOpen { url: "https://docs.example.com".into() },
        ));

        let (status, listed) =
            call(&fixture.app, "GET", "/api/v1/mobile/agent-requests", None, Some("phone-1"), false).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed["requests"][0]["kind"], "browserOpen");
        assert_eq!(listed["requests"][0]["url"], "https://docs.example.com");

        let (status, answered) = call(
            &fixture.app,
            "POST",
            "/api/v1/mobile/agent-requests/bo-1/respond",
            Some(json!({ "allow": true })),
            Some("phone-1"),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        assert_eq!(answered["outcome"]["by"], "phone");
        let outcome = waiting.await.expect("the agent hears the answer");
        assert_eq!(outcome.decision, AgentRequestDecision::Allowed);
        assert_eq!(outcome.by, AnswerOrigin::Phone);

        // The desktop (API key) answering second learns who won.
        let (status, conflict) = call(
            &fixture.app,
            "POST",
            "/api/v1/mobile/agent-requests/bo-1/respond",
            Some(json!({ "allow": false })),
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(conflict["error"]["code"], "already_resolved");
        assert_eq!(conflict["error"]["outcome"]["decision"], "allowed");
        assert_eq!(conflict["error"]["outcome"]["by"], "phone");
        assert_eq!(conflict["error"]["message"], "Esse pedido já foi aprovado pelo celular.");

        let (status, _) = call(
            &fixture.app,
            "POST",
            "/api/v1/mobile/agent-requests/bo-unknown/respond",
            Some(json!({ "allow": true })),
            Some("phone-1"),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_request_routes_need_a_paired_phone_or_the_api_key() {
        let fixture = embedded_fixture().await;
        let queue = crate::agent_requests::InMemoryAgentRequests::new();
        fixture.backend.attach_agent_requests(queue.clone());
        let _waiting = queue.enqueue(agent_request(
            "bo-2",
            crate::agent_requests::AgentRequestDetail::BrowserOpen { url: "https://a.example".into() },
        ));
        let (status, _) =
            call(&fixture.app, "GET", "/api/v1/mobile/agent-requests", None, None, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = call(
            &fixture.app,
            "POST",
            "/api/v1/mobile/agent-requests/bo-2/respond",
            Some(json!({ "allow": true })),
            None,
            false,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        use crate::agent_requests::AgentRequestSource;
        assert_eq!(queue.pending().len(), 1, "nothing answered without credentials");
        // Unknown fields (a path, say) are refused, not ignored.
        let (status, _) = call(
            &fixture.app,
            "POST",
            "/api/v1/mobile/agent-requests/bo-2/respond",
            Some(json!({ "allow": true, "path": "/etc" })),
            Some("phone-1"),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        // A phone can never fill the fixture queue.
        fixture.backend.attach_fixture_agent_requests();
        let (status, _) = call(
            &fixture.app,
            "POST",
            "/api/v1/remote/fixture/agent-requests",
            Some(serde_json::to_value(agent_request(
                "bo-3",
                crate::agent_requests::AgentRequestDetail::BrowserOpen { url: "https://a.example".into() },
            ))
            .unwrap()),
            Some("phone-1"),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn computer_use_from_the_phone_needs_apps_from_the_list() {
        use crate::agent_requests::{AgentRequestApp, AgentRequestDetail};
        let fixture = embedded_fixture().await;
        let queue = crate::agent_requests::InMemoryAgentRequests::new();
        fixture.backend.attach_agent_requests(queue.clone());
        let waiting = queue.enqueue(agent_request(
            "cu-1",
            AgentRequestDetail::ComputerControl {
                apps: vec![AgentRequestApp { bundle_id: "com.apple.Safari".into(), name: "Safari".into() }],
                blocker: None,
            },
        ));
        let respond = |body: Value| call(&fixture.app, "POST", "/api/v1/mobile/agent-requests/cu-1/respond", Some(body), Some("phone-1"), false);
        let (status, error) = respond(json!({ "allow": true })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["message"], "Escolha pelo menos um app que o agente pode controlar.");
        let (status, _) = respond(json!({ "allow": true, "allowedBundleIds": ["com.apple.Terminal"] })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = respond(json!({ "allow": true, "allowedBundleIds": ["com.apple.Safari"] })).await;
        assert_eq!(status, StatusCode::OK);
        assert!(waiting.await.is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_agent_requests_reach_the_stream_and_the_push_channel() {
        let fixture = embedded_fixture().await;
        let mut attention = fixture.backend.attention.subscribe();
        let mut body = open_stream(&fixture.app, "/api/v1/events/stream?sessionId=other").await;
        let queue = crate::agent_requests::InMemoryAgentRequests::new();
        fixture.backend.attach_agent_requests(queue.clone());
        let _waiting = queue.enqueue(agent_request(
            "bo-9",
            crate::agent_requests::AgentRequestDetail::BrowserOpen { url: "https://b.example".into() },
        ));
        let seen = read_until(&mut body, "bo-9").await;
        assert!(seen.contains("event: agent-requests"), "{seen}");
        let pushed = tokio::time::timeout(Duration::from_secs(3), attention.recv())
            .await
            .expect("push in time")
            .expect("attention");
        assert_eq!(pushed.kind, crate::remote_push::AttentionKind::AgentRequest);
        assert_eq!(pushed.session_id, "http-respond");
        queue.cancel("bo-9");
        let seen = read_until(&mut body, "\"requests\":[]").await;
        assert!(seen.contains("agent-requests"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_phone_reads_why_each_task_needs_the_person() {
        use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, TurnId};
        use dcc_core::domain::thread::{Thread, ThreadId};
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        use dcc_core::ports::ThreadRepo;
        let fixture = embedded_fixture().await;
        let db_path = fixture.backend.workspace.db_path.clone();
        let workspaces = dcc_infra::db::SqliteWorkspaceRepo::open(&db_path).expect("workspace repo");
        for (id, state) in [("workspace", WorkspaceState::Ready), ("archived", WorkspaceState::Archived)] {
            dcc_core::ports::WorkspaceRepo::save_workspace(
                &workspaces,
                &Workspace {
                    id: WorkspaceId(id.to_string()),
                    project_id: ProjectId("project".to_string()),
                    name: Some(id.to_string()),
                    // Not a folder: no git blockers, only the session rule.
                    root_path: format!("/nonexistent/{id}"),
                    base_branch: "main".to_string(),
                    worktree_path: None,
                    source: None,
                    state,
                    setup_report: None,
                    pinned_at: None,
                    snoozed_until: None,
                    created_at: "2026-10-08T00:00:00Z".to_string(),
                    updated_at: "2026-10-08T00:00:00Z".to_string(),
                },
            )
            .await
            .expect("save workspace");
        }
        ThreadRepo::save_thread(
            &fixture.state,
            &Thread {
                id: ThreadId("thread".to_string()),
                project_id: ProjectId("project".to_string()),
                session_id: Some(fixture.session_id.clone()),
                title: "Tarefa".to_string(),
                archived_at: None,
            },
        )
        .await
        .expect("thread");
        for kind in [
            SessionEventKind::TurnStarted {
                turn_id: TurnId("t1".to_string()),
                prompt: "oi".to_string(),
                plan_mode: None,
                model: None,
                evidence: None,
                retry_of_turn_id: None,
                approval_policy: None,
            },
            SessionEventKind::TurnPermissionRequested {
                turn_id: TurnId("t1".to_string()),
                request_id: "p1".to_string(),
                tool_name: "Bash".to_string(),
                title: None,
                description: None,
                command: Some("yarn test".to_string()),
                file: None,
            },
        ] {
            SessionEventRepo::append_event(
                &fixture.state,
                &SessionEventRecord {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    session_id: fixture.session_id.clone(),
                    sequence: 0,
                    occurred_at: "2026-10-12T10:00:00Z".to_string(),
                    kind,
                },
            )
            .await
            .expect("append");
        }
        let (status, body) =
            call(&fixture.app, "GET", "/api/v1/mobile/attention", None, Some("phone-1"), false).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let items = body["items"].as_array().expect("items");
        assert_eq!(items.len(), 1, "the archived task is left out: {body}");
        assert_eq!(items[0]["workspaceId"], "workspace");
        assert_eq!(items[0]["reason"], "permission");
        assert_eq!(items[0]["sessionId"], fixture.session_id.0);
        assert_eq!(items[0]["returnedAt"], "2026-10-12T10:00:00Z");
        let (status, _) =
            call(&fixture.app, "GET", "/api/v1/mobile/attention", None, None, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn conflicts_and_red_checks_notify_once_per_appearance() {
        use crate::remote_push::AttentionKind;
        use dcc_tauri::commands::workspace_attention::{WorkspaceBlocker, WorkspaceBlockerKind};
        let blocker = |workspace: &str, kind| WorkspaceBlocker {
            workspace_id: workspace.to_string(),
            kind,
            count: None,
            since: None,
        };
        let mut news = attention::BlockerNews::default();
        // Already there when the watcher starts: old news.
        assert!(news.next(&[blocker("a", WorkspaceBlockerKind::Conflicts)]).is_empty());
        let fresh = news.next(&[
            blocker("a", WorkspaceBlockerKind::Conflicts),
            blocker("b", WorkspaceBlockerKind::ChecksFailing),
            blocker("c", WorkspaceBlockerKind::DelegatedEditsReview),
            blocker("d", WorkspaceBlockerKind::PrConflicts),
        ]);
        let mut kinds: Vec<_> = fresh.iter().map(|(w, k, _)| (w.as_str(), *k)).collect();
        kinds.sort_by_key(|(w, _)| *w);
        assert_eq!(kinds, [("b", AttentionKind::ChecksFailing), ("d", AttentionKind::Conflict)]);
        // Still red: nothing new.
        assert!(news
            .next(&[blocker("b", WorkspaceBlockerKind::ChecksFailing)])
            .is_empty());
        // Green, then red again: a new key, so it notifies again.
        news.next(&[]);
        let again = news.next(&[blocker("b", WorkspaceBlockerKind::ChecksFailing)]);
        assert_eq!(again.len(), 1);
        assert_ne!(again[0].2, fresh.iter().find(|(w, _, _)| w == "b").unwrap().2);
    }

    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "DCC Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "DCC Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_phone_commits_pushes_updates_and_archives_a_task() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("git dirs");
        let base = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        let remote = base.join("remote.git");
        let repo = base.join("repo");
        let other = base.join("other");
        std::fs::create_dir_all(&repo).unwrap();
        git(&base, &["init", "--bare", "-b", "main", remote.to_str().unwrap()]);
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.name", "DCC Test"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        std::fs::write(repo.join("README.md"), "oi\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-m", "init"]);
        git(&repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&repo, &["push", "-u", "origin", "main"]);
        git(&repo, &["switch", "-c", "dcc/tarefa"]);
        git(&repo, &["push", "-u", "origin", "dcc/tarefa"]);
        let workspaces = dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-git".to_string()),
                project_id: ProjectId("project".to_string()),
                name: Some("Ajustar o guia".to_string()),
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-12T00:00:00Z".to_string(),
                updated_at: "2026-10-12T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/guia.md"), "um\ndois\n").unwrap();

        let phone = |method: &'static str, uri: String, body: Option<Value>| {
            let app = fixture.app.clone();
            async move { call(&app, method, &uri, body, Some("phone-1"), false).await }
        };
        let (status, summary) = phone("GET", "/api/v1/mobile/workspaces/w-git/delivery".into(), None).await;
        assert_eq!(status, StatusCode::OK, "{summary}");
        assert_eq!(summary["branch"], "dcc/tarefa");
        assert_eq!(summary["changes"][0]["path"], "docs/guia.md");
        let message = summary["commitMessage"].as_str().unwrap().to_string();
        assert!(!message.is_empty());
        let token = summary["changesToken"].as_str().unwrap().to_string();

        // Without auth, nothing; a path from the phone is refused.
        let (status, _) = call(&fixture.app, "GET", "/api/v1/mobile/workspaces/w-git/delivery", None, None, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = phone(
            "POST",
            "/api/v1/mobile/workspaces/w-git/delivery/commit".into(),
            Some(json!({ "changesToken": token, "workspaceRoot": "/etc" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = phone("GET", "/api/v1/mobile/workspaces/..%2F..%2Fetc/delivery".into(), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // The files moved after the phone looked: refused.
        std::fs::write(repo.join("docs/guia.md"), "um\ndois\ntrês\n").unwrap();
        let (status, stale) = phone(
            "POST",
            "/api/v1/mobile/workspaces/w-git/delivery/commit".into(),
            Some(json!({ "changesToken": token })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(stale["error"]["code"], "stale");

        let (_, summary) = phone("GET", "/api/v1/mobile/workspaces/w-git/delivery".into(), None).await;
        let token = summary["changesToken"].as_str().unwrap().to_string();
        let (status, body) = phone(
            "POST",
            "/api/v1/mobile/workspaces/w-git/delivery/commit".into(),
            Some(json!({ "changesToken": token, "message": "docs: ajustar o guia", "push": true })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(git(&repo, &["log", "-1", "--format=%s"]), "docs: ajustar o guia");
        assert_eq!(
            git(&remote, &["rev-parse", "dcc/tarefa"]),
            git(&repo, &["rev-parse", "HEAD"]),
            "pushed"
        );

        // Someone else pushed to the branch: the phone's push is refused, then
        // "Atualizar e enviar" brings it in and pushes.
        git(&base, &["clone", "-b", "dcc/tarefa", remote.to_str().unwrap(), other.to_str().unwrap()]);
        std::fs::write(other.join("CHANGELOG.md"), "outro\n").unwrap();
        git(&other, &["add", "-A"]);
        git(&other, &["-c", "user.name=O", "-c", "user.email=o@example.com", "commit", "-m", "outro"]);
        git(&other, &["push"]);
        std::fs::write(repo.join("docs/guia.md"), "um\ndois\ntrês\nquatro\n").unwrap();
        let (_, summary) = phone("GET", "/api/v1/mobile/workspaces/w-git/delivery".into(), None).await;
        let token = summary["changesToken"].as_str().unwrap().to_string();
        let (status, behind) = phone(
            "POST",
            "/api/v1/mobile/workspaces/w-git/delivery/commit".into(),
            Some(json!({ "changesToken": token, "push": true })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{behind}");
        assert_eq!(behind["error"]["code"], "behind_remote");
        assert_eq!(behind["error"]["outcome"]["target"], "origin/dcc/tarefa");
        let (_, summary) = phone("GET", "/api/v1/mobile/workspaces/w-git/delivery".into(), None).await;
        assert_eq!(summary["pushFailure"]["canSync"], true, "{summary}");
        assert_eq!(summary["aheadOfRemoteCount"], 1);
        let (status, body) =
            phone("POST", "/api/v1/mobile/workspaces/w-git/delivery/sync-and-push".into(), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            git(&remote, &["rev-parse", "dcc/tarefa"]),
            git(&repo, &["rev-parse", "HEAD"]),
        );
        assert!(repo.join("CHANGELOG.md").exists(), "the other commit came in");

        let (status, _) = phone("POST", "/api/v1/mobile/workspaces/w-git/delivery/archive".into(), None).await;
        assert_eq!(status, StatusCode::OK);
        let archived = dcc_core::ports::WorkspaceRepo::get_workspace(&workspaces, &WorkspaceId("w-git".into()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(archived.state, WorkspaceState::Archived);
        let (status, _) = phone("POST", "/api/v1/mobile/workspaces/w-git/delivery/reset-hard".into(), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
