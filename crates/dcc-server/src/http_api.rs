use async_trait::async_trait;
use axum::{
    extract::{DefaultBodyLimit, FromRef, Path, Query, State},
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, Method, StatusCode},
    middleware,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post, put},
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

mod agent_conversation;
mod agent_memory;
mod agents;
mod ai_memory;
mod ai_memory_settings;
mod attention;
mod automation;
mod browser_bridge;
mod browser_opens;
mod computer_bridge;
mod decision_provider;
mod delegation_review;
mod delivery;
mod desktop_app;
mod files;
mod forge;
mod fetch_base;
mod git_changes;
mod git_conflicts;
mod keep_awake;
mod local_branches;
mod mcp;
mod menu_bar;
mod mobile;
mod mobile_agents;
mod mobile_room;
mod provider_accounts;
mod provider_usage;
mod providers;
mod pull_request_hub;
mod remote_devices;
mod shortcuts;
mod skills;
mod task_proposals;
mod terminals;
mod turn_review;

pub use browser_bridge::{
    native_browser_bridge, BridgeOpKind, BridgeReply, BrowserFuture, DesktopBrowser, PageReport,
};
pub use browser_opens::native_browser_opens;
pub use computer_bridge::{native_computer, DesktopComputer};
pub use desktop_app::DesktopApp;
pub use keep_awake::DesktopKeepAwake;
pub use menu_bar::DesktopMenuBar;
pub use shortcuts::native_quick_composer;
pub use shortcuts::DesktopShortcuts;
pub use terminals::{publish_desktop_terminal_event, DesktopTerminalEvent, DesktopTerminals};

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
    /// Inside the desktop app: its own PTYs, for the native app's dock.
    desktop_terminals: OnceLock<Arc<dyn DesktopTerminals>>,
    /// The desktop's Browser state, for the native app's Browser (local only).
    desktop_browser: OnceLock<Arc<dyn DesktopBrowser>>,
    /// The desktop's computer use, for the native app (local only).
    desktop_computer: OnceLock<Arc<dyn DesktopComputer>>,
    /// The desktop's keep-awake hold, for the native app's Settings.
    desktop_keep_awake: OnceLock<Arc<dyn DesktopKeepAwake>>,
    /// The desktop's menu bar snapshot, for the native app's menu bar item.
    desktop_menu_bar: OnceLock<Arc<dyn DesktopMenuBar>>,
    /// The desktop's global shortcuts, for the native app's Settings.
    desktop_shortcuts: OnceLock<Arc<dyn DesktopShortcuts>>,
    /// The relay connector serving this router, for the operator routes and
    /// to drop a revoked phone at once. Weak: the hub holds the router.
    remote: OnceLock<std::sync::Weak<RemoteHub>>,
    /// The app the server runs in: a task's events, the connectors, the
    /// ai-memory sidecar and restarting (`DesktopApp`).
    app: OnceLock<Arc<dyn DesktopApp>>,
}

