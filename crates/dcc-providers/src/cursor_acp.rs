use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{self, BoxStream, StreamExt};
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
    application::{compose_fallback_prompt_for_provider, PromptInjectionOptions},
    domain::{
        mcp::{
            McpErrorCategory, McpRuntimeError, McpRuntimeState, McpRuntimeStatus,
            McpToolAnnotations, McpToolPolicyDecision, McpToolSummary,
        },
        provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
    },
    ports::{
        provider::{ProviderPermissionRequest, ProviderPermissionResponse},
        Input, Provider, ProviderTurnInput, SessionConfig,
    },
    CoreError, Result,
};

use crate::{
    common::{append_tool_instructions, apply_cli_spawn_environment},
    cursor::CursorProvider,
    cursor_mcp::{
        detect_cursor_mcp_projection_version, prepare_cursor_acp_session_request,
        CursorMcpDefinitionMap, CursorMcpToolPolicyMap, CURSOR_MCP_RUNTIME_VERSION,
        SUPPORTED_CURSOR_CLI_VERSION,
    },
};

const PROVIDER_ID: &str = "cursor";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const PROMPT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_PENDING_PERMISSIONS: usize = 64;

#[derive(Deserialize)]
struct Incoming {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
}

type PendingResponse = oneshot::Sender<std::result::Result<Value, ()>>;

#[derive(Clone)]
struct OwnedMcpToolCall {
    definition_id: dcc_core::domain::mcp::McpDefinitionId,
    tool_name: String,
}

struct PendingCursorPermission {
    rpc_id: Value,
    allow_once_option: Option<String>,
    reject_once_option: Option<String>,
}

struct CursorAcpSessionRuntime {
    handle: SessionHandle,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    cursor_session_id: Mutex<Option<String>>,
    pending: Arc<Mutex<HashMap<u64, PendingResponse>>>,
    pending_permissions: Mutex<HashMap<String, PendingCursorPermission>>,
    next_id: AtomicU64,
    events_tx: broadcast::Sender<ProviderEvent>,
    reasoning_active: Mutex<bool>,
    definitions_by_wire_name: RwLock<CursorMcpDefinitionMap>,
    tool_policies_by_definition: RwLock<CursorMcpToolPolicyMap>,
    owned_tool_calls: Mutex<HashMap<String, OwnedMcpToolCall>>,
    mcp_status_snapshot: RwLock<Option<Vec<McpRuntimeStatus>>>,
    current_mode: Mutex<String>,
}

