use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::{self, BoxStream};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, oneshot, Mutex},
    time::timeout,
};
use uuid::Uuid;

use dcc_core::{
    application::{compose_fallback_prompt_for_provider, PromptInjectionOptions},
    domain::{
        provider::{
            Capabilities, HealthStatus, ProviderAccountUsage, ProviderAccountUsageState,
            ProviderEvent, ProviderId, ProviderUsageWindow, SessionHandle,
        },
        session::SessionId,
    },
    ports::{Input, Provider, ProviderRuntimeConfig, SessionConfig},
    CoreError, Result,
};

use crate::common::{append_tool_instructions, augmented_path};

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

fn initialize_params(experimental_api: bool) -> Value {
    json!({
        "clientInfo": { "name": "dcc", "version": env!("CARGO_PKG_VERSION") },
        // runtimeWorkspaceRoots is currently part of the Codex app-server
        // experimental API. Opt in only for multi-root sessions so the existing
        // single-workspace and account-usage flows stay on the stable protocol.
        "capabilities": if experimental_api {
            json!({ "experimentalApi": true })
        } else {
            json!({})
        },
    })
}

fn thread_start_params(cwd: &str, additional_working_directories: &[String]) -> Value {
    let mut params = json!({
        "cwd": cwd,
        "approvalPolicy": "never",
        "sandbox": "workspace-write",
    });
    if !additional_working_directories.is_empty() {
        let mut runtime_workspace_roots = vec![cwd.to_string()];
        runtime_workspace_roots.extend(additional_working_directories.iter().cloned());
        params["runtimeWorkspaceRoots"] = json!(runtime_workspace_roots);
    }
    params
}

fn codex_reasoning_effort(effort: Option<&str>) -> Option<&'static str> {
    match effort.map(str::trim).filter(|value| !value.is_empty()) {
        Some("none") => Some("none"),
        Some("minimal") => Some("minimal"),
        Some("low") => Some("low"),
        Some("balanced") | Some("medium") => Some("medium"),
        Some("high") => Some("high"),
        Some("xhigh") | Some("max") | Some("ultrathink") => Some("xhigh"),
        Some(_) | None => None,
    }
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

fn codex_reset_time(value: &Value) -> Option<String> {
    let timestamp = value.as_i64()?;
    DateTime::<Utc>::from_timestamp(timestamp, 0).map(|value| value.to_rfc3339())
}

fn codex_usage_window(id: &str, value: &Value) -> Option<ProviderUsageWindow> {
    let used_percent = value.get("usedPercent")?.as_f64()?.clamp(0.0, 100.0);
    Some(ProviderUsageWindow {
        id: id.to_string(),
        used_percent,
        remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
        resets_at: value.get("resetsAt").and_then(codex_reset_time),
        window_duration_minutes: value.get("windowDurationMins").and_then(Value::as_u64),
        is_exhausted: used_percent >= 100.0,
    })
}

fn parse_codex_account_usage(value: &Value) -> Result<ProviderAccountUsage> {
    let limits = value.get("rateLimits").unwrap_or(value);
    let mut windows = Vec::new();
    for id in ["primary", "secondary"] {
        if let Some(window) = limits
            .get(id)
            .and_then(|value| codex_usage_window(id, value))
        {
            windows.push(window);
        }
    }
    let limit_reached = limits
        .get("rateLimitReachedType")
        .is_some_and(|value| !value.is_null());
    if limit_reached {
        for window in &mut windows {
            window.is_exhausted = true;
        }
    }

    if windows.is_empty() {
        return Err(CoreError::Provider(
            "codex account/rateLimits/read returned no usage windows".to_string(),
        ));
    }

    Ok(ProviderAccountUsage {
        provider_id: ProviderId("codex".to_string()),
        state: ProviderAccountUsageState::Available,
        windows,
        plan_type: limits
            .get("planType")
            .and_then(Value::as_str)
            .map(str::to_string),
        updated_at: Utc::now().to_rfc3339(),
        is_cached: false,
    })
}

