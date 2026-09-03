use std::{collections::HashMap, fs, path::PathBuf, process::Stdio, sync::Arc};

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
    application::{
        compose_fallback_prompt_for_provider, compose_wire_prompt_for_provider,
        PromptInjectionOptions,
    },
    domain::{
        provider::{
            Capabilities, HealthStatus, ProviderApprovalPolicy, ProviderEvent, ProviderId,
            SessionHandle,
        },
        session::SessionId,
    },
    ports::{Input, Provider, SessionConfig},
    CoreError, Result,
};

use crate::common::{
    append_tool_instructions, apply_cli_spawn_environment, augmented_path, now_iso,
    parse_provider_stream_line, ParsedProviderLine, ProviderStreamState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadlessCliKind {
    Claude,
    Gemini,
}

#[derive(Clone)]
pub struct HeadlessCliProviderAdapter {
    pub id: ProviderId,
    pub label: String,
    pub description: String,
    pub binary: String,
    pub capabilities: Capabilities,
    pub stable: bool,
    kind: HeadlessCliKind,
    runtime: Arc<ProviderRuntimeState>,
}

#[derive(Default)]
struct ProviderRuntimeState {
    sessions: Mutex<HashMap<String, Arc<SessionRuntime>>>,
}

struct SessionRuntime {
    handle: SessionHandle,
    cfg: SessionConfig,
    model: Option<String>,
    cwd: PathBuf,
    continuation_id: Mutex<Option<String>>,
    active_turn: Mutex<Option<ActiveTurn>>,
    events_tx: broadcast::Sender<ProviderEvent>,
}

struct ActiveTurn {
    child: Arc<Mutex<Child>>,
}

impl HeadlessCliProviderAdapter {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        binary: impl Into<String>,
        capabilities: Capabilities,
        stable: bool,
        kind: HeadlessCliKind,
    ) -> Self {
        Self {
            id: ProviderId(id.into()),
            label: label.into(),
            description: description.into(),
            binary: binary.into(),
            capabilities,
            stable,
            kind,
            runtime: Arc::new(ProviderRuntimeState::default()),
        }
    }

    fn binary_command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.arg("--version");
        command.env("PATH", augmented_path());
        command
    }

    fn auth_status_command(&self) -> Option<Command> {
        match self.kind {
            HeadlessCliKind::Claude => {
                let mut command = Command::new(&self.binary);
                command.args(["auth", "status"]);
                command.env("PATH", augmented_path());
                Some(command)
            }
            HeadlessCliKind::Gemini => None,
        }
    }

    fn runtime_binary(&self, cfg: &SessionConfig) -> Result<String> {
        let Some(path) = cfg
            .provider_runtime
            .as_ref()
            .and_then(|runtime| runtime.binary_path.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(self.binary.clone());
        };
        let path = PathBuf::from(path);
        let metadata = fs::metadata(&path).map_err(|_| {
            CoreError::Provider(format!(
                "the configured {} executable path is invalid",
                self.label
            ))
        })?;
        if !path.is_absolute() || !metadata.is_file() {
            return Err(CoreError::Provider(format!(
                "the configured {} executable path is invalid",
                self.label
            )));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(CoreError::Provider(format!(
                    "the configured {} executable is not executable",
                    self.label
                )));
            }
        }
        Ok(path.display().to_string())
    }

    fn turn_command(&self, cfg: &SessionConfig) -> Result<Command> {
        let mut command = Command::new(self.runtime_binary(cfg)?);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        Ok(command)
    }

    async fn prepare_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        self.runtime_binary(&cfg)?;
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
        let runtime = Arc::new(SessionRuntime {
            handle: handle.clone(),
            cfg: cfg.clone(),
            model: cfg.model.clone(),
            cwd,
            continuation_id: Mutex::new(None),
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

    async fn runtime_for_session(&self, session_id: &SessionId) -> Option<Arc<SessionRuntime>> {
        self.runtime
            .sessions
            .lock()
            .await
            .get(&session_id.0)
            .cloned()
    }

    fn build_turn_args(
        &self,
        continuation_id: Option<&str>,
        model: Option<&str>,
        prompt: &str,
        plan_mode: Option<bool>,
        approval_policy: Option<ProviderApprovalPolicy>,
        additional_directories: &[String],
    ) -> Vec<String> {
        match self.kind {
            HeadlessCliKind::Claude => {
                let mut args = vec![
                    "--print".to_string(),
                    "--output-format".to_string(),
                    "stream-json".to_string(),
                    "--verbose".to_string(),
                    "--include-partial-messages".to_string(),
                    "--dangerously-skip-permissions".to_string(),
                ];
                if let Some(continuation_id) =
                    continuation_id.filter(|value| !value.trim().is_empty())
                {
                    args.push("--resume".to_string());
                    args.push(continuation_id.to_string());
                }
                if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
                    args.push("--model".to_string());
                    args.push(model.to_string());
                }
                for directory in additional_directories {
                    args.push("--add-dir".to_string());
                    args.push(directory.clone());
                }
                args.push(prompt.to_string());
                args
            }
            HeadlessCliKind::Gemini => {
                let approval_mode = if plan_mode.unwrap_or(false) {
                    "plan"
                } else {
                    match approval_policy {
                        Some(ProviderApprovalPolicy::Ask) => "default",
                        Some(ProviderApprovalPolicy::Auto) => "auto_edit",
                        Some(ProviderApprovalPolicy::FullAccess) | None => "yolo",
                    }
                };
                let mut args = vec![
                    "--prompt".to_string(),
                    prompt.to_string(),
                    "--output-format".to_string(),
                    "stream-json".to_string(),
                    "--skip-trust".to_string(),
                    "--approval-mode".to_string(),
                    approval_mode.to_string(),
                ];
                if approval_policy != Some(ProviderApprovalPolicy::FullAccess) {
                    args.push("--sandbox".to_string());
                }
                if let Some(continuation_id) =
                    continuation_id.filter(|value| !value.trim().is_empty())
                {
                    args.push("--resume".to_string());
                    args.push(continuation_id.to_string());
                }
                if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
                    args.push("--model".to_string());
                    args.push(model.to_string());
                }
                for directory in additional_directories {
                    args.push("--include-directories".to_string());
                    args.push(directory.clone());
                }
                args
            }
        }
    }

    async fn spawn_turn(
        &self,
        runtime: Arc<SessionRuntime>,
        prompt: String,
        plan_mode: Option<bool>,
        approval_policy: Option<ProviderApprovalPolicy>,
    ) -> Result<()> {
        {
            let active_turn = runtime.active_turn.lock().await;
            if active_turn.is_some() {
                return Err(CoreError::Provider(format!(
                    "{} turn already active for this session",
                    self.label
                )));
            }
        }

        let continuation_id = runtime.continuation_id.lock().await.clone();
        let args = self.build_turn_args(
            continuation_id.as_deref(),
            runtime.model.as_deref(),
            &prompt,
            plan_mode,
            approval_policy,
            &runtime.cfg.additional_working_directories,
        );
        let mut command = self.turn_command(&runtime.cfg)?;
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
        let kind = self.kind;
        let runtime_for_task = runtime.clone();
        tokio::spawn(async move {
            let _ = runtime_for_task
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });

            let stderr_task = tokio::spawn(async move { collect_stream_to_string(stderr).await });
            let mut stream_state = ProviderStreamState::default();
            let mut reader = BufReader::new(stdout).lines();
            let mut saw_terminal_event = false;
            let mut should_force_kill = false;

            while let Ok(Some(line)) = reader.next_line().await {
                let content = line.trim_end().to_string();
                if content.is_empty() {
                    continue;
                }

                if let Some(event) = parse_provider_prompt_line(kind, &content) {
                    saw_terminal_event = true;
                    should_force_kill = true;
                    let _ = runtime_for_task.events_tx.send(event);
                    break;
                }

                if let Some(continuation_id) = extract_continuation_id(kind, &content) {
                    *runtime_for_task.continuation_id.lock().await = Some(continuation_id);
                }

                let parsed = match kind {
                    HeadlessCliKind::Claude => {
                        parse_provider_stream_line(&content, &mut stream_state)
                    }
                    HeadlessCliKind::Gemini => {
                        parse_gemini_stream_line(&content, &mut stream_state)
                    }
                };

                match parsed {
                    ParsedProviderLine::Event(event) => {
                        if matches!(
                            event,
                            ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. }
                        ) {
                            saw_terminal_event = true;
                            should_force_kill = true;
                        }
                        let _ = runtime_for_task.events_tx.send(event);
                        if should_force_kill {
                            break;
                        }
                    }
                    ParsedProviderLine::Events(events) => {
                        for event in events {
                            if matches!(
                                event,
                                ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. }
                            ) {
                                saw_terminal_event = true;
                                should_force_kill = true;
                            }
                            let _ = runtime_for_task.events_tx.send(event);
                            if should_force_kill {
                                break;
                            }
                        }
                        if should_force_kill {
                            break;
                        }
                    }
                    ParsedProviderLine::Text(text) => {
                        let _ = runtime_for_task
                            .events_tx
                            .send(ProviderEvent::TextDelta { content: text });
                    }
                    ParsedProviderLine::Ignored => {}
                }
            }

            let stderr_output = stderr_task.await.unwrap_or_default();
            let exit_result = {
                let mut child = child.lock().await;
                if should_force_kill && child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill().await;
                }
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
                        let message = if let Some(message) =
                            normalized_headless_failure_message(kind, &stderr_output)
                        {
                            message
                        } else if stderr_output.trim().is_empty() {
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

fn extract_continuation_id(kind: HeadlessCliKind, line: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    match kind {
        HeadlessCliKind::Claude => value
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        HeadlessCliKind::Gemini => value
            .get("session_id")
            .or_else(|| value.get("sessionId"))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn parse_provider_prompt_line(kind: HeadlessCliKind, line: &str) -> Option<ProviderEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let at = now_iso();

    match kind {
        HeadlessCliKind::Claude => None,
        HeadlessCliKind::Gemini => {
            normalized_gemini_auth_failure(&lower).map(|message| ProviderEvent::Failed {
                message: message.to_string(),
                at,
            })
        }
    }
}

const GEMINI_AUTH_FAILURE_MESSAGE: &str =
    "Gemini CLI is not authenticated. Open `gemini` in a terminal, complete sign-in, and try again.";

const GEMINI_UNSUPPORTED_CLIENT_MESSAGE: &str = "Personal Google sign-in is no longer supported by Gemini CLI. Configure the Antigravity provider in DCC Settings and use Gemini CLI (legacy) only with an API key, Vertex AI, or an eligible enterprise account.";

fn normalized_gemini_auth_failure(message: &str) -> Option<&'static str> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unsupported_client")
        || lower.contains("ineligibletiererror")
        || lower.contains("this client is no longer supported")
    {
        return Some(GEMINI_UNSUPPORTED_CLIENT_MESSAGE);
    }

    if lower.contains("error authenticating")
        || lower.contains("opening authentication page in your browser")
        || lower.contains("authentication required")
        || lower.contains("not authenticated")
        || lower.contains("not logged in")
        || lower.contains("do you want to continue?")
    {
        return Some(GEMINI_AUTH_FAILURE_MESSAGE);
    }

    None
}

fn normalized_headless_failure_message(
    kind: HeadlessCliKind,
    stderr_output: &str,
) -> Option<String> {
    match kind {
        HeadlessCliKind::Claude => None,
        HeadlessCliKind::Gemini => {
            normalized_gemini_auth_failure(stderr_output).map(str::to_string)
        }
    }
}

fn parse_gemini_stream_line(line: &str, state: &mut ProviderStreamState) -> ParsedProviderLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedProviderLine::Ignored;
    }

    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => return ParsedProviderLine::Text(trimmed.to_string()),
    };

    match parse_gemini_stream_value(&value, state) {
        Some(ProviderEvent::TextDelta { content }) => ParsedProviderLine::Text(content),
        Some(event) => ParsedProviderLine::Event(event),
        None => ParsedProviderLine::Ignored,
    }
}

