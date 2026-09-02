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
    application::compose_wire_prompt_for_provider,
    domain::{
        provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
    },
    ports::{Input, Provider, ProviderTurnInput, SessionConfig},
    CoreError, Result,
};

use crate::common::{append_tool_instructions, apply_cli_spawn_environment, augmented_path};

const PROVIDER_ID: &str = "grok";
const PROVIDER_LABEL: &str = "Grok Build";
const PROVIDER_DESCRIPTION: &str = "Grok Build provider through the Agent Client Protocol.";

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

type PendingResponse = oneshot::Sender<std::result::Result<Value, String>>;

struct SessionRuntime {
    handle: SessionHandle,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    grok_session_id: Mutex<Option<String>>,
    pending: Arc<Mutex<HashMap<u64, PendingResponse>>>,
    next_id: AtomicU64,
    events_tx: broadcast::Sender<ProviderEvent>,
    reasoning_active: Mutex<bool>,
}

impl SessionRuntime {
    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;

        timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| CoreError::Provider(format!("grok {method} timed out")))?
            .map_err(|_| CoreError::Provider(format!("grok {method} cancelled")))?
            .map_err(|error| CoreError::Provider(format!("grok {method} error: {error}")))
    }

    async fn start_prompt(&self, prompt: String) -> Result<()> {
        let session_id = self
            .grok_session_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| CoreError::Provider("grok session has no session ID".to_string()))?;
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": prompt }],
            },
        }))
        .await?;

        let events_tx = self.events_tx.clone();
        tokio::spawn(async move {
            let at = now_iso();
            match timeout(Duration::from_secs(30 * 60), rx).await {
                Ok(Ok(Ok(_))) => {
                    let _ = events_tx.send(ProviderEvent::Completed { at });
                }
                Ok(Ok(Err(message))) => {
                    let _ = events_tx.send(ProviderEvent::Failed { message, at });
                }
                Ok(Err(_)) => {
                    let _ = events_tx.send(ProviderEvent::Failed {
                        message: "grok session/prompt response channel closed".to_string(),
                        at,
                    });
                }
                Err(_) => {
                    let _ = events_tx.send(ProviderEvent::Failed {
                        message: "grok session/prompt timed out".to_string(),
                        at,
                    });
                }
            }
        });

        Ok(())
    }

    async fn write_message(&self, message: Value) -> Result<()> {
        let serialized = message.to_string();
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(serialized.as_bytes())
            .await
            .map_err(|error| CoreError::Provider(format!("grok stdin write: {error}")))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|error| CoreError::Provider(format!("grok stdin newline: {error}")))?;
        stdin
            .flush()
            .await
            .map_err(|error| CoreError::Provider(format!("grok stdin flush: {error}")))
    }
}

#[derive(Default)]
struct AdapterState {
    sessions: Mutex<HashMap<String, Arc<SessionRuntime>>>,
}

#[derive(Clone)]
pub struct GrokAcpAdapter {
    id: ProviderId,
    capabilities: Capabilities,
    state: Arc<AdapterState>,
}

impl GrokAcpAdapter {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            id: ProviderId(PROVIDER_ID.to_string()),
            capabilities,
            state: Arc::new(AdapterState::default()),
        }
    }

    async fn session_runtime(&self, id: &SessionId) -> Option<Arc<SessionRuntime>> {
        self.state.sessions.lock().await.get(&id.0).cloned()
    }

    async fn start_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let mut command = Command::new("grok");
        command.arg("--no-auto-update");
        if let Some(model) = cfg
            .model
            .as_deref()
            .filter(|model| !model.trim().is_empty())
        {
            command.args(["--model", model]);
        }
        command.args(["agent", "stdio"]);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
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
            .map_err(|error| CoreError::Provider(format!("failed to spawn grok: {error}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CoreError::Provider("grok ACP missing stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Provider("grok ACP missing stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::Provider("grok ACP missing stderr".to_string()))?;

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
            grok_session_id: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            events_tx,
            reasoning_active: Mutex::new(false),
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
            return Err(error);
        }
        Ok(handle)
    }

    async fn handshake(runtime: &Arc<SessionRuntime>, cfg: &SessionConfig) -> Result<()> {
        let initialized = runtime
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientInfo": { "name": "dcc", "version": "0.1.8" },
                    "clientCapabilities": {},
                }),
            )
            .await?;
        let auth_method = select_auth_method(&initialized).ok_or_else(|| {
            CoreError::Provider(
                "Grok Build is not authenticated. Run `grok login` or set XAI_API_KEY.".to_string(),
            )
        })?;
        runtime
            .send_request(
                "authenticate",
                json!({ "methodId": auth_method, "_meta": { "headless": true } }),
            )
            .await?;

        let cwd = cfg
            .working_directory
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .unwrap_or(".");
        let session = runtime
            .send_request("session/new", json!({ "cwd": cwd, "mcpServers": [] }))
            .await?;
        let session_id = session
            .get("sessionId")
            .or_else(|| session.get("session_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Provider("grok session/new missing sessionId".to_string()))?
            .to_string();
        *runtime.grok_session_id.lock().await = Some(session_id);
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
            let stderr_task = tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                let mut output = String::new();
                while let Ok(Some(line)) = reader.next_line().await {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(line.trim_end());
                }
                output
            });

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
                            let result = match message.result {
                                Some(result) => Ok(result),
                                None => Err(message
                                    .error
                                    .as_ref()
                                    .and_then(|error| error.get("message"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("grok ACP error")
                                    .to_string()),
                            };
                            let _ = sender.send(result);
                        }
                        continue;
                    }
                }

                if let (Some(method), Some(params)) =
                    (message.method.as_deref(), message.params.as_ref())
                {
                    for event in
                        acp_notification_events(method, params, &runtime.reasoning_active).await
                    {
                        let _ = runtime.events_tx.send(event);
                    }
                }
            }

            let stderr_output = stderr_task.await.unwrap_or_default();
            let at = now_iso();
            let exit = runtime.child.lock().await.wait().await;
            for (_, sender) in runtime.pending.lock().await.drain() {
                let _ = sender.send(Err("grok ACP process exited".to_string()));
            }
            match exit {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    let message = if stderr_output.trim().is_empty() {
                        format!("grok exited with status {status}")
                    } else {
                        stderr_output.trim().to_string()
                    };
                    let _ = runtime
                        .events_tx
                        .send(ProviderEvent::Failed { message, at });
                }
                Err(error) => {
                    let _ = runtime.events_tx.send(ProviderEvent::Failed {
                        message: format!("failed to wait for grok: {error}"),
                        at,
                    });
                }
            }
            state.sessions.lock().await.remove(&session_key);
        });
    }
}

