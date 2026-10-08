use async_trait::async_trait;
use axum::{
    extract::{FromRef, Path, Query, State},
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
    ports::{CoreEvent, EventBus},
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
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

mod mobile;

pub fn recover_mobile_tasks(db_path: &FsPath) -> Result<(), String> {
    mobile::recover_interrupted_tasks(db_path)
}

use crate::daemon_client::rpc_with_db_path_timeout;
use crate::http_auth::auth_middleware;
use crate::http_config::{HttpAuthMode, HttpConfig};
use crate::http_rpc_handler::handle_rpc as handle_json_rpc;
use crate::pairing::{
    self, PairedDeviceSummary, PairingChallenge, PairingError, SignedRequestHeaders,
};

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

/// Feeds `/api/v1/events/stream`. Publishing only hands the event to a
/// broadcast channel: a slow phone never holds up the desktop UI's events.
#[derive(Clone)]
struct HttpEventBus {
    sender: tokio::sync::broadcast::Sender<CoreEvent>,
}

#[async_trait]
impl EventBus for HttpEventBus {
    async fn publish(&self, event: CoreEvent) -> dcc_core::Result<()> {
        let _ = self.sender.send(event);
        Ok(())
    }
}

/// The sessions the HTTP API acts on. Inside the desktop app it is the app's
/// own `SessionCommandState`, so the phone sees and answers the sessions the
/// desktop runs; `dccd-http` builds a headless one of its own.
pub struct HttpBackend {
    sessions: SessionCommandState,
    events: tokio::sync::broadcast::Sender<CoreEvent>,
    // The runtime keeps only a weak reference to its buses.
    _bus: Arc<dyn EventBus>,
    terminals: bool,
}

impl HttpBackend {
    /// Serves sessions the desktop app owns. Remote shells are left out: the
    /// phone has no use for them yet and they are the riskiest route.
    pub fn embedded(sessions: SessionCommandState) -> Result<Self, String> {
        let (events, _) = tokio::sync::broadcast::channel(256);
        let bus: Arc<dyn EventBus> = Arc::new(HttpEventBus {
            sender: events.clone(),
        });
        sessions
            .register_event_bus(&bus)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            sessions,
            events,
            _bus: bus,
            terminals: false,
        })
    }

    /// The standalone `dccd-http` server, owner of the sessions it starts.
    pub fn headless(db_path: PathBuf, app_data_dir: PathBuf) -> Self {
        let (events, _) = tokio::sync::broadcast::channel(256);
        let bus: Arc<dyn EventBus> = Arc::new(HttpEventBus {
            sender: events.clone(),
        });
        let sessions = SessionCommandState::new_with_event_bus(db_path, app_data_dir, bus.clone());
        Self {
            sessions,
            events,
            _bus: bus,
            terminals: true,
        }
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
    NotFound(String),
    /// The request was already answered; carries the answer that won.
    Conflict(String, Value),
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
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(..) => StatusCode::CONFLICT,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::GatewayTimeout(_) => StatusCode::GATEWAY_TIMEOUT,
            Self::BadGateway(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::NotFound(_) => "not_found",
            Self::Conflict(..) => "already_resolved",
            Self::ServiceUnavailable(_) => "service_unavailable",
            Self::GatewayTimeout(_) => "gateway_timeout",
            Self::BadGateway(_) => "bad_gateway",
            Self::Internal(_) => "internal_error",
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::NotFound(message)
            | Self::Conflict(message, _)
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
        if let Self::Conflict(_, outcome) = &self {
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
        .route("/auth/pair-init", post(pair_init_handler))
        .route("/auth/devices", get(list_paired_devices_handler))
        .route(
            "/auth/devices/:device_id",
            axum::routing::delete(revoke_device_handler),
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

    let mobile_spa = mobile_spa_service();

    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .route("/openapi.json", get(openapi_handler))
        .route("/auth/pair", post(complete_pairing_handler))
        .nest_service("/m", mobile_spa)
        .merge(protected_routes)
        .layer(build_cors_layer(&cors_config))
        .layer(TraceLayer::new_for_http())
        .with_state(HttpState { config, backend })
}

/// Serves the Vite-built mobile SPA from `apps/mobile-web/dist/` with an
/// `index.html` fallback so client-side routes (TanStack Router) resolve when
/// reloaded directly from the phone. Resolution order:
///   1. `DCC_MOBILE_WEB_DIST` env var
///   2. `<repo>/apps/mobile-web/dist` (dev: works when running from source)
///   3. `<exe_dir>/mobile-web` (release layouts that bundle next to the binary)
///   4. `<exe_dir>/resources/mobile-web` (Linux package layouts)
///   5. `<app>/Contents/Resources/mobile-web` (macOS Tauri app bundle)
fn mobile_spa_dist_candidates() -> Vec<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from));

    [
        std::env::var("DCC_MOBILE_WEB_DIST").ok().map(PathBuf::from),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../apps/mobile-web/dist")),
        exe_dir.as_ref().map(|p| p.join("mobile-web")),
        exe_dir.as_ref().map(|p| p.join("resources/mobile-web")),
        exe_dir.as_ref().and_then(|p| {
            p.parent()
                .map(|contents| contents.join("Resources/mobile-web"))
        }),
        exe_dir
            .as_ref()
            .and_then(|p| p.parent().map(|parent| parent.join("resources/mobile-web"))),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn mobile_spa_dist_path() -> PathBuf {
    let candidates = mobile_spa_dist_candidates();

    candidates
        .iter()
        .find(|p| p.join("index.html").is_file())
        .cloned()
        .unwrap_or_else(|| {
            candidates
                .into_iter()
                .next()
                .unwrap_or_else(|| PathBuf::from("."))
        })
}

fn mobile_spa_diagnostics() -> Value {
    let dist = mobile_spa_dist_path();
    json!({
        "available": dist.join("index.html").is_file(),
        "distPath": dist.to_string_lossy(),
    })
}

fn mobile_spa_service() -> ServeDir<ServeFile> {
    let dist = mobile_spa_dist_path();

    let fallback = ServeFile::new(dist.join("index.html"));
    ServeDir::new(dist).fallback(fallback)
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
            "branch": base_branch,
            "worktreePath": row.get::<_, Option<String>>(4).map_err(|e| e.to_string())?,
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
    let (database, network_exposed) = {
        let config = config.read().await;
        (
            config.db_path.to_string_lossy().to_string(),
            config.is_network_exposed(),
        )
    };

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
                    // The desktop only reuses a running server for pairing when
                    // it is reachable from the LAN.
                    "lanListener": network_exposed,
                },
                "mobileWeb": mobile_spa_diagnostics(),
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
                    "lanListener": network_exposed,
                },
                "mobileWeb": mobile_spa_diagnostics(),
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

async fn events_stream_handler(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    HttpApiError,
> {
    let receiver = backend.events.subscribe();
    let stream = BroadcastStream::new(receiver).filter_map(|message| match message {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(payload) => Some(Ok(Event::default().event("core-event").data(payload))),
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
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<SendTurnInput>,
) -> Result<Json<Value>, HttpApiError> {
    input.session_id = SessionId(session_id);
    let output = send_turn_with_state(&backend.sessions, input)
        .await
        .map_err(classify_session_error)?;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn list_session_events_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = session_repo_operation(config, move |repo| {
        let items = repo
            .list_events_by_session_sync(&SessionId(session_id))
            .map_err(|error| error.to_string())?;
        serde_json::to_value(items).map_err(|error| error.to_string())
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

// =============================================================================
// Mobile pairing handlers (ECDSA P-256 signed-request auth)
// =============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletePairingRequest {
    nonce: String,
    pin: String,
    /// SPKI-encoded ECDSA P-256 pubkey for native clients that can sign each
    /// request. Omit to fall back to bearer-token auth — required for browser
    /// clients reaching the backend over `http://<lan-ip>:...` where
    /// `crypto.subtle` is unavailable.
    #[serde(default)]
    public_key_spki: Option<String>,
    device_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletePairingResponse {
    ok: bool,
    device_id: String,
    /// Issued exactly once when `public_key_spki` was not provided. The client
    /// stores it and sends `Authorization: Bearer <token>` on every subsequent
    /// request. Only the SHA-256 of this value is persisted server-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PairInitResponse {
    ok: bool,
    nonce: String,
    pin: String,
    expires_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListDevicesResponse {
    ok: bool,
    devices: Vec<PairedDeviceJson>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PairedDeviceJson {
    device_id: String,
    device_name: String,
    user_agent: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
    last_ip: Option<String>,
    revoked: bool,
}

impl From<PairedDeviceSummary> for PairedDeviceJson {
    fn from(summary: PairedDeviceSummary) -> Self {
        Self {
            device_id: summary.device_id,
            device_name: summary.device_name,
            user_agent: summary.user_agent,
            created_at: summary.created_at,
            last_used_at: summary.last_used_at,
            last_ip: summary.last_ip,
            revoked: summary.revoked,
        }
    }
}

fn pairing_error_to_http(err: PairingError) -> HttpApiError {
    use PairingError as P;
    match err {
        P::InvalidNonce | P::InvalidPin => HttpApiError::bad_request("invalid pairing credentials"),
        P::Expired => HttpApiError::bad_request("pairing expired"),
        P::AlreadyConsumed => HttpApiError::bad_request("pairing already used"),
        P::NonceLocked => HttpApiError::bad_request("too many invalid PIN attempts"),
        P::DeviceLimitReached => {
            HttpApiError::bad_request("device limit reached; revoke an existing device first")
        }
        P::SessionExpired => HttpApiError::bad_request("device session expired"),
        P::InvalidPublicKey => HttpApiError::bad_request("invalid public key"),
        P::InvalidSignature => HttpApiError::bad_request("invalid signature"),
        P::SignatureMismatch => HttpApiError::bad_request("signature verification failed"),
        P::UnknownDevice => HttpApiError::not_found("device not found"),
        P::DeviceRevoked => HttpApiError::bad_request("device revoked"),
        P::TimestampOutOfWindow => HttpApiError::bad_request("timestamp outside replay window"),
        P::InvalidTimestamp => HttpApiError::bad_request("invalid timestamp"),
        P::Database(message) => HttpApiError::internal(message),
    }
}

async fn with_pairing_db<T, F>(config: Arc<RwLock<HttpConfig>>, op: F) -> Result<T, HttpApiError>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T, PairingError> + Send + 'static,
{
    let db_path = {
        let config = config.read().await;
        config.db_path.clone()
    };

    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| PairingError::Database(e.to_string()))?;
        op(&conn)
    })
    .await
    .map_err(|e| HttpApiError::internal(format!("pairing worker join failed: {e}")))?
    .map_err(pairing_error_to_http)
}

async fn pair_init_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
) -> Result<Json<PairInitResponse>, HttpApiError> {
    let challenge: PairingChallenge =
        with_pairing_db(config, |conn| pairing::create_pairing_nonce(conn)).await?;

    Ok(Json(PairInitResponse {
        ok: true,
        nonce: challenge.nonce,
        pin: challenge.pin,
        expires_at: challenge.expires_at,
    }))
}

// --- per-IP rate limit on /auth/pair (anti brute force) ---

const PAIR_RATE_WINDOW_SECS: u64 = 60;
const PAIR_RATE_MAX_PER_WINDOW: usize = 20;

static PAIR_RATE_LIMITER: OnceLock<Mutex<HashMap<String, VecDeque<std::time::Instant>>>> =
    OnceLock::new();

fn pair_rate_limit_check(client_ip: &str) -> bool {
    let store = PAIR_RATE_LIMITER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = match store.lock() {
        Ok(g) => g,
        Err(_) => return true, // never reject because a lock got poisoned
    };
    let now = std::time::Instant::now();
    let window = Duration::from_secs(PAIR_RATE_WINDOW_SECS);
    let entry = guard.entry(client_ip.to_string()).or_default();
    while let Some(front) = entry.front() {
        if now.duration_since(*front) > window {
            entry.pop_front();
        } else {
            break;
        }
    }
    if entry.len() >= PAIR_RATE_MAX_PER_WINDOW {
        return false;
    }
    entry.push_back(now);
    // opportunistic shrink to keep the map small
    if guard.len() > 1024 {
        guard.retain(|_, q| !q.is_empty());
    }
    true
}

async fn complete_pairing_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    headers: axum::http::HeaderMap,
    extensions: axum::http::Extensions,
    Json(body): Json<CompletePairingRequest>,
) -> Result<Json<CompletePairingResponse>, HttpApiError> {
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let client_ip = crate::http_auth::client_ip(&headers, &extensions);

    let rate_key = client_ip.clone().unwrap_or_else(|| "unknown".to_string());
    if !pair_rate_limit_check(&rate_key) {
        return Err(HttpApiError::bad_request(
            "rate limit exceeded for /auth/pair",
        ));
    }

    let (device_id, session_token) = if let Some(pubkey) = body.public_key_spki.clone() {
        let device_id = with_pairing_db(config, move |conn| {
            pairing::complete_pairing(
                conn,
                &body.nonce,
                &body.pin,
                &pubkey,
                &body.device_name,
                user_agent.as_deref(),
                client_ip.as_deref(),
            )
        })
        .await?;
        (device_id, None)
    } else {
        let (device_id, token) = with_pairing_db(config, move |conn| {
            pairing::complete_pairing_bearer(
                conn,
                &body.nonce,
                &body.pin,
                &body.device_name,
                user_agent.as_deref(),
                client_ip.as_deref(),
            )
        })
        .await?;
        (device_id, Some(token))
    };

    Ok(Json(CompletePairingResponse {
        ok: true,
        device_id,
        session_token,
    }))
}

async fn list_paired_devices_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<ListDevicesResponse>, HttpApiError> {
    let include_revoked = query
        .get("includeRevoked")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let devices = with_pairing_db(config, move |conn| {
        pairing::list_paired_devices(conn, include_revoked)
    })
    .await?;

    Ok(Json(ListDevicesResponse {
        ok: true,
        devices: devices.into_iter().map(PairedDeviceJson::from).collect(),
    }))
}

async fn revoke_device_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    headers: axum::http::HeaderMap,
    extensions: axum::http::Extensions,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let ip = crate::http_auth::client_ip(&headers, &extensions);

    let revoked = with_pairing_db(config, move |conn| {
        pairing::revoke_device(conn, &device_id, ip.as_deref())
    })
    .await?;

    if !revoked {
        return Err(HttpApiError::not_found(
            "device not found or already revoked",
        ));
    }

    Ok(Json(json!({ "ok": true })))
}

/// Validates a signed mobile request. To be wired in as a tower middleware in a follow-up step.
#[allow(dead_code)]
async fn verify_signed_request_for_path(
    config: Arc<RwLock<HttpConfig>>,
    method: String,
    path: String,
    body: Vec<u8>,
    device_id: String,
    timestamp: String,
    signature_b64: String,
    client_ip: Option<String>,
) -> Result<PairedDeviceSummary, HttpApiError> {
    with_pairing_db(config, move |conn| {
        pairing::verify_signed_request(
            conn,
            SignedRequestHeaders {
                device_id: &device_id,
                timestamp: &timestamp,
                signature_b64: &signature_b64,
            },
            &method,
            &path,
            &body,
            client_ip.as_deref(),
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::{
        domain::{
            project::ProjectId,
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
    async fn pair_rate_limit_keys_on_the_socket_peer_not_forwarded_for() {
        use axum::extract::ConnectInfo;
        use tower::util::ServiceExt;

        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        let db_path = root_path.join("state.sqlite");
        rusqlite::Connection::open(&db_path)
            .unwrap()
            .execute_batch(include_str!("../sql/schema.sql"))
            .unwrap();
        let config = Arc::new(RwLock::new(HttpConfig {
            db_path: db_path.clone(),
            api_key: "k".to_string(),
            ..HttpConfig::default()
        }));
        let backend = Arc::new(HttpBackend::headless(db_path, root_path.join("app-data")));
        let app = build_router(config, backend);
        // Unique to this test: the limiter is process-wide.
        let peer: std::net::SocketAddr = "203.0.113.77:40000".parse().unwrap();

        let mut last_body = String::new();
        for attempt in 0..=PAIR_RATE_MAX_PER_WINDOW {
            let mut request = axum::http::Request::builder()
                .method("POST")
                .uri("/auth/pair")
                .header("content-type", "application/json")
                // A fresh spoofed address each time must not buy a fresh bucket.
                .header("x-forwarded-for", format!("10.0.0.{attempt}"))
                .body(axum::body::Body::from(
                    r#"{"nonce":"nope","pin":"000000","deviceName":"x"}"#,
                ))
                .unwrap();
            request.extensions_mut().insert(ConnectInfo(peer));
            let response = app.clone().oneshot(request).await.unwrap();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            last_body = String::from_utf8_lossy(&bytes).to_string();
        }

        assert!(last_body.contains("rate limit"), "{last_body}");
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
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
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
            Ok(CoreEvent::SessionResumed { .. })
        ));
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
}
