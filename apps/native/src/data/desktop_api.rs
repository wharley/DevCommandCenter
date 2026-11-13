//! Actions on a running agent go through the desktop app: it holds the
//! agent's process, and a waiting permission or question lives only in that
//! process's memory. The desktop serves the same loopback API the phone's
//! commands run through (`127.0.0.1:9876`, `X-API-Key` from
//! `~/.dcc/http-config.json`); the native app calls it like any local client.
//! Blocking: call it off the UI thread.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use dcc_core::domain::provider::ProviderDescriptor;
use dcc_core::domain::session::QueuedTurn;
use dcc_core::ports::provider::{ProviderRuntimeConfig, ProviderUserInputAnswer};
use serde_json::{Value, json};

const DEFAULT_PORT: u16 = 9876;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(800);
const IO_TIMEOUT: Duration = Duration::from_secs(20);
/// Commits and pushes run the repository's hooks (`lint-staged`, a lint on
/// pre-push), which outlast `IO_TIMEOUT`; the desktop bounds its own git
/// calls, so the app waits for its answer instead of reporting a failure
/// while the push goes on.
const DELIVERY_WAIT: Duration = Duration::from_secs(300);
/// Starting a turn may first start the provider (its process, its MCP
/// servers and their sign-in check), which outlasts `IO_TIMEOUT` on a cold
/// start; giving up early drops the request and the turn never starts.
const TURN_START_WAIT: Duration = Duration::from_secs(120);

/// The changes list's git actions (`workspaceGitStageFile`…), run by the
/// desktop so they wait for an agent's edit to the same checkout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitAction {
    Stage,
    Unstage,
    Discard,
    StageAll,
}

impl GitAction {
    fn route(self) -> &'static str {
        match self {
            Self::Stage => "/api/v1/git/stage-file",
            Self::Unstage => "/api/v1/git/unstage-file",
            Self::Discard => "/api/v1/git/discard-file",
            Self::StageAll => "/api/v1/git/stage-all",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiError {
    /// Nothing answers: the desktop is closed (or its local API is off).
    DesktopClosed,
    /// Someone answered first (the desktop, the phone).
    AlreadyAnswered,
    /// The local key did not match.
    Unauthorized,
    /// A 409 with a reason of its own (`steer_window_closed`).
    Refused {
        code: String,
        message: String,
        /// The outcome's `target` (`behind_remote`: the branch that moved).
        target: Option<String>,
        /// The outcome's `report` (`checks_failed`: what each check printed).
        report: Option<Box<dcc_core::domain::merge_conflict::WorkspaceGitValidationReport>>,
    },
    /// The answer did not come within the wait (macOS reports it as
    /// "Resource temporarily unavailable").
    TimedOut,
    Failed(String),
}

impl ApiError {
    /// What the person reads: the cases with a sentence of their own, the
    /// server's message otherwise.
    pub fn text(&self, cx: &gpui::App) -> String {
        use crate::i18n::Translate;
        match self {
            Self::DesktopClosed => cx.t("native.desktopClosed").to_string(),
            Self::TimedOut => cx.t("native.timedOut").to_string(),
            error => error.to_string(),
        }
    }
}

/// A read past its timeout is `WouldBlock` on macOS, `TimedOut` elsewhere.
fn read_error(error: std::io::Error) -> ApiError {
    match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => ApiError::TimedOut,
        _ => ApiError::Failed(error.to_string()),
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DesktopClosed => write!(f, "the desktop app is not running"),
            Self::AlreadyAnswered => write!(f, "already answered"),
            Self::Unauthorized => write!(f, "the desktop refused the local key"),
            Self::Refused { message, .. } => write!(f, "{message}"),
            Self::TimedOut => write!(f, "the request took too long"),
            Self::Failed(message) => write!(f, "{message}"),
        }
    }
}

