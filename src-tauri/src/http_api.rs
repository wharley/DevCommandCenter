use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
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
        abort_run, close_session, restore_session, resume_session, send_turn, start_thread,
        AbortRunInput, CloseSessionInput, RestoreSessionInput, ResumeSessionInput, SendTurnInput,
        StartThreadInput,
    },
    domain::session::SessionId,
    domain::workspace::WorkspaceId,
    ports::{
        provider::ProviderPermissionResponse, provider::ProviderUserInputResponse, CoreEvent,
        EventBus, Input, ProviderTurnInput, SessionEventRepo,
    },
};
use dcc_infra::db::SqliteSessionRepo;
use dcc_tauri::{
    commands::session_commands::{
        RespondToPermissionRequestInput, RespondToPermissionRequestOutput, RespondToUserInputInput,
        RespondToUserInputOutput,
    },
    state::SessionCommandState,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use tokio::sync::RwLock;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::daemon_client::{default_app_data_dir, rpc_with_db_path_timeout};
use crate::http_auth::auth_middleware;
use crate::http_config::{HttpAuthMode, HttpConfig};
use crate::http_rpc_handler::handle_rpc as handle_json_rpc;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
static HEADLESS_SESSION_STATES: OnceLock<Mutex<HashMap<PathBuf, Arc<SessionCommandState>>>> =
    OnceLock::new();
static HEADLESS_EVENT_SENDERS: OnceLock<
    Mutex<HashMap<PathBuf, tokio::sync::broadcast::Sender<CoreEvent>>>,
> = OnceLock::new();

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
    #[serde(default)]
    query: String,
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

#[derive(Clone)]
struct HeadlessEventBus {
    sender: tokio::sync::broadcast::Sender<CoreEvent>,
}

#[async_trait]
impl EventBus for HeadlessEventBus {
    async fn publish(&self, event: CoreEvent) -> dcc_core::Result<()> {
        let _ = self.sender.send(event);
        Ok(())
    }
}

#[derive(Debug)]
enum HttpApiError {
    BadRequest(String),
    NotFound(String),
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
        let body = Json(json!({
            "ok": false,
            "error": {
                "code": self.code(),
                "message": self.message(),
            }
        }));
        (status, body).into_response()
    }
}