fn select_auth_method(initialized: &Value) -> Option<&'static str> {
    let auth_methods = initialized.get("authMethods")?.as_array()?;
    let has_method = |id| {
        auth_methods
            .iter()
            .any(|method| method.get("id").and_then(Value::as_str) == Some(id))
    };
    if std::env::var("XAI_API_KEY").is_ok_and(|key| !key.trim().is_empty())
        && has_method("xai.api_key")
    {
        Some("xai.api_key")
    } else if has_method("cached_token") {
        Some("cached_token")
    } else {
        None
    }
}

async fn acp_notification_events(
    method: &str,
    params: &Value,
    reasoning_active: &Mutex<bool>,
) -> Vec<ProviderEvent> {
    if method != "session/update" {
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
            .map(|content| match update_message_id(update) {
                Some(id) => ProviderEvent::AssistantMessageDelta { id, content },
                None => ProviderEvent::TextDelta { content },
            })
            .into_iter()
            .collect(),
        "agent_thought_chunk" | "agent_reasoning_chunk" => {
            let id = update_id(update, "reasoning");
            let mut active = reasoning_active.lock().await;
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
        "tool_call" | "tool_call_started" => vec![ProviderEvent::ToolCallStarted {
            id: update_id(update, "tool"),
            action: update
                .get("title")
                .or_else(|| update.get("toolName"))
                .or_else(|| update.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string(),
            command: update_command(update),
            file: update_file(update),
            at,
        }],
        "tool_call_update" => tool_call_update_events(update, at),
        _ => Vec::new(),
    }
}

fn tool_call_update_events(update: &Value, at: String) -> Vec<ProviderEvent> {
    let id = update_id(update, "tool");
    let status = update.get("status").and_then(Value::as_str).unwrap_or("");
    if matches!(status, "failed" | "error") {
        return vec![ProviderEvent::ToolCallFailed {
            id,
            reason: update
                .get("error")
                .and_then(|error| error.get("message"))
                .or_else(|| update.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string),
            at,
        }];
    }
    if matches!(status, "completed" | "complete" | "success") {
        return vec![ProviderEvent::ToolCallCompleted { id, at }];
    }
    update_text(update)
        .map(|content| ProviderEvent::ToolCallDelta { id, content })
        .into_iter()
        .collect()
}

fn update_id(update: &Value, fallback: &str) -> String {
    update
        .get("toolCallId")
        .or_else(|| update.get("tool_call_id"))
        .or_else(|| update.get("id"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn update_message_id(update: &Value) -> Option<String> {
    update
        .get("messageId")
        .or_else(|| update.get("message_id"))
        .or_else(|| update.pointer("/_meta/messageId"))
        .or_else(|| update.pointer("/_meta/message_id"))
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        })
        .map(str::to_string)
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

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn compose_grok_prompt(turn: &ProviderTurnInput) -> String {
    append_tool_instructions(
        compose_wire_prompt_for_provider(
            PROVIDER_ID,
            &turn.prompt,
            turn.plan_mode,
            turn.effort.as_deref(),
            turn.fast_mode,
        ),
        turn.tool_instructions.as_deref(),
    )
}

#[async_trait]
impl Provider for GrokAcpAdapter {
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
                    "no grok runtime for session {}",
                    handle.session_id.0
                ))
            })?;
        let prompt = match input {
            Input::Text(text) => text,
            Input::Turn(turn) => compose_grok_prompt(&turn),
            Input::UserInputResponse(_) => {
                return Err(CoreError::Provider(
                    "Grok ACP user-input responses are not supported yet".to_string(),
                ));
            }
            Input::PermissionResponse(_) => {
                return Err(CoreError::Provider(
                    "Grok ACP permission responses are not supported yet".to_string(),
                ));
            }
        };
        runtime.start_prompt(prompt).await
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
        let receiver = runtime.events_tx.subscribe();
        Box::pin(stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => return Some((Ok(event), receiver)),
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
                    "no grok runtime for session {}",
                    handle.session_id.0
                ))
            })?;
        let result = runtime
            .child
            .lock()
            .await
            .kill()
            .await
            .map_err(|error| CoreError::Provider(format!("failed to kill grok: {error}")));
        result
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        self.session_runtime(previous)
            .await
            .map(|runtime| runtime.handle.clone())
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no resumable grok runtime for session {}",
                    previous.0
                ))
            })
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        let mut command = Command::new("grok");
        command.arg("--version");
        command.env("PATH", augmented_path());
        match command.output().await {
            Ok(output) if output.status.success() => Ok(HealthStatus::Healthy),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                Ok(HealthStatus::Degraded {
                    reason: if !stderr.is_empty() { stderr } else { stdout },
                })
            }
            Err(error) => Ok(HealthStatus::Unhealthy {
                reason: format!("failed to execute grok: {error}"),
            }),
        }
    }
}