/// How the turn runs, as the composer chose it; `None` leaves the session's.
/// A reply on a pull request conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadReply {
    pub root: String,
    pub number: u32,
    /// The conversation's root comment (GitHub replies to it).
    pub comment_id: String,
    pub thread_id: String,
    pub body: String,
    pub login: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnOptions {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `read_only` | `ask` | `auto` | `full_access`
    pub approval_policy: Option<String>,
    pub plan_mode: bool,
    pub fast_mode: bool,
    /// Background instructions (delegation, a provider handoff).
    pub tool_instructions: Option<String>,
    /// Settings → Providers → Advanced for the turn's provider; empty means
    /// "cleared" (the backend keeps the conversation's account).
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

impl TurnOptions {
    fn write_into(&self, body: &mut Value) {
        let Some(map) = body.as_object_mut() else {
            return;
        };
        let mut set = |key: &str, value: Option<&String>| {
            if let Some(value) = value {
                map.insert(key.into(), Value::String(value.clone()));
            }
        };
        set("providerId", self.provider_id.as_ref());
        set("model", self.model.as_ref());
        set("effort", self.effort.as_ref());
        set("approvalPolicy", self.approval_policy.as_ref());
        set("toolInstructions", self.tool_instructions.as_ref());
        if let Some(runtime) = &self.provider_runtime {
            map.insert("providerRuntime".into(), json!(runtime));
        }
        map.insert("planMode".into(), Value::Bool(self.plan_mode));
        map.insert("fastMode".into(), Value::Bool(self.fast_mode));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopApi {
    port: u16,
    api_key: String,
    /// The machine whose API this is, when it is not this computer's: its
    /// calls go to `/api/v1/machines/<id>/proxy<path>`, which this
    /// computer's server sends through the machine's channel.
    machine: Option<String>,
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".dcc").join("http-config.json"))
}

impl DesktopApi {
    /// The desktop's local endpoint: `DCC_HTTP_PORT` / `DCC_HTTP_API_KEY`
    /// first, like the desktop, then its `http-config.json`.
    pub fn load() -> Option<Self> {
        let file: Value = config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        if file.get("enabled").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let port = std::env::var("DCC_HTTP_PORT")
            .ok()
            .and_then(|port| port.parse().ok())
            .or_else(|| {
                file.get("port")
                    .and_then(Value::as_u64)
                    .and_then(|port| u16::try_from(port).ok())
            })
            .unwrap_or(DEFAULT_PORT);
        let api_key = std::env::var("DCC_HTTP_API_KEY")
            .ok()
            .or_else(|| {
                file.get("api_key")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .filter(|key| !key.is_empty())?;
        Some(Self {
            port,
            api_key,
            machine: None,
        })
    }

    pub fn new(port: u16, api_key: impl Into<String>) -> Self {
        Self {
            port,
            api_key: api_key.into(),
            machine: None,
        }
    }

    /// The same API, of a paired machine instead of this computer.
    pub fn on_machine(&self, machine_id: &str) -> Self {
        Self {
            machine: Some(machine_id.to_string()),
            ..self.clone()
        }
    }

    /// The machine this API is of; none for this computer.
    pub fn machine(&self) -> Option<&str> {
        self.machine.as_deref()
    }

    /// Where a call goes: the path itself, or through the machine's proxy.
    fn routed(&self, path: &str) -> String {
        match &self.machine {
            Some(id) => format!("/api/v1/machines/{}/proxy{path}", encode(id)),
            None => path.to_string(),
        }
    }

    pub(crate) fn post(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        self.request("POST", path, Some(body))
    }

    pub(crate) fn get(&self, path: &str) -> Result<Value, ApiError> {
        self.request("GET", path, None)
    }

    pub(crate) fn put(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        self.request("PUT", path, Some(body))
    }

    pub(crate) fn delete(&self, path: &str) -> Result<Value, ApiError> {
        self.request("DELETE", path, None)
    }

    /// A long-lived `GET` (server-sent events): the response's body, past
    /// its head, and whether it comes chunked. `read_timeout` bounds the
    /// wait between two reads (the server's keep-alive is every 15s).
    pub(crate) fn open_stream(
        &self,
        path: &str,
        read_timeout: Duration,
    ) -> Result<(std::io::BufReader<TcpStream>, bool), ApiError> {
        use std::io::BufRead;
        if self.machine.is_some() {
            // A machine's streams come as subscriptions on its channel,
            // which the proxy does not carry yet.
            return Err(ApiError::Failed(
                "streams of a remote machine are not read here".into(),
            ));
        }
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
            .map_err(|_| ApiError::DesktopClosed)?;
        stream.set_read_timeout(Some(read_timeout)).ok();
        stream.set_write_timeout(Some(IO_TIMEOUT)).ok();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-API-Key: {key}\r\nAccept: text/event-stream\r\nConnection: close\r\n\r\n",
            port = self.port,
            key = self.api_key,
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|error| ApiError::Failed(error.to_string()))?;
        let mut reader = std::io::BufReader::new(stream);
        let mut status = 0u16;
        let mut chunked = false;
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| ApiError::Failed(error.to_string()))?;
            if read == 0 {
                return Err(ApiError::Failed("invalid response".into()));
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if status == 0 {
                status = trimmed
                    .split_whitespace()
                    .nth(1)
                    .and_then(|code| code.parse().ok())
                    .ok_or_else(|| ApiError::Failed("invalid response".into()))?;
            } else {
                let lower = trimmed.to_ascii_lowercase();
                if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
                    chunked = true;
                }
            }
        }
        match status {
            200..=299 => Ok((reader, chunked)),
            401 | 403 => Err(ApiError::Unauthorized),
            _ => Err(ApiError::Failed(format!("HTTP {status}"))),
        }
    }

    fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, ApiError> {
        self.request_waiting(method, path, body, IO_TIMEOUT)
    }

    /// A request whose answer may take `wait` (a sign-in waiting on the
    /// browser).
    pub(crate) fn request_waiting(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        wait: Duration,
    ) -> Result<Value, ApiError> {
        let path = &self.routed(path);
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
            .map_err(|_| ApiError::DesktopClosed)?;
        stream.set_read_timeout(Some(wait)).ok();
        stream.set_write_timeout(Some(IO_TIMEOUT)).ok();
        let payload = body.map(Value::to_string).unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-API-Key: {key}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{payload}",
            port = self.port,
            key = self.api_key,
            length = payload.len(),
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|error| ApiError::Failed(error.to_string()))?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(read_error)?;
        let (status, body) =
            parse_response(&raw).ok_or_else(|| ApiError::Failed("invalid response".into()))?;
        answer(status, &body)
    }
}

/// What a response means, whichever way it came (loopback or a machine).
fn answer(status: u16, body: &[u8]) -> Result<Value, ApiError> {
    {
        let value: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        match status {
            200..=299 => Ok(value),
            401 | 403 => Err(ApiError::Unauthorized),
            409 => match value.pointer("/error/code").and_then(Value::as_str) {
                None | Some("already_resolved") => Err(ApiError::AlreadyAnswered),
                Some(code) => Err(ApiError::Refused {
                    code: code.to_string(),
                    message: error_message(&value).unwrap_or_else(|| code.to_string()),
                    target: value
                        .pointer("/error/outcome/target")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    report: value
                        .pointer("/error/outcome/report")
                        .and_then(|report| serde_json::from_value(report.clone()).ok())
                        .map(Box::new),
                }),
            },
            _ => Err(ApiError::Failed(
                error_message(&value).unwrap_or_else(|| format!("HTTP {status}")),
            )),
        }
    }
}