impl CursorAcpSessionRuntime {
    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        if let Err(error) = self
            .write_message(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            }))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        await_response(method, rx).await
    }

    async fn send_mcp_session_new(
        &self,
        cfg: &SessionConfig,
        initialize_result: &Value,
    ) -> Result<Value> {
        let cwd = cfg
            .working_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CoreError::InvalidInput(
                    "Cursor ACP requires an absolute working directory".to_string(),
                )
            })?;
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let prepared = prepare_cursor_acp_session_request(
            id,
            cwd,
            &cfg.additional_working_directories,
            &cfg.mcp_servers,
            initialize_result,
            SUPPORTED_CURSOR_CLI_VERSION,
        )?;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let definitions = prepared.definitions_by_wire_name().clone();
        let policies = prepared.tool_policies_by_definition().clone();
        let write_result = {
            let mut stdin = self.stdin.lock().await;
            prepared.write_to(&mut *stdin).await
        };
        drop(prepared);
        if let Err(error) = write_result {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        *self
            .definitions_by_wire_name
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = definitions;
        *self
            .tool_policies_by_definition
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = policies;
        let result = await_response("session/new", rx).await?;
        self.publish_initial_mcp_statuses();
        Ok(result)
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    async fn send_server_result(&self, id: &Value, result: Value) -> Result<()> {
        if !valid_rpc_id(id) {
            return Err(CoreError::InvalidInput(
                "Cursor ACP request ID is invalid".to_string(),
            ));
        }
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
        .await
    }

    async fn write_message(&self, message: Value) -> Result<()> {
        let serialized = message.to_string();
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(serialized.as_bytes())
            .await
            .map_err(|_| cursor_channel_error())?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|_| cursor_channel_error())?;
        stdin.flush().await.map_err(|_| cursor_channel_error())
    }

    async fn start_prompt(&self, turn: ProviderTurnInput) -> Result<()> {
        let session_id =
            self.cursor_session_id.lock().await.clone().ok_or_else(|| {
                CoreError::Provider("Cursor ACP session is unavailable".to_string())
            })?;
        self.set_mode(&session_id, turn.plan_mode.unwrap_or(false))
            .await?;
        let prompt = append_tool_instructions(
            compose_fallback_prompt_for_provider(
                PROVIDER_ID,
                &turn.prompt,
                turn.plan_mode,
                turn.effort.as_deref(),
                turn.fast_mode,
                PromptInjectionOptions {
                    plan: false,
                    effort: true,
                    fast: true,
                },
            ),
            turn.tool_instructions.as_deref(),
        );
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        if let Err(error) = self
            .write_message(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "session/prompt",
                "params": {
                    "sessionId": session_id,
                    "prompt": [{ "type": "text", "text": prompt }],
                },
            }))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        let events_tx = self.events_tx.clone();
        tokio::spawn(async move {
            let at = now_iso();
            match timeout(PROMPT_TIMEOUT, rx).await {
                Ok(Ok(Ok(_))) => {
                    let _ = events_tx.send(ProviderEvent::Completed { at });
                }
                Ok(Ok(Err(()))) => {
                    let _ = events_tx.send(ProviderEvent::Failed {
                        message: "Cursor ACP prompt failed".to_string(),
                        at,
                    });
                }
                Ok(Err(_)) => {
                    let _ = events_tx.send(ProviderEvent::Failed {
                        message: "Cursor ACP prompt was cancelled".to_string(),
                        at,
                    });
                }
                Err(_) => {
                    let _ = events_tx.send(ProviderEvent::Failed {
                        message: "Cursor ACP prompt timed out".to_string(),
                        at,
                    });
                }
            }
        });
        Ok(())
    }

    async fn set_mode(&self, session_id: &str, plan: bool) -> Result<()> {
        let desired = if plan { "plan" } else { "agent" };
        if self.current_mode.lock().await.as_str() == desired {
            return Ok(());
        }
        self.send_request(
            "session/set_config_option",
            json!({
                "sessionId": session_id,
                "configId": "mode",
                "value": desired,
            }),
        )
        .await?;
        *self.current_mode.lock().await = desired.to_string();
        Ok(())
    }

    async fn resolve_permission(&self, response: ProviderPermissionResponse) -> Result<()> {
        let pending = self
            .pending_permissions
            .lock()
            .await
            .remove(&response.request_id)
            .ok_or_else(|| {
                CoreError::InvalidInput("Cursor ACP permission is not pending".to_string())
            })?;
        let option = match response.behavior.as_str() {
            "allow" => pending.allow_once_option,
            "deny" => pending.reject_once_option,
            _ => None,
        };
        let (result, resolved_behavior) = match option {
            Some(option_id) => (
                json!({
                    "outcome": {
                        "outcome": "selected",
                        "optionId": option_id,
                    }
                }),
                response.behavior,
            ),
            None => (
                json!({ "outcome": { "outcome": "cancelled" } }),
                "cancelled".to_string(),
            ),
        };
        self.send_server_result(&pending.rpc_id, result).await?;
        let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
            id: response.request_id,
            behavior: resolved_behavior,
            at: now_iso(),
        });
        Ok(())
    }

    fn publish_initial_mcp_statuses(&self) {
        let statuses = self
            .definitions_by_wire_name
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .cloned()
            .map(|definition_id| McpRuntimeStatus {
                definition_id,
                provider_id: self.handle.provider_id.clone(),
                provider_version: CURSOR_MCP_RUNTIME_VERSION.to_string(),
                session_id: self.handle.session_id.clone(),
                state: McpRuntimeState::AttachingProvider,
                tools: Vec::new(),
                checked_at: now_iso(),
                bounded_error: None,
            })
            .collect::<Vec<_>>();
        self.publish_mcp_statuses(statuses);
    }

    fn publish_observed_mcp_tool(&self, owned: &OwnedMcpToolCall) {
        let statuses = {
            let mut snapshot = self
                .mcp_status_snapshot
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let statuses = snapshot.get_or_insert_with(Vec::new);
            if let Some(status) = statuses
                .iter_mut()
                .find(|status| status.definition_id == owned.definition_id)
            {
                status.state = McpRuntimeState::Connected;
                status.checked_at = now_iso();
                if !status.tools.iter().any(|tool| tool.name == owned.tool_name) {
                    status.tools.push(McpToolSummary {
                        name: owned.tool_name.clone(),
                        annotations: McpToolAnnotations::default(),
                    });
                    status
                        .tools
                        .sort_unstable_by(|left, right| left.name.cmp(&right.name));
                }
            }
            statuses.clone()
        };
        let _ = self
            .events_tx
            .send(ProviderEvent::McpRuntimeStatusSnapshot { statuses });
    }

    fn publish_failed_mcp_statuses(&self) {
        let statuses = {
            let mut snapshot = self
                .mcp_status_snapshot
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let statuses = snapshot.get_or_insert_with(Vec::new);
            for status in statuses.iter_mut() {
                status.state = McpRuntimeState::Failed;
                status.tools.clear();
                status.checked_at = now_iso();
                status.bounded_error = Some(McpRuntimeError::bounded(
                    McpErrorCategory::Provider,
                    "Cursor ACP MCP session stopped",
                ));
            }
            statuses.clone()
        };
        let _ = self
            .events_tx
            .send(ProviderEvent::McpRuntimeStatusSnapshot { statuses });
    }

    fn publish_mcp_statuses(&self, statuses: Vec<McpRuntimeStatus>) {
        *self
            .mcp_status_snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(statuses.clone());
        let _ = self
            .events_tx
            .send(ProviderEvent::McpRuntimeStatusSnapshot { statuses });
    }

    fn latest_mcp_status_snapshot(&self) -> Option<Vec<McpRuntimeStatus>> {
        self.mcp_status_snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

async fn await_response(
    method: &str,
    receiver: oneshot::Receiver<std::result::Result<Value, ()>>,
) -> Result<Value> {
    timeout(REQUEST_TIMEOUT, receiver)
        .await
        .map_err(|_| CoreError::Provider(format!("Cursor ACP {method} timed out")))?
        .map_err(|_| CoreError::Provider(format!("Cursor ACP {method} was cancelled")))?
        .map_err(|()| CoreError::Provider(format!("Cursor ACP {method} failed")))
}

#[derive(Default)]
struct CursorAcpState {
    sessions: Mutex<HashMap<String, Arc<CursorAcpSessionRuntime>>>,
}

#[derive(Clone)]
pub(crate) struct CursorAcpAdapter {
    id: ProviderId,
    binary: String,
    capabilities: Capabilities,
    state: Arc<CursorAcpState>,
}

impl CursorAcpAdapter {
    fn new(binary: impl Into<String>, capabilities: Capabilities) -> Self {
        Self {
            id: ProviderId(PROVIDER_ID.to_string()),
            binary: binary.into(),
            capabilities,
            state: Arc::new(CursorAcpState::default()),
        }
    }

    async fn session_runtime(
        &self,
        session_id: &SessionId,
    ) -> Option<Arc<CursorAcpSessionRuntime>> {
        self.state.sessions.lock().await.get(&session_id.0).cloned()
    }

    async fn start_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        if cfg.mcp_servers.is_empty() {
            return Err(CoreError::InvalidInput(
                "Cursor ACP bridge requires a DCC MCP projection".to_string(),
            ));
        }
        let mut command = Command::new(&self.binary);
        command.arg("acp");
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);
        if let Some(cwd) = cfg
            .working_directory
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            command.current_dir(cwd);
        }
        apply_cli_spawn_environment(&mut command, PROVIDER_ID, &cfg)?;

        let mut child = command
            .spawn()
            .map_err(|_| CoreError::Provider("failed to start Cursor ACP".to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CoreError::Provider("Cursor ACP stdin is unavailable".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Provider("Cursor ACP stdout is unavailable".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::Provider("Cursor ACP stderr is unavailable".to_string()))?;
        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id.clone(),
            handle_id: Uuid::new_v4().to_string(),
        };
        let (events_tx, _) = broadcast::channel(128);
        let runtime = Arc::new(CursorAcpSessionRuntime {
            handle: handle.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            cursor_session_id: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            pending_permissions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            events_tx,
            reasoning_active: Mutex::new(false),
            definitions_by_wire_name: RwLock::new(HashMap::new()),
            tool_policies_by_definition: RwLock::new(HashMap::new()),
            owned_tool_calls: Mutex::new(HashMap::new()),
            mcp_status_snapshot: RwLock::new(None),
            current_mode: Mutex::new("agent".to_string()),
        });
        let session_key = cfg.session_id.0.clone();
        self.state
            .sessions
            .lock()
            .await
            .insert(session_key.clone(), runtime.clone());
        Self::spawn_reader(
            runtime.clone(),
            stdout,
            stderr,
            Arc::clone(&self.state),
            session_key.clone(),
        );
        if let Err(error) = Self::handshake(&runtime, &cfg).await {
            self.state.sessions.lock().await.remove(&session_key);
            let _ = runtime.child.lock().await.start_kill();
            return Err(error);
        }
        Ok(handle)
    }

    async fn handshake(runtime: &Arc<CursorAcpSessionRuntime>, cfg: &SessionConfig) -> Result<()> {
        let initialized = runtime
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientInfo": { "name": "dcc", "version": env!("CARGO_PKG_VERSION") },
                    "clientCapabilities": {},
                }),
            )
            .await?;
        let cursor_login = initialized
            .get("authMethods")
            .and_then(Value::as_array)
            .is_some_and(|methods| {
                methods
                    .iter()
                    .any(|method| method.get("id").and_then(Value::as_str) == Some("cursor_login"))
            });
        if !cursor_login {
            return Err(CoreError::Provider(
                "Cursor Agent is not authenticated".to_string(),
            ));
        }
        runtime
            .send_request("authenticate", json!({ "methodId": "cursor_login" }))
            .await?;
        let session = runtime.send_mcp_session_new(cfg, &initialized).await?;
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                CoreError::Provider("Cursor ACP session/new omitted sessionId".to_string())
            })?
            .to_string();
        *runtime.cursor_session_id.lock().await = Some(session_id.clone());
        configure_cursor_model(runtime, &session_id, cfg.model.as_deref(), &session).await
    }

    fn spawn_reader(
        runtime: Arc<CursorAcpSessionRuntime>,
        stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr,
        state: Arc<CursorAcpState>,
        session_key: String,
    ) {
        tokio::spawn(async move {
            let _ = runtime
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });
            let stderr_task = tokio::spawn(discard_stderr(stderr));
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(message) = serde_json::from_str::<Incoming>(trimmed) else {
                    continue;
                };
                if let Some(id) = message.id.as_ref().and_then(Value::as_u64) {
                    if message.result.is_some() || message.error.is_some() {
                        if let Some(sender) = runtime.pending.lock().await.remove(&id) {
                            let result = message.result.ok_or(());
                            let _ = sender.send(result);
                        }
                        continue;
                    }
                }
                if let (Some(id), Some(method)) = (message.id.as_ref(), message.method.as_deref()) {
                    let params = message.params.as_ref().unwrap_or(&Value::Null);
                    if method == "session/request_permission" {
                        handle_cursor_permission_request(&runtime, id, params).await;
                    } else if valid_rpc_id(id) {
                        let _ = runtime
                            .send_server_result(
                                id,
                                json!({ "outcome": { "outcome": "cancelled" } }),
                            )
                            .await;
                    }
                    continue;
                }
                if let (Some(method), Some(params)) =
                    (message.method.as_deref(), message.params.as_ref())
                {
                    for event in cursor_acp_notification_events(&runtime, method, params).await {
                        let _ = runtime.events_tx.send(event);
                    }
                }
            }
            runtime.publish_failed_mcp_statuses();
            for (_, sender) in runtime.pending.lock().await.drain() {
                let _ = sender.send(Err(()));
            }
            cancel_pending_permissions(&runtime).await;
            let _ = runtime.child.lock().await.start_kill();
            let exit = runtime.child.lock().await.wait().await;
            let _ = stderr_task.await;
            if !matches!(exit, Ok(status) if status.success()) {
                let _ = runtime.events_tx.send(ProviderEvent::Failed {
                    message: "Cursor ACP process stopped unexpectedly".to_string(),
                    at: now_iso(),
                });
            }
            state.sessions.lock().await.remove(&session_key);
        });
    }
}