async fn read_rpc_response_with_timeout<R>(
    lines: &mut Lines<R>,
    expected_id: u64,
    response_timeout: Duration,
) -> Result<Value>
where
    R: AsyncBufRead + Unpin,
{
    timeout(response_timeout, async {
        loop {
            let line = lines
                .next_line()
                .await
                .map_err(|error| CoreError::Provider(format!("codex stdout read: {error}")))?
                .ok_or_else(|| {
                    CoreError::Provider("codex app-server exited before responding".to_string())
                })?;
            let message = serde_json::from_str::<Incoming>(&line).map_err(|error| {
                CoreError::Provider(format!("invalid codex app-server response: {error}"))
            })?;
            if message.id.as_ref().and_then(Value::as_u64) != Some(expected_id) {
                continue;
            }
            if let Some(error) = message.error {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("rpc error");
                return Err(CoreError::Provider(format!(
                    "codex account usage error: {message}"
                )));
            }
            return message.result.ok_or_else(|| {
                CoreError::Provider("codex account usage response had no result".to_string())
            });
        }
    })
    .await
    .map_err(|_| CoreError::Provider("codex account usage request timed out".to_string()))?
}

async fn read_rpc_response<R>(lines: &mut Lines<R>, expected_id: u64) -> Result<Value>
where
    R: AsyncBufRead + Unpin,
{
    read_rpc_response_with_timeout(lines, expected_id, Duration::from_secs(15)).await
}

async fn fetch_codex_account_usage(
    runtime_config: Option<&ProviderRuntimeConfig>,
) -> Result<ProviderAccountUsage> {
    let mut command = Command::new("codex");
    command
        .arg("app-server")
        .arg("-c")
        .arg("notify=[]")
        .kill_on_drop(true)
        .env("PATH", augmented_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = runtime_config.and_then(|config| config.home_path.as_deref()) {
        command.env("CODEX_HOME", home);
    }

    let mut child = command.spawn().map_err(|error| {
        CoreError::Provider(format!("failed to start codex for account usage: {error}"))
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| CoreError::Provider("codex app-server missing stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CoreError::Provider("codex app-server missing stdout".to_string()))?;
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(_)) = lines.next_line().await {}
        }
    });
    let mut lines = BufReader::new(stdout).lines();

    let result = async {
        write_line(
            &mut stdin,
            &rpc_request(1, "initialize", initialize_params(false)),
        )
        .await?;
        read_rpc_response(&mut lines, 1).await?;
        write_line(&mut stdin, &rpc_notification("initialized")).await?;
        write_line(
            &mut stdin,
            &rpc_request(2, "account/rateLimits/read", Value::Null),
        )
        .await?;
        let response = read_rpc_response(&mut lines, 2).await?;
        parse_codex_account_usage(&response)
    }
    .await;

    drop(stdin);
    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(2), child.wait()).await;
    stderr_task.abort();
    let _ = stderr_task.await;
    result
}

// ── Notification → ProviderEvent ────────────────────────────────────────────

fn codex_agent_message_delta_content(
    params: &Value,
    last_agent_message_id: &mut Option<String>,
) -> String {
    let item_id = params.get("itemId").and_then(Value::as_str);
    let delta = params.get("delta").and_then(Value::as_str).unwrap_or("");
    let should_prefix_separator = item_id.is_some_and(|current_id| {
        last_agent_message_id
            .as_deref()
            .is_some_and(|previous_id| previous_id != current_id)
    }) && !delta.is_empty();

    if let Some(current_id) = item_id.filter(|value| !value.is_empty()) {
        *last_agent_message_id = Some(current_id.to_string());
    }

    if should_prefix_separator {
        format!("\n\n{delta}")
    } else {
        delta.to_string()
    }
}