impl HttpBackend {
    /// Serves sessions the desktop app owns. Free shells are left out; the
    /// desktop's own terminals come with `with_desktop_terminals`, for local
    /// clients only (the relay refuses `/api/v1/terminals/`).
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
            desktop_terminals: OnceLock::new(),
            desktop_browser: OnceLock::new(),
            desktop_computer: OnceLock::new(),
            desktop_keep_awake: OnceLock::new(),
            desktop_menu_bar: OnceLock::new(),
            desktop_shortcuts: OnceLock::new(),
            remote: OnceLock::new(),
            agent_requests: OnceLock::new(),
            fixture_agent_requests: OnceLock::new(),
            app: OnceLock::new(),
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
            desktop_terminals: OnceLock::new(),
            desktop_browser: OnceLock::new(),
            desktop_computer: OnceLock::new(),
            desktop_keep_awake: OnceLock::new(),
            desktop_menu_bar: OnceLock::new(),
            desktop_shortcuts: OnceLock::new(),
            remote: OnceLock::new(),
            agent_requests: OnceLock::new(),
            fixture_agent_requests: OnceLock::new(),
            app: OnceLock::new(),
        }
    }

    /// The desktop app the server runs in (task creation needs it).
    pub fn with_app(self, app: Arc<dyn DesktopApp>) -> Self {
        let _ = self.app.set(app);
        self
    }

    /// The desktop's PTYs, served under `/api/v1/terminals/` to local clients.
    pub fn with_desktop_terminals(self, terminals: Arc<dyn DesktopTerminals>) -> Self {
        let _ = self.desktop_terminals.set(terminals);
        self
    }

    /// The desktop's Browser, driven from the native app's under
    /// `/api/v1/browser/bridge/` (local clients only).
    pub fn with_desktop_browser(self, browser: Arc<dyn DesktopBrowser>) -> Self {
        let _ = self.desktop_browser.set(browser);
        self
    }

    /// The desktop's computer use, for the native app under
    /// `/api/v1/computer/bridge/` (local clients only).
    pub fn with_desktop_computer(self, computer: Arc<dyn DesktopComputer>) -> Self {
        let _ = self.desktop_computer.set(computer);
        self
    }

    /// The desktop's global shortcuts, for `/api/v1/quick-composer/*` and
    /// `/api/v1/appshots/*`.
    pub fn with_desktop_shortcuts(self, shortcuts: Arc<dyn DesktopShortcuts>) -> Self {
        let _ = self.desktop_shortcuts.set(shortcuts);
        self
    }

    /// The desktop's menu bar snapshot, for `/api/v1/menu-bar`.
    pub fn with_desktop_menu_bar(self, menu_bar: Arc<dyn DesktopMenuBar>) -> Self {
        let _ = self.desktop_menu_bar.set(menu_bar);
        self
    }

    /// The desktop's keep-awake controller, for `/api/v1/keep-awake`.
    pub fn with_desktop_keep_awake(self, keep_awake: Arc<dyn DesktopKeepAwake>) -> Self {
        let _ = self.desktop_keep_awake.set(keep_awake);
        self
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
            "/api/v1/mobile/workspaces/:workspace_id/file",
            get(mobile::file),
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
        // The same delivery for the native app's review and conversation headers.
        .route(
            "/api/v1/workspaces/:workspace_id/delivery",
            get(delivery::summary),
        )
        .route(
            "/api/v1/workspaces/:workspace_id/delivery/:action",
            post(delivery::act),
        )
        .route(
            "/api/v1/mobile/agent-requests/:request_id/respond",
            post(attention::respond_agent_request),
        )
        .route(
            "/api/v1/mobile/task-proposals",
            get(attention::list_task_proposals),
        )
        .route("/api/v1/mobile/agents", get(mobile_agents::agents))
        .route("/api/v1/mobile/room", get(mobile_room::room))
        .route("/api/v1/mobile/room/messages", post(mobile_room::send))
        .route(
            "/api/v1/task-proposals/:proposal_id/start",
            post(task_proposals::start),
        )
        .route(
            "/api/v1/task-proposals/:proposal_id/dismiss",
            post(task_proposals::dismiss),
        )
        .route(
            "/api/v1/mobile/task-proposals/:proposal_id/:action",
            post(attention::answer_task_proposal),
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
            "/api/v1/usage-limit/settings",
            get(usage_limit_settings_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/usage-limit/resume",
            post(resume_after_usage_limit_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/usage-limit/auto-resume",
            get(usage_limit_auto_resume_handler).put(set_usage_limit_auto_resume_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/goal/dismiss",
            post(dismiss_goal_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/objective",
            get(get_objective_handler)
                .put(set_objective_handler)
                .delete(clear_objective_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/objective/transition",
            post(transition_objective_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/steer",
            post(steer_turn_handler),
        )
        .route("/api/v1/workspaces/create", post(create_task_handler))
        .route("/api/v1/workspaces", post(create_workspace_handler))
        .route("/api/v1/workspaces/clone", post(clone_workspace_handler))
        .route(
            "/api/v1/workspaces/source-url/resolve",
            post(resolve_source_url_handler),
        )
        .route(
            "/api/v1/workspaces/from-source-url",
            post(create_from_source_url_handler),
        )
        .route(
            "/api/v1/workspaces/local-branches",
            post(local_branches::list),
        )
        .route(
            "/api/v1/workspaces/local-branches/switch",
            post(local_branches::switch),
        )
        .route(
            "/api/v1/workspace-bundles",
            post(create_workspace_bundle_handler),
        )
        .route("/api/v1/files/write", post(files::write))
        .route(
            "/api/v1/keep-awake",
            get(keep_awake::status).put(keep_awake::set_mode),
        )
        .route("/api/v1/menu-bar", get(menu_bar::snapshot))
        .route(
            "/api/v1/decision-provider/settings",
            get(decision_provider::settings).put(decision_provider::save),
        )
        .route(
            "/api/v1/decision-provider/history",
            get(decision_provider::history),
        )
        .route(
            "/api/v1/providers/:provider_id/availability",
            put(providers::set_availability),
        )
        .route(
            "/api/v1/provider-accounts",
            get(provider_accounts::list).post(provider_accounts::create),
        )
        .route(
            "/api/v1/provider-accounts/status",
            post(provider_accounts::status),
        )
        .route(
            "/api/v1/provider-accounts/sign-in",
            post(provider_accounts::sign_in),
        )
        .route(
            "/api/v1/provider-accounts/sign-in/cancel",
            post(provider_accounts::cancel_sign_in),
        )
        .route(
            "/api/v1/provider-accounts/usage",
            post(provider_usage::usage),
        )
        .route(
            "/api/v1/provider-accounts/reset",
            post(provider_usage::reset),
        )
        .route(
            "/api/v1/antigravity/status",
            post(provider_usage::antigravity_status),
        )
        .route(
            "/api/v1/antigravity/install",
            post(provider_usage::install_antigravity),
        )
        .route(
            "/api/v1/antigravity/connect",
            post(provider_usage::connect_antigravity),
        )
        .route(
            "/api/v1/provider-accounts/:account_id",
            put(provider_accounts::rename).delete(provider_accounts::remove),
        )
        .route(
            "/api/v1/quick-composer/status",
            get(shortcuts::quick_composer),
        )
        .route(
            "/api/v1/quick-composer/shortcut",
            put(shortcuts::set_quick_composer),
        )
        .route(
            "/api/v1/quick-composer/toggle",
            post(shortcuts::toggle_quick_composer),
        )
        .route(
            "/api/v1/quick-composer/stream",
            get(shortcuts::quick_composer_stream),
        )
        .route(
            "/api/v1/quick-composer/begin",
            post(shortcuts::begin_quick_launch),
        )
        .route(
            "/api/v1/quick-composer/checkpoint",
            post(shortcuts::checkpoint_quick_launch),
        )
        .route("/api/v1/appshots/status", get(shortcuts::appshots))
        .route("/api/v1/appshots/shortcut", put(shortcuts::set_appshots))
        .route("/api/v1/skills/save", post(skills::save))
        .route("/api/v1/skills/delete", post(skills::delete))
        .route("/api/v1/skills/import", post(skills::import))
        .route("/api/v1/skills/compile", post(skills::compile))
        .route("/api/v1/git/stage-file", post(git_changes::stage_file))
        .route("/api/v1/git/unstage-file", post(git_changes::unstage_file))
        .route("/api/v1/git/discard-file", post(git_changes::discard_file))
        .route("/api/v1/git/stage-all", post(git_changes::stage_all))
        .route("/api/v1/git/fetch-base", post(fetch_base::fetch_base))
        .route("/api/v1/git/conflict-state", post(git_conflicts::conflict_state))
        .route("/api/v1/git/accept-conflict", post(git_conflicts::accept_conflict))
        .route(
            "/api/v1/git/mark-conflict-resolved",
            post(git_conflicts::mark_conflict_resolved),
        )
        .route("/api/v1/git/abort-merge", post(git_conflicts::abort_merge))
        .route("/api/v1/git/validation-config", post(git_conflicts::validation_config))
        .route("/api/v1/git/complete-merge", post(git_conflicts::complete_merge))
        .route("/api/v1/git/delivery-failure", post(forge::delivery_failure))
        .route("/api/v1/git/delivery-recovery", post(forge::delivery_recovery))
        .route("/api/v1/forge/context", post(forge::context))
        .route(
            "/api/v1/forge/change-request-context",
            post(forge::change_request_context),
        )
        .route("/api/v1/mcp/integrations", get(mcp::list).post(mcp::create))
        .route("/api/v1/mcp/integrations/activate", post(mcp::activate))
        .route("/api/v1/mcp/integrations/remove", post(mcp::remove))
        .route("/api/v1/mcp/integrations/disable", post(mcp::disable))
        .route("/api/v1/mcp/runtime", post(mcp::runtime))
        .route("/api/v1/mcp/scopes", post(mcp::set_scopes))
        .route("/api/v1/mcp/connect/start", post(mcp::start_connect))
        .route("/api/v1/mcp/connect/wait", post(mcp::wait_connect))
        .route("/api/v1/mcp/connect/cancel", post(mcp::cancel_connect))
        .route("/api/v1/mcp/disconnect", post(mcp::disconnect))
        .route("/api/v1/mcp/test", post(mcp::test))
        .route("/api/v1/mcp/logo", post(mcp::set_logo))
        .route("/api/v1/mcp/logo/fetch", post(mcp::fetch_logo))
        .route("/api/v1/mcp/project", post(mcp::set_project))
        .route("/api/v1/mcp/tool-policy", post(mcp::set_tool_policy))
        .route(
            "/api/v1/mcp/native-servers",
            get(mcp::native_servers).post(mcp::import_native),
        )
        .route(
            "/api/v1/workspaces/automation",
            post(automation::config).put(automation::save),
        )
        .route("/api/v1/workspaces/automation/run", post(automation::run))
        .route("/api/v1/forge/accounts", post(forge::accounts))
        .route("/api/v1/forge/cli-status", post(forge::cli_status))
        .route("/api/v1/forge/hosts", post(forge::hosts))
        .route("/api/v1/forge/select-login", post(forge::select_login))
        .route(
            "/api/v1/forge/backfill-bindings",
            post(forge::backfill_bindings),
        )
        .route(
            "/api/v1/forge/retry-binding",
            post(forge::retry_binding),
        )
        .route("/api/v1/forge/review-state", post(forge::review_state))
        .route(
            "/api/v1/forge/reviewer-candidates",
            post(forge::reviewer_candidates),
        )
        .route("/api/v1/forge/request-reviewers", post(forge::request_reviewers))
        .route("/api/v1/forge/review-comments", post(forge::review_comments))
        .route("/api/v1/forge/reply-thread", post(forge::reply_thread))
        .route("/api/v1/forge/resolve-thread", post(forge::resolve_thread))
        .route("/api/v1/forge/pipeline", post(forge::pipeline))
        .route("/api/v1/forge/pipeline-job-log", post(forge::pipeline_job_log))
        .route("/api/v1/forge/pipeline-job-retry", post(forge::pipeline_job_retry))
        .route(
            "/api/v1/delegations/:delegation_id/apply",
            post(delegation_review::apply),
        )
        .route(
            "/api/v1/delegations/:delegation_id/discard",
            post(delegation_review::discard),
        )
        .route(
            "/api/v1/delegations/:delegation_id/cancel",
            post(delegation_review::cancel),
        )
        .route(
            "/api/v1/delegations/:delegation_id/approve",
            post(delegation_review::approve),
        )
        .route(
            "/api/v1/sessions/:session_id/ai-memory/export-status",
            get(ai_memory::export_status),
        )
        .route(
            "/api/v1/sessions/:session_id/ai-memory/sources",
            get(ai_memory::sources),
        )
        .route(
            "/api/v1/ai-memory/source-actions",
            get(ai_memory::source_actions).post(ai_memory::save_source_action),
        )
        .route("/api/v1/ai-memory/sidecar", get(ai_memory::sidecar_status))
        .route(
            "/api/v1/ai-memory/settings",
            get(ai_memory_settings::settings).put(ai_memory_settings::save),
        )
        .route("/api/v1/ai-memory/outbox", get(ai_memory_settings::outbox))
        .route(
            "/api/v1/ai-memory/outbox/:session_id/retry",
            post(ai_memory_settings::retry),
        )
        .route(
            "/api/v1/ai-memory/export-history",
            get(ai_memory_settings::history),
        )
        .route("/api/v1/app/restart", post(ai_memory_settings::restart))
        .route(
            "/api/v1/sessions/:session_id/agent",
            post(agents::bind_session),
        )
        .route("/api/v1/agents", post(agents::create))
        .route("/api/v1/agents/:agent_id", post(agents::save))
        .route(
            "/api/v1/agents/:agent_id/limit-override",
            post(agents::let_past_limit),
        )
        .route(
            "/api/v1/agents/:agent_id/conversation",
            post(agent_conversation::home),
        )
        .route(
            "/api/v1/agents/:agent_id/memory",
            get(agent_memory::show).post(agent_memory::add),
        )
        .route(
            "/api/v1/agents/:agent_id/memory/:note_id",
            post(agent_memory::edit).delete(agent_memory::remove),
        )
        .route(
            "/api/v1/agents/:agent_id/memory/versions/:version_id/restore",
            post(agent_memory::restore),
        )
        .route(
            "/api/v1/sessions/:session_id/last-turn-review",
            post(turn_review::last_turn),
        )
        .route("/api/v1/turn-review/file-diff", post(turn_review::file_diff))
        .route("/api/v1/guarded-undo/prepare", post(turn_review::prepare_undo))
        .route("/api/v1/guarded-undo/execute", post(turn_review::execute_undo))
        .route(
            "/api/v1/conversation-rewind/prepare",
            post(turn_review::prepare_rewind),
        )
        .route(
            "/api/v1/conversation-rewind/execute",
            post(turn_review::execute_rewind),
        )
        .route(
            "/api/v1/sessions/:session_id/task-title",
            post(apply_task_title_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/queue",
            post(queue_turn_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/queue/order",
            axum::routing::put(reorder_turn_queue_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/queue/dispatch-next",
            post(dispatch_next_queued_turn_handler),
        )
        .route(
            "/api/v1/sessions/:session_id/queue/:queued_turn_id",
            axum::routing::delete(remove_queued_turn_handler),
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
        )
        .merge(remote_devices::routes())
        .merge(pull_request_hub::routes());
    let protected_routes = if backend.terminals {
        protected_routes.merge(terminal_routes)
    } else {
        protected_routes
            .merge(terminals::routes())
            .merge(browser_bridge::routes())
            .merge(browser_opens::routes())
            .merge(computer_bridge::routes())
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

    if error.starts_with(dcc_core::domain::agent_budget::AGENT_LIMIT_ERROR) {
        HttpApiError::Refused("agentLimitReached", error, json!({}))
    } else if normalized.contains("not found") {
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
            "POST /api/v1/sessions/:session_id/goal/dismiss",
            "GET /api/v1/sessions/:session_id/objective",
            "PUT /api/v1/sessions/:session_id/objective",
            "DELETE /api/v1/sessions/:session_id/objective",
            "POST /api/v1/sessions/:session_id/objective/transition",
            "POST /api/v1/sessions/:session_id/steer",
            "POST /api/v1/workspaces/create",
            "POST /api/v1/workspaces",
            "POST /api/v1/workspaces/clone",
            "POST /api/v1/workspaces/source-url/resolve",
            "POST /api/v1/workspaces/from-source-url",
            "POST /api/v1/workspaces/local-branches",
            "POST /api/v1/workspaces/local-branches/switch",
            "POST /api/v1/workspace-bundles",
            "POST /api/v1/git/stage-file",
            "POST /api/v1/git/unstage-file",
            "POST /api/v1/git/discard-file",
            "POST /api/v1/git/stage-all",
            "POST /api/v1/git/fetch-base",
            "POST /api/v1/git/conflict-state",
            "POST /api/v1/git/accept-conflict",
            "POST /api/v1/git/mark-conflict-resolved",
            "POST /api/v1/git/abort-merge",
            "POST /api/v1/git/validation-config",
            "POST /api/v1/git/complete-merge",
            "POST /api/v1/git/delivery-failure",
            "POST /api/v1/git/delivery-recovery",
            "POST /api/v1/forge/context",
            "POST /api/v1/forge/change-request-context",
            "GET /api/v1/mcp/integrations",
            "POST /api/v1/mcp/integrations",
            "POST /api/v1/mcp/integrations/activate",
            "POST /api/v1/mcp/integrations/remove",
            "POST /api/v1/mcp/integrations/disable",
            "POST /api/v1/mcp/runtime",
            "GET /api/v1/remote/devices",
            "POST /api/v1/remote/devices/revoke",
            "GET /api/v1/remote/audit",
            "POST /api/v1/remote/relay",
            "POST /api/v1/mcp/scopes",
            "POST /api/v1/mcp/connect/start",
            "POST /api/v1/mcp/connect/wait",
            "POST /api/v1/mcp/connect/cancel",
            "POST /api/v1/mcp/disconnect",
            "POST /api/v1/mcp/test",
            "POST /api/v1/mcp/logo",
            "POST /api/v1/mcp/logo/fetch",
            "POST /api/v1/mcp/project",
            "POST /api/v1/mcp/tool-policy",
            "GET /api/v1/mcp/native-servers",
            "POST /api/v1/mcp/native-servers",
            "POST /api/v1/workspaces/automation",
            "PUT /api/v1/workspaces/automation",
            "POST /api/v1/workspaces/automation/run",
            "POST /api/v1/forge/accounts",
            "POST /api/v1/forge/cli-status",
            "POST /api/v1/forge/hosts",
            "POST /api/v1/forge/select-login",
            "POST /api/v1/forge/backfill-bindings",
            "POST /api/v1/forge/retry-binding",
            "POST /api/v1/forge/review-state",
            "POST /api/v1/forge/reviewer-candidates",
            "POST /api/v1/forge/request-reviewers",
            "POST /api/v1/forge/review-comments",
            "POST /api/v1/forge/reply-thread",
            "POST /api/v1/forge/resolve-thread",
            "POST /api/v1/forge/pipeline",
            "POST /api/v1/forge/pipeline-job-log",
            "POST /api/v1/forge/pipeline-job-retry",
            "POST /api/v1/delegations/:delegation_id/apply",
            "POST /api/v1/delegations/:delegation_id/discard",
            "POST /api/v1/delegations/:delegation_id/cancel",
            "POST /api/v1/delegations/:delegation_id/approve",
            "GET /api/v1/sessions/:session_id/ai-memory/export-status",
            "GET /api/v1/sessions/:session_id/ai-memory/sources",
            "GET /api/v1/ai-memory/source-actions",
            "POST /api/v1/ai-memory/source-actions",
            "GET /api/v1/ai-memory/sidecar",
            "GET /api/v1/ai-memory/settings",
            "PUT /api/v1/ai-memory/settings",
            "GET /api/v1/ai-memory/outbox",
            "POST /api/v1/ai-memory/outbox/:session_id/retry",
            "GET /api/v1/ai-memory/export-history",
            "POST /api/v1/app/restart",
            "GET /api/v1/keep-awake",
            "PUT /api/v1/keep-awake",
            "GET /api/v1/menu-bar",
            "PUT /api/v1/providers/:provider_id/availability",
            "GET /api/v1/decision-provider/settings",
            "PUT /api/v1/decision-provider/settings",
            "GET /api/v1/decision-provider/history",
            "GET /api/v1/provider-accounts",
            "POST /api/v1/provider-accounts",
            "POST /api/v1/provider-accounts/status",
            "POST /api/v1/provider-accounts/sign-in",
            "POST /api/v1/provider-accounts/sign-in/cancel",
            "PUT /api/v1/provider-accounts/:account_id",
            "DELETE /api/v1/provider-accounts/:account_id",
            "POST /api/v1/provider-accounts/usage",
            "POST /api/v1/provider-accounts/reset",
            "POST /api/v1/antigravity/status",
            "POST /api/v1/antigravity/install",
            "POST /api/v1/antigravity/connect",
            "GET /api/v1/quick-composer/status",
            "PUT /api/v1/quick-composer/shortcut",
            "POST /api/v1/quick-composer/toggle",
            "GET /api/v1/quick-composer/stream",
            "POST /api/v1/quick-composer/begin",
            "POST /api/v1/quick-composer/checkpoint",
            "GET /api/v1/appshots/status",
            "PUT /api/v1/appshots/shortcut",
            "POST /api/v1/skills/save",
            "POST /api/v1/skills/delete",
            "POST /api/v1/skills/import",
            "POST /api/v1/skills/compile",
            "POST /api/v1/sessions/:session_id/agent",
            "POST /api/v1/agents/:agent_id",
            "POST /api/v1/agents/:agent_id/limit-override",
            "POST /api/v1/agents/:agent_id/conversation",
            "POST /api/v1/task-proposals/:proposal_id/start",
            "POST /api/v1/task-proposals/:proposal_id/dismiss",
            "GET /api/v1/agents/:agent_id/memory",
            "POST /api/v1/agents/:agent_id/memory",
            "POST /api/v1/agents/:agent_id/memory/:note_id",
            "DELETE /api/v1/agents/:agent_id/memory/:note_id",
            "POST /api/v1/agents/:agent_id/memory/versions/:version_id/restore",
            "POST /api/v1/conversation-rewind/prepare",
            "POST /api/v1/conversation-rewind/execute",
            "POST /api/v1/pull-request-hub/list",
            "POST /api/v1/pull-request-hub/reviewed",
            "POST /api/v1/pull-request-hub/detail",
            "POST /api/v1/pull-request-hub/comment",
            "POST /api/v1/pull-request-hub/merge",
            "POST /api/v1/pull-request-hub/submit-review",
            "POST /api/v1/pull-request-hub/reply-thread",
            "POST /api/v1/pull-request-hub/resolve-thread",
            "POST /api/v1/pull-request-hub/review-agent",
            "GET /api/v1/workspaces/:workspace_id/delivery",
            "POST /api/v1/workspaces/:workspace_id/delivery/:action",
            "POST /api/v1/sessions/:session_id/task-title",
            "POST /api/v1/sessions/:session_id/queue",
            "PUT /api/v1/sessions/:session_id/queue/order",
            "POST /api/v1/sessions/:session_id/queue/dispatch-next",
            "DELETE /api/v1/sessions/:session_id/queue/:queued_turn_id",
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
    /// `lifecycle`: only the events that change what a task is doing (turns
    /// starting and ending, requests, delegations), never the streamed text
    /// or tool output. The phone's home and banners need nothing else, and
    /// over the relay the deltas of every running agent add up.
    kinds: Option<String>,
}

/// The events `kinds=lifecycle` keeps: those that move a task between
/// running, waiting on the person and done, plus new tasks and delegations.
fn is_lifecycle_event(event: &CoreEvent) -> bool {
    matches!(
        event,
        CoreEvent::WorkspacePrepared { .. }
            | CoreEvent::WorkspaceReady { .. }
            | CoreEvent::SessionStarted { .. }
            | CoreEvent::SessionCompleted { .. }
            | CoreEvent::SessionAborted { .. }
            | CoreEvent::SessionResumed { .. }
            | CoreEvent::SessionObjectivePaused { .. }
            | CoreEvent::SessionUsageLimitReached { .. }
            | CoreEvent::SessionUsageLimitResumed { .. }
            | CoreEvent::SessionConversationRewound { .. }
            | CoreEvent::SessionTurnStarted { .. }
            | CoreEvent::SessionTurnCompleted { .. }
            | CoreEvent::SessionTurnAborted { .. }
            | CoreEvent::SessionTurnUserInputRequested { .. }
            | CoreEvent::SessionTurnUserInputResolved { .. }
            | CoreEvent::SessionTurnPermissionRequested { .. }
            | CoreEvent::SessionTurnPermissionResolved { .. }
            | CoreEvent::SessionDelegationRequested { .. }
            | CoreEvent::SessionDelegationStarted { .. }
            | CoreEvent::SessionDelegationCompleted { .. }
            | CoreEvent::SessionDelegationFailed { .. }
            | CoreEvent::SessionDelegationCancelled { .. }
    )
}

/// Turns one broadcast item into an SSE frame. A session event carries its
/// durable sequence as the SSE `id`; falling behind the broadcast becomes an
/// explicit `resync` so the client refetches instead of silently missing
/// events.
fn events_stream_frame(
    message: Result<HttpStreamItem, BroadcastStreamRecvError>,
    session_filter: Option<&str>,
    lifecycle_only: bool,
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
            if lifecycle_only && !is_lifecycle_event(event) {
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
    let lifecycle_only = query.kinds.as_deref() == Some("lifecycle");
    let stream = BroadcastStream::new(receiver).filter_map(move |message| {
        events_stream_frame(message, session_filter.as_deref(), lifecycle_only).map(Ok)
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
/// `reset: true` means the cursor can no longer be continued: reload.
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
        // Settled turns get their streamed chunks merged; a cursor inside a
        // merged run never saw the rest of it. `lastSeq: 0` is below any such
        // cursor, so clients that predate `reset` reload too.
        if repo
            .cursor_inside_compacted_run_sync(&session_id, after)
            .map_err(|error| error.to_string())?
        {
            return Ok(json!({ "events": [], "lastSeq": 0, "hasMore": false, "reset": true }));
        }
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

/// The desktop's default for "Continuar sozinho se o limite acabar", so the
/// phone's "Nova tarefa" starts from it.
async fn usage_limit_settings_handler(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    Ok(Json(json!(backend.sessions.usage_limit_settings())))
}

/// "Fechar" on a finished goal (the desktop's `session_dismiss_goal`), for
/// the native app's composer.
async fn dismiss_goal_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .dismiss_finished_goal(&SessionId(session_id))
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

/// The objective's own errors: a bad draft or a stale generation is the
/// client's to fix, a missing record is a 404.
fn objective_error(error: dcc_core::CoreError) -> HttpApiError {
    match error {
        dcc_core::CoreError::InvalidInput(message) => HttpApiError::bad_request(message),
        dcc_core::CoreError::Repository(message) if message.contains("not found") => {
            HttpApiError::not_found(message)
        }
        other => HttpApiError::internal(other.to_string()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetObjectiveBody {
    draft: dcc_core::domain::objective::SessionObjectiveDraft,
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransitionObjectiveBody {
    transition: dcc_core::domain::objective::ObjectiveTransition,
    #[serde(default)]
    expected_generation: Option<u64>,
}

/// The task's durable objective (the composer's "Objetivo" control).
async fn get_objective_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let objective = backend
        .sessions
        .session_objective(&SessionId(session_id))
        .map_err(objective_error)?;
    Ok(Json(json!({ "objective": objective })))
}

/// Creates or rewrites it; `expectedGeneration` makes a concurrent edit fail.
async fn set_objective_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<SetObjectiveBody>,
) -> Result<Json<Value>, HttpApiError> {
    let objective = backend
        .sessions
        .set_session_objective(&SessionId(session_id), body.draft, body.expected_generation)
        .await
        .map_err(objective_error)?;
    Ok(Json(json!({ "objective": objective })))
}

/// Pause, resume or complete it.
async fn transition_objective_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<TransitionObjectiveBody>,
) -> Result<Json<Value>, HttpApiError> {
    let objective = backend
        .sessions
        .transition_session_objective(
            &SessionId(session_id),
            body.transition,
            body.expected_generation,
        )
        .map_err(objective_error)?;
    Ok(Json(json!({ "objective": objective })))
}

async fn clear_objective_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .clear_session_objective(&SessionId(session_id))
        .map_err(objective_error)?;
    Ok(Json(json!({ "objective": null })))
}

/// The queue's and steering's errors: a bad request, a queued turn that is
/// gone (404), or a turn already wrapping up (409 `steer_window_closed`, the
/// client queues the guidance instead).
fn follow_up_error(error: String) -> HttpApiError {
    if error.contains(dcc_core::ports::STEER_WINDOW_CLOSED) {
        return HttpApiError::Refused(dcc_core::ports::STEER_WINDOW_CLOSED, error, Value::Null);
    }
    let normalized = error.to_lowercase();
    if normalized.contains("not found") {
        HttpApiError::not_found(error)
    } else if normalized.starts_with("invalid input")
        || normalized.contains("only be queued while")
        || normalized.contains("no active turn")
    {
        HttpApiError::bad_request(error)
    } else {
        classify_session_error(error)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SteerBody {
    prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskBody {
    project_id: String,
    workspace_root: String,
    base_branch: String,
    #[serde(default)]
    isolation_mode: Option<dcc_core::application::WorkspaceIsolationMode>,
    /// False leaves the project's linked folders out of this task.
    #[serde(default = "default_true")]
    include_linked_folders: bool,
    /// The multi-project task's name (the desktop's "Nova tarefa").
    #[serde(default)]
    bundle_name: Option<String>,
    /// The task's own name (the quick composer's first line).
    #[serde(default)]
    name: Option<String>,
}

fn default_true() -> bool {
    true
}

/// "Nova tarefa" in a project (`handleQuickCreateTask`): the project's linked
/// Git folders make it a multi-project task, folders without Git come as
/// context, and the desktop's own creation runs (setup report, mission spec,
/// forge refresh), so its window shows the task at once.
async fn create_task_handler(
    State(backend): State<Arc<HttpBackend>>,
    Json(body): Json<CreateTaskBody>,
) -> Result<Json<Value>, HttpApiError> {
    use dcc_core::application::linked_task::{
        normalize_workspace_root, plan_linked_task, LinkedFolders, LinkedTaskPlan,
        LinkedTaskPrimary,
    };
    use dcc_tauri::commands::workspace_commands::{
        create_workspace_bundle_with_state, create_workspace_for_repo_with_repo,
        CreateWorkspaceBundleForReposInput,
    };
    let app = desktop_app(&backend)?;
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(&backend.workspace.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let repositories = dcc_core::ports::RepositoryRepo::list_repositories(&repo)
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let root = normalize_workspace_root(body.workspace_root.trim());
    let paths = repositories
        .iter()
        .find(|repository| normalize_workspace_root(repository.root_path.trim()) == root)
        .and_then(|repository| {
            repo.list_repository_linked_folders()
                .ok()?
                .into_iter()
                .find(|(id, _)| *id == repository.id)
                .map(|(_, paths)| paths)
        })
        .unwrap_or_default();
    let linked = LinkedFolders {
        context_paths: dcc_infra::project_actions::linked_context_paths(&paths),
        paths,
    };
    let primary = LinkedTaskPrimary {
        project_id: body.project_id.clone(),
        workspace_root: body.workspace_root.clone(),
        base_branch: body.base_branch.clone(),
    };
    let output = match plan_linked_task(
        &primary,
        &linked,
        &repositories,
        body.isolation_mode.clone(),
        body.include_linked_folders,
    ) {
        LinkedTaskPlan::Single { context_folders } => create_workspace_for_repo_with_repo(
            &repo,
            &app,
            dcc_core::application::CreateWorkspaceForRepoInput {
                project_id: dcc_core::domain::project::ProjectId(body.project_id),
                workspace_root: body.workspace_root,
                base_branch: body.base_branch,
                name: body.name.clone(),
                isolation_mode: body.isolation_mode,
                context_folders,
            },
        )
        .await
        .map_err(HttpApiError::bad_request)?,
        LinkedTaskPlan::Bundle { projects } => {
            let mut created = create_workspace_bundle_with_state(
                &backend.workspace,
                &app,
                CreateWorkspaceBundleForReposInput {
                    name: body
                        .bundle_name
                        .or(body.name)
                        .unwrap_or_else(|| "Nova tarefa".to_string()),
                    projects,
                },
            )
            .await
            .map_err(HttpApiError::bad_request)?;
            created.workspaces.remove(0)
        }
    };
    Ok(Json(json!({
        "workspaceId": output.workspace.id,
        "setupReport": output.setup_report,
        "setupHints": output.setup_hints,
    })))
}

/// Where the creation's events go: the desktop app it runs in.
fn desktop_app(backend: &HttpBackend) -> Result<dcc_tauri::app_events::AppEvents, HttpApiError> {
    backend
        .app
        .get()
        .map(|app| app.events())
        .ok_or_else(|| HttpApiError::service_unavailable("creating a task needs the desktop app"))
}

/// The open-project dialog's single task, exactly as chosen there (base
/// branch, name, context folders): `createWorkspace`.
async fn create_workspace_handler(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<dcc_core::application::CreateWorkspaceForRepoInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = desktop_app(&backend)?;
    let repo = dcc_infra::db::SqliteWorkspaceRepo::open(&backend.workspace.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let output = dcc_tauri::commands::workspace_commands::create_workspace_for_repo_with_repo(
        &repo, &app, input,
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({
        "workspaceId": output.workspace.id,
        "setupReport": output.setup_report,
        "setupHints": output.setup_hints,
    })))
}

/// "Clonar": clones the repository into the folder chosen and starts a task
/// in it (`cloneWorkspaceFromUrl`).
async fn clone_workspace_handler(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<
        dcc_core::application::create_workspace_from_url::CreateWorkspaceFromUrlInput,
    >,
) -> Result<Json<Value>, HttpApiError> {
    let app = desktop_app(&backend)?;
    let output = dcc_tauri::commands::workspace_commands::create_workspace_from_url_with_state(
        &backend.workspace,
        &app,
        input,
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({
        "workspaceId": output.workspace.id,
        "setupReport": output.setup_report,
        "setupHints": output.setup_hints,
    })))
}

/// "Validar": what a branch or PR URL points at in the project.
async fn resolve_source_url_handler(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<dcc_tauri::commands::workspace_commands::ResolveWorkspaceSourceUrlInput>,
) -> Result<Json<Value>, HttpApiError> {
    let resolution =
        dcc_tauri::commands::workspace_commands::resolve_workspace_source_url_with_state(
            &backend.workspace,
            input,
        )
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(resolution)))
}

/// "Abrir branch": a task on an existing branch of the project.
async fn create_from_source_url_handler(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<dcc_tauri::commands::workspace_commands::CreateWorkspaceFromSourceUrlInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = desktop_app(&backend)?;
    let output =
        dcc_tauri::commands::workspace_commands::create_workspace_from_source_url_with_state(
            &backend.workspace,
            &app,
            input,
        )
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({
        "workspaceId": output.workspace.id,
        "setupReport": output.setup_report,
        "setupHints": output.setup_hints,
    })))
}

/// The dialog's multi-project task: `createWorkspaceBundle`, with the
/// desktop's rollback when one project fails.
async fn create_workspace_bundle_handler(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<dcc_tauri::commands::workspace_commands::CreateWorkspaceBundleForReposInput>,
) -> Result<Json<Value>, HttpApiError> {
    let app = desktop_app(&backend)?;
    let created = dcc_tauri::commands::workspace_commands::create_workspace_bundle_with_state(
        &backend.workspace,
        &app,
        input,
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({
        "workspaceId": created.workspaces.first().map(|output| output.workspace.id.clone()),
        "count": created.workspaces.len(),
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskTitleBody {
    workspace_id: String,
    title: String,
}

/// Names a task after its first prompt (`persistAutomaticTaskTitle`); a task
/// the person named keeps its name (`applied: false`).
async fn apply_task_title_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<TaskTitleBody>,
) -> Result<Json<Value>, HttpApiError> {
    let titled = dcc_tauri::commands::session_commands::apply_task_title_to_state(
        &backend.sessions,
        dcc_tauri::commands::session_commands::ApplyTaskTitleInput {
            workspace_id: body.workspace_id,
            session_id,
            title: body.title,
        },
    )
    .await
    .map_err(|error| {
        if error.contains("cannot") || error.contains("does not belong") {
            HttpApiError::bad_request(error)
        } else {
            classify_session_error(error)
        }
    })?;
    Ok(Json(json!({ "applied": titled.applied })))
}

/// Guidance into the running turn (the composer's "Orientar agora").
async fn steer_turn_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<SteerBody>,
) -> Result<Json<Value>, HttpApiError> {
    let output = dcc_tauri::commands::session_commands::steer_turn_with_state(
        &backend.sessions,
        dcc_core::application::SteerTurnInput {
            session_id: SessionId(session_id),
            prompt: body.prompt,
        },
    )
    .await
    .map_err(follow_up_error)?;
    Ok(Json(json!({ "turnId": output.turn_id })))
}

/// A follow-up for after the running turn, with the same body as a turn.
async fn queue_turn_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<SendTurnInput>,
) -> Result<Json<Value>, HttpApiError> {
    input.session_id = SessionId(session_id);
    mobile::resolve_turn_approval_policy(config, &backend, &mut input).await?;
    let queued = dcc_tauri::commands::session_commands::queue_turn_with_state(
        &backend.sessions,
        dcc_core::application::QueueTurnInput { turn: input },
    )
    .await
    .map_err(follow_up_error)?;
    Ok(Json(json!({ "queuedTurn": queued })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderQueueBody {
    queued_turn_ids: Vec<String>,
}

async fn reorder_turn_queue_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<ReorderQueueBody>,
) -> Result<Json<Value>, HttpApiError> {
    let queue = dcc_core::application::reorder_turn_queue(
        &backend.sessions,
        &backend.sessions,
        dcc_core::application::ReorderTurnQueueInput {
            session_id: SessionId(session_id),
            queued_turn_ids: body.queued_turn_ids,
        },
    )
    .await
    .map_err(|error| follow_up_error(error.to_string()))?;
    Ok(Json(json!({ "queue": queue })))
}

async fn remove_queued_turn_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path((session_id, queued_turn_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let queue = dcc_core::application::remove_queued_turn(
        &backend.sessions,
        &backend.sessions,
        dcc_core::application::RemoveQueuedTurnInput {
            session_id: SessionId(session_id),
            queued_turn_id,
        },
    )
    .await
    .map_err(|error| follow_up_error(error.to_string()))?;
    Ok(Json(json!({ "queue": queue })))
}

/// "Enviar o próximo": the head of the queue when no turn runs (after a
/// pause, a stop or a limit).
async fn dispatch_next_queued_turn_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let dispatched = backend
        .sessions
        .dispatch_next_queued_turn(&SessionId(session_id))
        .await
        .map_err(|error| follow_up_error(error.to_string()))?;
    Ok(Json(json!({ "dispatched": dispatched })))
}

/// "Continuar" on a task stopped by its usage limit.
async fn resume_after_usage_limit_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let turn_id = backend
        .sessions
        .resume_after_usage_limit(&SessionId(session_id), false)
        .await
        .map_err(|error| match error {
            dcc_core::CoreError::InvalidInput(message) => HttpApiError::BadRequest(message),
            error => classify_session_error(error.to_string()),
        })?;
    Ok(Json(json!({ "turnId": turn_id.0 })))
}

async fn usage_limit_auto_resume_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let enabled = backend
        .sessions
        .session_auto_resume_enabled(&SessionId(session_id))
        .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(json!({ "enabled": enabled })))
}

#[derive(Deserialize)]
struct AutoResumeBody {
    enabled: bool,
}

async fn set_usage_limit_auto_resume_handler(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<AutoResumeBody>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .set_session_auto_resume(&SessionId(session_id), body.enabled)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(json!({ "enabled": body.enabled })))
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
        send_json(app, "POST", uri, body).await
    }

    async fn send_json(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        use tower::util::ServiceExt;
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
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
    async fn the_objective_routes_set_transition_and_clear_it() {
        let fixture = embedded_fixture().await;
        let uri = "/api/v1/sessions/http-respond/objective";
        let (status, body) = send_json(&fixture.app, "GET", uri, Value::Null).await;
        assert_eq!((status, &body["objective"]), (StatusCode::OK, &Value::Null));

        let draft = json!({ "draft": { "intent": "Checkout verde", "doneWhen": "testes passam", "maxTurns": 5 } });
        let (status, body) = send_json(&fixture.app, "PUT", uri, draft.clone()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["objective"]["status"], "active");
        let generation = body["objective"]["generation"].as_u64().unwrap();

        // A second writer that never loaded it fails instead of overwriting.
        let stale = json!({ "draft": { "intent": "Outro" }, "expectedGeneration": generation + 7 });
        let (status, _) = send_json(&fixture.app, "PUT", uri, stale).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = send_json(
            &fixture.app,
            "PUT",
            uri,
            json!({ "draft": { "intent": "  " } }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/objective/transition",
            json!({ "transition": "pause", "expectedGeneration": generation }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["objective"]["status"], "paused");
        assert_eq!(body["objective"]["pauseReason"], "manual");

        let (status, body) = send_json(&fixture.app, "DELETE", uri, Value::Null).await;
        assert_eq!((status, &body["objective"]), (StatusCode::OK, &Value::Null));
        let (_, body) = send_json(&fixture.app, "GET", uri, Value::Null).await;
        assert_eq!(body["objective"], Value::Null);
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/objective/transition",
            json!({ "transition": "resume" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_queue_routes_add_reorder_and_remove_follow_ups() {
        use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, TurnId};
        let fixture = embedded_fixture().await;
        let base = "/api/v1/sessions/http-respond/queue";
        // Nothing runs yet: a follow-up has nothing to wait for.
        let (status, _) = post_json(
            &fixture.app,
            base,
            json!({ "sessionId": "", "prompt": "depois" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        SessionEventRepo::append_event(
            &fixture.state,
            &SessionEventRecord {
                event_id: Uuid::new_v4().to_string(),
                session_id: fixture.session_id.clone(),
                sequence: 0,
                occurred_at: "2026-10-24T00:00:01Z".to_string(),
                kind: SessionEventKind::TurnStarted {
                    turn_id: TurnId("t1".to_string()),
                    prompt: "agora".to_string(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: None,
                },
            },
        )
        .await
        .expect("turn started");
        let mut ids = Vec::new();
        for prompt in ["primeiro", "segundo"] {
            let (status, body) = post_json(
                &fixture.app,
                base,
                json!({ "sessionId": "", "prompt": prompt, "planMode": false }),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["queuedTurn"]["prompt"], prompt);
            ids.push(body["queuedTurn"]["id"].as_str().unwrap().to_string());
        }
        let (status, body) = send_json(
            &fixture.app,
            "PUT",
            &format!("{base}/order"),
            json!({ "queuedTurnIds": [ids[1], ids[0]] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["queue"][0]["prompt"], "segundo");
        let (status, _) = send_json(
            &fixture.app,
            "PUT",
            &format!("{base}/order"),
            json!({ "queuedTurnIds": [ids[0]] }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "every queued turn once");
        let (status, body) = send_json(
            &fixture.app,
            "DELETE",
            &format!("{base}/{}", ids[1]),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["queue"].as_array().unwrap().len(), 1);
        let (status, _) = send_json(
            &fixture.app,
            "DELETE",
            &format!("{base}/{}", ids[1]),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn naming_a_task_checks_the_session_and_the_title() {
        let fixture = embedded_fixture().await;
        let uri = "/api/v1/sessions/http-respond/task-title";
        let (status, body) = post_json(
            &fixture.app,
            uri,
            json!({ "workspaceId": "workspace", "title": "   " }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let (status, body) = post_json(
            &fixture.app,
            uri,
            json!({ "workspaceId": "other", "title": "Corrigir" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/sessions/missing/task-title",
            json!({ "workspaceId": "workspace", "title": "Corrigir" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_local_branch_pick_refuses_a_task_that_does_not_exist() {
        let fixture = embedded_fixture().await;
        for (uri, body) in [
            (
                "/api/v1/workspaces/local-branches",
                json!({ "workspaceId": "missing", "refresh": false }),
            ),
            (
                "/api/v1/workspaces/local-branches/switch",
                json!({ "workspaceId": "missing", "expectedBranch": "main", "reference": "refs/heads/x", "newBranch": null }),
            ),
        ] {
            let (status, body) = post_json(&fixture.app, uri, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resolving_a_source_url_refuses_a_folder_that_is_no_project() {
        let fixture = embedded_fixture().await;
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/workspaces/source-url/resolve",
            json!({ "workspaceRoot": "/definitely/not/here", "url": "https://github.com/o/r/tree/main", "forgeLogin": null }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn creating_a_task_needs_the_desktop_app() {
        let fixture = embedded_fixture().await;
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/workspaces/create",
            json!({ "projectId": "p", "workspaceRoot": "/tmp/p", "baseBranch": "main" }),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        for (uri, body) in [
            (
                "/api/v1/workspaces",
                json!({ "projectId": "p", "workspaceRoot": "/tmp/p", "baseBranch": "main", "name": null, "isolationMode": null }),
            ),
            (
                "/api/v1/workspace-bundles",
                json!({ "name": "Nova tarefa", "projects": [] }),
            ),
            (
                "/api/v1/workspaces/clone",
                json!({ "projectId": "p", "repositoryUrl": "https://x/y.git", "workspaceRoot": "/tmp/y", "baseBranch": "", "name": null }),
            ),
            (
                "/api/v1/workspaces/from-source-url",
                json!({ "projectId": "p", "workspaceRoot": "/tmp/p", "url": "https://x/y/tree/b", "name": null, "forgeLogin": null }),
            ),
        ] {
            let (status, answer) = post_json(&fixture.app, uri, body).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}: {answer}");
        }
    }

    #[test]
    fn a_turn_wrapping_up_refuses_steering_with_its_own_code() {
        let error = follow_up_error(format!(
            "provider error: {}",
            dcc_core::ports::STEER_WINDOW_CLOSED
        ));
        assert_eq!(error.status(), StatusCode::CONFLICT);
        assert_eq!(error.code(), dcc_core::ports::STEER_WINDOW_CLOSED);
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
    async fn a_cursor_inside_merged_history_asks_the_client_to_reload() {
        let fixture = embedded_fixture().await;
        append_resumed_events(&fixture, 5).await;
        let db_path = std::fs::canonicalize(fixture._root.path())
            .unwrap()
            .join("state.sqlite");
        // As the delta compaction leaves it: row 2 absorbed rows 3 and 4.
        rusqlite::Connection::open(db_path)
            .unwrap()
            .execute_batch(
                "UPDATE dcc_session_events SET compacted_through = 4 WHERE sequence = 2;
                 DELETE FROM dcc_session_events WHERE sequence IN (3, 4);",
            )
            .unwrap();

        let (status, inside) = get_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/events?after=3",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{inside}");
        assert_eq!(inside["reset"], true);
        assert_eq!(inside["lastSeq"], 0, "older phones reset on lastSeq < cursor");

        let (_, past) = get_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/events?after=4",
        )
        .await;
        assert_eq!(past["reset"], Value::Null);
        assert_eq!(past["lastSeq"], 5);
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
    async fn lifecycle_stream_skips_streamed_text() {
        let fixture = embedded_fixture().await;
        let mut body = open_stream(&fixture.app, "/api/v1/events/stream?kinds=lifecycle").await;
        let item = |event| HttpStreamItem::core(event, None);
        let events = &fixture.backend.events;
        events
            .send(item(CoreEvent::SessionTurnDelta {
                session_id: "s".into(),
                turn_id: "t".into(),
                content: "token-text".into(),
            }))
            .unwrap();
        events
            .send(item(CoreEvent::SessionTurnCompleted {
                session_id: "s".into(),
                turn_id: "t".into(),
            }))
            .unwrap();

        let seen = read_until(&mut body, "sessionTurnCompleted").await;
        assert!(!seen.contains("token-text"), "{seen}");
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
        // Only the headless server spawns free shells.
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/terminals/spawn",
            json!({ "cwd": "/tmp" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[derive(Default)]
    struct FakeTerminals {
        calls: Mutex<Vec<String>>,
    }

    impl DesktopTerminals for FakeTerminals {
        fn get_or_create(&self, owner_key: &str, options: Value) -> Result<Value, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("open {owner_key} {options}"));
            Ok(json!({ "ptyId": "pty-1", "existing": false, "chunks": ["hi"] }))
        }
        fn write(&self, pty_id: &str, data: String) -> Result<bool, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("write {pty_id} {data}"));
            Ok(true)
        }
        fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<bool, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("resize {pty_id} {cols}x{rows}"));
            Ok(true)
        }
        fn kill(&self, pty_id: &str) -> Result<bool, String> {
            self.calls.lock().unwrap().push(format!("kill {pty_id}"));
            Ok(false)
        }
        fn activity(&self) -> Result<Vec<Value>, String> {
            Ok(vec![json!({ "ptyId": "pty-1", "status": "idle" })])
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_desktop_terminals_stream_what_they_print() {
        let fixture = embedded_fixture().await;
        let mut body = open_stream(&fixture.app, "/api/v1/terminals/events/stream").await;
        publish_desktop_terminal_event(DesktopTerminalEvent::Output {
            pty_id: "pty-3".into(),
            data: "hello".into(),
        });
        let seen = read_until(&mut body, "hello").await;
        assert!(seen.contains("event: terminal-output"), "{seen}");
        assert!(seen.contains(r#""ptyId":"pty-3""#), "{seen}");
        publish_desktop_terminal_event(DesktopTerminalEvent::Exit {
            pty_id: "pty-3".into(),
            code: Some(0),
        });
        let seen = read_until(&mut body, "terminal-exit").await;
        assert!(seen.contains(r#""code":0"#), "{seen}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_desktop_terminals_open_in_a_task_checkout() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let fake = Arc::new(FakeTerminals::default());
        let _ = fixture.backend.desktop_terminals.set(fake.clone());
        let repo = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(repo.path())
            .unwrap()
            .to_string_lossy()
            .to_string();
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-term".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: root.clone(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-25T00:00:00Z".to_string(),
                updated_at: "2026-10-25T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/terminals/by-owner/terminal%3Aworktree%3Aw-term%3At1",
            json!({ "cwd": root, "command": "/bin/zsh", "args": ["-l"], "cols": 90, "rows": 20 }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["ptyId"], "pty-1");
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/terminals/pty-1/write",
            json!({ "data": "ls\r" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, body) = post_json(
            &fixture.app,
            "/api/v1/terminals/pty-1/resize",
            json!({ "cols": 100, "rows": 30 }),
        )
        .await;
        assert_eq!(body["ok"], true);
        let (_, body) = post_json(&fixture.app, "/api/v1/terminals/pty-1/kill", json!({})).await;
        assert_eq!(body["ok"], false);
        let (status, body) = get_json(&fixture.app, "/api/v1/terminals/activity").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["items"][0]["status"], "idle");
        let calls = fake.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 4, "{calls:?}");
        assert!(
            calls[0].starts_with("open terminal:worktree:w-term:t1 "),
            "{}",
            calls[0]
        );
        let mut options: Value =
            serde_json::from_str(calls[0].splitn(3, ' ').nth(2).unwrap()).unwrap();
        // The task's own shell gets its variables and its port block.
        let env = options.as_object_mut().unwrap().remove("env").unwrap();
        assert_eq!(env["DCC_WORKSPACE_ID"], "w-term");
        assert_eq!(env["DCC_WORKSPACE_PATH"], root.as_str());
        assert_eq!(env["DCC_DEFAULT_BRANCH"], "main");
        let port: u16 = env["DCC_PORT"].as_str().unwrap().parse().unwrap();
        assert!(port >= dcc_infra::db::PORT_BLOCK_START, "{port}");
        assert_eq!(
            options,
            json!({
                "cwd": root, "command": "/bin/zsh", "args": ["-l"], "cols": 90, "rows": 20,
                "ptyOwnerKey": "terminal:worktree:w-term:t1",
            })
        );
        assert_eq!(calls[1], "write pty-1 ls\r");
        assert_eq!(calls[2], "resize pty-1 100x30");
        assert_eq!(calls[3], "kill pty-1");
        // A folder that is no task's checkout gets no shell.
        let other = tempfile::tempdir().unwrap();
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/terminals/by-owner/x",
            json!({ "cwd": other.path(), "command": "/bin/zsh" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(fake.calls.lock().unwrap().len(), 4);
        // The app's own commands (a sign-in, a CLI update) open in the home
        // folder; nothing else does, and they open nowhere else.
        let home = std::env::var("HOME").unwrap();
        let open = |owner: &'static str, cwd: String| {
            post_json(
                &fixture.app,
                owner,
                json!({ "cwd": cwd, "command": "/bin/zsh", "args": ["-lc", "true"] }),
            )
        };
        let (status, _) = open("/api/v1/terminals/by-owner/cli-update%3Acodex", home.clone()).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) =
            open("/api/v1/terminals/by-owner/forge-connect%3Agithub%3Agithub.com", home.clone())
                .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = open("/api/v1/terminals/by-owner/terminal%3Ax", home.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = open("/api/v1/terminals/by-owner/cli-update%3Acodex", "/".into()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(fake.calls.lock().unwrap().len(), 6);
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
                // Ignored files never reach the phone, new or not.
                std::fs::write(tree_path.join(".gitignore"), "secret.env\n").unwrap();
                std::fs::write(tree_path.join("secret.env"), "x").unwrap();
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
        // A new file not yet committed can be mentioned, as in the file tree.
        let (_, body) = get_json(
            &fixture.app,
            "/api/v1/mobile/workspaces/ws-files/files?q=untracked",
        )
        .await;
        assert_eq!(body["files"], json!([{ "path": "untracked.txt" }]), "{body}");
        let (_, body) = get_json(
            &fixture.app,
            "/api/v1/mobile/workspaces/ws-files/files?q=secret",
        )
        .await;
        assert!(
            body["files"].as_array().unwrap().is_empty(),
            "ignored files stay out: {body}"
        );
        let (_, body) = get_json(
            &fixture.app,
            "/api/v1/mobile/workspaces/ws-files/files?limit=2",
        )
        .await;
        assert_eq!(body["files"].as_array().unwrap().len(), 2);
        // README, three under src, untracked.txt and .gitignore.
        assert_eq!(body["total"], 6);

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
    async fn a_phone_sees_a_task_proposal_and_dismisses_it_once() {
        use dcc_core::domain::delegation::{DelegationBudget, DelegationMode, DelegationStatus};
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        use dcc_core::ports::WorkspaceRepo;
        let fixture = embedded_fixture().await;
        let db_path = std::fs::canonicalize(fixture._root.path())
            .unwrap()
            .join("state.sqlite");
        dcc_infra::db::SqliteWorkspaceRepo::open(&db_path)
            .unwrap()
            .save_workspace(&Workspace {
                id: WorkspaceId("workspace".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: "/workspace".to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-08T00:00:00Z".to_string(),
                updated_at: "2026-10-08T00:00:00Z".to_string(),
            })
            .await
            .unwrap();
        let proposal = dcc_tauri::commands::delegation_commands::create_task_proposal_with_state(
            &fixture.state,
            dcc_tauri::commands::delegation_commands::CreateTaskProposalInput {
                parent_session_id: fixture.session_id.clone(),
                parent_turn_id: None,
                workspace_id: WorkspaceId("workspace".to_string()),
                target_provider_id: dcc_core::domain::provider::ProviderId("codex".to_string()),
                target_model_id: None,
                mode: DelegationMode::Implement,
                title: "Fase 2: push".to_string(),
                instruction: "Implement push notifications.".to_string(),
                budget: DelegationBudget::default(),
            },
        )
        .await
        .unwrap();

        let (status, listed) =
            call(&fixture.app, "GET", "/api/v1/mobile/task-proposals", None, Some("phone-1"), false).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed["proposals"][0]["id"], proposal.id.0);
        assert_eq!(listed["proposals"][0]["title"], "Fase 2: push");
        assert_eq!(listed["proposals"][0]["brief"], "Implement push notifications.");
        assert_eq!(listed["proposals"][0]["provider"], "Codex");
        assert_eq!(listed["proposals"][0]["edits"], true);

        let path = format!("/api/v1/mobile/task-proposals/{}/dismiss", proposal.id.0);
        let (status, _) = call(&fixture.app, "POST", &path, None, Some("phone-1"), false).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = call(&fixture.app, "POST", &path, None, None, true).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let start = format!("/api/v1/mobile/task-proposals/{}/start", proposal.id.0);
        let (status, _) = call(&fixture.app, "POST", &start, None, Some("phone-1"), false).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let stored = dcc_core::ports::DelegationRepo::get_delegation(&fixture.state, &proposal.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, DelegationStatus::Cancelled);
        let (_, listed) =
            call(&fixture.app, "GET", "/api/v1/mobile/task-proposals", None, Some("phone-1"), false).await;
        assert_eq!(listed["proposals"], json!([]));

        // The desktop's card answers the same proposals.
        let card = dcc_tauri::commands::delegation_commands::create_task_proposal_with_state(
            &fixture.state,
            dcc_tauri::commands::delegation_commands::CreateTaskProposalInput {
                parent_session_id: fixture.session_id.clone(),
                parent_turn_id: None,
                workspace_id: WorkspaceId("workspace".to_string()),
                target_provider_id: dcc_core::domain::provider::ProviderId("codex".to_string()),
                target_model_id: None,
                mode: DelegationMode::Implement,
                title: "Fase 3".to_string(),
                instruction: "Implement it.".to_string(),
                budget: DelegationBudget::default(),
            },
        )
        .await
        .unwrap();
        let dismiss = format!("/api/v1/task-proposals/{}/dismiss", card.id.0);
        let (status, body) = call(&fixture.app, "POST", &dismiss, None, None, true).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let start = format!("/api/v1/task-proposals/{}/start", card.id.0);
        let (status, _) = call(
            &fixture.app,
            "POST",
            &start,
            Some(json!({"brief": "Edited."})),
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, _) = call(
            &fixture.app,
            "POST",
            "/api/v1/task-proposals/missing/start",
            Some(json!({})),
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
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
    async fn the_pull_request_hub_answers_for_known_repositories_only() {
        let fixture = embedded_fixture().await;
        // No project registered: nothing to list.
        let (status, listed) =
            post_json(&fixture.app, "/api/v1/pull-request-hub/list", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(listed["items"], json!([]));
        let other = tempfile::tempdir().unwrap();
        let root = other.path().to_string_lossy().to_string();
        for (uri, body) in [
            (
                "/api/v1/pull-request-hub/detail",
                json!({ "repositoryRoot": root, "number": 1 }),
            ),
            (
                "/api/v1/pull-request-hub/comment",
                json!({ "repositoryRoot": root, "number": 1, "body": "ok" }),
            ),
            (
                "/api/v1/pull-request-hub/merge",
                json!({ "repositoryRoot": root, "number": 1, "method": "squash", "expectedHeadSha": "abc" }),
            ),
            (
                "/api/v1/pull-request-hub/submit-review",
                json!({ "repositoryRoot": root, "number": 1, "event": "comment", "body": "ok", "comments": [] }),
            ),
            (
                "/api/v1/pull-request-hub/reply-thread",
                json!({ "repositoryRoot": root, "number": 1, "commentId": "1", "threadId": "t", "body": "ok" }),
            ),
            (
                "/api/v1/pull-request-hub/resolve-thread",
                json!({ "repositoryRoot": root, "number": 1, "threadId": "t", "resolved": true }),
            ),
            (
                "/api/v1/pull-request-hub/review-agent",
                json!({ "workingDirectory": root, "providerId": "codex", "prompt": "x" }),
            ),
        ] {
            let (status, error) = post_json(&fixture.app, uri, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri} {error}");
            assert!(!crate::remote_connector::remote_path_allowed(uri), "{uri}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn paired_phones_are_listed_revoked_and_audited_by_the_operator_only() {
        let fixture = embedded_fixture().await;
        let conn = crate::pairing::open_pairing_db(&fixture.backend.workspace.db_path).unwrap();
        conn.execute(
            "INSERT INTO paired_devices (device_id, device_name, public_key_spki, noise_public_key)
             VALUES ('phone-1', 'Pixel', x'00', x'01')",
            [],
        )
        .unwrap();
        let (status, listed) = get_json(&fixture.app, "/api/v1/remote/devices").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed["devices"][0]["deviceId"], "phone-1");
        assert_eq!(listed["devices"][0]["deviceName"], "Pixel");
        assert_eq!(listed["devices"][0]["kind"], "relay");
        // A paired phone cannot manage phones, even with the key.
        let (status, _) = call(&fixture.app, "GET", "/api/v1/remote/devices", None, Some("phone-1"), true).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let revoke = json!({ "deviceId": "phone-1" });
        let (status, _) = call(&fixture.app, "POST", "/api/v1/remote/devices/revoke", Some(revoke.clone()), Some("phone-1"), true).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, revoked) = post_json(&fixture.app, "/api/v1/remote/devices/revoke", revoke).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(revoked, json!({ "revoked": true }));
        let (_, listed) = get_json(&fixture.app, "/api/v1/remote/devices").await;
        assert_eq!(listed["devices"], json!([]));
        let (_, listed) = get_json(&fixture.app, "/api/v1/remote/devices?includeRevoked=true").await;
        assert_eq!(listed["devices"][0]["revoked"], true);
        let (status, audit) = get_json(&fixture.app, "/api/v1/remote/audit?limit=50").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(audit["entries"][0]["event"], "revoke");
        assert_eq!(audit["entries"][0]["deviceId"], "phone-1");
        // Without the relay running there is nothing to save.
        let (status, _) = post_json(&fixture.app, "/api/v1/remote/relay", json!({ "relayUrl": null })).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        for path in ["/api/v1/remote/devices", "/api/v1/remote/devices/revoke", "/api/v1/remote/audit", "/api/v1/remote/relay"] {
            assert!(!crate::remote_connector::remote_path_allowed(path), "{path}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_session_s_computer_use_grant_ends_through_the_bridge() {
        let fixture = embedded_fixture().await;
        let body = json!({ "sessionId": "s-1" });
        let (status, _) = post_json(&fixture.app, "/api/v1/computer/bridge/disarm", body.clone()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let _ = fixture
            .backend
            .desktop_computer
            .set(Arc::new(computer_bridge::tests::Fixed(json!([]))));
        let (status, grant) = post_json(&fixture.app, "/api/v1/computer/bridge/disarm", body).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(grant, json!({ "sessionId": "s-1", "armed": false }));
        assert!(!crate::remote_connector::remote_path_allowed("/api/v1/computer/bridge/disarm"));
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
    async fn the_changes_list_stages_unstages_and_discards_in_a_task_checkout() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("git dir");
        let repo = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.name", "DCC Test"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        std::fs::write(repo.join("a.txt"), "um\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-m", "init"]);
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-changes".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-24T00:00:00Z".to_string(),
                updated_at: "2026-10-24T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        let root = repo.to_string_lossy().to_string();
        let file = |path: &str| json!({ "workspaceRoot": root, "relativePath": path });
        std::fs::write(repo.join("a.txt"), "dois\n").unwrap();
        std::fs::write(repo.join("novo.txt"), "x\n").unwrap();

        let (status, body) = post_json(&fixture.app, "/api/v1/git/stage-file", file("a.txt")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            git(&repo, &["diff", "--cached", "--name-only"]).trim(),
            "a.txt"
        );
        let (status, _) = post_json(&fixture.app, "/api/v1/git/unstage-file", file("a.txt")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]).trim(), "");
        let (status, _) = post_json(&fixture.app, "/api/v1/git/stage-all", file(".")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            git(&repo, &["diff", "--cached", "--name-only"]).trim(),
            "a.txt\nnovo.txt"
        );
        git(&repo, &["reset", "-q"]);
        // Discarding a new file removes it; a tracked one goes back to HEAD.
        let (status, _) =
            post_json(&fixture.app, "/api/v1/git/discard-file", file("novo.txt")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!repo.join("novo.txt").exists());
        let (status, _) = post_json(&fixture.app, "/api/v1/git/discard-file", file("a.txt")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "um\n");

        // A folder that is no task's checkout is refused; a path out of it too.
        let other = tempfile::tempdir().unwrap();
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/git/discard-file",
            json!({ "workspaceRoot": other.path(), "relativePath": "a.txt" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let (status, _) = post_json(&fixture.app, "/api/v1/git/discard-file", file("../x")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn global_shortcuts_pass_through_to_the_desktop() {
        struct Fake(std::sync::Mutex<Vec<String>>);
        impl DesktopShortcuts for Fake {
            fn quick_composer(&self) -> Result<Value, String> {
                Ok(json!({ "supported": true, "shortcut": "Super+Shift+KeyK" }))
            }
            fn set_quick_composer(&self, shortcut: Option<String>) -> Result<(), String> {
                match shortcut.as_deref() {
                    Some("Shift+KeyA") => Err("shortcutInvalid".into()),
                    other => {
                        self.0.lock().unwrap().push(format!("quick {other:?}"));
                        Ok(())
                    }
                }
            }
            fn toggle_quick_composer(&self) -> Result<(), String> {
                self.0.lock().unwrap().push("toggle".into());
                Ok(())
            }
            fn begin_quick_launch(&self, launch: Value) -> Result<Value, String> {
                self.0
                    .lock()
                    .unwrap()
                    .push(format!("begin {}", launch["phase"]));
                Ok(launch)
            }
            fn checkpoint_quick_launch(&self, launch: Value) -> Result<Value, String> {
                self.0
                    .lock()
                    .unwrap()
                    .push(format!("checkpoint {}", launch["phase"]));
                Ok(launch)
            }
            fn appshots(&self) -> Result<Value, String> {
                Ok(json!({ "supported": true, "shortcut": null }))
            }
            fn set_appshots(&self, shortcut: Option<String>) -> Result<(), String> {
                self.0
                    .lock()
                    .unwrap()
                    .push(format!("appshots {shortcut:?}"));
                Ok(())
            }
        }
        let fixture = embedded_fixture().await;
        let (status, _) = get_json(&fixture.app, "/api/v1/appshots/status").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let fake = Arc::new(Fake(std::sync::Mutex::new(Vec::new())));
        let shared: Arc<dyn DesktopShortcuts> = fake.clone();
        let _ = fixture.backend.desktop_shortcuts.set(shared);
        let (_, quick) = get_json(&fixture.app, "/api/v1/quick-composer/status").await;
        assert_eq!(quick["shortcut"], "Super+Shift+KeyK");
        let (status, refused) = send_json(
            &fixture.app,
            "PUT",
            "/api/v1/quick-composer/shortcut",
            json!({ "shortcut": "Shift+KeyA" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(refused.to_string().contains("shortcutInvalid"), "{refused}");
        send_json(
            &fixture.app,
            "PUT",
            "/api/v1/appshots/shortcut",
            json!({ "shortcut": null }),
        )
        .await;
        post_json(&fixture.app, "/api/v1/quick-composer/toggle", json!({})).await;
        let (_, begun) = post_json(
            &fixture.app,
            "/api/v1/quick-composer/begin",
            json!({ "phase": "pending" }),
        )
        .await;
        assert_eq!(begun["phase"], "pending");
        post_json(
            &fixture.app,
            "/api/v1/quick-composer/checkpoint",
            json!({ "phase": "creating" }),
        )
        .await;
        assert_eq!(
            *fake.0.lock().unwrap(),
            [
                "appshots None".to_string(),
                "toggle".to_string(),
                "begin \"pending\"".to_string(),
                "checkpoint \"creating\"".to_string()
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn keep_awake_answers_only_with_the_desktops_controller() {
        use crate::keep_awake::{KeepAwakeMode, KeepAwakeStatus};
        struct Fake(std::sync::Mutex<KeepAwakeMode>);
        impl DesktopKeepAwake for Fake {
            fn status(&self) -> Option<KeepAwakeStatus> {
                Some(KeepAwakeStatus {
                    supported: true,
                    mode: *self.0.lock().unwrap(),
                    active: None,
                })
            }
            fn set_mode(&self, mode: KeepAwakeMode) -> Option<Result<KeepAwakeStatus, String>> {
                *self.0.lock().unwrap() = mode;
                self.status().map(Ok)
            }
        }
        let fixture = embedded_fixture().await;
        let (status, _) = get_json(&fixture.app, "/api/v1/keep-awake").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let fake: Arc<dyn DesktopKeepAwake> =
            Arc::new(Fake(std::sync::Mutex::new(KeepAwakeMode::Off)));
        let _ = fixture.backend.desktop_keep_awake.set(fake);
        let (status, read) = get_json(&fixture.app, "/api/v1/keep-awake").await;
        assert_eq!(status, StatusCode::OK, "{read}");
        assert_eq!(read["mode"], "off");
        let (status, set) = send_json(
            &fixture.app,
            "PUT",
            "/api/v1/keep-awake",
            json!({ "mode": "during_agents" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{set}");
        assert_eq!(set["mode"], "during_agents");
        assert_eq!(set["supported"], true);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_menu_bar_snapshot_comes_from_the_desktop() {
        struct Fake;
        impl DesktopMenuBar for Fake {
            fn snapshot(&self) -> Result<Value, String> {
                Ok(
                    json!({ "tasks": [{ "workspaceId": "w", "status": "running" }], "metrics": null }),
                )
            }
        }
        let fixture = embedded_fixture().await;
        let (status, _) = get_json(&fixture.app, "/api/v1/menu-bar").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let fake: Arc<dyn DesktopMenuBar> = Arc::new(Fake);
        let _ = fixture.backend.desktop_menu_bar.set(fake);
        let (status, read) = get_json(&fixture.app, "/api/v1/menu-bar").await;
        assert_eq!(status, StatusCode::OK, "{read}");
        assert_eq!(read["tasks"][0]["status"], "running");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn providers_are_turned_off_and_on_through_the_desktop() {
        let fixture = embedded_fixture().await;
        let path = "/api/v1/providers/codex/availability";
        let (status, off) = send_json(&fixture.app, "PUT", path, json!({ "enabled": false })).await;
        assert_eq!(status, StatusCode::OK, "{off}");
        assert_eq!(off["availability"]["state"], "disabled", "{off}");
        let (status, on) = send_json(&fixture.app, "PUT", path, json!({ "enabled": true })).await;
        assert_eq!(status, StatusCode::OK, "{on}");
        assert_eq!(on["availability"]["state"], "enabled", "{on}");
        let (status, _) = send_json(
            &fixture.app,
            "PUT",
            "/api/v1/providers/nope/availability",
            json!({ "enabled": false }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn provider_accounts_are_added_renamed_and_removed_through_the_desktop() {
        let fixture = embedded_fixture().await;
        let (status, created) = send_json(
            &fixture.app,
            "POST",
            "/api/v1/provider-accounts",
            json!({ "providerId": "claude_code", "label": "Zedy" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let id = created["account"]["id"].as_str().unwrap().to_string();
        assert_eq!(created["account"]["label"], "Zedy");
        // The same name twice, or a provider without accounts: refused.
        let (status, _) = send_json(
            &fixture.app,
            "POST",
            "/api/v1/provider-accounts",
            json!({ "providerId": "claude_code", "label": "zedy" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, listed) = get_json(&fixture.app, "/api/v1/provider-accounts").await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(listed["accounts"].as_array().unwrap().len(), 1);
        let path = format!("/api/v1/provider-accounts/{id}");
        let (status, renamed) =
            send_json(&fixture.app, "PUT", &path, json!({ "label": "Trabalho" })).await;
        assert_eq!(status, StatusCode::OK, "{renamed}");
        assert_eq!(renamed["account"]["label"], "Trabalho");
        let (status, _) = send_json(&fixture.app, "DELETE", &path, json!({})).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = send_json(&fixture.app, "DELETE", &path, json!({})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // Cancelling a sign-in that is not waiting is fine.
        let (status, _) = send_json(
            &fixture.app,
            "POST",
            "/api/v1/provider-accounts/sign-in/cancel",
            json!({ "providerId": "claude_code" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn plan_usage_and_antigravity_go_through_the_desktop() {
        let fixture = embedded_fixture().await;
        // A provider without usage is refused before any CLI runs.
        let (status, refused) = send_json(
            &fixture.app,
            "POST",
            "/api/v1/provider-accounts/usage",
            json!({ "providerId": "droid" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            refused
                .to_string()
                .contains("does not support account usage"),
            "{refused}"
        );
        let (status, _) = send_json(
            &fixture.app,
            "POST",
            "/api/v1/provider-accounts/reset",
            json!({ "providerId": "droid" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // Nothing installed or signed in yet.
        let (status, antigravity) = send_json(
            &fixture.app,
            "POST",
            "/api/v1/antigravity/status",
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{antigravity}");
        assert_eq!(antigravity["managedRuntimeInstalled"], false);
        assert_eq!(antigravity["signedIn"], false);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn decision_provider_settings_and_history_go_through_the_desktop() {
        let fixture = embedded_fixture().await;
        let (status, history) =
            get_json(&fixture.app, "/api/v1/decision-provider/history?limit=5").await;
        assert_eq!(status, StatusCode::OK, "{history}");
        assert_eq!(history, json!([]));
        // An invalid mode is refused before anything is written.
        let (status, _) = send_json(
            &fixture.app,
            "PUT",
            "/api/v1/decision-provider/settings",
            json!({ "provider": "disabled", "mode": "nope", "modelRouting": "manual",
                "baseUrl": null, "model": null, "memoryThreshold": null,
                "skillConfidenceThreshold": null, "modelConfidenceThreshold": null,
                "completionThreshold": null, "toolRiskThreshold": null, "apiKey": null }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ai_memory_outbox_history_and_restart_go_through_the_desktop() {
        let fixture = embedded_fixture().await;
        let (status, outbox) = get_json(&fixture.app, "/api/v1/ai-memory/outbox?limit=5").await;
        assert_eq!(status, StatusCode::OK, "{outbox}");
        assert_eq!(outbox, json!([]));
        let (status, history) = get_json(&fixture.app, "/api/v1/ai-memory/export-history").await;
        assert_eq!(status, StatusCode::OK, "{history}");
        assert_eq!(history, json!([]));
        // An invalid mode is refused before anything is written.
        let (status, _) = send_json(
            &fixture.app,
            "PUT",
            "/api/v1/ai-memory/settings",
            json!({ "mode": "nope", "baseUrl": null, "workspace": "w", "project": "p",
                "dataDir": null, "token": null }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // No desktop app around this backend: nothing to restart.
        let (status, _) = send_json(&fixture.app, "POST", "/api/v1/app/restart", json!({})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn forge_logins_are_chosen_and_bound_through_the_desktop() {
        let fixture = embedded_fixture().await;
        let (status, chosen) = post_json(
            &fixture.app,
            "/api/v1/forge/select-login",
            json!({ "provider": "github", "host": "github.com", "login": "ana" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{chosen}");
        let (status, bound) =
            post_json(&fixture.app, "/api/v1/forge/backfill-bindings", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{bound}");
        assert_eq!(bound["bound"], 0);
        // An unknown repository has no login to bind.
        let (status, retried) = post_json(
            &fixture.app,
            "/api/v1/forge/retry-binding",
            json!({ "repositoryId": "missing" }),
        )
        .await;
        assert_eq!(retried["login"], Value::Null, "{status} {retried}");
        assert!(!crate::remote_connector::remote_path_allowed(
            "/api/v1/forge/retry-binding"
        ));
        // A host that is not one is refused before any CLI runs.
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/forge/select-login",
            json!({ "provider": "github", "host": "bad host/x", "login": "ana" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_automation_is_read_saved_and_run_in_the_tasks_checkout() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("checkout");
        let repo = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        git(&repo, &["init", "-b", "main"]);
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-automation".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-27T00:00:00Z".to_string(),
                updated_at: "2026-10-27T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        let root = repo.to_string_lossy().to_string();
        let (status, empty) = post_json(
            &fixture.app,
            "/api/v1/workspaces/automation",
            json!({ "workspaceRoot": root }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{empty}");
        assert_eq!(empty["tasks"], json!([]));
        assert_eq!(empty["configHash"], Value::Null);
        let task = json!({ "id": "check_1", "label": "Echo", "command": "echo ok",
            "kind": "check", "cwd": null, "timeoutSeconds": 60 });
        let (status, saved) = send_json(
            &fixture.app,
            "PUT",
            "/api/v1/workspaces/automation",
            json!({ "workspaceRoot": root, "setupCommand": null, "tasks": [task],
                "beforeMerge": ["check_1"], "beforePush": [],
                "deliveryPolicy": { "minimumApprovals": 1, "requirePipeline": false,
                    "requireResolvedDiscussions": false, "requireCurrentBase": false,
                    "requireBeforeMergeChecks": false },
                "expectedConfigHash": null }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert_eq!(saved["tasks"][0]["id"], "check_1");
        assert!(repo.join(".dcc.toml").is_file());
        let (status, ran) = post_json(
            &fixture.app,
            "/api/v1/workspaces/automation/run",
            json!({ "workspaceRoot": root, "taskIds": ["check_1"],
                "expectedConfigHash": saved["configHash"] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{ran}");
        assert_eq!(ran["report"]["status"], "passed", "{ran}");
        // A folder that is no task's checkout is refused.
        let other = tempfile::tempdir().expect("other");
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/workspaces/automation",
            json!({ "workspaceRoot": other.path().to_string_lossy() }),
        )
        .await;
        assert_ne!(status, StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connectors_are_added_activated_and_removed_through_the_desktop() {
        let fixture = embedded_fixture().await;
        let (status, empty) = get_json(&fixture.app, "/api/v1/mcp/integrations").await;
        assert_eq!(status, StatusCode::OK, "{empty}");
        assert_eq!(empty["integrations"], json!([]));
        let (status, created) = post_json(
            &fixture.app,
            "/api/v1/mcp/integrations",
            json!({ "displayName": "Linear", "transport": { "type": "http",
                "url": "https://mcp.linear.app/mcp" }, "scope": { "type": "global" },
                "credentials": [] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let definition = &created["integration"]["definition"];
        let id = definition["id"].clone();
        let (status, activated) = post_json(
            &fixture.app,
            "/api/v1/mcp/integrations/activate",
            json!({ "definitionId": id,
                "expectedFingerprint": definition["trust"]["currentFingerprint"] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{activated}");
        assert_eq!(activated["integration"]["definition"]["enabled"], true);
        let (status, scoped) = post_json(
            &fixture.app,
            "/api/v1/mcp/scopes",
            json!({ "definitionId": id, "scopes": "read  write" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{scoped}");
        let (status, disabled) = post_json(
            &fixture.app,
            "/api/v1/mcp/integrations/disable",
            json!({ "definitionId": id }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{disabled}");
        assert_eq!(disabled["integration"]["definition"]["enabled"], false);
        // The session's runtime: none reported yet.
        let (status, runtime) = post_json(
            &fixture.app,
            "/api/v1/mcp/runtime",
            json!({ "sessionId": fixture.session_id.0 }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{runtime}");
        assert_eq!(runtime["statuses"], json!([]));
        // No desktop app around this backend: no sign-in to start.
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/mcp/connect/start",
            json!({ "definitionId": id }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, removed) = post_json(
            &fixture.app,
            "/api/v1/mcp/integrations/remove",
            json!({ "definitionId": id, "deleteCredentials": true }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{removed}");
        let (_, after) = get_json(&fixture.app, "/api/v1/mcp/integrations").await;
        assert_eq!(after["integrations"], json!([]));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_connectors_logo_projects_and_tool_policies_are_set_through_the_desktop() {
        let fixture = embedded_fixture().await;
        let (_, created) = post_json(
            &fixture.app,
            "/api/v1/mcp/integrations",
            json!({ "displayName": "Acme", "transport": { "type": "http",
                "url": "https://mcp.acme.dev/mcp" }, "scope": { "type": "global" },
                "credentials": [] }),
        )
        .await;
        let id = created["integration"]["definition"]["id"].clone();
        let logo = "data:image/png;base64,iVBORw0KGgo=";
        let (status, details) = post_json(
            &fixture.app,
            "/api/v1/mcp/logo",
            json!({ "definitionId": id, "logo": logo }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{details}");
        assert_eq!(details["logo"], logo);
        let (status, excluded) = post_json(
            &fixture.app,
            "/api/v1/mcp/project",
            json!({ "definitionId": id, "projectId": "p-1", "enabled": false }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{excluded}");
        assert_eq!(excluded["excludedProjectIds"], json!(["p-1"]));
        let (status, policy) = post_json(
            &fixture.app,
            "/api/v1/mcp/tool-policy",
            json!({ "definitionId": id, "toolName": "search", "decision": "allow" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{policy}");
        let (_, listed) = get_json(&fixture.app, "/api/v1/mcp/integrations").await;
        let record = &listed["integrations"][0];
        assert_eq!(record["logo"], logo);
        assert_eq!(record["toolPolicies"][0]["decision"], "allow");
        let (status, servers) = get_json(&fixture.app, "/api/v1/mcp/native-servers").await;
        assert_eq!(status, StatusCode::OK, "{servers}");
        assert!(servers["servers"].is_array());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn skills_are_saved_compiled_and_deleted_in_the_tasks_checkout() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("checkout");
        let repo = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        git(&repo, &["init", "-b", "main"]);
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-skills".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-27T00:00:00Z".to_string(),
                updated_at: "2026-10-27T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        let root = repo.to_string_lossy().to_string();
        let skill = json!({
            "name": "review-pr", "description": "Reviews a PR", "body": "Read the diff.",
            "targetAgents": ["claude"], "disableModelInvocation": false, "scope": "project",
        });
        let (status, saved) = post_json(
            &fixture.app,
            "/api/v1/skills/save",
            json!({ "projectRoot": root, "workspaceId": "w-skills", "skill": skill }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert!(repo
            .join(".devcommandcenter/skills/review-pr/SKILL.md")
            .exists());
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/skills/compile",
            json!({ "checkoutRoot": root, "workspaceId": "w-skills" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(repo.join(".claude/skills/review-pr/SKILL.md").exists());
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/skills/delete",
            json!({ "projectRoot": root, "workspaceId": "w-skills", "name": "review-pr" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!repo.join(".devcommandcenter/skills/review-pr").exists());
        // An invalid name is refused.
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/skills/save",
            json!({ "projectRoot": root, "workspaceId": "w-skills",
                "skill": { "name": "Bad Name", "description": "x" } }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_task_file_saves_only_when_the_disk_still_matches() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("git dir");
        let repo = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        git(&repo, &["init", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "um\n").unwrap();
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-file".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-25T00:00:00Z".to_string(),
                updated_at: "2026-10-25T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        let root = repo.to_string_lossy().to_string();
        let write = |content: &str, expected: Option<&str>| {
            json!({
                "workspaceRoot": root, "relativePath": "a.txt",
                "content": content, "expectedPrevious": expected,
            })
        };
        let (status, saved) =
            post_json(&fixture.app, "/api/v1/files/write", write("dois\n", Some("um\n"))).await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert_eq!(saved["conflicted"], false);
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "dois\n");
        // The disk moved on (an agent): nothing is written, the disk comes back.
        let (_, conflict) =
            post_json(&fixture.app, "/api/v1/files/write", write("tres\n", Some("um\n"))).await;
        assert_eq!(conflict["conflicted"], true);
        assert_eq!(conflict["diskContent"], "dois\n");
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "dois\n");
        let other = tempfile::tempdir().unwrap();
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/files/write",
            json!({ "workspaceRoot": other.path(), "relativePath": "a.txt", "content": "x", "expectedPrevious": null }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_merge_conflict_is_read_resolved_and_aborted_for_a_task_only() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("git dir");
        let repo = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "first"]);
        git(&repo, &["checkout", "-qb", "feature"]);
        std::fs::write(repo.join("a.txt"), "feature\n").unwrap();
        git(&repo, &["commit", "-qam", "feature"]);
        git(&repo, &["checkout", "-q", "main"]);
        std::fs::write(repo.join("a.txt"), "main\n").unwrap();
        git(&repo, &["commit", "-qam", "main"]);
        git(&repo, &["checkout", "-q", "feature"]);
        let _ = std::process::Command::new("git")
            .args(["merge", "main"])
            .current_dir(&repo)
            .output();
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-merge".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-24T00:00:00Z".to_string(),
                updated_at: "2026-10-24T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        let root = repo.to_string_lossy().to_string();
        let (status, state) = post_json(
            &fixture.app,
            "/api/v1/git/conflict-state",
            json!({ "workspaceRoot": root }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{state}");
        assert_eq!(state["operation"], "merge");
        assert_eq!(state["conflicts"][0]["path"], "a.txt");
        assert_eq!(state["conflicts"][0]["kind"], "both-modified");
        let (status, config) = post_json(
            &fixture.app,
            "/api/v1/git/validation-config",
            json!({ "workspaceRoot": root }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{config}");
        assert_eq!(config["commands"], json!([]));
        // The current side taken: no conflict left.
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/git/accept-conflict",
            json!({ "workspaceRoot": root, "relativePath": "a.txt", "side": "current" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, state) = post_json(
            &fixture.app,
            "/api/v1/git/conflict-state",
            json!({ "workspaceRoot": root }),
        )
        .await;
        assert_eq!(state["conflicts"], json!([]));
        // Marking a path that is no conflict is refused; the merge aborts.
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/git/mark-conflict-resolved",
            json!({ "workspaceRoot": root, "relativePath": "a.txt" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/git/abort-merge",
            json!({ "workspaceRoot": root }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "feature\n");
        // A folder that is no task's checkout is refused.
        let other = tempfile::tempdir().unwrap();
        for (uri, body) in [
            ("/api/v1/git/conflict-state", json!({ "workspaceRoot": other.path() })),
            ("/api/v1/git/validation-config", json!({ "workspaceRoot": other.path() })),
            ("/api/v1/git/abort-merge", json!({ "workspaceRoot": other.path() })),
            (
                "/api/v1/git/accept-conflict",
                json!({ "workspaceRoot": other.path(), "relativePath": "a.txt", "side": "current" }),
            ),
            (
                "/api/v1/git/mark-conflict-resolved",
                json!({ "workspaceRoot": other.path(), "relativePath": "a.txt" }),
            ),
            (
                "/api/v1/git/complete-merge",
                json!({ "workspaceRoot": other.path(), "validationCommands": [] }),
            ),
        ] {
            let (status, _) = post_json(&fixture.app, uri, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_review_reads_its_forge_and_delivery_failure_for_a_task_only() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("git dir");
        let repo = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        git(&repo, &["init", "-b", "main"]);
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-forge".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-24T00:00:00Z".to_string(),
                updated_at: "2026-10-24T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        let root = repo.to_string_lossy().to_string();
        // No remote: no forge, and no failure captured.
        let (status, context) =
            post_json(&fixture.app, "/api/v1/forge/context", json!({ "workspaceRoot": root })).await;
        assert_eq!(status, StatusCode::OK, "{context}");
        assert_eq!(context["provider"], Value::Null);
        let (status, failure) = post_json(
            &fixture.app,
            "/api/v1/git/delivery-failure",
            json!({ "workspaceRoot": root }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{failure}");
        assert_eq!(failure["snapshot"], Value::Null);
        // A folder that is no task's checkout is refused.
        let other = tempfile::tempdir().unwrap();
        for uri in [
            "/api/v1/forge/context",
            "/api/v1/forge/change-request-context",
            "/api/v1/forge/review-state",
            "/api/v1/git/delivery-failure",
            "/api/v1/forge/review-comments",
            "/api/v1/forge/pipeline",
            "/api/v1/delegations/d-1/apply",
            "/api/v1/delegations/d-1/discard",
        ] {
            let (status, _) =
                post_json(&fixture.app, uri, json!({ "workspaceRoot": other.path() })).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        }
        // Replying to and resolving a conversation, too.
        let thread = json!({
            "repositoryRoot": other.path(),
            "number": 1,
            "commentId": "1",
            "threadId": "t-1",
            "body": "ok",
            "resolved": true,
        });
        for uri in ["/api/v1/forge/reply-thread", "/api/v1/forge/resolve-thread"] {
            let (status, _) = post_json(&fixture.app, uri, thread.clone()).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        }
        // A delegation that doesn't exist is not found, on the task's own checkout.
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/delegations/missing/apply",
            json!({ "workspaceRoot": root }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/delegations/missing/cancel",
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/delegations/missing/approve",
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ai_memory_reads_a_sessions_sources_and_keeps_the_curation() {
        let fixture = embedded_fixture().await;
        let get = |uri: &'static str| call(&fixture.app, "GET", uri, None, None, true);
        // Nothing exported waits, nothing was recovered, no sidecar here.
        let (status, waiting) = get("/api/v1/sessions/s-none/ai-memory/export-status").await;
        assert_eq!(status, StatusCode::OK, "{waiting}");
        assert_eq!(waiting, Value::Null);
        let (status, sources) = get("/api/v1/sessions/s-none/ai-memory/sources").await;
        assert_eq!(status, StatusCode::OK, "{sources}");
        assert_eq!(sources, json!([]));
        let (status, sidecar) = get("/api/v1/ai-memory/sidecar").await;
        assert_eq!(status, StatusCode::OK, "{sidecar}");
        assert_eq!(sidecar["mode"], "disabled");
        // A pin is kept, then cleared.
        let (status, saved) = post_json(
            &fixture.app,
            "/api/v1/ai-memory/source-actions",
            json!({ "sourceKey": "docs/a.md|A|um", "action": "pinned", "correction": null }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        let (_, actions) = get("/api/v1/ai-memory/source-actions").await;
        assert_eq!(actions[0]["sourceKey"], "docs/a.md|A|um");
        assert_eq!(actions[0]["action"], "pinned");
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/ai-memory/source-actions",
            json!({ "sourceKey": "docs/a.md|A|um", "action": "cleared", "correction": null }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, actions) = get("/api/v1/ai-memory/source-actions").await;
        assert_eq!(actions, json!([]));
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/ai-memory/source-actions",
            json!({ "sourceKey": "x", "action": "deleted", "correction": null }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_session_binds_to_a_live_agent_only() {
        let fixture = embedded_fixture().await;
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/sessions/s-1/agent",
            json!({ "agentId": "nobody" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        // The built-in reviewer, seeded as the desktop does on its first list.
        let agents = fixture
            .backend
            .sessions
            .list_resident_agents(&Default::default())
            .unwrap();
        let reviewer = agents
            .iter()
            .find(|agent| agent.preset.as_deref() == Some("reviewer"))
            .unwrap();
        let (status, body) = post_json(
            &fixture.app,
            "/api/v1/sessions/http-respond/agent",
            json!({ "agentId": reviewer.id }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let bound = fixture
            .backend
            .sessions
            .list_agent_session_bindings()
            .unwrap();
        assert!(bound.iter().any(|binding| {
            binding.session_id == fixture.session_id.0 && binding.agent_id == reviewer.id
        }));
        // The page saves the person's changes; the role stays DCC's.
        let (status, saved) = post_json(
            &fixture.app,
            &format!("/api/v1/agents/{}", reviewer.id),
            json!({
                "name": "Crítico",
                "role": reviewer.role,
                "kickoffPrompt": reviewer.kickoff_prompt,
                "offerPrompt": "",
                "extraInstructions": "Seja breve.",
                "providerId": "codex",
                "model": null,
                "effort": "high",
                "avatar": { "color": "pink", "arms": 5, "eyes": "visor" },
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert_eq!(saved["agent"]["name"], "Crítico");
        assert_eq!(saved["agent"]["extraInstructions"], "Seja breve.");
        assert_eq!(saved["agent"]["avatar"]["color"], "pink");
        assert!(!crate::remote_connector::remote_path_allowed(&format!(
            "/api/v1/agents/{}",
            reviewer.id
        )));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_agents_memory_is_edited_from_its_page() {
        let fixture = embedded_fixture().await;
        let reviewer = fixture
            .backend
            .sessions
            .list_resident_agents(&Default::default())
            .unwrap()
            .remove(0);
        let base = format!("/api/v1/agents/{}/memory", reviewer.id);
        let (status, empty) = get_json(&fixture.app, &base).await;
        assert_eq!(status, StatusCode::OK, "{empty}");
        assert_eq!(empty["notes"], json!([]));
        assert_eq!(empty["usage"], 0);

        let (status, added) =
            post_json(&fixture.app, &base, json!({ "body": "Responder em pt-BR" })).await;
        assert_eq!(status, StatusCode::OK, "{added}");
        assert_eq!(added["notes"][0]["source"], "person");
        let note = added["notes"][0]["id"].as_str().unwrap().to_string();
        let (status, edited) = post_json(
            &fixture.app,
            &format!("{base}/{note}"),
            json!({ "body": "Sempre em pt-BR" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{edited}");
        assert_eq!(edited["notes"][0]["body"], "Sempre em pt-BR");
        let (status, removed) =
            send_json(&fixture.app, "DELETE", &format!("{base}/{note}"), json!({})).await;
        assert_eq!(status, StatusCode::OK, "{removed}");
        assert_eq!(removed["notes"], json!([]));
        // Newest first: removed, edited, added. Back to the added one.
        assert_eq!(removed["versions"][2]["change"], "added");
        let version = removed["versions"][2]["id"].as_i64().unwrap();
        let (status, restored) = post_json(
            &fixture.app,
            &format!("{base}/versions/{version}/restore"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{restored}");
        assert_eq!(restored["notes"][0]["body"], "Responder em pt-BR");
        assert_eq!(restored["versions"][0]["change"], "restored");

        let (status, refused) =
            post_json(&fixture.app, &base, json!({ "body": "x".repeat(301) })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert!(!crate::remote_connector::remote_path_allowed(&base));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_agents_conversation_lives_in_its_home() {
        let fixture = embedded_fixture().await;
        let reviewer = fixture
            .backend
            .sessions
            .list_resident_agents(&Default::default())
            .unwrap()
            .remove(0);
        let path = format!("/api/v1/agents/{}/conversation", reviewer.id);
        let (status, home) = post_json(&fixture.app, &path, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{home}");
        assert_eq!(home["projectId"], format!("dcc-agent-{}", reviewer.id));
        let root = home["rootPath"].as_str().unwrap();
        assert!(
            root.ends_with(&format!("agent-homes/{}", reviewer.id)),
            "{root}"
        );
        let (status, again) = post_json(&fixture.app, &path, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{again}");
        assert_eq!(again["rootPath"], home["rootPath"]);
        let (status, missing) = post_json(
            &fixture.app,
            "/api/v1/agents/nobody/conversation",
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{missing}");
        assert!(!crate::remote_connector::remote_path_allowed(&path));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_agent_past_its_monthly_limit_waits_for_the_person() {
        let fixture = embedded_fixture().await;
        let mut draft = dcc_core::domain::agent::ResidentAgentDraft::reviewer(&Default::default());
        draft.name = "QA".into();
        draft.role = "Teste.".into();
        draft.monthly_token_limit = Some(1_000);
        let qa = fixture
            .backend
            .sessions
            .save_resident_agent(None, draft)
            .unwrap();
        let session = fixture.session_id.0.clone();
        let (status, body) = post_json(
            &fixture.app,
            &format!("/api/v1/sessions/{session}/agent"),
            json!({ "agentId": qa.id }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let db = fixture.backend.workspace.db_path.clone();
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "INSERT INTO dcc_turn_model_usage (session_id, turn_id, model, total_tokens, recorded_at) VALUES (?1, 't-1', 'm', 5000, ?2)",
                rusqlite::params![
                    session,
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                ],
            )
            .unwrap();
        }
        let (status, refused) = post_json(
            &fixture.app,
            &format!("/api/v1/sessions/{session}/turns"),
            json!({ "sessionId": session, "prompt": "oi" }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(
            refused.to_string().contains("agentLimitReached"),
            "{refused}"
        );
        let path = format!("/api/v1/agents/{}/limit-override", qa.id);
        let (status, body) = post_json(&fixture.app, &path, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(fixture
            .backend
            .sessions
            .agent_over_limit(&SessionId(session.clone()))
            .unwrap()
            .is_none());
        assert!(!crate::remote_connector::remote_path_allowed(&path));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_phone_reads_the_team_room_and_writes_to_it_once() {
        let fixture = embedded_fixture().await;
        // The Reviewer, seeded as the desktop does on its first list.
        fixture
            .backend
            .sessions
            .list_resident_agents(&Default::default())
            .unwrap();
        let (status, room) = get_json(&fixture.app, "/api/v1/mobile/room").await;
        assert_eq!(status, StatusCode::OK, "{room}");
        assert_eq!(room["lines"], json!([]));
        let snapshot = room["snapshotId"].as_str().unwrap().to_string();
        let (status, same) = get_json(
            &fixture.app,
            &format!("/api/v1/mobile/room?after={snapshot}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{same}");
        assert_eq!(same["unchanged"], true);
        // The Reviewer takes it, but its conversation starts on the desktop.
        let (status, refused) = post_json(
            &fixture.app,
            "/api/v1/mobile/room/messages",
            json!({ "text": "o CI falhou" }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(
            refused.to_string().contains("roomNoConversation"),
            "{refused}"
        );
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/mobile/room/messages",
            json!({ "text": "  " }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(crate::remote_connector::remote_path_allowed(
            "/api/v1/mobile/room?after=x"
        ));
        assert!(crate::remote_connector::remote_path_allowed(
            "/api/v1/mobile/room/messages"
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_last_turn_review_and_guarded_undo_answer_without_a_capture() {
        let fixture = embedded_fixture().await;
        // No turn captured for the session: no review.
        let (status, review) = post_json(
            &fixture.app,
            "/api/v1/sessions/s-none/last-turn-review",
            json!({ "workspaceId": "w-none" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{review}");
        assert_eq!(review, Value::Null);
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/sessions/s-none/last-turn-review",
            json!({ "workspaceId": " " }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/turn-review/file-diff",
            json!({ "snapshotId": "missing", "path": "a.rs" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = post_json(
            &fixture.app,
            "/api/v1/guarded-undo/prepare",
            json!({ "snapshotId": "" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // A token no preparation gave runs nothing.
        let (status, outcome) = post_json(
            &fixture.app,
            "/api/v1/guarded-undo/execute",
            json!({ "previewToken": "not-a-token" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{outcome}");
        assert_eq!(outcome["status"], "blocked");
        assert_eq!(outcome["reasonCode"], "preview_consumed");
        // "Editar a partir daqui" needs a session it knows.
        for body in [
            json!({ "sessionId": "", "anchorTurnId": "t1" }),
            json!({ "sessionId": "missing", "anchorTurnId": "t1" }),
        ] {
            let (status, _) =
                post_json(&fixture.app, "/api/v1/conversation-rewind/prepare", body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_review_header_previews_commits_pushes_and_syncs_with_the_base() {
        use dcc_core::domain::workspace::{Workspace, WorkspaceState};
        let fixture = embedded_fixture().await;
        let base = tempfile::tempdir().expect("git dirs");
        let base = std::fs::canonicalize(base.path()).unwrap().to_path_buf();
        let remote = base.join("remote.git");
        let repo = base.join("repo");
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
        git(&repo, &["switch", "-c", "dcc/revisao"]);
        git(&repo, &["push", "-u", "origin", "dcc/revisao"]);
        let workspaces =
            dcc_infra::db::SqliteWorkspaceRepo::open(&fixture.backend.workspace.db_path).unwrap();
        dcc_core::ports::WorkspaceRepo::save_workspace(
            &workspaces,
            &Workspace {
                id: WorkspaceId("w-review".to_string()),
                project_id: ProjectId("project".to_string()),
                name: None,
                root_path: repo.to_string_lossy().to_string(),
                base_branch: "main".to_string(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                snoozed_until: None,
                created_at: "2026-10-24T00:00:00Z".to_string(),
                updated_at: "2026-10-24T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/guia.md"), "um\n").unwrap();
        let action = |name: &str| format!("/api/v1/workspaces/w-review/delivery/{name}");

        // The conversation header's "Criar PR": the summary's token guards
        // the local changes, the context names the route and the title.
        let (status, summary) = call(
            &fixture.app,
            "GET",
            "/api/v1/workspaces/w-review/delivery",
            None,
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{summary}");
        assert_eq!(summary["changes"].as_array().unwrap().len(), 1);
        assert!(!summary["changesToken"].as_str().unwrap().is_empty());
        let (status, context) = post_json(
            &fixture.app,
            "/api/v1/forge/change-request-context",
            json!({ "workspaceRoot": repo.to_string_lossy() }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{context}");
        assert_eq!(context["headBranch"], "dcc/revisao");
        assert_eq!(context["baseBranch"], "main");

        // The preview stages everything and reads the deterministic message.
        let (status, preview) = post_json(&fixture.app, &action("prepare-commit"), json!({})).await;
        assert_eq!(status, StatusCode::OK, "{preview}");
        assert_eq!(preview["stagedFileCount"], 1);
        assert!(!preview["subject"].as_str().unwrap().is_empty());
        assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]).trim(), "docs/guia.md");

        // The ✨ asks the task's agent; a task without a conversation has none.
        let (status, draft) = post_json(
            &fixture.app,
            &action("draft"),
            json!({ "field": "commitSubject" }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{draft}");
        assert!(draft.to_string().contains("ainda não tem conversa"), "{draft}");

        // The person's edited message is what lands, pushed.
        let (status, body) = post_json(
            &fixture.app,
            &action("commit-reviewed"),
            json!({
                "message": "docs: add the guide",
                "body": "Written by hand.",
                "stagedFingerprint": preview["stagedFingerprint"],
                "push": true,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            git(&remote, &["log", "-1", "--format=%s%n%b", "dcc/revisao"]).trim(),
            "docs: add the guide\nWritten by hand."
        );

        // The base moved: syncing merges it in.
        git(&repo, &["switch", "main"]);
        std::fs::write(repo.join("base.txt"), "nova\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-m", "base"]);
        git(&repo, &["push", "origin", "main"]);
        git(&repo, &["switch", "dcc/revisao"]);
        let (status, synced) =
            post_json(&fixture.app, &action("sync-base"), json!({ "baseBranch": "main" })).await;
        assert_eq!(status, StatusCode::OK, "{synced}");
        assert_eq!(synced["updated"], true);
        assert!(repo.join("base.txt").exists());

        // Nothing left to commit: the preview says so.
        let (status, _) = post_json(&fixture.app, &action("prepare-commit"), json!({})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
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
