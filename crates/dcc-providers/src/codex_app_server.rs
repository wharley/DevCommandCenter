use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{self, BoxStream};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, oneshot, Mutex},
    time::timeout,
};
use uuid::Uuid;

use dcc_core::{
    domain::{
        provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
    },
    ports::{Input, Provider, SessionConfig},
    CoreError, Result,
};

// ── JSON-RPC helpers ────────────────────────────────────────────────────────

fn rpc_request(id: u64, method: &str, params: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
    .to_string()
}

fn rpc_notification(method: &str) -> String {
    json!({ "jsonrpc": "2.0", "method": method }).to_string()
}

async fn write_line(stdin: &mut ChildStdin, line: &str) -> Result<()> {
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| CoreError::Provider(format!("codex stdin write: {e}")))?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|e| CoreError::Provider(format!("codex stdin write newline: {e}")))?;
    stdin
        .flush()
        .await
        .map_err(|e| CoreError::Provider(format!("codex stdin flush: {e}")))
}

// ── Incoming JSON-RPC message ───────────────────────────────────────────────

#[derive(Deserialize)]
struct Incoming {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    params: Option<Value>,
}

// ── Notification → ProviderEvent ────────────────────────────────────────────

fn notification_to_event(method: &str, params: &Value) -> Option<ProviderEvent> {
    let at = Utc::now().to_rfc3339();
    match method {
        "item/agentMessage/delta" => Some(ProviderEvent::TextDelta {
            content: params
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "turn/completed" => {
            let status = params
                .get("turn")
                .and_then(|t| t.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("completed");
            if status == "failed" {
                Some(ProviderEvent::Failed {
                    message: params
                        .get("turn")
                        .and_then(|t| t.get("error"))
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("turn failed")
                        .to_string(),
                    at,
                })
            } else {
                Some(ProviderEvent::Completed { at })
            }
        }
        "error" => Some(ProviderEvent::Failed {
            message: params
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .or_else(|| params.get("message").and_then(Value::as_str))
                .unwrap_or("codex app-server error")
                .to_string(),
            at,
        }),
        "item/started" => {
            let item = params.get("item")?;
            let kind = item.get("type").and_then(Value::as_str)?;
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("item")
                .to_string();
            match kind {
                "commandExecution" => Some(ProviderEvent::ToolCallStarted {
                    id,
                    action: "Bash".to_string(),
                    command: item
                        .get("command")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    file: None,
                    at,
                }),
                "file_change" | "fileChange" => Some(ProviderEvent::ToolCallStarted {
                    id,
                    action: "apply_patch".to_string(),
                    command: None,
                    file: item
                        .get("file_path")
                        .or_else(|| item.get("filePath"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    at,
                }),
                "reasoning" => Some(ProviderEvent::ReasoningStarted {
                    id,
                    label: Some("Thinking".to_string()),
                    at,
                }),
                _ => None,
            }
        }
        "item/completed" => {
            let item = params.get("item")?;
            let kind = item.get("type").and_then(Value::as_str)?;
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("item")
                .to_string();
            match kind {
                "commandExecution" | "file_change" | "fileChange" | "web_search"
                | "mcp_tool_call" => {
                    let failed = item
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s == "failed");
                    if failed {
                        Some(ProviderEvent::ToolCallFailed {
                            id,
                            reason: item
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            at,
                        })
                    } else {
                        Some(ProviderEvent::ToolCallCompleted { id, at })
                    }
                }
                "reasoning" => Some(ProviderEvent::ReasoningCompleted { id, at }),
                _ => None,
            }
        }
        "item/reasoning/textDelta" | "item/reasoning/summaryTextDelta" => {
            Some(ProviderEvent::ReasoningDelta {
                id: params
                    .get("itemId")
                    .and_then(Value::as_str)
                    .unwrap_or("reasoning")
                    .to_string(),
                content: params
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        }
        _ => None,
    }
}

// ── Per-session runtime ─────────────────────────────────────────────────────

type PendingResponse = oneshot::Sender<std::result::Result<Value, String>>;
type PendingMap = Arc<Mutex<HashMap<u64, PendingResponse>>>;

struct SessionRuntime {
    handle: SessionHandle,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    thread_id: Mutex<Option<String>>,
    pending: PendingMap,
    next_id: AtomicU64,
    events_tx: broadcast::Sender<ProviderEvent>,
}

impl SessionRuntime {
    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        {
            let mut stdin = self.stdin.lock().await;
            write_line(&mut stdin, &rpc_request(id, method, params)).await?;
        }
        timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| CoreError::Provider(format!("codex {method} timed out")))?
            .map_err(|_| CoreError::Provider(format!("codex {method} cancelled")))?
            .map_err(|e| CoreError::Provider(format!("codex {method} error: {e}")))
    }

    async fn send_notification(&self, method: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        write_line(&mut stdin, &rpc_notification(method)).await
    }
}

// ── Adapter runtime state ───────────────────────────────────────────────────

#[derive(Default)]
struct AdapterState {
    sessions: Mutex<HashMap<String, Arc<SessionRuntime>>>,
}

// ── Public adapter ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CodexAppServerAdapter {
    pub id: ProviderId,
    pub label: String,
    pub description: String,
    pub capabilities: Capabilities,
    pub stable: bool,
    state: Arc<AdapterState>,
}

impl CodexAppServerAdapter {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            id: ProviderId("codex".to_string()),
            label: "Codex".to_string(),
            description: "OpenAI Codex provider via app-server protocol.".to_string(),
            capabilities,
            stable: true,
            state: Arc::new(AdapterState::default()),
        }
    }

    async fn session_runtime(&self, id: &SessionId) -> Option<Arc<SessionRuntime>> {
        self.state.sessions.lock().await.get(&id.0).cloned()
    }

    async fn start_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let mut cmd = Command::new("codex");
        cmd.arg("app-server");
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        cmd.env("DCC_PROVIDER_ID", "codex");
        cmd.env("DCC_WORKSPACE_ID", &cfg.workspace_id.0);
        cmd.env("DCC_SESSION_ID", &cfg.session_id.0);
        if let Some(ref m) = cfg.model {
            cmd.env("DCC_MODEL", m);
        }
        if let Some(ref rc) = cfg.provider_runtime {
            if let Some(ref home) = rc.home_path {
                cmd.env("CODEX_HOME", home);
            }
        }
        if let Some(ref wd) = cfg.working_directory {
            if !wd.trim().is_empty() {
                cmd.current_dir(wd);
            }
        }

        let mut child = cmd.spawn().map_err(|e| {
            CoreError::Provider(format!("failed to spawn codex app-server: {e}"))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stdout".to_string()))?;

        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id.clone(),
            handle_id: Uuid::new_v4().to_string(),
        };

        let (events_tx, _) = broadcast::channel(128);
        let runtime = Arc::new(SessionRuntime {
            handle: handle.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            thread_id: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            events_tx: events_tx.clone(),
        });

        let session_key = cfg.session_id.0.clone();
        self.state
            .sessions
            .lock()
            .await
            .insert(session_key.clone(), runtime.clone());

        // Start background reader before initializing (needs to be live to route responses)
        Self::spawn_reader(
            runtime.clone(),
            stdout,
            Arc::clone(&self.state),
            session_key.clone(),
        );

        // Handshake + thread/start
        if let Err(e) = Self::handshake(&runtime, &cfg).await {
            self.state.sessions.lock().await.remove(&session_key);
            return Err(e);
        }

        Ok(handle)
    }

    async fn handshake(runtime: &Arc<SessionRuntime>, cfg: &SessionConfig) -> Result<()> {
        // initialize
        runtime
            .send_request(
                "initialize",
                json!({
                    "clientInfo": { "name": "dcc", "version": "0.1.0" },
                    "capabilities": {},
                }),
            )
            .await?;

        // initialized notification (no response expected)
        runtime.send_notification("initialized").await?;

        // thread/start
        let cwd = cfg
            .working_directory
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(".");

        let result = runtime
            .send_request(
                "thread/start",
                json!({
                    "cwd": cwd,
                    "approvalPolicy": "never",
                    "sandbox": "danger-full-access",
                }),
            )
            .await?;

        let thread_id = result
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CoreError::Provider("codex thread/start missing thread.id".to_string())
            })?
            .to_string();

        *runtime.thread_id.lock().await = Some(thread_id);
        Ok(())
    }

    fn spawn_reader(
        runtime: Arc<SessionRuntime>,
        stdout: tokio::process::ChildStdout,
        state: Arc<AdapterState>,
        session_key: String,
    ) {
        tokio::spawn(async move {
            let _ = runtime
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });

            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }

                let Ok(msg) = serde_json::from_str::<Incoming>(&trimmed) else {
                    continue;
                };

                // Response: has numeric id + result/error
                if let Some(id_val) = &msg.id {
                    if let Some(id) = id_val.as_u64() {
                        if msg.result.is_some() || msg.error.is_some() {
                            let mut pending = runtime.pending.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                if let Some(r) = msg.result {
                                    let _ = tx.send(Ok(r));
                                } else if let Some(e) = msg.error {
                                    let msg = e
                                        .get("message")
                                        .and_then(Value::as_str)
                                        .unwrap_or("rpc error")
                                        .to_string();
                                    let _ = tx.send(Err(msg));
                                }
                            }
                            continue;
                        }
                    }
                }

                // Notification: has method, no response id
                if let (Some(method), Some(params)) = (&msg.method, &msg.params) {
                    if let Some(event) = notification_to_event(method, params) {
                        let _ = runtime.events_tx.send(event);
                    }
                }
            }

            // Process exited
            let at = now_iso();
            let exit = {
                let mut child = runtime.child.lock().await;
                child.wait().await
            };

            // Drain pending requests
            for (_, tx) in runtime.pending.lock().await.drain() {
                let _ = tx.send(Err("codex process exited".to_string()));
            }

            match exit {
                Ok(status) if status.success() => {
                    let _ = runtime.events_tx.send(ProviderEvent::Completed { at });
                }
                Ok(status) => {
                    let _ = runtime.events_tx.send(ProviderEvent::Failed {
                        message: format!("codex exited with status {status}"),
                        at,
                    });
                }
                Err(e) => {
                    let _ = runtime.events_tx.send(ProviderEvent::Failed {
                        message: format!("codex wait error: {e}"),
                        at,
                    });
                }
            }

            state.sessions.lock().await.remove(&session_key);
        });
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