fn notification_to_event(
    method: &str,
    params: &Value,
    last_agent_message_id: &mut Option<String>,
) -> Option<ProviderEvent> {
    let at = Utc::now().to_rfc3339();
    match method {
        "item/agentMessage/delta" => Some(ProviderEvent::TextDelta {
            content: codex_agent_message_delta_content(params, last_agent_message_id),
        }),
        "turn/completed" => {
            *last_agent_message_id = None;
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
        "error" => {
            *last_agent_message_id = None;
            Some(ProviderEvent::Failed {
                message: codex_error_message(params),
                at,
            })
        }
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
    last_retry_at: Mutex<Option<Instant>>,
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
        cmd.arg("-c");
        cmd.arg("notify=[]");
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        cmd.env("PATH", augmented_path());
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

        let mut child = cmd
            .spawn()
            .map_err(|e| CoreError::Provider(format!("failed to spawn codex app-server: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stderr".to_string()))?;

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
            last_retry_at: Mutex::new(None),
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
            stderr,
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
        let uses_experimental_multi_root = !cfg.additional_working_directories.is_empty();
        runtime
            .send_request(
                "initialize",
                initialize_params(uses_experimental_multi_root),
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
                thread_start_params(cwd, &cfg.additional_working_directories),
            )
            .await?;

        let thread_id = result
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Provider("codex thread/start missing thread.id".to_string()))?
            .to_string();

        *runtime.thread_id.lock().await = Some(thread_id);
        Ok(())
    }

    fn spawn_reader(
        runtime: Arc<SessionRuntime>,
        stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr,
        state: Arc<AdapterState>,
        session_key: String,
    ) {
        tokio::spawn(async move {
            let _ = runtime
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });

            let retry_runtime = runtime.clone();
            let stderr_task = tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let trimmed = line.trim();
                    if is_codex_reconnect_notice(trimmed) {
                        *retry_runtime.last_retry_at.lock().await = Some(Instant::now());
                    }
                }
            });

            let mut reader = BufReader::new(stdout).lines();
            let mut last_agent_message_id = None;
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
                    if method == "error" && should_suppress_codex_error(params, &runtime).await {
                        continue;
                    }
                    if let Some(event) =
                        notification_to_event(method, params, &mut last_agent_message_id)
                    {
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

            let _ = stderr_task.await;
            state.sessions.lock().await.remove(&session_key);
        });
    }
}

fn codex_error_message(params: &Value) -> String {
    params
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| params.get("message").and_then(Value::as_str))
        .unwrap_or("codex app-server error")
        .to_string()
}