impl DesktopApi {
    /// `respondToPermissionRequest` with the person's reason for a denial or
    /// "Sempre permitir"; answers whether the allow was kept for good.
    pub fn respond_permission_with(
        &self,
        session_id: &str,
        request_id: &str,
        allow: bool,
        message: Option<&str>,
        always: bool,
    ) -> Result<bool, ApiError> {
        let value = self.post(
            &format!("/api/v1/sessions/{}/respond-permission", encode(session_id)),
            &json!({
                "sessionId": session_id,
                "requestId": request_id,
                "behavior": if allow { "allow" } else { "deny" },
                "message": message,
                "always": always,
            }),
        )?;
        Ok(value
            .get("alwaysSaved")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
    }

    /// `respondToPermissionRequest`: `allow` or `deny`.
    pub fn respond_permission(
        &self,
        session_id: &str,
        request_id: &str,
        allow: bool,
    ) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/respond-permission", encode(session_id)),
            &json!({
                "sessionId": session_id,
                "requestId": request_id,
                "behavior": if allow { "allow" } else { "deny" },
            }),
        )
        .map(|_| ())
    }

    /// `respondToUserInput`: one answer per question.
    pub fn respond_user_input(
        &self,
        session_id: &str,
        request_id: &str,
        answers: &[ProviderUserInputAnswer],
    ) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/respond-user-input", encode(session_id)),
            &json!({ "sessionId": session_id, "requestId": request_id, "answers": answers }),
        )
        .map(|_| ())
    }
}

impl DesktopApi {
    /// `sendTurn`: the session keeps its provider and model, and the desktop
    /// reuses the last approval level, like the phone's sends.
    pub fn send_turn(
        &self,
        session_id: &str,
        prompt: &str,
        retry_of_turn_id: Option<&str>,
        options: &TurnOptions,
    ) -> Result<(), ApiError> {
        let mut body =
            json!({ "sessionId": session_id, "prompt": prompt, "retryOfTurnId": retry_of_turn_id });
        options.write_into(&mut body);
        self.request_waiting(
            "POST",
            &format!("/api/v1/sessions/{}/turns", encode(session_id)),
            Some(&body),
            TURN_START_WAIT,
        )
        .map(|_| ())
    }