async fn configure_cursor_model(
    runtime: &Arc<CursorAcpSessionRuntime>,
    session_id: &str,
    requested: Option<&str>,
    session_result: &Value,
) -> Result<()> {
    let requested = requested.map(str::trim).filter(|value| {
        !value.is_empty()
            && !matches!(
                value.to_ascii_lowercase().as_str(),
                "auto" | "default" | "cursor-agent" | "cursor-editor"
            )
    });
    let Some(requested) = requested else {
        return Ok(());
    };
    let config = session_result
        .get("configOptions")
        .and_then(Value::as_array)
        .and_then(|options| {
            options.iter().find(|option| {
                option.get("id").and_then(Value::as_str) == Some("model")
                    || option.get("category").and_then(Value::as_str) == Some("model")
            })
        })
        .ok_or_else(|| {
            CoreError::Provider("Cursor ACP model inventory is unavailable".to_string())
        })?;
    let matches = config
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| {
            let value = option.get("value").and_then(Value::as_str)?;
            let name = option.get("name").and_then(Value::as_str);
            (value == requested || name == Some(requested)).then(|| value.to_string())
        })
        .collect::<HashSet<_>>();
    let matches = matches.into_iter().collect::<Vec<_>>();
    let [selected] = matches.as_slice() else {
        return Err(CoreError::Provider(
            "Cursor ACP model selection is ambiguous or unsupported".to_string(),
        ));
    };
    runtime
        .send_request(
            "session/set_config_option",
            json!({
                "sessionId": session_id,
                "configId": "model",
                "value": selected,
            }),
        )
        .await?;
    Ok(())
}