fn is_codex_reconnect_notice(message: &str) -> bool {
    let trimmed = message.trim_start();
    let suffix = if let Some(rest) = trimmed.strip_prefix("Reconnecting...") {
        Some(rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix("Reconnecting…") {
        Some(rest.trim_start())
    } else {
        None
    };

    match suffix {
        None => false,
        Some("") => true,
        Some(rest) => {
            let mut parts = rest.split('/');
            let left = parts.next().map(str::trim);
            let right = parts.next().map(str::trim);
            left.is_some_and(|value| value.chars().all(|ch| ch.is_ascii_digit()))
                && right.is_some_and(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        }
    }
}

async fn should_suppress_codex_error(params: &Value, runtime: &SessionRuntime) -> bool {
    if params
        .get("willRetry")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }

    let message = codex_error_message(params);
    if !is_codex_reconnect_notice(&message) {
        return false;
    }

    runtime
        .last_retry_at
        .lock()
        .await
        .as_ref()
        .is_some_and(|at| at.elapsed() <= Duration::from_secs(30))
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

        let thread_id = runtime
            .thread_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| CoreError::Provider("codex session has no thread ID".to_string()))?;

        let (prompt, effort, summary) = match input {
            Input::Text(text) => (text, None, None),
            Input::Turn(turn) => (
                append_tool_instructions(
                    compose_fallback_prompt_for_provider(
                        "codex",
                        &turn.prompt,
                        turn.plan_mode,
                        turn.effort.as_deref(),
                        turn.fast_mode,
                        PromptInjectionOptions {
                            plan: true,
                            effort: false,
                            fast: false,
                        },
                    ),
                    turn.tool_instructions.as_deref(),
                ),
                codex_reasoning_effort(turn.effort.as_deref()),
                if turn.fast_mode.unwrap_or(true) {
                    Some("concise")
                } else {
                    Some("auto")
                },
            ),
            Input::UserInputResponse(_) => {
                return Err(CoreError::Provider(
                    "Codex does not support mid-turn user input responses".to_string(),
                ));
            }
            Input::PermissionResponse(_) => {
                return Err(CoreError::Provider(
                    "Codex does not support mid-turn permission responses".to_string(),
                ));
            }
        };

        runtime
            .send_request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{ "type": "text", "text": prompt }],
                    "effort": effort,
                    "approvalPolicy": "never",
                    "sandboxPolicy": { "type": "dangerFullAccess" },
                    "summary": summary,
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
            loop {
                match rx.recv().await {
                    Ok(event) => return Some((Ok(event), rx)),
                    Err(broadcast::error::RecvError::Closed) => return None,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
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
        let mut command = Command::new("codex");
        command.arg("--version");
        command.env("PATH", augmented_path());
        match command.output().await {
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

    async fn account_usage(
        &self,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<Option<ProviderAccountUsage>> {
        fetch_codex_account_usage(runtime).await.map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opts_into_experimental_api_only_for_runtime_workspace_roots() {
        let initialize = initialize_params(true);
        assert_eq!(
            initialize
                .pointer("/capabilities/experimentalApi")
                .and_then(Value::as_bool),
            Some(true)
        );

        let thread = thread_start_params(
            "/tmp/app",
            &["/tmp/api".to_string(), "/tmp/shared".to_string()],
        );
        assert_eq!(
            thread
                .get("runtimeWorkspaceRoots")
                .and_then(Value::as_array)
                .expect("runtime roots should be sent"),
            &vec![json!("/tmp/app"), json!("/tmp/api"), json!("/tmp/shared")]
        );

        let stable_initialize = initialize_params(false);
        assert!(stable_initialize
            .pointer("/capabilities/experimentalApi")
            .is_none());
        let single_thread = thread_start_params("/tmp/app", &[]);
        assert!(single_thread.get("runtimeWorkspaceRoots").is_none());
    }

    #[test]
    fn separates_codex_agent_message_items() {
        let mut last_agent_message_id = None;

        let first = notification_to_event(
            "item/agentMessage/delta",
            &json!({
                "itemId": "msg_1",
                "delta": "Primeira mensagem.",
            }),
            &mut last_agent_message_id,
        );
        match first {
            Some(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "Primeira mensagem.");
            }
            other => panic!("expected first text delta, got {other:?}"),
        }

        let second = notification_to_event(
            "item/agentMessage/delta",
            &json!({
                "itemId": "msg_2",
                "delta": "Segunda mensagem.",
            }),
            &mut last_agent_message_id,
        );
        match second {
            Some(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "\n\nSegunda mensagem.");
            }
            other => panic!("expected separated text delta, got {other:?}"),
        }
    }

    #[test]
    fn recognizes_codex_reconnect_notices() {
        assert!(is_codex_reconnect_notice("Reconnecting... 2/5"));
        assert!(is_codex_reconnect_notice("Reconnecting… 1/5"));
        assert!(!is_codex_reconnect_notice("plain error"));
    }

    #[test]
    fn reads_retryable_error_message() {
        let payload = json!({
            "willRetry": true,
            "error": { "message": "Reconnecting... 2/5" }
        });
        assert_eq!(codex_error_message(&payload), "Reconnecting... 2/5");
    }

    #[test]
    fn parses_codex_account_usage_windows() {
        let usage = parse_codex_account_usage(&json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": 82.5,
                    "windowDurationMins": 300,
                    "resetsAt": 1_800_000_000
                },
                "secondary": {
                    "usedPercent": 25.0,
                    "windowDurationMins": 10_080,
                    "resetsAt": 1_800_100_000
                },
                "planType": "plus"
            }
        }))
        .expect("usage should parse");

        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[0].remaining_percent, 17.5);
        assert_eq!(usage.plan_type.as_deref(), Some("plus"));
    }

    #[tokio::test]
    async fn account_usage_response_timeout_is_not_extended_by_notifications() {
        let (reader, mut writer) = tokio::io::duplex(1_024);
        let writer_task = tokio::spawn(async move {
            loop {
                if writer
                    .write_all(b"{\"method\":\"status/changed\",\"params\":{}}\n")
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        let mut lines = BufReader::new(reader).lines();

        let error = read_rpc_response_with_timeout(&mut lines, 2, Duration::from_millis(25))
            .await
            .expect_err("notifications must not keep the request alive indefinitely");

        assert!(error.to_string().contains("timed out"));
        writer_task.abort();
    }
}