// ── Provider trait ──────────────────────────────────────────────────────────

#[async_trait]
impl Provider for CodexAppServerAdapter {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        self.start_runtime(cfg).await
    }

    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()> {
        let runtime = self
            .session_runtime(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no codex runtime for session {}",
                    handle.session_id.0
                ))
            })?;

        let Input::Text(text) = input;

        let thread_id = runtime
            .thread_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| CoreError::Provider("codex session has no thread ID".to_string()))?;

        runtime
            .send_request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{ "type": "text", "text": text }],
                    "approvalPolicy": "never",
                    "sandboxPolicy": { "type": "dangerFullAccess" },
                }),
            )
            .await?;

        Ok(())
    }

    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
        let runtime = self
            .state
            .sessions
            .try_lock()
            .ok()
            .and_then(|s| s.get(&handle.session_id.0).cloned());

        let Some(runtime) = runtime else {
            return Box::pin(stream::empty());
        };

        let rx = runtime.events_tx.subscribe();
        Box::pin(stream::unfold(rx, |mut rx| async move {
            match rx.recv().await {
                Ok(event) => Some((Ok(event), rx)),
                Err(broadcast::error::RecvError::Closed) => None,
                Err(broadcast::error::RecvError::Lagged(_)) => Some((
                    Ok(ProviderEvent::Failed {
                        message: "codex event stream lagged".to_string(),
                        at: now_iso(),
                    }),
                    rx,
                )),
            }
        }))
    }

    async fn cancel(&self, handle: &SessionHandle) -> Result<()> {
        let runtime = self
            .session_runtime(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no codex runtime for session {}",
                    handle.session_id.0
                ))
            })?;

        let mut child = runtime.child.lock().await;
        child
            .kill()
            .await
            .map_err(|e| CoreError::Provider(format!("failed to kill codex: {e}")))?;
        Ok(())
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        let runtime = self.session_runtime(previous).await.ok_or_else(|| {
            CoreError::Provider(format!(
                "no resumable codex runtime for session {}",
                previous.0
            ))
        })?;
        Ok(runtime.handle.clone())
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        match Command::new("codex").arg("--version").output().await {
            Ok(output) if output.status.success() => Ok(HealthStatus::Healthy),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let reason = if !stderr.is_empty() { stderr } else { stdout };
                Ok(HealthStatus::Degraded { reason })
            }
            Err(e) => Ok(HealthStatus::Unhealthy {
                reason: format!("failed to execute codex: {e}"),
            }),
        }
    }
}