fn parse_gemini_stream_value(
    value: &Value,
    state: &mut ProviderStreamState,
) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    let at = now_iso();

    match kind {
        "init" => {
            state.gemini_streamed_text_emitted = false;
            state.gemini_active_message_id = Some("gemini:assistant:0".to_string());
            None
        }
        "message" => {
            let role = value.get("role").and_then(Value::as_str)?;
            if role != "assistant" {
                return None;
            }
            let content = value.get("content").and_then(Value::as_str).unwrap_or("");
            if content.is_empty() {
                None
            } else {
                state.gemini_streamed_text_emitted = true;
                Some(ProviderEvent::AssistantMessageDelta {
                    id: state
                        .gemini_active_message_id
                        .get_or_insert_with(|| "gemini:assistant:0".to_string())
                        .clone(),
                    content: content.to_string(),
                })
            }
        }
        "tool_use" => {
            let id = value
                .get("tool_id")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let tool_name = value
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let parameters = value.get("parameters").and_then(Value::as_object);
            Some(ProviderEvent::ToolCallStarted {
                id,
                action: tool_name,
                command: parameters.and_then(gemini_tool_command),
                file: parameters.and_then(gemini_tool_file),
                at,
            })
        }
        "tool_result" => {
            let id = value
                .get("tool_id")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("success");
            if status == "error" {
                Some(ProviderEvent::ToolCallFailed {
                    id,
                    reason: value
                        .get("error")
                        .and_then(|error| error.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| {
                            value
                                .get("output")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        }),
                    at,
                })
            } else {
                Some(ProviderEvent::ToolCallCompleted { id, at })
            }
        }
        "error" => {
            let severity = value
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or("error");
            if severity == "warning" {
                None
            } else {
                let raw_message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("gemini runtime error");
                Some(ProviderEvent::Failed {
                    message: normalized_gemini_auth_failure(raw_message)
                        .unwrap_or(raw_message)
                        .to_string(),
                    at,
                })
            }
        }
        "result" => {
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("success");
            if status == "success" {
                let content = gemini_result_text(value);
                if state.gemini_streamed_text_emitted || content.is_some() {
                    state.gemini_streamed_text_emitted = false;
                    return Some(ProviderEvent::AssistantMessageCompleted {
                        id: state
                            .gemini_active_message_id
                            .take()
                            .unwrap_or_else(|| "gemini:assistant:0".to_string()),
                        phase: dcc_core::domain::session::AssistantMessagePhase::Unknown,
                        content,
                        model: None,
                        at,
                    });
                }
                Some(ProviderEvent::Completed { at })
            } else {
                let raw_message = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("gemini turn failed");
                Some(ProviderEvent::Failed {
                    message: normalized_gemini_auth_failure(raw_message)
                        .unwrap_or(raw_message)
                        .to_string(),
                    at,
                })
            }
        }
        _ => None,
    }
}