async fn cursor_acp_notification_events(
    runtime: &Arc<CursorAcpSessionRuntime>,
    method: &str,
    params: &Value,
) -> Vec<ProviderEvent> {
    if method != "session/update"
        || params.get("sessionId").and_then(Value::as_str)
            != runtime.cursor_session_id.lock().await.as_deref()
    {
        return Vec::new();
    }
    let Some(update) = params.get("update") else {
        return Vec::new();
    };
    let kind = update
        .get("sessionUpdate")
        .or_else(|| update.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let at = now_iso();
    match kind {
        "agent_message_chunk" => update_text(update)
            .map(|content| ProviderEvent::TextDelta { content })
            .into_iter()
            .collect(),
        "agent_thought_chunk" | "agent_reasoning_chunk" => {
            let id = update_id(update, "reasoning");
            let mut active = runtime.reasoning_active.lock().await;
            let mut events = Vec::new();
            if !*active {
                *active = true;
                events.push(ProviderEvent::ReasoningStarted {
                    id: id.clone(),
                    label: Some("Thinking".to_string()),
                    at: at.clone(),
                });
            }
            if let Some(content) = update_text(update) {
                events.push(ProviderEvent::ReasoningDelta { id, content });
            }
            events
        }
        "tool_call" | "tool_call_started" => {
            let id = update_id(update, "tool");
            let owned = structured_tool_call_id(update)
                .and_then(|_| claim_owned_mcp_tool(update, &runtime.definitions_by_wire_name));
            if let Some(owned) = owned {
                runtime
                    .owned_tool_calls
                    .lock()
                    .await
                    .insert(id.clone(), owned.clone());
                runtime.publish_observed_mcp_tool(&owned);
                vec![ProviderEvent::ToolCallStarted {
                    id,
                    action: owned.tool_name,
                    command: None,
                    file: None,
                    at,
                }]
            } else {
                vec![ProviderEvent::ToolCallStarted {
                    id,
                    action: update
                        .get("title")
                        .or_else(|| update.get("kind"))
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string(),
                    command: update_command(update),
                    file: update_file(update),
                    at,
                }]
            }
        }
        "tool_call_update" => cursor_tool_call_update_events(runtime, update, at).await,
        _ => Vec::new(),
    }
}

async fn cursor_tool_call_update_events(
    runtime: &Arc<CursorAcpSessionRuntime>,
    update: &Value,
    at: String,
) -> Vec<ProviderEvent> {
    let id = update_id(update, "tool");
    let status = update.get("status").and_then(Value::as_str).unwrap_or("");
    let owned = runtime.owned_tool_calls.lock().await.contains_key(&id);
    if matches!(status, "failed" | "error") {
        runtime.owned_tool_calls.lock().await.remove(&id);
        return vec![ProviderEvent::ToolCallFailed {
            id,
            reason: (!owned).then(|| "Cursor tool call failed".to_string()),
            at,
        }];
    }
    if matches!(status, "completed" | "complete" | "success") {
        runtime.owned_tool_calls.lock().await.remove(&id);
        return vec![ProviderEvent::ToolCallCompleted { id, at }];
    }
    if owned {
        Vec::new()
    } else {
        update_text(update)
            .map(|content| ProviderEvent::ToolCallDelta { id, content })
            .into_iter()
            .collect()
    }
}

async fn handle_cursor_permission_request(
    runtime: &Arc<CursorAcpSessionRuntime>,
    rpc_id: &Value,
    params: &Value,
) {
    let request_id = Uuid::new_v4().to_string();
    let session_matches = params.get("sessionId").and_then(Value::as_str)
        == runtime.cursor_session_id.lock().await.as_deref();
    let tool_call = params.get("toolCall");
    let call_id = tool_call
        .and_then(structured_tool_call_id)
        .unwrap_or_default();
    let active = runtime.owned_tool_calls.lock().await.get(call_id).cloned();
    let claimed = tool_call
        .and_then(|tool_call| claim_owned_mcp_tool(tool_call, &runtime.definitions_by_wire_name));
    let owned = match (active, claimed) {
        (Some(active), Some(claimed))
            if active.definition_id == claimed.definition_id
                && active.tool_name == claimed.tool_name =>
        {
            Some(active)
        }
        _ => None,
    };
    let options = permission_options(params);
    let valid = session_matches && valid_rpc_id(rpc_id);
    let Some(owned) = owned.filter(|_| valid) else {
        if valid_rpc_id(rpc_id) {
            let _ = runtime
                .send_server_result(rpc_id, json!({ "outcome": { "outcome": "cancelled" } }))
                .await;
        }
        return;
    };
    let request = ProviderPermissionRequest {
        request_id: request_id.clone(),
        tool_name: owned.tool_name.clone(),
        title: Some(format!("Allow MCP tool {}", owned.tool_name)),
        description: None,
        command: None,
        file: None,
    };
    let decision = cursor_tool_policy(
        &runtime.tool_policies_by_definition,
        &owned.definition_id,
        &owned.tool_name,
    );
    if matches!(
        decision,
        McpToolPolicyDecision::Allow | McpToolPolicyDecision::Deny
    ) {
        let behavior = if decision == McpToolPolicyDecision::Allow {
            "allow"
        } else {
            "deny"
        };
        let option = if decision == McpToolPolicyDecision::Allow {
            options.0
        } else {
            options.1
        };
        let selected = option.is_some();
        let result = option
            .map(|option_id| {
                json!({
                    "outcome": {
                        "outcome": "selected",
                        "optionId": option_id,
                    }
                })
            })
            .unwrap_or_else(|| json!({ "outcome": { "outcome": "cancelled" } }));
        if runtime.send_server_result(rpc_id, result).await.is_ok() {
            let _ = runtime.events_tx.send(ProviderEvent::PermissionRequested {
                request,
                at: now_iso(),
            });
            let _ = runtime.events_tx.send(ProviderEvent::PermissionResolved {
                id: request_id,
                behavior: if selected {
                    behavior.to_string()
                } else {
                    "cancelled".to_string()
                },
                at: now_iso(),
            });
        }
        return;
    }
    let pending = PendingCursorPermission {
        rpc_id: rpc_id.clone(),
        allow_once_option: options.0,
        reject_once_option: options.1,
    };
    let inserted = {
        let mut pending_permissions = runtime.pending_permissions.lock().await;
        if pending_permissions.len() >= MAX_PENDING_PERMISSIONS {
            false
        } else {
            pending_permissions.insert(request_id.clone(), pending);
            true
        }
    };
    if inserted {
        let _ = runtime.events_tx.send(ProviderEvent::PermissionRequested {
            request,
            at: now_iso(),
        });
    } else {
        let _ = runtime
            .send_server_result(rpc_id, json!({ "outcome": { "outcome": "cancelled" } }))
            .await;
    }
}

fn permission_options(params: &Value) -> (Option<String>, Option<String>) {
    let mut allow_once = HashSet::new();
    let mut reject_once = HashSet::new();
    for option in params
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(option_id) = option.get("optionId").and_then(Value::as_str) else {
            continue;
        };
        if option_id.is_empty() || option_id.len() > 256 || option_id.chars().any(char::is_control)
        {
            continue;
        }
        match option.get("kind").and_then(Value::as_str) {
            Some("allow_once") => {
                allow_once.insert(option_id.to_string());
            }
            Some("reject_once") => {
                reject_once.insert(option_id.to_string());
            }
            _ => {}
        }
    }
    (only_value(allow_once), only_value(reject_once))
}

