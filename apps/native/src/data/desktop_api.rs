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

use dcc_core::domain::objective::{ObjectiveTransition, SessionObjective, SessionObjectiveDraft};
use dcc_core::domain::provider::ProviderDescriptor;
use dcc_core::domain::session::QueuedTurn;
use dcc_core::ports::provider::ProviderUserInputAnswer;
use serde_json::{Value, json};

const DEFAULT_PORT: u16 = 9876;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(800);
const IO_TIMEOUT: Duration = Duration::from_secs(20);

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
    },
    Failed(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DesktopClosed => write!(f, "the desktop app is not running"),
            Self::AlreadyAnswered => write!(f, "already answered"),
            Self::Unauthorized => write!(f, "the desktop refused the local key"),
            Self::Refused { message, .. } => write!(f, "{message}"),
            Self::Failed(message) => write!(f, "{message}"),
        }
    }
}

/// How the turn runs, as the composer chose it; `None` leaves the session's.
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
        map.insert("planMode".into(), Value::Bool(self.plan_mode));
        map.insert("fastMode".into(), Value::Bool(self.fast_mode));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopApi {
    port: u16,
    api_key: String,
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
        Some(Self { port, api_key })
    }

    pub fn new(port: u16, api_key: impl Into<String>) -> Self {
        Self {
            port,
            api_key: api_key.into(),
        }
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        self.request("POST", path, Some(body))
    }

    fn get(&self, path: &str) -> Result<Value, ApiError> {
        self.request("GET", path, None)
    }

    fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, ApiError> {
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
            .map_err(|_| ApiError::DesktopClosed)?;
        stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
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
        stream
            .read_to_end(&mut raw)
            .map_err(|error| ApiError::Failed(error.to_string()))?;
        let (status, body) =
            parse_response(&raw).ok_or_else(|| ApiError::Failed("invalid response".into()))?;
        let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        match status {
            200..=299 => Ok(value),
            401 | 403 => Err(ApiError::Unauthorized),
            409 => match value.pointer("/error/code").and_then(Value::as_str) {
                None | Some("already_resolved") => Err(ApiError::AlreadyAnswered),
                Some(code) => Err(ApiError::Refused {
                    code: code.to_string(),
                    message: error_message(&value).unwrap_or_else(|| code.to_string()),
                }),
            },
            _ => Err(ApiError::Failed(
                error_message(&value).unwrap_or_else(|| format!("HTTP {status}")),
            )),
        }
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
        self.post(
            &format!("/api/v1/sessions/{}/turns", encode(session_id)),
            &body,
        )
        .map(|_| ())
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
        let value = self.post(
            "/api/v1/sessions/start",
            &json!({
                "workspaceId": start.workspace_id,
                "additionalWorkspaceIds": start.additional_workspace_ids,
                "projectId": start.project_id,
                "providerId": provider_id,
                "model": model,
                "title": title,
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

    /// Creates or rewrites the task's objective; `expected_generation` (the
    /// one the form was opened from) makes a concurrent edit fail.
    pub fn set_objective(
        &self,
        session_id: &str,
        draft: &SessionObjectiveDraft,
        expected_generation: Option<u64>,
    ) -> Result<Option<SessionObjective>, ApiError> {
        self.request(
            "PUT",
            &format!("/api/v1/sessions/{}/objective", encode(session_id)),
            Some(&json!({ "draft": draft, "expectedGeneration": expected_generation })),
        )
        .map(objective_of)
    }

    /// Pause, resume or complete the objective.
    pub fn transition_objective(
        &self,
        session_id: &str,
        transition: ObjectiveTransition,
        expected_generation: u64,
    ) -> Result<Option<SessionObjective>, ApiError> {
        self.post(
            &format!(
                "/api/v1/sessions/{}/objective/transition",
                encode(session_id)
            ),
            &json!({ "transition": transition, "expectedGeneration": expected_generation }),
        )
        .map(objective_of)
    }

    pub fn clear_objective(&self, session_id: &str) -> Result<(), ApiError> {
        self.request(
            "DELETE",
            &format!("/api/v1/sessions/{}/objective", encode(session_id)),
            None,
        )
        .map(|_| ())
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
fn encode(segment: &str) -> String {
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

/// `{objective}` from the objective routes.
fn objective_of(value: Value) -> Option<SessionObjective> {
    serde_json::from_value(value.get("objective")?.clone()).ok()
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
