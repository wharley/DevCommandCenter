use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::{Child, Command},
    sync::{broadcast, Mutex},
};
use uuid::Uuid;

use dcc_core::{
    application::{compose_fallback_prompt_for_provider, PromptInjectionOptions},
    domain::{
        model_registry,
        provider::{
            Capabilities, HealthStatus, ProviderDescriptor, ProviderEvent, ProviderId,
            SessionHandle,
        },
        session::{AssistantMessagePhase, SessionId},
    },
    ports::{Input, Provider, SessionConfig},
    CoreError, Result,
};

use crate::common::{
    append_tool_instructions, apply_cli_spawn_environment, augmented_path, now_iso,
    stable_cli_capabilities,
};

const PROVIDER_ID: &str = "droid";
const PROVIDER_LABEL: &str = "Droid";
const PROVIDER_DESCRIPTION: &str = "Factory Droid exec provider for workspace coding workflows.";
const DROID_AUTONOMY_LEVEL: &str = "medium";

#[derive(Clone)]
pub struct DroidProvider {
    id: ProviderId,
    binary: String,
    capabilities: Capabilities,
    runtime: Arc<ProviderRuntimeState>,
}

#[derive(Default)]
struct ProviderRuntimeState {
    sessions: Mutex<HashMap<String, Arc<DroidSessionRuntime>>>,
}

struct DroidSessionRuntime {
    handle: SessionHandle,
    cfg: SessionConfig,
    model: Option<String>,
    cwd: PathBuf,
    remote_session_id: Mutex<Option<String>>,
    active_turn: Mutex<Option<ActiveTurn>>,
    events_tx: broadcast::Sender<ProviderEvent>,
}

struct ActiveTurn {
    child: Arc<Mutex<Child>>,
}

#[derive(Default)]
struct DroidStreamState {
    active_assistant_message_id: Option<String>,
    active_assistant_text: String,
    legacy_assistant_text: String,
    reasoning_started: HashMap<String, bool>,
}

pub fn adapter() -> DroidProvider {
    DroidProvider::new(PROVIDER_ID, "droid", stable_cli_capabilities())
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId(PROVIDER_ID.to_string()),
        label: PROVIDER_LABEL.to_string(),
        description: PROVIDER_DESCRIPTION.to_string(),
        models: model_registry::DROID
            .iter()
            .map(|m| m.to_descriptor())
            .collect(),
        capabilities: stable_cli_capabilities(),
        health,
        enabled: true,
        availability_generation: 0,
        stable: true,
    }
}

impl DroidProvider {
    pub fn new(
        id: impl Into<String>,
        binary: impl Into<String>,
        capabilities: Capabilities,
    ) -> Self {
        Self {
            id: ProviderId(id.into()),
            binary: binary.into(),
            capabilities,
            runtime: Arc::new(ProviderRuntimeState::default()),
        }
    }