    /// `send_turn`, with the id of the turn it started.
    pub fn send_turn_for_id(
        &self,
        session_id: &str,
        prompt: &str,
        options: &TurnOptions,
    ) -> Result<String, ApiError> {
        let mut body = json!({ "sessionId": session_id, "prompt": prompt, "retryOfTurnId": null });
        options.write_into(&mut body);
        let value = self.request_waiting(
            "POST",
            &format!("/api/v1/sessions/{}/turns", encode(session_id)),
            Some(&body),
            TURN_START_WAIT,
        )?;
        value
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError::Failed("the desktop did not answer a turn".into()))
    }

    /// The providers the desktop runs, as its own composer sees them (the
    /// phone's catalog: models, effort levels, approval modes, health).
    pub fn providers(&self) -> Result<Vec<ProviderDescriptor>, ApiError> {
        let value = self.get("/api/v1/mobile/catalog")?;
        serde_json::from_value(value.get("providers").cloned().unwrap_or(Value::Null))
            .map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// Stops the running turn (the composer's stop button).
    pub fn abort(&self, session_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/abort", encode(session_id)),
            &json!({ "sessionId": session_id }),
        )
        .map(|_| ())
    }

    /// Reopens an aborted session before retrying or continuing it.
    pub fn resume(&self, session_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/resume", encode(session_id)),
            &json!({ "sessionId": session_id }),
        )
        .map(|_| ())
    }

    /// "Continuar sozinho se o limite acabar" for one conversation.
    /// `closeSession`: archived, or gone with its history when it never had
    /// a turn. Returns whether the history went.
    pub fn close_session(&self, session_id: &str, delete_history: bool) -> Result<bool, ApiError> {
        let value = self.post(
            &format!("/api/v1/sessions/{}/close", encode(session_id)),
            &json!({ "deleteHistory": delete_history }),
        )?;
        Ok(value
            .get("deletedHistory")
            .and_then(Value::as_bool)
            .unwrap_or(delete_history))
    }

    /// `restoreSession`: an archived conversation comes back.
    pub fn restore_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/restore", encode(session_id)),
            &json!({}),
        )
        .map(|_| ())
    }

    pub fn set_auto_resume(&self, session_id: &str, enabled: bool) -> Result<(), ApiError> {
        self.request(
            "PUT",
            &format!(
                "/api/v1/sessions/{}/usage-limit/auto-resume",
                encode(session_id)
            ),
            Some(&json!({ "enabled": enabled })),
        )
        .map(|_| ())
    }

    /// "Fechar" on a finished goal (a route desktops from 2026-10-24 serve).
    pub fn dismiss_goal(&self, session_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/goal/dismiss", encode(session_id)),
            &json!({}),
        )
        .map(|_| ())
    }

    /// "Enfileirar para depois": a follow-up the desktop sends when the
    /// running turn ends, on the session's own provider and model.
    pub fn queue_turn(
        &self,
        session_id: &str,
        prompt: &str,
        options: &TurnOptions,
    ) -> Result<(), ApiError> {
        let mut body = json!({ "sessionId": session_id, "prompt": prompt });
        TurnOptions {
            provider_id: None,
            model: None,
            ..options.clone()
        }
        .write_into(&mut body);
        self.post(
            &format!("/api/v1/sessions/{}/queue", encode(session_id)),
            &body,
        )
        .map(|_| ())
    }

    /// `queue_turn`, with the id of the turn queued (to take it out again).
    pub fn queue_turn_for_id(
        &self,
        session_id: &str,
        prompt: &str,
        options: &TurnOptions,
    ) -> Result<String, ApiError> {
        let mut body = json!({ "sessionId": session_id, "prompt": prompt });
        TurnOptions {
            provider_id: None,
            model: None,
            ..options.clone()
        }
        .write_into(&mut body);
        let value = self.post(
            &format!("/api/v1/sessions/{}/queue", encode(session_id)),
            &body,
        )?;
        value
            .pointer("/queuedTurn/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError::Failed("the desktop did not answer a queued turn".into()))
    }

    /// "Orientar o agente agora": guidance into the running turn. A turn
    /// already wrapping up answers `Refused { code: "steer_window_closed" }`.
    pub fn steer_turn(&self, session_id: &str, prompt: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/steer", encode(session_id)),
            &json!({ "prompt": prompt }),
        )
        .map(|_| ())
    }

    pub fn remove_queued_turn(
        &self,
        session_id: &str,
        queued_turn_id: &str,
    ) -> Result<Vec<QueuedTurn>, ApiError> {
        self.request(
            "DELETE",
            &format!(
                "/api/v1/sessions/{}/queue/{}",
                encode(session_id),
                encode(queued_turn_id)
            ),
            None,
        )
        .map(queue_of)
    }

    pub fn reorder_queue(
        &self,
        session_id: &str,
        queued_turn_ids: &[String],
    ) -> Result<Vec<QueuedTurn>, ApiError> {
        self.request(
            "PUT",
            &format!("/api/v1/sessions/{}/queue/order", encode(session_id)),
            Some(&json!({ "queuedTurnIds": queued_turn_ids })),
        )
        .map(queue_of)
    }

    /// "Enviar próxima": the head of the queue while no turn runs.
    pub fn dispatch_next_queued_turn(&self, session_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!(
                "/api/v1/sessions/{}/queue/dispatch-next",
                encode(session_id)
            ),
            &json!({}),
        )
        .map(|_| ())
    }

    /// `startThread`: the task's first conversation, on the composer's
    /// provider and model. Answers the new session's id.
    pub fn start_thread(
        &self,
        start: &crate::data::conversation::ThreadStart,
        provider_id: &str,
        model: Option<&str>,
        title: Option<&str>,
    ) -> Result<String, ApiError> {
        let runtime = crate::data::provider_runtime::for_provider(provider_id);
        self.start_thread_with(start, provider_id, model, title, runtime)
    }

    /// `start_thread` with the runtime given (an agent's own account).
    pub fn start_thread_with(
        &self,
        start: &crate::data::conversation::ThreadStart,
        provider_id: &str,
        model: Option<&str>,
        title: Option<&str>,
        runtime: Option<ProviderRuntimeConfig>,
    ) -> Result<String, ApiError> {
        let value = self.post(
            "/api/v1/sessions/start",
            &json!({
                "workspaceId": start.workspace_id,
                "additionalWorkspaceIds": start.additional_workspace_ids,
                "projectId": start.project_id,
                "providerId": provider_id,
                "model": model,
                "title": title,
                "providerRuntime": runtime,
            }),
        )?;
        value
            .pointer("/session/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError::Failed("the desktop did not answer a session".into()))
    }

    /// Names an automatically named task after its first prompt.
    pub fn apply_task_title(
        &self,
        session_id: &str,
        workspace_id: &str,
        title: &str,
    ) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/task-title", encode(session_id)),
            &json!({ "workspaceId": workspace_id, "title": title }),
        )
        .map(|_| ())
    }

    /// "Nova tarefa" in a project: the desktop's own creation (linked
    /// folders, setup report). Answers the new task and how its setup went.
    pub fn create_task(&self, request: &CreateTaskRequest) -> Result<CreatedTask, ApiError> {
        let value = self.post(
            "/api/v1/workspaces/create",
            &json!({
                "projectId": request.project_id,
                "workspaceRoot": request.workspace_root,
                "baseBranch": request.base_branch,
                "isolationMode": if request.worktree { "protectedWorktree" } else { "localDirect" },
                "includeLinkedFolders": request.include_linked_folders,
                "bundleName": request.bundle_name,
                "name": request.name,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// The open-project dialog's single task, exactly as chosen there.
    pub fn create_workspace(
        &self,
        input: &dcc_core::application::CreateWorkspaceForRepoInput,
    ) -> Result<CreatedTask, ApiError> {
        let value = self.post("/api/v1/workspaces", &json!(input))?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// One of the task's delivery actions (`workspace_delivery`), by its id.
    fn delivery(&self, workspace_id: &str, action: &str, body: &Value) -> Result<Value, ApiError> {
        self.request_waiting(
            "POST",
            &format!(
                "/api/v1/workspaces/{}/delivery/{action}",
                encode(workspace_id)
            ),
            Some(body),
            DELIVERY_WAIT,
        )
    }

    /// The header's commit preview: everything staged when nothing is, and
    /// the staged snapshot's message.
    pub fn prepare_commit(&self, workspace_id: &str) -> Result<CommitSuggestion, ApiError> {
        let value = self.delivery(workspace_id, "prepare-commit", &json!({}))?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// The ✨: the task's agent drafts one field (`commitSubject`,
    /// `commitBody`, `requestTitle` or `requestBody`), agreeing with the
    /// dialog's other one.
    pub fn draft(
        &self,
        workspace_id: &str,
        field: &str,
        companion: Option<&str>,
    ) -> Result<String, ApiError> {
        let value = self.delivery(
            workspace_id,
            "draft",
            &json!({ "field": field, "companion": companion }),
        )?;
        value
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError::Failed("no draft".into()))
    }

    /// The reviewed commit, pushed unless the repository has no remote.
    pub fn commit_reviewed(
        &self,
        workspace_id: &str,
        message: &str,
        body: Option<&str>,
        staged_fingerprint: &str,
        push: bool,
    ) -> Result<(), ApiError> {
        self.delivery(
            workspace_id,
            "commit-reviewed",
            &json!({
                "message": message,
                "body": body,
                "stagedFingerprint": staged_fingerprint,
                "push": push,
            }),
        )
        .map(|_| ())
    }

    /// `push`, `merge` or `complete`: no input.
    pub fn deliver(&self, workspace_id: &str, action: &str) -> Result<(), ApiError> {
        self.delivery(workspace_id, action, &json!({})).map(|_| ())
    }

    /// The review header's "Criar PR": no title or body (the forge fills
    /// it), not a draft.
    pub fn create_request(&self, workspace_id: &str) -> Result<(), ApiError> {
        self.create_request_with(workspace_id, &RequestInput::default())
    }

    /// The "Criar PR" dialog's request: its title, body and draft, and the
    /// local changes committed and pushed first when `changes_token` (the
    /// status it was read from) still holds.
    pub fn create_request_with(
        &self,
        workspace_id: &str,
        input: &RequestInput,
    ) -> Result<(), ApiError> {
        self.delivery(
            workspace_id,
            "request",
            &json!({
                "title": input.title,
                "body": input.body,
                "draft": input.draft,
                "includeLocalChanges": input.changes_token.is_some(),
                "changesToken": input.changes_token,
            }),
        )
        .map(|_| ())
    }

    /// The token that guards committing the task's local changes now.
    pub fn changes_token(&self, workspace_id: &str) -> Result<String, ApiError> {
        let value = self.get(&format!(
            "/api/v1/workspaces/{}/delivery",
            encode(workspace_id)
        ))?;
        value
            .get("changesToken")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError::Failed("no changes token".into()))
    }

    /// `workspaceChangeRequestContext`: the dialog's route and default title.
    pub fn change_request_context(
        &self,
        root: &str,
        login: Option<&str>,
    ) -> Result<ChangeRequestContext, ApiError> {
        Self::decoded(self.post(
            "/api/v1/forge/change-request-context",
            &json!({ "workspaceRoot": root, "forgeLogin": login }),
        )?)
    }

    /// "Puxar alterações": what someone else pushed to the task's branch.
    pub fn pull(&self, workspace_id: &str) -> Result<SyncedBase, ApiError> {
        let value = self.delivery(workspace_id, "pull", &json!({}))?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// The task's own branch fetched (refs only).
    pub fn fetch_upstream(&self, workspace_id: &str) -> Result<(), ApiError> {
        self.delivery(workspace_id, "fetch-upstream", &json!({}))
            .map(|_| ())
    }

    /// "Atualizar worktree" with the base (the PR's when it has one).
    pub fn sync_base(
        &self,
        workspace_id: &str,
        base_branch: Option<&str>,
    ) -> Result<SyncedBase, ApiError> {
        let value = self.delivery(
            workspace_id,
            "sync-base",
            &json!({ "baseBranch": base_branch }),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    pub(crate) fn decoded<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// `workspaceForgeContext`: the checkout's forge and who acts on it.
    pub fn forge_context(&self, root: &str) -> Result<crate::data::forge::ForgeContext, ApiError> {
        Self::decoded(self.post("/api/v1/forge/context", &json!({ "workspaceRoot": root }))?)
    }

    /// `getForgeCliAccounts`: the CLI's accounts on that host.
    pub fn forge_accounts(
        &self,
        provider: &str,
        host: Option<&str>,
    ) -> Result<crate::data::forge::ForgeAccounts, ApiError> {
        Self::decoded(self.post(
            "/api/v1/forge/accounts",
            &json!({ "provider": provider, "host": host, "forceRefresh": null }),
        )?)
    }

    pub fn review_state(
        &self,
        root: &str,
        branch: Option<&str>,
        login: Option<&str>,
    ) -> Result<crate::data::forge::ReviewState, ApiError> {
        Self::decoded(self.post(
            "/api/v1/forge/review-state",
            &json!({ "workspaceRoot": root, "branch": branch, "forgeLogin": login }),
        )?)
    }

    pub fn reviewer_candidates(
        &self,
        root: &str,
        number: u32,
        query: Option<&str>,
        login: Option<&str>,
    ) -> Result<crate::data::forge::ReviewerCandidates, ApiError> {
        Self::decoded(self.post(
            "/api/v1/forge/reviewer-candidates",
            &json!({ "workspaceRoot": root, "number": number, "query": query, "forgeLogin": login }),
        )?)
    }

    /// Ask (or ask again) `add`, withdraw `remove`.
    pub fn request_reviewers(
        &self,
        root: &str,
        number: u32,
        add: &[String],
        remove: &[String],
        login: Option<&str>,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/forge/request-reviewers",
            &json!({
                "workspaceRoot": root,
                "number": number,
                "add": add,
                "remove": remove,
                "forgeLogin": login,
            }),
        )
        .map(|_| ())
    }

    /// "Aplicar": the delegation's worktree onto the task's checkout, then
    /// approved; the number of files applied.
    pub fn apply_delegation(&self, id: &str, root: &str) -> Result<usize, ApiError> {
        let value = self.post(
            &format!("/api/v1/delegations/{}/apply", encode(id)),
            &json!({ "workspaceRoot": root }),
        )?;
        Ok(value
            .get("changedFiles")
            .and_then(Value::as_array)
            .map_or(0, Vec::len))
    }

    /// "Descartar": the worktree thrown away and the delegation cancelled.
    pub fn discard_delegation(&self, id: &str, root: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/delegations/{}/discard", encode(id)),
            &json!({ "workspaceRoot": root }),
        )
        .map(|_| ())
    }

    /// `loadAiMemoryExportStatus`: the export that still waits, if any.
    pub fn ai_memory_export_status(
        &self,
        session_id: &str,
    ) -> Result<Option<crate::data::ai_memory::ExportStatus>, ApiError> {
        Self::decoded(self.get(&format!(
            "/api/v1/sessions/{}/ai-memory/export-status",
            encode(session_id)
        ))?)
    }

    /// `loadAiMemoryRecoveredSources`: what the latest turn received.
    pub fn ai_memory_sources(
        &self,
        session_id: &str,
    ) -> Result<Vec<crate::data::ai_memory::SourceHit>, ApiError> {
        Self::decoded(self.get(&format!(
            "/api/v1/sessions/{}/ai-memory/sources",
            encode(session_id)
        ))?)
    }

    /// `loadAiMemorySourceActions`: the person's curation.
    pub fn ai_memory_source_actions(
        &self,
    ) -> Result<Vec<crate::data::ai_memory::SourceAction>, ApiError> {
        Self::decoded(self.get("/api/v1/ai-memory/source-actions")?)
    }

    /// `saveAiMemorySourceAction`: `pinned`, `ignored`, `corrected` or `cleared`.
    pub fn save_ai_memory_source_action(
        &self,
        source_key: &str,
        action: &str,
        correction: Option<&str>,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/ai-memory/source-actions",
            &json!({ "sourceKey": source_key, "action": action, "correction": correction }),
        )
        .map(|_| ())
    }

    /// `loadAiMemorySidecarStatus`: its mode (`managed`, `unavailable`, …).
    pub fn ai_memory_sidecar_mode(&self) -> Result<Option<String>, ApiError> {
        let value = self.get("/api/v1/ai-memory/sidecar")?;
        Ok(value
            .get("mode")
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    /// `bindSessionAgent`: the session runs as this agent from now on.
    pub fn bind_session_agent(&self, session_id: &str, agent_id: &str) -> Result<(), ApiError> {
        self.bind_session_agent_in(session_id, agent_id, None)
    }

    /// The binding, with the project whose room the conversation is in.
    pub fn bind_session_agent_in(
        &self,
        session_id: &str,
        agent_id: &str,
        room: Option<&str>,
    ) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/agent", encode(session_id)),
            &json!({ "agentId": agent_id, "roomProjectId": room }),
        )
        .map(|_| ())
    }

    /// `cancelRunningDelegation`: a running delegation stopped.
    pub fn cancel_delegation(&self, id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/delegations/{}/cancel", encode(id)),
            &json!({}),
        )
        .map(|_| ())
    }

    /// Saves a task's file: only when the disk still has `expected` (the
    /// compare-and-swap of `write_workspace_file`); otherwise the disk's
    /// text comes back and nothing is written.
    pub fn write_file(
        &self,
        root: &str,
        path: &str,
        content: &str,
        expected: Option<&str>,
    ) -> Result<Result<(), String>, ApiError> {
        let value = self.post(
            "/api/v1/files/write",
            &json!({
                "workspaceRoot": root, "relativePath": path,
                "content": content, "expectedPrevious": expected,
            }),
        )?;
        if value.get("conflicted").and_then(Value::as_bool) == Some(true) {
            let disk = value
                .get("diskContent")
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Ok(Err(disk.to_string()));
        }
        Ok(Ok(()))
    }

    /// What the session's last turn changed in this task (`last_turn_review`);
    /// `None` before any turn was captured.
    pub fn last_turn_review(
        &self,
        session_id: &str,
        workspace_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<dcc_core::domain::turn_review::TurnReviewSummary>, ApiError> {
        Self::decoded(self.post(
            &format!("/api/v1/sessions/{}/last-turn-review", encode(session_id)),
            &json!({ "workspaceId": workspace_id, "turnId": turn_id }),
        )?)
    }

    /// One file's patch in a turn's capture.
    pub fn turn_review_file_diff(
        &self,
        snapshot_id: &str,
        path: &str,
    ) -> Result<dcc_core::domain::turn_review::TurnReviewFileDiffOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/turn-review/file-diff",
            &json!({ "snapshotId": snapshot_id, "path": path }),
        )?)
    }

    /// Guarded Undo's read-only step: what it would put back, and its token.
    pub fn prepare_guarded_undo(
        &self,
        snapshot_id: &str,
    ) -> Result<dcc_core::domain::turn_review::PrepareGuardedUndoOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/guarded-undo/prepare",
            &json!({ "snapshotId": snapshot_id }),
        )?)
    }

    /// Guarded Undo's restore, with a prepared preview's token.
    pub fn execute_guarded_undo(
        &self,
        preview_token: &str,
    ) -> Result<dcc_core::domain::turn_review::ExecuteGuardedUndoOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/guarded-undo/execute",
            &json!({ "previewToken": preview_token }),
        )?)
    }

    /// The pull request's review comments (`workspacePrReviewComments`).
    pub fn review_comments(
        &self,
        root: &str,
        branch: Option<&str>,
        login: Option<&str>,
    ) -> Result<dcc_core::domain::review_threads::WorkspacePrReviewCommentsOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/forge/review-comments",
            &json!({ "workspaceRoot": root, "branch": branch, "forgeLogin": login }),
        )?)
    }

    /// A reply on a review conversation, as the forge's signed-in account
    /// (`pull_request_hub_reply_thread`).
    pub fn reply_thread(&self, reply: &ThreadReply) -> Result<(), ApiError> {
        self.post(
            "/api/v1/forge/reply-thread",
            &json!({
                "repositoryRoot": reply.root,
                "number": reply.number,
                "commentId": reply.comment_id,
                "threadId": reply.thread_id,
                "body": reply.body,
                "forgeLogin": reply.login,
            }),
        )
        .map(|_| ())
    }

    /// Resolves (or reopens) a review conversation.
    pub fn resolve_thread(
        &self,
        root: &str,
        number: u32,
        thread_id: &str,
        resolved: bool,
        login: Option<&str>,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/forge/resolve-thread",
            &json!({
                "repositoryRoot": root,
                "number": number,
                "threadId": thread_id,
                "resolved": resolved,
                "forgeLogin": login,
            }),
        )
        .map(|_| ())
    }

    /// The project's base branch brought from its remote (refs only), so a
    /// task created next starts from it.
    pub fn fetch_base(&self, root: &str, base_branch: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/git/fetch-base",
            &json!({ "repositoryRoot": root, "baseBranch": base_branch }),
        )
        .map(|_| ())
    }

    /// The current commit's GitLab pipeline.
    pub fn pipeline(
        &self,
        root: &str,
        login: Option<&str>,
    ) -> Result<crate::data::forge::PipelineStatus, ApiError> {
        Self::decoded(self.post(
            "/api/v1/forge/pipeline",
            &json!({ "workspaceRoot": root, "forgeLogin": login }),
        )?)
    }

    pub fn pipeline_job_log(
        &self,
        root: &str,
        pipeline: u64,
        job: u64,
        login: Option<&str>,
    ) -> Result<crate::data::forge::JobLog, ApiError> {
        Self::decoded(self.post(
            "/api/v1/forge/pipeline-job-log",
            &json!({ "workspaceRoot": root, "pipelineId": pipeline, "jobId": job, "forgeLogin": login }),
        )?)
    }

    pub fn pipeline_job_retry(
        &self,
        root: &str,
        pipeline: u64,
        job: u64,
        login: Option<&str>,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/forge/pipeline-job-retry",
            &json!({ "workspaceRoot": root, "pipelineId": pipeline, "jobId": job, "forgeLogin": login }),
        )
        .map(|_| ())
    }

    /// The last delivery failure the desktop captured for this checkout.
    pub fn delivery_failure(
        &self,
        root: &str,
    ) -> Result<Option<crate::data::forge::DeliveryFailure>, ApiError> {
        let output: crate::data::forge::DeliveryFailureOutput = Self::decoded(self.post(
            "/api/v1/git/delivery-failure",
            &json!({ "workspaceRoot": root }),
        )?)?;
        Ok(output.snapshot)
    }

    /// `workspaceDeliveryRecoveryExecute`: one of the failure's actions.
    pub fn delivery_recovery(
        &self,
        root: &str,
        attempt_token: &str,
        action: &str,
        login: Option<&str>,
    ) -> Result<crate::data::forge::Recovered, ApiError> {
        Self::decoded(self.request_waiting(
            "POST",
            "/api/v1/git/delivery-recovery",
            Some(&json!({
                "workspaceRoot": root,
                "attemptToken": attempt_token,
                "action": action,
                "forgeLogin": login,
            })),
            DELIVERY_WAIT,
        )?)
    }

    /// A git action of the changes list on one file (or `.` for "stage all").
    pub fn git_action(&self, action: GitAction, root: &str, path: &str) -> Result<(), ApiError> {
        self.post(
            action.route(),
            &json!({ "workspaceRoot": root, "relativePath": path }),
        )
        .map(|_| ())
    }

    /// "Clonar": the repository cloned into `workspaceRoot`, with a task in it.
    pub fn clone_workspace(
        &self,
        input: &dcc_core::application::CreateWorkspaceFromUrlInput,
    ) -> Result<CreatedTask, ApiError> {
        let value = self.post("/api/v1/workspaces/clone", &json!(input))?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// "Validar": what a branch URL points at in the project.
    pub fn resolve_source_url(
        &self,
        workspace_root: &str,
        url: &str,
    ) -> Result<SourceResolution, ApiError> {
        let value = self.post(
            "/api/v1/workspaces/source-url/resolve",
            &json!({ "workspaceRoot": workspace_root, "url": url, "forgeLogin": null }),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// "Abrir branch": a task on an existing branch of the project.
    pub fn create_from_source_url(
        &self,
        project_id: &str,
        workspace_root: &str,
        url: &str,
        name: Option<&str>,
        forge_login: Option<&str>,
    ) -> Result<CreatedTask, ApiError> {
        let value = self.post(
            "/api/v1/workspaces/from-source-url",
            &json!({
                "projectId": project_id,
                "workspaceRoot": workspace_root,
                "url": url,
                "name": name,
                "forgeLogin": forge_login,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// The dialog's multi-project task. Answers its primary task and how many
    /// projects it has.
    pub fn create_workspace_bundle(
        &self,
        name: &str,
        projects: &[dcc_core::application::CreateWorkspaceForRepoInput],
    ) -> Result<(String, usize), ApiError> {
        let value = self.post(
            "/api/v1/workspace-bundles",
            &json!({ "name": name, "projects": projects }),
        )?;
        let id = value
            .get("workspaceId")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::Failed("the desktop did not answer a task".into()))?;
        let count = value.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
        Ok((id.to_string(), count))
    }

    /// "Continuar" on a task stopped by its usage limit.
    pub fn resume_after_usage_limit(&self, session_id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/sessions/{}/usage-limit/resume", encode(session_id)),
            &json!({}),
        )
        .map(|_| ())
    }
}

/// What "Criar PR" sends: `changes_token` set means the local changes go too.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestInput {
    pub title: Option<String>,
    pub body: Option<String>,
    pub draft: bool,
    pub changes_token: Option<String>,
}

/// `WorkspaceChangeRequestContextOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChangeRequestContext {
    pub head_branch: String,
    pub base_branch: String,
    pub title: Option<String>,
    pub request_label: String,
}

fn error_message(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| {
            error
                .as_str()
                .map(str::to_string)
                .or_else(|| error.get("message")?.as_str().map(str::to_string))
        })
        .or_else(|| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

/// Path segment escaping for ids (they are UUIDs; anything else is escaped).
pub(crate) fn encode(segment: &str) -> String {
    segment
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Status code and body of an HTTP/1.1 response, chunked or not.
fn parse_response(raw: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = raw.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..split]).ok()?;
    let status: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
    let body = &raw[split + 4..];
    let chunked = head.lines().any(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with("transfer-encoding:") && line.contains("chunked")
    });
    if !chunked {
        return Some((status, body.to_vec()));
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let line_end = rest.windows(2).position(|window| window == b"\r\n")?;
        let size_text = std::str::from_utf8(&rest[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            return Some((status, out));
        }
        out.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}

/// What "Nova tarefa" sends for a project.
/// `WorkspaceGitCommitSuggestionOutput`, as the preview reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSuggestion {
    pub subject: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub staged_file_count: u32,
    pub staged_fingerprint: String,
}

/// `WorkspaceGitSyncBaseOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedBase {
    pub remote: String,
    pub base_branch: String,
    pub updated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTaskRequest {
    pub project_id: String,
    pub workspace_root: String,
    pub base_branch: String,
    /// The "Worktree" checkbox: a protected worktree, else the checkout itself.
    pub worktree: bool,
    pub include_linked_folders: bool,
    /// The name of a task over several projects.
    pub bundle_name: String,
    /// The task's own name (the quick composer's first line).
    pub name: Option<String>,
}

/// What a branch or PR URL points at (`WorkspaceSourceUrlResolution`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceResolution {
    /// `branch` or `pull_request`.
    pub kind: String,
    pub repository: String,
    pub head_branch: String,
    pub base_branch: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub is_cross_repository: bool,
    #[serde(default)]
    pub source_repository: Option<String>,
}

/// The task the desktop created, with its setup outcome.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTask {
    pub workspace_id: String,
    pub setup_report: dcc_core::domain::workspace::WorkspaceSetupReport,
    #[serde(default)]
    pub setup_hints: Vec<crate::data::setup_report::SetupHint>,
}