fn cursor_tool_policy(
    policies: &RwLock<CursorMcpToolPolicyMap>,
    definition_id: &dcc_core::domain::mcp::McpDefinitionId,
    tool_name: &str,
) -> McpToolPolicyDecision {
    policies
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(definition_id)
        .and_then(|tools| tools.get(tool_name))
        .cloned()
        .unwrap_or(McpToolPolicyDecision::Ask)
}

fn claim_owned_mcp_tool(
    value: &Value,
    definitions: &RwLock<CursorMcpDefinitionMap>,
) -> Option<OwnedMcpToolCall> {
    let server_name = structured_string(
        value,
        &[
            "serverName",
            "mcpServerName",
            "server_name",
            "mcp_server_name",
        ],
    )?;
    let tool_name = structured_string(value, &["toolName", "tool_name"])?;
    if tool_name.is_empty()
        || tool_name.len() > 128
        || !tool_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return None;
    }
    let definition_id = definitions
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(server_name)?
        .clone();
    Some(OwnedMcpToolCall {
        definition_id,
        tool_name: tool_name.to_string(),
    })
}

fn structured_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let direct = keys
        .iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str));
    let metadata = keys.iter().filter_map(|key| {
        value
            .get("_meta")
            .and_then(|metadata| metadata.get(*key))
            .and_then(Value::as_str)
    });
    let values = direct
        .chain(metadata)
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    let values = values.into_iter().collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        return None;
    };
    Some(value)
}