fn gemini_result_text(value: &Value) -> Option<String> {
    let direct = value
        .get("result")
        .or_else(|| value.get("response"))
        .or_else(|| value.get("output"))
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    if direct.is_some() {
        return direct;
    }

    let nested = value
        .get("result")
        .or_else(|| value.get("response"))
        .or_else(|| value.get("output"))
        .and_then(Value::as_object)?;

    nested
        .get("plan")
        .or_else(|| nested.get("plan_markdown"))
        .or_else(|| nested.get("planMarkdown"))
        .or_else(|| nested.get("text"))
        .or_else(|| nested.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn gemini_tool_command(parameters: &serde_json::Map<String, Value>) -> Option<String> {
    parameters
        .get("command")
        .or_else(|| parameters.get("query"))
        .or_else(|| parameters.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn gemini_tool_file(parameters: &serde_json::Map<String, Value>) -> Option<String> {
    parameters
        .get("file_path")
        .or_else(|| parameters.get("filePath"))
        .or_else(|| parameters.get("path"))
        .or_else(|| parameters.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
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
impl Provider for HeadlessCliProviderAdapter {
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
                let prompt = match self.kind {
                    HeadlessCliKind::Claude => compose_wire_prompt_for_provider(
                        &self.id.0,
                        &turn.prompt,
                        turn.plan_mode,
                        turn.effort.as_deref(),
                        turn.fast_mode,
                    ),
                    HeadlessCliKind::Gemini => compose_fallback_prompt_for_provider(
                        &self.id.0,
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
                };
                let prompt = append_tool_instructions(prompt, turn.tool_instructions.as_deref());
                self.spawn_turn(runtime, prompt, turn.plan_mode, turn.approval_policy)
                    .await
            }
            Input::UserInputResponse(_) => Err(CoreError::Provider(format!(
                "{} does not support mid-turn user input responses",
                self.label
            ))),
            Input::PermissionResponse(_) => Err(CoreError::Provider(format!(
                "{} does not support mid-turn permission responses",
                self.label
            ))),
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
        if let Some(mut auth_command) = self.auth_status_command() {
            match auth_command.output().await {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if let Ok(value) = serde_json::from_str::<Value>(&stdout) {
                        if value.get("loggedIn").and_then(Value::as_bool) == Some(false) {
                            return Ok(HealthStatus::Unhealthy {
                                reason:
                                    "Claude Code is not authenticated. Run `claude auth login`."
                                        .to_string(),
                            });
                        }
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }

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
    fn parses_gemini_init_continuation_id() {
        let line = r#"{"type":"init","session_id":"gemini-session-1","model":"gemini-3.1-pro"}"#;
        assert_eq!(
            extract_continuation_id(HeadlessCliKind::Gemini, line).as_deref(),
            Some("gemini-session-1")
        );
    }

    #[test]
    fn parses_gemini_tool_lifecycle() {
        let mut state = ProviderStreamState::default();
        let started = parse_gemini_stream_line(
            r#"{"type":"tool_use","tool_name":"run_shell_command","tool_id":"tool-1","parameters":{"command":"ls"}}"#,
            &mut state,
        );
        assert!(matches!(
            started,
            ParsedProviderLine::Event(ProviderEvent::ToolCallStarted { .. })
        ));

        let completed = parse_gemini_stream_line(
            r#"{"type":"tool_result","tool_id":"tool-1","status":"success"}"#,
            &mut state,
        );
        assert!(matches!(
            completed,
            ParsedProviderLine::Event(ProviderEvent::ToolCallCompleted { .. })
        ));
    }

    #[test]
    fn parses_gemini_result_text_when_stream_has_no_assistant_message() {
        let mut state = ProviderStreamState::default();
        let parsed = parse_gemini_stream_line(
            r##"{"type":"result","status":"success","result":{"plan":"# Ship\n\n- inspect\n- patch"}}"##,
            &mut state,
        );
        assert!(matches!(
            parsed,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageCompleted {
                id,
                content: Some(text),
                ..
            }) if id == "gemini:assistant:0" && text == "# Ship\n\n- inspect\n- patch"
        ));
    }

    #[test]
    fn reconciles_gemini_deltas_with_the_terminal_result() {
        let mut state = ProviderStreamState::default();
        let _ = parse_gemini_stream_line(
            r#"{"type":"init","session_id":"gemini-session-1"}"#,
            &mut state,
        );
        let delta = parse_gemini_stream_line(
            r#"{"type":"message","role":"assistant","content":"Hello"}"#,
            &mut state,
        );
        assert!(matches!(
            delta,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageDelta { id, content })
                if id == "gemini:assistant:0" && content == "Hello"
        ));

        let result = parse_gemini_stream_line(
            r#"{"type":"result","status":"success","response":"Hello world"}"#,
            &mut state,
        );
        assert!(matches!(
            result,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageCompleted {
                id,
                content: Some(content),
                ..
            }) if id == "gemini:assistant:0" && content == "Hello world"
        ));
    }

    #[test]
    fn detects_gemini_auth_prompt_as_failure() {
        let event = parse_provider_prompt_line(
            HeadlessCliKind::Gemini,
            "Opening authentication page in your browser. Do you want to continue? [Y/n]: ",
        );
        assert!(matches!(
            event,
            Some(ProviderEvent::Failed { message, .. })
                if message.contains("Gemini CLI is not authenticated")
        ));
    }

    #[test]
    fn normalizes_gemini_unsupported_client_failure_without_exposing_stack_trace() {
        let stderr = "YOLO mode is enabled.\nError authenticating: IneligibleTierError: This client is no longer supported for Gemini Code Assist for individuals.\n    at throwIneligibleOrProjectIdError (bundle.js:307474:11)\nreasonCode: 'UNSUPPORTED_CLIENT'";

        let message = normalized_headless_failure_message(HeadlessCliKind::Gemini, stderr)
            .expect("known Gemini authentication failure");

        assert!(message.contains("Personal Google sign-in is no longer supported"));
        assert!(message.contains("Antigravity"));
        assert!(!message.contains("bundle.js"));
        assert!(!message.contains("YOLO mode"));
    }

    #[test]
    fn normalizes_gemini_stream_authentication_errors() {
        let mut state = ProviderStreamState::default();
        let parsed = parse_gemini_stream_line(
            r#"{"type":"error","message":"Error authenticating: not logged in\n    at internal.js:1:1"}"#,
            &mut state,
        );

        assert!(matches!(
            parsed,
            ParsedProviderLine::Event(ProviderEvent::Failed { message, .. })
                if message == GEMINI_AUTH_FAILURE_MESSAGE && !message.contains("internal.js")
        ));
    }

    #[test]
    fn gemini_build_turn_args_use_native_plan_mode() {
        let provider = HeadlessCliProviderAdapter::new(
            "gemini",
            "Gemini",
            "Gemini",
            "gemini",
            Capabilities {
                streaming: true,
                supports_steering: false,
                supports_native_subagent_steering: false,
                supports_native_subagent_interrupt: false,
                mcp_support: dcc_core::domain::provider::McpSupportLevel::Unsupported,
                mcp_oauth_support: dcc_core::domain::provider::McpOauthSupport::Unsupported,
                tools: true,
                vision: false,
                resumable: false,
                experimental: true,
                can_be_delegation_target: true,
                can_request_delegation: false,
                supports_read_only_delegation: true,
                supports_edit_delegation: true,
                supports_multi_root: true,
                approval_policies: vec![
                    ProviderApprovalPolicy::Ask,
                    ProviderApprovalPolicy::Auto,
                    ProviderApprovalPolicy::FullAccess,
                ],
                supports_runtime_home: true,
                supports_runtime_binary: false,
                supports_shadow_home: false,
                supports_subagent_concurrency: false,
                supports_account_usage: false,
                plan_mode_support: dcc_core::domain::provider::TurnControlSupport::Native,
                fast_mode_support: dcc_core::domain::provider::TurnControlSupport::PromptFallback,
                supports_dynamic_models: false,
                supports_compaction_command: false,
            },
            true,
            HeadlessCliKind::Gemini,
        );

        let plan_args = provider.build_turn_args(
            None,
            None,
            "ship it",
            Some(true),
            Some(ProviderApprovalPolicy::FullAccess),
            &[],
        );
        assert!(plan_args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "plan"]));

        let execute_args = provider.build_turn_args(
            None,
            None,
            "ship it",
            Some(false),
            Some(ProviderApprovalPolicy::FullAccess),
            &[],
        );
        assert!(execute_args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "yolo"]));
        assert!(!execute_args.iter().any(|arg| arg == "--sandbox"));

        let ask_args = provider.build_turn_args(
            None,
            None,
            "ship it",
            Some(false),
            Some(ProviderApprovalPolicy::Ask),
            &[],
        );
        assert!(ask_args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "default"]));
        assert!(ask_args.iter().any(|arg| arg == "--sandbox"));

        let auto_args = provider.build_turn_args(
            None,
            None,
            "ship it",
            Some(false),
            Some(ProviderApprovalPolicy::Auto),
            &[],
        );
        assert!(auto_args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "auto_edit"]));
        assert!(auto_args.iter().any(|arg| arg == "--sandbox"));
    }

    #[test]
    fn gemini_build_turn_args_include_each_authorized_directory() {
        let provider = HeadlessCliProviderAdapter::new(
            "gemini",
            "Gemini",
            "Gemini",
            "gemini",
            crate::gemini::descriptor(HealthStatus::Healthy).capabilities,
            true,
            HeadlessCliKind::Gemini,
        );
        let directories = vec!["/tmp/api".to_string(), "/tmp/web".to_string()];
        let args = provider.build_turn_args(
            None,
            None,
            "ship it",
            Some(false),
            Some(ProviderApprovalPolicy::Auto),
            &directories,
        );
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--include-directories", "/tmp/api"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--include-directories", "/tmp/web"]));
        assert!(args.iter().any(|arg| arg == "--sandbox"));
    }
}