    fn binary_command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.arg("-v");
        command.env("PATH", augmented_path());
        command
    }

    fn turn_command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command
    }

    async fn prepare_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id.clone(),
            handle_id: Uuid::new_v4().to_string(),
        };
        let cwd = cfg
            .working_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let (events_tx, _) = broadcast::channel(128);
        let runtime = Arc::new(DroidSessionRuntime {
            handle: handle.clone(),
            cfg: cfg.clone(),
            model: cfg.model.clone(),
            cwd,
            remote_session_id: Mutex::new(None),
            active_turn: Mutex::new(None),
            events_tx,
        });
        self.runtime
            .sessions
            .lock()
            .await
            .insert(cfg.session_id.0.clone(), runtime);
        Ok(handle)
    }

    async fn runtime_for_session(
        &self,
        session_id: &SessionId,
    ) -> Option<Arc<DroidSessionRuntime>> {
        self.runtime
            .sessions
            .lock()
            .await
            .get(&session_id.0)
            .cloned()
    }

    fn normalize_model_arg(model: Option<&str>) -> Option<String> {
        let trimmed = model.map(str::trim).unwrap_or_default();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
            return None;
        }
        Some(trimmed.to_string())
    }

    fn normalize_reasoning_arg(effort: Option<&str>) -> Option<&'static str> {
        match effort.map(str::trim).filter(|value| !value.is_empty()) {
            Some("minimal") | Some("low") => Some("low"),
            Some("medium") => Some("medium"),
            Some("high") | Some("xhigh") | Some("max") => Some("high"),
            Some("off") | Some("none") => Some("none"),
            _ => None,
        }
    }

    fn build_turn_args(
        &self,
        remote_session_id: Option<&str>,
        model: Option<&str>,
        prompt: &str,
        plan_mode: Option<bool>,
        effort: Option<&str>,
    ) -> Vec<String> {
        let mut args = vec![
            "exec".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--auto".to_string(),
            DROID_AUTONOMY_LEVEL.to_string(),
        ];
        if let Some(remote_session_id) = remote_session_id.filter(|value| !value.trim().is_empty())
        {
            args.push("--session-id".to_string());
            args.push(remote_session_id.to_string());
        }
        if plan_mode.unwrap_or(false) {
            args.push("--use-spec".to_string());
        }
        if let Some(reasoning) = Self::normalize_reasoning_arg(effort) {
            args.push("--reasoning-effort".to_string());
            args.push(reasoning.to_string());
        }
        if let Some(model) = Self::normalize_model_arg(model) {
            args.push("--model".to_string());
            args.push(model);
        }
        args.push(prompt.to_string());
        args
    }

    async fn spawn_turn(
        &self,
        runtime: Arc<DroidSessionRuntime>,
        prompt: String,
        plan_mode: Option<bool>,
        effort: Option<&str>,
    ) -> Result<()> {
        {
            let active_turn = runtime.active_turn.lock().await;
            if active_turn.is_some() {
                return Err(CoreError::Provider(
                    "Droid turn already active for this session".to_string(),
                ));
            }
        }

        let remote_session_id = runtime.remote_session_id.lock().await.clone();
        let args = self.build_turn_args(
            remote_session_id.as_deref(),
            runtime.model.as_deref(),
            &prompt,
            plan_mode,
            effort,
        );

        let mut command = self.turn_command();
        command.args(&args);
        command.current_dir(&runtime.cwd);
        apply_cli_spawn_environment(&mut command, &self.id.0, &runtime.cfg)?;

        let mut child = command.spawn().map_err(|error| {
            CoreError::Provider(format!("failed to spawn {}: {}", self.binary, error))
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Provider(format!("{} did not expose stdout", self.binary)))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::Provider(format!("{} did not expose stderr", self.binary)))?;

        let child = Arc::new(Mutex::new(child));
        {
            let mut active_turn = runtime.active_turn.lock().await;
            *active_turn = Some(ActiveTurn {
                child: child.clone(),
            });
        }

        let binary = self.binary.clone();
        let runtime_for_task = runtime.clone();
        tokio::spawn(async move {
            let _ = runtime_for_task
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });

            let stderr_task = tokio::spawn(async move { collect_stream_to_string(stderr).await });
            let mut stream_state = DroidStreamState::default();
            let mut reader = BufReader::new(stdout).lines();
            let mut saw_terminal_event = false;

            while let Ok(Some(line)) = reader.next_line().await {
                let content = line.trim_end().to_string();
                if content.is_empty() {
                    continue;
                }

                if let Some(session_id) = extract_droid_session_id(&content) {
                    *runtime_for_task.remote_session_id.lock().await = Some(session_id);
                }

                let events = parse_droid_stream_line(&content, &mut stream_state);
                for event in events {
                    if matches!(
                        event,
                        ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. }
                    ) {
                        saw_terminal_event = true;
                    }
                    let _ = runtime_for_task.events_tx.send(event);
                }
            }

            let stderr_output = stderr_task.await.unwrap_or_default();
            let exit_result = {
                let mut child = child.lock().await;
                child.wait().await
            };

            if !saw_terminal_event {
                let at = now_iso();
                match exit_result {
                    Ok(exit) if exit.success() => {
                        let _ = runtime_for_task
                            .events_tx
                            .send(ProviderEvent::Completed { at });
                    }
                    Ok(exit) => {
                        let message = if stderr_output.trim().is_empty() {
                            format!("{binary} exited with status {exit}")
                        } else {
                            stderr_output.trim().to_string()
                        };
                        let _ = runtime_for_task
                            .events_tx
                            .send(ProviderEvent::Failed { message, at });
                    }
                    Err(error) => {
                        let _ = runtime_for_task.events_tx.send(ProviderEvent::Failed {
                            message: format!("failed to wait for {binary}: {error}"),
                            at,
                        });
                    }
                }
            }

            *runtime_for_task.active_turn.lock().await = None;
        });

        Ok(())
    }
}