pub fn build_router(config: Arc<RwLock<HttpConfig>>) -> Router {
    let cors_config = config.blocking_read().clone();
    let protected_routes = Router::new()
        .route("/rpc", post(handle_json_rpc))
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
        .with_state(config)
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

async fn headless_session_state(
    config: Arc<RwLock<HttpConfig>>,
) -> Result<Arc<SessionCommandState>, HttpApiError> {
    let db_path = {
        let config = config.read().await;
        config.db_path.clone()
    };

    let cache = HEADLESS_SESSION_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().map_err(|error| {
        HttpApiError::internal(format!("session runtime cache poisoned: {error}"))
    })?;
    if let Some(state) = guard.get(&db_path).cloned() {
        return Ok(state);
    }

    let sender = headless_event_sender_for_db(&db_path)?;
    let state = Arc::new(SessionCommandState::new_with_event_bus(
        db_path.clone(),
        default_app_data_dir(),
        Arc::new(HeadlessEventBus { sender }),
    ));
    guard.insert(db_path, state.clone());
    Ok(state)
}

fn headless_event_sender_for_db(
    db_path: &PathBuf,
) -> Result<tokio::sync::broadcast::Sender<CoreEvent>, HttpApiError> {
    let cache = HEADLESS_EVENT_SENDERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .map_err(|error| HttpApiError::internal(format!("event sender cache poisoned: {error}")))?;
    if let Some(sender) = guard.get(db_path).cloned() {
        return Ok(sender);
    }
    let (sender, _) = tokio::sync::broadcast::channel(256);
    guard.insert(db_path.clone(), sender.clone());
    Ok(sender)
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
            "GET /api/v1/events/stream",
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
    let database = {
        let config = config.read().await;
        config.db_path.to_string_lossy().to_string()
    };

    match rpc_value(config.clone(), "daemon.health", Value::Null, HEALTH_TIMEOUT).await {
        Ok(payload) => (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "daemon": "connected",
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

async fn events_stream_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    HttpApiError,
> {
    let db_path = {
        let config = config.read().await;
        config.db_path.clone()
    };
    let sender = headless_event_sender_for_db(&db_path)?;
    let stream = BroadcastStream::new(sender.subscribe()).filter_map(|message| match message {
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
            "description": "REST facade over the DCC daemon RPC store."
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
    let payload = rpc_value(
        config,
        "combs.list",
        json!({ "projectId": query.project_id }),
        DEFAULT_TIMEOUT,
    )
    .await?;
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
    let payload = rpc_value(
        config,
        "diffs.bundle",
        json!({
            "worktreePaths": body.worktree_paths,
            "combIds": body.comb_ids,
        }),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Json(payload))
}

async fn start_thread_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Json(input): Json<StartThreadInput>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config).await?;
    let output = start_thread(&*state, &*state, &*state, &*state, input)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    if let Err(error) = state.attach_provider_session(&output.session).await {
        eprintln!("[DCC HTTP] provider session attach failed: {error}");
    }
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
    let payload = session_repo_operation(config, move |repo| {
        let items = repo
            .search_sessions(&query.query, query.limit)
            .map_err(|error| error.to_string())?;
        serde_json::to_value(items).map_err(|error| error.to_string())
    })
    .await?;
    Ok(Json(payload))
}

async fn send_turn_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<SendTurnInput>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config).await?;
    input.session_id = SessionId(session_id);

    if let Some(session) = state
        .peek_session(&input.session_id)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?
    {
        if dcc_core::application::send_turn_selection_differs_from_session(&session, &input) {
            let _ = state.cancel_provider_session(&input.session_id).await;
        }
    }

    let provider_turn_input = ProviderTurnInput {
        prompt: input.prompt.clone(),
        plan_mode: input.plan_mode,
        effort: input.effort.clone(),
        fast_mode: input.fast_mode,
    };
    let output = send_turn(&*state, &*state, &*state, input)
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    let turn_id = output.turn.id.clone();

    if let Err(error) = state.attach_provider_session(&output.session).await {
        let _ = state
            .emit_turn_aborted(&output.session.id, &turn_id, Some(error.to_string()))
            .await;
        return Err(classify_session_error(error.to_string()));
    }

    if let Err(error) = state
        .set_active_turn(&output.session.id, Some(turn_id.0.clone()))
        .await
    {
        let _ = state
            .emit_turn_aborted(&output.session.id, &turn_id, Some(error.to_string()))
            .await;
        return Err(classify_session_error(error.to_string()));
    }

    if let Err(error) = state
        .send_provider_input(&output.session.id, Input::Turn(provider_turn_input))
        .await
    {
        let _ = state.set_active_turn(&output.session.id, None).await;
        let _ = state
            .emit_turn_aborted(&output.session.id, &turn_id, Some(error.to_string()))
            .await;
        return Err(classify_session_error(error.to_string()));
    }

    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn list_session_events_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let payload = session_repo_operation(config, move |repo| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let items = runtime
            .block_on(SessionEventRepo::list_events_by_session(
                &repo,
                &SessionId(session_id),
            ))
            .map_err(|error| error.to_string())?;
        serde_json::to_value(items).map_err(|error| error.to_string())
    })
    .await?;
    Ok(Json(payload))
}

async fn abort_session_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config).await?;
    let output = abort_run(
        &*state,
        &*state,
        &*state,
        AbortRunInput {
            session_id: SessionId(session_id),
            reason: Some("Stopped from remote HTTP".to_string()),
        },
    )
    .await
    .map_err(|error| classify_session_error(error.to_string()))?;
    let _ = state.cancel_provider_session(&output.session.id).await;
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn resume_session_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config).await?;
    let output = resume_session(
        &*state,
        &*state,
        &*state,
        ResumeSessionInput {
            session_id: SessionId(session_id),
        },
    )
    .await
    .map_err(|error| classify_session_error(error.to_string()))?;
    if let Err(error) = state.attach_provider_session(&output.session).await {
        eprintln!("[DCC HTTP] provider session attach failed: {error}");
    }
    Ok(Json(serde_json::to_value(output).map_err(|error| {
        HttpApiError::internal(error.to_string())
    })?))
}

async fn close_session_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
    body: Option<Json<CloseSessionRequest>>,
) -> Result<Json<Value>, HttpApiError> {
    let delete_history = body.map(|Json(body)| body.delete_history).unwrap_or(false);
    let state = headless_session_state(config).await?;
    let _ = state
        .cancel_provider_session(&SessionId(session_id.clone()))
        .await;
    let output = close_session(
        &*state,
        &*state,
        &*state,
        CloseSessionInput {
            session_id: SessionId(session_id),
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
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config).await?;
    let output = restore_session(
        &*state,
        &*state,
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

async fn respond_to_user_input_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<RespondToUserInputInput>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config).await?;
    input.session_id = session_id;
    state
        .send_provider_input(
            &SessionId(input.session_id),
            Input::UserInputResponse(ProviderUserInputResponse {
                request_id: input.request_id,
                answers: input.answers,
            }),
        )
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(
        serde_json::to_value(RespondToUserInputOutput { ok: true })
            .map_err(|error| HttpApiError::internal(error.to_string()))?,
    ))
}

async fn respond_to_permission_request_handler(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    Path(session_id): Path<String>,
    Json(mut input): Json<RespondToPermissionRequestInput>,
) -> Result<Json<Value>, HttpApiError> {
    let state = headless_session_state(config).await?;
    input.session_id = session_id;
    state
        .send_provider_input(
            &SessionId(input.session_id),
            Input::PermissionResponse(ProviderPermissionResponse {
                request_id: input.request_id,
                behavior: input.behavior,
            }),
        )
        .await
        .map_err(|error| classify_session_error(error.to_string()))?;
    Ok(Json(
        serde_json::to_value(RespondToPermissionRequestOutput { ok: true })
            .map_err(|error| HttpApiError::internal(error.to_string()))?,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