pub fn descriptor(
    health: HealthStatus,
    capabilities: Capabilities,
) -> dcc_core::domain::provider::ProviderDescriptor {
    dcc_core::domain::provider::ProviderDescriptor {
        id: ProviderId(PROVIDER_ID.to_string()),
        label: PROVIDER_LABEL.to_string(),
        description: PROVIDER_DESCRIPTION.to_string(),
        models: dcc_core::domain::model_registry::GROK
            .iter()
            .map(|model| model.to_descriptor())
            .collect(),
        capabilities,
        health,
        enabled: true,
        availability_generation: 0,
        stable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_tool_instructions_in_the_wire_prompt() {
        let prompt = compose_grok_prompt(&ProviderTurnInput {
            prompt: "Continue the task".to_string(),
            tool_instructions: Some("DCC handoff context".to_string()),
            plan_mode: None,
            effort: None,
            fast_mode: None,
            approval_policy: None,
        });
        assert!(prompt.contains("Continue the task"));
        assert!(prompt.contains("DCC handoff context"));
    }

    #[test]
    fn uses_cached_token_when_the_cli_exposes_it() {
        let initialized = json!({ "authMethods": [{ "id": "cached_token" }] });
        assert_eq!(select_auth_method(&initialized), Some("cached_token"));
    }

    #[tokio::test]
    async fn maps_acp_text_and_tool_updates_to_provider_events() {
        let reasoning_active = Mutex::new(false);
        let text = acp_notification_events(
            "session/update",
            &json!({ "update": { "sessionUpdate": "agent_message_chunk", "content": { "text": "Hello" } } }),
            &reasoning_active,
        )
        .await;
        assert!(
            matches!(text.as_slice(), [ProviderEvent::TextDelta { content }] if content == "Hello")
        );

        let identified_text = acp_notification_events(
            "session/update",
            &json!({ "update": { "sessionUpdate": "agent_message_chunk", "messageId": "550e8400-e29b-41d4-a716-446655440000", "content": { "text": "World" } } }),
            &reasoning_active,
        )
        .await;
        assert!(matches!(
            identified_text.as_slice(),
            [ProviderEvent::AssistantMessageDelta { id, content }]
                if id == "550e8400-e29b-41d4-a716-446655440000" && content == "World"
        ));

        let tool = acp_notification_events(
            "session/update",
            &json!({ "update": { "sessionUpdate": "tool_call", "toolCallId": "call_1", "title": "Bash", "rawInput": { "command": "pwd" } } }),
            &reasoning_active,
        )
        .await;
        assert!(
            matches!(tool.as_slice(), [ProviderEvent::ToolCallStarted { id, command: Some(command), .. }] if id == "call_1" && command == "pwd")
        );
    }

    #[test]
    fn maps_tool_completion_and_failure_updates() {
        let completed = tool_call_update_events(
            &json!({ "toolCallId": "call_1", "status": "completed" }),
            "now".to_string(),
        );
        assert!(
            matches!(completed.as_slice(), [ProviderEvent::ToolCallCompleted { id, .. }] if id == "call_1")
        );
        let failed = tool_call_update_events(
            &json!({ "toolCallId": "call_2", "status": "failed", "message": "denied" }),
            "now".to_string(),
        );
        assert!(
            matches!(failed.as_slice(), [ProviderEvent::ToolCallFailed { id, reason: Some(reason), .. }] if id == "call_2" && reason == "denied")
        );
    }
}