async fn cancel_pending_permissions(runtime: &Arc<CursorAcpSessionRuntime>) {
    let pending = runtime
        .pending_permissions
        .lock()
        .await
        .drain()
        .collect::<Vec<_>>();
    for (request_id, permission) in pending {
        let _ = runtime
            .send_server_result(
                &permission.rpc_id,
                json!({ "outcome": { "outcome": "cancelled" } }),
            )
            .await;
        let _ = runtime.events_tx.send(ProviderEvent::PermissionResolved {
            id: request_id,
            behavior: "cancelled".to_string(),
            at: now_iso(),
        });
    }
}

fn valid_rpc_id(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.as_u64().is_some(),
        Value::String(value) => {
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        }
        _ => false,
    }
}

fn only_value(values: HashSet<String>) -> Option<String> {
    let values = values.into_iter().collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        return None;
    };
    Some(value.clone())
}

fn structured_tool_call_id(update: &Value) -> Option<&str> {
    let id = structured_string(update, &["toolCallId", "tool_call_id", "id"])?;
    (!id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)).then_some(id)
}

fn update_id(update: &Value, fallback: &str) -> String {
    update
        .get("toolCallId")
        .or_else(|| update.get("tool_call_id"))
        .or_else(|| update.get("id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .unwrap_or(fallback)
        .to_string()
}

fn update_text(update: &Value) -> Option<String> {
    update
        .get("content")
        .and_then(|content| content.get("text").or(Some(content)))
        .or_else(|| update.get("text"))
        .or_else(|| update.get("delta"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn update_command(update: &Value) -> Option<String> {
    update
        .get("rawInput")
        .or_else(|| update.get("input"))
        .or_else(|| update.get("command"))
        .and_then(|input| {
            input.as_str().map(str::to_string).or_else(|| {
                input
                    .get("command")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        })
}

fn update_file(update: &Value) -> Option<String> {
    update
        .get("locations")
        .and_then(Value::as_array)
        .and_then(|locations| locations.first())
        .and_then(|location| location.get("path"))
        .or_else(|| update.get("path"))
        .or_else(|| update.get("filePath"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

async fn discard_stderr(mut stderr: tokio::process::ChildStderr) {
    let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
}

fn cursor_channel_error() -> CoreError {
    CoreError::Provider("Cursor ACP private channel failed".to_string())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

#[async_trait]
impl Provider for CursorAcpAdapter {
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
            .ok_or_else(|| CoreError::Provider("Cursor ACP runtime is unavailable".to_string()))?;
        match input {
            Input::Text(prompt) => {
                runtime
                    .start_prompt(ProviderTurnInput {
                        prompt,
                        tool_instructions: None,
                        plan_mode: None,
                        effort: None,
                        fast_mode: None,
                        approval_policy: None,
                    })
                    .await
            }
            Input::Turn(turn) => runtime.start_prompt(turn).await,
            Input::PermissionResponse(response) => runtime.resolve_permission(response).await,
            Input::UserInputResponse(_) => Err(CoreError::Provider(
                "Cursor ACP user-input responses are not supported".to_string(),
            )),
        }
    }

    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
        let runtime = self
            .state
            .sessions
            .try_lock()
            .ok()
            .and_then(|sessions| sessions.get(&handle.session_id.0).cloned());
        let Some(runtime) = runtime else {
            return Box::pin(stream::empty());
        };
        let initial = runtime
            .latest_mcp_status_snapshot()
            .map(|statuses| ProviderEvent::McpRuntimeStatusSnapshot { statuses });
        let receiver = runtime.events_tx.subscribe();
        let live = stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => return Some((Ok(event), receiver)),
                    Err(broadcast::error::RecvError::Closed) => return None,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });
        Box::pin(stream::iter(initial.map(Ok)).chain(live))
    }

    async fn cancel(&self, handle: &SessionHandle) -> Result<()> {
        let runtime = self
            .session_runtime(&handle.session_id)
            .await
            .ok_or_else(|| CoreError::Provider("Cursor ACP runtime is unavailable".to_string()))?;
        cancel_pending_permissions(&runtime).await;
        if let Some(session_id) = runtime.cursor_session_id.lock().await.clone() {
            runtime
                .send_notification("session/cancel", json!({ "sessionId": session_id }))
                .await?;
        }
        Ok(())
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        self.session_runtime(previous)
            .await
            .map(|runtime| runtime.handle.clone())
            .ok_or_else(|| CoreError::Provider("Cursor ACP runtime is unavailable".to_string()))
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::Healthy)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorRoute {
    Legacy,
    Acp,
}

#[derive(Clone)]
pub struct CursorBridgeProvider {
    legacy: CursorProvider,
    acp: CursorAcpAdapter,
    capabilities: Capabilities,
    mcp_projection_version: Option<&'static str>,
    routes: Arc<RwLock<HashMap<String, CursorRoute>>>,
}

impl CursorBridgeProvider {
    pub(crate) fn new(
        binary: impl Into<String>,
        capabilities: Capabilities,
        legacy: CursorProvider,
    ) -> Self {
        let binary = binary.into();
        Self::with_projection_version(
            binary.clone(),
            capabilities.clone(),
            legacy,
            detect_cursor_mcp_projection_version(&binary),
        )
    }

    fn with_projection_version(
        binary: String,
        capabilities: Capabilities,
        legacy: CursorProvider,
        mcp_projection_version: Option<&'static str>,
    ) -> Self {
        Self {
            legacy,
            acp: CursorAcpAdapter::new(binary, capabilities.clone()),
            capabilities,
            mcp_projection_version,
            routes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn route(&self, session_id: &SessionId) -> Option<CursorRoute> {
        self.routes
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&session_id.0)
            .copied()
    }
}

#[async_trait]
impl Provider for CursorBridgeProvider {
    fn id(&self) -> ProviderId {
        ProviderId(PROVIDER_ID.to_string())
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    fn dcc_mcp_projection_version(&self) -> Option<&str> {
        self.mcp_projection_version
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let route = if cfg.mcp_servers.is_empty() {
            CursorRoute::Legacy
        } else {
            if self.mcp_projection_version != Some(CURSOR_MCP_RUNTIME_VERSION) {
                return Err(CoreError::Provider(
                    "Cursor MCP requires the audited Cursor ACP runtime".to_string(),
                ));
            }
            CursorRoute::Acp
        };
        let handle = match route {
            CursorRoute::Legacy => self.legacy.prepare_session(cfg).await?,
            CursorRoute::Acp => self.acp.prepare_session(cfg).await?,
        };
        self.routes
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(handle.session_id.0.clone(), route);
        Ok(handle)
    }

    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()> {
        match self.route(&handle.session_id) {
            Some(CursorRoute::Legacy) => self.legacy.send_input(handle, input).await,
            Some(CursorRoute::Acp) => self.acp.send_input(handle, input).await,
            None => Err(CoreError::Provider(
                "Cursor session route is unavailable".to_string(),
            )),
        }
    }

    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
        match self.route(&handle.session_id) {
            Some(CursorRoute::Legacy) => self.legacy.stream_events(handle),
            Some(CursorRoute::Acp) => self.acp.stream_events(handle),
            None => Box::pin(stream::empty()),
        }
    }

    async fn cancel(&self, handle: &SessionHandle) -> Result<()> {
        match self.route(&handle.session_id) {
            Some(CursorRoute::Legacy) => self.legacy.cancel(handle).await,
            Some(CursorRoute::Acp) => self.acp.cancel(handle).await,
            None => Ok(()),
        }
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        match self.route(previous) {
            Some(CursorRoute::Legacy) => self.legacy.resume(previous).await,
            Some(CursorRoute::Acp) => self.acp.resume(previous).await,
            None => Err(CoreError::Provider(
                "Cursor session route is unavailable".to_string(),
            )),
        }
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        self.legacy.healthcheck().await
    }
}

#[cfg(test)]
mod tests {
    use dcc_core::{
        domain::mcp::{McpDefinitionId, McpToolPolicyDecision},
        ports::ProviderMcpToolPolicy,
    };

    use super::*;

    fn definitions() -> RwLock<CursorMcpDefinitionMap> {
        RwLock::new(HashMap::from([(
            "dcc-wire".to_string(),
            McpDefinitionId("fixture".to_string()),
        )]))
    }

    #[test]
    fn claims_only_structured_unambiguous_dcc_owned_tool_calls() {
        let definitions = definitions();
        let claimed = claim_owned_mcp_tool(
            &json!({
                "toolCallId": "call-1",
                "serverName": "dcc-wire",
                "toolName": "fixture.echo",
                "title": "anything"
            }),
            &definitions,
        )
        .expect("owned tool");
        assert_eq!(claimed.definition_id.0, "fixture");
        assert_eq!(claimed.tool_name, "fixture.echo");

        assert!(claim_owned_mcp_tool(
            &json!({
                "serverName": "user-server",
                "toolName": "fixture.echo"
            }),
            &definitions,
        )
        .is_none());
        assert!(claim_owned_mcp_tool(
            &json!({
                "title": "dcc-wire__fixture.echo"
            }),
            &definitions,
        )
        .is_none());
        assert!(claim_owned_mcp_tool(
            &json!({
                "serverName": "dcc-wire",
                "name": "fixture.echo"
            }),
            &definitions,
        )
        .is_none());
        assert!(claim_owned_mcp_tool(
            &json!({
                "serverName": "dcc-wire",
                "_meta": { "serverName": "different" },
                "toolName": "fixture.echo"
            }),
            &definitions,
        )
        .is_none());
    }

    #[test]
    fn tool_call_ids_must_be_structured_and_unambiguous() {
        assert_eq!(
            structured_tool_call_id(&json!({ "toolCallId": "call-1" })),
            Some("call-1")
        );
        assert_eq!(structured_tool_call_id(&json!({ "title": "call-1" })), None);
        assert_eq!(
            structured_tool_call_id(&json!({
                "toolCallId": "call-1",
                "_meta": { "toolCallId": "call-2" }
            })),
            None
        );
    }

    #[test]
    fn selects_only_once_scoped_permission_options() {
        let (allow, deny) = permission_options(&json!({
            "options": [
                { "optionId": "always", "kind": "allow_always" },
                { "optionId": "yes", "kind": "allow_once" },
                { "optionId": "never", "kind": "reject_always" },
                { "optionId": "no", "kind": "reject_once" }
            ]
        }));
        assert_eq!(allow.as_deref(), Some("yes"));
        assert_eq!(deny.as_deref(), Some("no"));
    }

    #[test]
    fn ambiguous_once_scoped_permission_options_fail_closed() {
        let (allow, deny) = permission_options(&json!({
            "options": [
                { "optionId": "yes-1", "kind": "allow_once" },
                { "optionId": "yes-2", "kind": "allow_once" },
                { "optionId": "no", "kind": "reject_once" }
            ]
        }));
        assert_eq!(allow, None);
        assert_eq!(deny.as_deref(), Some("no"));
    }

    #[test]
    fn unknown_tools_default_to_ask_and_explicit_policies_are_exact() {
        let policies = RwLock::new(HashMap::from([(
            McpDefinitionId("fixture".to_string()),
            HashMap::from([("fixture.mutate".to_string(), McpToolPolicyDecision::Deny)]),
        )]));
        assert_eq!(
            cursor_tool_policy(
                &policies,
                &McpDefinitionId("fixture".to_string()),
                "fixture.mutate"
            ),
            McpToolPolicyDecision::Deny
        );
        assert_eq!(
            cursor_tool_policy(
                &policies,
                &McpDefinitionId("fixture".to_string()),
                "fixture.echo"
            ),
            McpToolPolicyDecision::Ask
        );
        let _ = ProviderMcpToolPolicy {
            tool_name: "fixture.mutate".to_string(),
            decision: McpToolPolicyDecision::Deny,
        };
    }

    #[test]
    fn hybrid_bridge_exposes_projection_only_for_an_audited_runtime() {
        let capabilities = crate::common::experimental_cli_capabilities();
        let legacy = CursorProvider::new("cursor", "cursor-agent", capabilities.clone());
        let unsupported = CursorBridgeProvider::with_projection_version(
            "cursor-agent".to_string(),
            capabilities.clone(),
            legacy.clone(),
            None,
        );
        assert_eq!(unsupported.dcc_mcp_projection_version(), None);
        let supported = CursorBridgeProvider::with_projection_version(
            "cursor-agent".to_string(),
            capabilities,
            legacy,
            Some(CURSOR_MCP_RUNTIME_VERSION),
        );
        assert_eq!(
            supported.dcc_mcp_projection_version(),
            Some(CURSOR_MCP_RUNTIME_VERSION)
        );
    }
}