fn extract_droid_session_id(line: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    value
        .get("session_id")
        .or_else(|| value.get("sessionId"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn parse_droid_stream_line(line: &str, state: &mut DroidStreamState) -> Vec<ProviderEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => {
            return vec![ProviderEvent::TextDelta {
                content: trimmed.to_string(),
            }];
        }
    };

    parse_droid_stream_value(&value, state)
}

fn parse_droid_stream_value(value: &Value, state: &mut DroidStreamState) -> Vec<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    let at = now_iso();

    match kind {
        "system" => Vec::new(),
        "message" => parse_droid_message(value, state),
        "reasoning" => {
            let mut events = complete_droid_active_message(state, None, at.clone());
            state.legacy_assistant_text.clear();
            events.extend(parse_droid_reasoning(value, state, at));
            events
        }
        "tool_call" => {
            let mut events = complete_droid_active_message(state, None, at.clone());
            state.legacy_assistant_text.clear();
            events.extend(parse_droid_tool_call(value, at));
            events
        }
        "tool_result" => parse_droid_tool_result(value, at),
        "completion" => {
            let final_text = droid_final_text(value);
            let mut events = complete_droid_turn_text(value, state, final_text, at.clone());
            events.push(ProviderEvent::Completed { at });
            events
        }
        "error" => vec![ProviderEvent::Failed {
            message: droid_error_message(value),
            at,
        }],
        "result" => {
            let success = value
                .get("is_error")
                .or_else(|| value.get("isError"))
                .and_then(Value::as_bool)
                != Some(true);
            if success {
                let final_text = droid_final_text(value);
                let mut events = complete_droid_turn_text(value, state, final_text, at.clone());
                events.push(ProviderEvent::Completed { at });
                events
            } else {
                vec![ProviderEvent::Failed {
                    message: droid_error_message(value),
                    at,
                }]
            }
        }
        _ => Vec::new(),
    }
}