/// `{queue}` from the queue routes.
fn queue_of(value: Value) -> Vec<QueuedTurn> {
    value
        .get("queue")
        .cloned()
        .and_then(|queue| serde_json::from_value(queue).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    use super::*;

    /// A one-shot server that answers `status` and returns the request it got.
    fn serve(status: &'static str, body: &'static str) -> (u16, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut payload = vec![0; length];
            reader.read_exact(&mut payload).unwrap();
            request.push_str(&String::from_utf8(payload).unwrap());
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            request
        });
        (port, handle)
    }

    #[test]
    fn answers_a_permission_with_the_local_key() {
        let (port, server) = serve("200 OK", r#"{"ok":true}"#);
        DesktopApi::new(port, "secret")
            .respond_permission("s-1", "p-1", true)
            .unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /api/v1/sessions/s-1/respond-permission HTTP/1.1"));
        assert!(request.contains("X-API-Key: secret"));
        assert!(request.contains(r#""behavior":"allow""#));
        assert!(request.contains(r#""requestId":"p-1""#));
    }

    #[test]
    fn reads_the_desktop_refusals() {
        let (port, server) = serve("409 Conflict", r#"{"error":"already answered"}"#);
        let answers = [ProviderUserInputAnswer {
            question: "Qual?".into(),
            answer: "Esta".into(),
        }];
        assert_eq!(
            DesktopApi::new(port, "k").respond_user_input("s", "q", &answers),
            Err(ApiError::AlreadyAnswered)
        );
        assert!(server.join().unwrap().contains(r#""answers":[{"#));
        let (port, server) = serve("500 Internal Server Error", r#"{"error":"turn is gone"}"#);
        assert_eq!(
            DesktopApi::new(port, "k").respond_permission("s", "p", false),
            Err(ApiError::Failed("turn is gone".into()))
        );
        server.join().unwrap();
        // A port nobody listens on reads as a closed desktop.
        let free = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(
            DesktopApi::new(free, "k").respond_permission("s", "p", true),
            Err(ApiError::DesktopClosed)
        );
    }

    #[test]
    fn turns_carry_the_composer_choices() {
        let (port, server) = serve("200 OK", "{}");
        let options = TurnOptions {
            effort: Some("high".into()),
            approval_policy: Some("full_access".into()),
            plan_mode: true,
            ..TurnOptions::default()
        };
        DesktopApi::new(port, "k")
            .send_turn("s", "oi", None, &options)
            .unwrap();
        let request = server.join().unwrap();
        for part in [
            r#""effort":"high""#,
            r#""approvalPolicy":"full_access""#,
            r#""planMode":true"#,
            r#""fastMode":false"#,
        ] {
            assert!(request.contains(part), "{part} in {request}");
        }
        assert!(!request.contains("providerId"));
    }

    #[test]
    fn decodes_chunked_bodies() {
        let raw = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(parse_response(raw), Some((200, b"{\"a\":1}".to_vec())));
    }
}