fn parse_droid_message(value: &Value, state: &mut DroidStreamState) -> Vec<ProviderEvent> {
    let role = value.get("role").and_then(Value::as_str).unwrap_or("");
    if role != "assistant" {
        return Vec::new();
    }

    if let Some(subtype) = value.get("subtype").and_then(Value::as_str) {
        if subtype.eq_ignore_ascii_case("reasoning") || subtype.eq_ignore_ascii_case("thinking") {
            let at = now_iso();
            let mut events = complete_droid_active_message(state, None, at.clone());
            state.legacy_assistant_text.clear();
            events.extend(parse_droid_reasoning(value, state, at));
            return events;
        }
    }

    if value
        .get("isReasoning")
        .or_else(|| value.get("is_reasoning"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        let at = now_iso();
        let mut events = complete_droid_active_message(state, None, at.clone());
        state.legacy_assistant_text.clear();
        events.extend(parse_droid_reasoning(value, state, at));
        return events;
    }

    let Some(text) = droid_text_field(value) else {
        return Vec::new();
    };
    let Some(id) = droid_message_id(value) else {
        state.legacy_assistant_text.push_str(&text);
        return vec![ProviderEvent::TextDelta { content: text }];
    };

    let mut events = Vec::new();
    if state.active_assistant_message_id.as_deref() != Some(id.as_str()) {
        events.extend(complete_droid_active_message(state, None, now_iso()));
        state.active_assistant_message_id = Some(id.clone());
        state.active_assistant_text.clear();
        events.push(ProviderEvent::AssistantMessageStarted {
            id: id.clone(),
            phase: AssistantMessagePhase::Unknown,
            at: now_iso(),
        });
    }
    state.active_assistant_text.push_str(&text);
    events.push(ProviderEvent::AssistantMessageDelta { id, content: text });
    events
}

fn droid_message_id(value: &Value) -> Option<String> {
    value
        .get("id")
        .or_else(|| value.get("messageId"))
        .or_else(|| value.get("message_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control))
        .map(str::to_string)
}

fn droid_final_text(value: &Value) -> Option<String> {
    value
        .get("finalText")
        .or_else(|| value.get("final_text"))
        .or_else(|| value.get("result"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn complete_droid_active_message(
    state: &mut DroidStreamState,
    authoritative: Option<String>,
    at: String,
) -> Vec<ProviderEvent> {
    let Some(id) = state.active_assistant_message_id.take() else {
        return Vec::new();
    };
    let content = authoritative.or_else(|| {
        (!state.active_assistant_text.is_empty()).then(|| state.active_assistant_text.clone())
    });
    state.active_assistant_text.clear();
    vec![ProviderEvent::AssistantMessageCompleted {
        id,
        phase: AssistantMessagePhase::Unknown,
        content,
        model: None,
        at,
    }]
}

fn complete_droid_turn_text(
    value: &Value,
    state: &mut DroidStreamState,
    final_text: Option<String>,
    at: String,
) -> Vec<ProviderEvent> {
    if state.active_assistant_message_id.is_some() {
        state.legacy_assistant_text.clear();
        return complete_droid_active_message(state, final_text, at);
    }

    let Some(final_text) = final_text else {
        state.legacy_assistant_text.clear();
        return Vec::new();
    };
    if final_text == state.legacy_assistant_text {
        state.legacy_assistant_text.clear();
        return Vec::new();
    }
    state.legacy_assistant_text.clear();
    vec![ProviderEvent::AssistantMessageCompleted {
        id: droid_message_id(value).unwrap_or_else(|| "droid:final".to_string()),
        phase: AssistantMessagePhase::Unknown,
        content: Some(final_text),
        model: None,
        at,
    }]
}

fn parse_droid_reasoning(
    value: &Value,
    state: &mut DroidStreamState,
    at: String,
) -> Vec<ProviderEvent> {
    let id = value
        .get("id")
        .or_else(|| value.get("reasoning_id"))
        .or_else(|| value.get("reasoningId"))
        .or_else(|| value.get("messageId"))
        .and_then(Value::as_str)
        .unwrap_or("reasoning")
        .to_string();
    let label = value
        .get("label")
        .or_else(|| value.get("title"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| Some("Thinking".to_string()));
    let subtype = value
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or("delta");
    let content = droid_text_field(value);
    let started = state.reasoning_started.contains_key(&id);
    let mut events = Vec::new();

    if !started {
        state.reasoning_started.insert(id.clone(), true);
        events.push(ProviderEvent::ReasoningStarted {
            id: id.clone(),
            label,
            at: at.clone(),
        });
    }

    if let Some(content) = content {
        events.push(ProviderEvent::ReasoningDelta {
            id: id.clone(),
            content,
        });
    }

    if matches!(subtype, "completed" | "complete" | "done") {
        state.reasoning_started.remove(&id);
        events.push(ProviderEvent::ReasoningCompleted { id, at });
    }

    events
}

fn parse_droid_tool_call(value: &Value, at: String) -> Option<ProviderEvent> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("tool-call")
        .to_string();
    let action = value
        .get("toolName")
        .or_else(|| value.get("tool_name"))
        .or_else(|| value.get("toolId"))
        .or_else(|| value.get("tool_id"))
        .and_then(Value::as_str)
        .unwrap_or("Tool")
        .to_string();
    let parameters = value.get("parameters").and_then(Value::as_object);
    let command = parameters.and_then(droid_tool_command);
    let file = parameters.and_then(droid_tool_file);

    Some(ProviderEvent::ToolCallStarted {
        id,
        action,
        command,
        file,
        at,
    })
}

fn parse_droid_tool_result(value: &Value, at: String) -> Vec<ProviderEvent> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("tool-call")
        .to_string();
    let mut events = Vec::new();
    if let Some(output) = value
        .get("value")
        .or_else(|| value.get("result"))
        .or_else(|| value.get("output"))
        .and_then(droid_value_to_text)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
    {
        events.push(ProviderEvent::ToolCallDelta {
            id: id.clone(),
            content: output,
        });
    }

    if value
        .get("isError")
        .or_else(|| value.get("is_error"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        events.push(ProviderEvent::ToolCallFailed {
            id,
            reason: value
                .get("error")
                .and_then(droid_value_to_text)
                .or_else(|| value.get("value").and_then(droid_value_to_text)),
            at,
        });
    } else {
        events.push(ProviderEvent::ToolCallCompleted { id, at });
    }

    events
}

fn droid_error_message(value: &Value) -> String {
    value
        .get("message")
        .or_else(|| value.get("error"))
        .and_then(droid_value_to_text)
        .unwrap_or_else(|| "droid exec failed".to_string())
}

fn droid_text_field(value: &Value) -> Option<String> {
    value
        .get("text")
        .or_else(|| value.get("content"))
        .or_else(|| value.get("message"))
        .and_then(droid_value_to_text)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn droid_tool_command(parameters: &serde_json::Map<String, Value>) -> Option<String> {
    parameters
        .get("command")
        .or_else(|| parameters.get("query"))
        .or_else(|| parameters.get("text"))
        .or_else(|| parameters.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn droid_tool_file(parameters: &serde_json::Map<String, Value>) -> Option<String> {
    parameters
        .get("file_path")
        .or_else(|| parameters.get("filePath"))
        .or_else(|| parameters.get("path"))
        .or_else(|| parameters.get("file"))
        .or_else(|| parameters.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn droid_value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).ok(),
    }
}

async fn collect_stream_to_string<T>(stream: T) -> String
where
    T: AsyncRead + Unpin,
{
    let mut output = String::new();
    let mut reader = BufReader::new(stream).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(line.trim_end());
    }
    output
}

#[async_trait]
impl Provider for DroidProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        self.prepare_runtime(cfg).await
    }

    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()> {
        let runtime = self
            .runtime_for_session(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no runtime for session {} on provider {}",
                    handle.session_id.0, self.binary
                ))
            })?;

        match input {
            Input::Text(text) => self.spawn_turn(runtime, text, None, None).await,
            Input::Turn(turn) => {
                let prompt = append_tool_instructions(
                    compose_fallback_prompt_for_provider(
                        &self.id.0,
                        &turn.prompt,
                        turn.plan_mode,
                        turn.effort.as_deref(),
                        turn.fast_mode,
                        PromptInjectionOptions {
                            plan: false,
                            effort: false,
                            fast: true,
                        },
                    ),
                    turn.tool_instructions.as_deref(),
                );
                self.spawn_turn(runtime, prompt, turn.plan_mode, turn.effort.as_deref())
                    .await
            }
            Input::UserInputResponse(_) => Err(CoreError::Provider(
                "Droid exec does not support mid-turn user input responses in stream-json mode"
                    .to_string(),
            )),
            Input::PermissionResponse(_) => Err(CoreError::Provider(
                "Droid exec does not support mid-turn permission responses in stream-json mode"
                    .to_string(),
            )),
        }
    }

    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
        let runtime = self
            .runtime
            .sessions
            .try_lock()
            .ok()
            .and_then(|sessions| sessions.get(&handle.session_id.0).cloned());

        let Some(runtime) = runtime else {
            return Box::pin(stream::empty());
        };

        let receiver = runtime.events_tx.subscribe();
        let stream = stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => return Some((Ok(event), receiver)),
                    Err(broadcast::error::RecvError::Closed) => return None,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });

        Box::pin(stream)
    }

    async fn cancel(&self, handle: &SessionHandle) -> Result<()> {
        let runtime = self
            .runtime
            .sessions
            .lock()
            .await
            .remove(&handle.session_id.0)
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no runtime for session {} on provider {}",
                    handle.session_id.0, self.binary
                ))
            })?;

        let active_turn = runtime.active_turn.lock().await.take();
        if let Some(active_turn) = active_turn {
            let mut child = active_turn.child.lock().await;
            child.kill().await.map_err(|error| {
                CoreError::Provider(format!("failed to cancel {}: {}", self.binary, error))
            })?;
        }

        Ok(())
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        let runtime = self.runtime_for_session(previous).await.ok_or_else(|| {
            CoreError::Provider(format!(
                "no resumable runtime for session {} on provider {}",
                previous.0, self.binary
            ))
        })?;

        Ok(runtime.handle.clone())
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        match self.binary_command().output().await {
            Ok(output) if output.status.success() => Ok(HealthStatus::Healthy),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let reason = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    format!("{} exited with status {}", self.binary, output.status)
                };
                Ok(HealthStatus::Degraded { reason })
            }
            Err(error) => Ok(HealthStatus::Unhealthy {
                reason: format!("failed to execute {}: {}", self.binary, error),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_droid_turn_args_with_resume_and_reasoning() {
        let provider = adapter();
        let args = provider.build_turn_args(
            Some("sess-123"),
            Some("gpt-5.4"),
            "fix the tests",
            Some(true),
            Some("xhigh"),
        );

        assert_eq!(args[0], "exec");
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--auto".to_string()));
        assert!(args.contains(&DROID_AUTONOMY_LEVEL.to_string()));
        assert!(args.contains(&"--session-id".to_string()));
        assert!(args.contains(&"sess-123".to_string()));
        assert!(args.contains(&"--use-spec".to_string()));
        assert!(args.contains(&"--reasoning-effort".to_string()));
        assert!(args.contains(&"high".to_string()));
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"gpt-5.4".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("fix the tests"));
    }

    #[test]
    fn omits_model_flag_for_auto() {
        let provider = adapter();
        let args =
            provider.build_turn_args(None, Some("auto"), "analyze repo", Some(false), Some("low"));
        assert!(!args.contains(&"--model".to_string()));
    }

    #[test]
    fn parses_droid_stream_assistant_tool_and_completion_events() {
        let mut state = DroidStreamState::default();

        let assistant = parse_droid_stream_line(
            r#"{"type":"message","role":"assistant","id":"msg-2","text":"I'll inspect the repo.","session_id":"sess"}"#,
            &mut state,
        );
        assert!(matches!(
            assistant.as_slice(),
            [
                ProviderEvent::AssistantMessageStarted { id: first_id, .. },
                ProviderEvent::AssistantMessageDelta { id: second_id, content }
            ] if first_id == "msg-2" && second_id == "msg-2" && content == "I'll inspect the repo."
        ));

        let tool_call = parse_droid_stream_line(
            r#"{"type":"tool_call","id":"call-1","toolName":"Execute","parameters":{"command":"ls -la"},"session_id":"sess"}"#,
            &mut state,
        );
        assert!(matches!(
            tool_call.as_slice(),
            [
                ProviderEvent::AssistantMessageCompleted {
                    id: message_id,
                    content: Some(message),
                    ..
                },
                ProviderEvent::ToolCallStarted {
                    id,
                    action,
                    command,
                    file,
                    ..
                }
            ] if message_id == "msg-2" && message == "I'll inspect the repo."
                && id == "call-1" && action == "Execute"
                && command.as_deref() == Some("ls -la") && file.is_none()
        ));

        let tool_result = parse_droid_stream_line(
            r#"{"type":"tool_result","id":"call-1","toolName":"Execute","isError":false,"value":"total 16","session_id":"sess"}"#,
            &mut state,
        );
        assert!(matches!(
            tool_result.as_slice(),
            [
                ProviderEvent::ToolCallDelta { id: first_id, content },
                ProviderEvent::ToolCallCompleted { id: second_id, .. }
            ] if first_id == "call-1" && second_id == "call-1" && content == "total 16"
        ));

        let completion = parse_droid_stream_line(
            r#"{"type":"completion","finalText":"Done.","numTurns":1,"durationMs":3000,"session_id":"sess"}"#,
            &mut state,
        );
        assert!(matches!(
            completion.as_slice(),
            [
                ProviderEvent::AssistantMessageCompleted {
                    id,
                    content: Some(content),
                    ..
                },
                ProviderEvent::Completed { .. }
            ] if id == "droid:final" && content == "Done."
        ));
    }

    #[test]
    fn parses_droid_reasoning_and_completion_text_when_needed() {
        let mut state = DroidStreamState::default();

        let reasoning = parse_droid_stream_line(
            r#"{"type":"reasoning","id":"think-1","text":"Checking candidate files"}"#,
            &mut state,
        );
        assert!(matches!(
            reasoning.as_slice(),
            [
                ProviderEvent::ReasoningStarted { id: first_id, .. },
                ProviderEvent::ReasoningDelta { id: second_id, content }
            ] if first_id == "think-1" && second_id == "think-1" && content == "Checking candidate files"
        ));

        let completion = parse_droid_stream_line(
            r#"{"type":"completion","finalText":"Final answer","session_id":"sess"}"#,
            &mut state,
        );
        assert!(matches!(
            completion.as_slice(),
            [
                ProviderEvent::AssistantMessageCompleted {
                    id,
                    content: Some(content),
                    ..
                },
                ProviderEvent::Completed { .. }
            ] if id == "droid:final" && content == "Final answer"
        ));
    }
}
