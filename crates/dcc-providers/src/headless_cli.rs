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
    application::{
        compose_fallback_prompt_for_provider, compose_wire_prompt_for_provider,
        PromptInjectionOptions,
    },
    domain::{
        provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
    },
    ports::{Input, Provider, SessionConfig},
    CoreError, Result,
};

use crate::common::{
    apply_cli_spawn_environment, augmented_path, now_iso, parse_provider_stream_line,
    ParsedProviderLine, ProviderStreamState,
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
                args.push(prompt.to_string());
                args
            }
            HeadlessCliKind::Gemini => {
                let approval_mode = if plan_mode.unwrap_or(false) {
                    "plan"
                } else {
                    "yolo"
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
                args
            }
        }
    }

    async fn spawn_turn(
        &self,
        runtime: Arc<SessionRuntime>,
        prompt: String,
        plan_mode: Option<bool>,
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
                    HeadlessCliKind::Gemini => parse_gemini_stream_line(&content),
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
            if lower.contains("opening authentication page in your browser")
                || lower.contains("do you want to continue?")
            {
                Some(ProviderEvent::Failed {
                    message:
                        "Gemini CLI is not authenticated. Run `gemini` in a terminal and complete login."
                            .to_string(),
                    at,
                })
            } else {
                None
            }
        }
    }
}

fn parse_gemini_stream_line(line: &str) -> ParsedProviderLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedProviderLine::Ignored;
    }

    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => return ParsedProviderLine::Text(trimmed.to_string()),
    };

    match parse_gemini_stream_value(&value) {
        Some(event) => ParsedProviderLine::Event(event),
        None => ParsedProviderLine::Ignored,
    }
}

fn parse_gemini_stream_value(value: &Value) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    let at = now_iso();

    match kind {
        "init" => None,
        "message" => {
            let role = value.get("role").and_then(Value::as_str)?;
            if role != "assistant" {
                return None;
            }
            let content = value.get("content").and_then(Value::as_str).unwrap_or("");
            if content.is_empty() {
                None
            } else {
                Some(ProviderEvent::TextDelta {
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
                Some(ProviderEvent::Failed {
                    message: value
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("gemini runtime error")
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
                Some(ProviderEvent::Completed { at })
            } else {
                Some(ProviderEvent::Failed {
                    message: value
                        .get("error")
                        .and_then(|error| error.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("gemini turn failed")
                        .to_string(),
                    at,
                })
            }
        }
        _ => None,
    }
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
            Input::Text(text) => self.spawn_turn(runtime, text, None).await,
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
                self.spawn_turn(runtime, prompt, turn.plan_mode).await
            }
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
        let started = parse_gemini_stream_line(
            r#"{"type":"tool_use","tool_name":"run_shell_command","tool_id":"tool-1","parameters":{"command":"ls"}}"#,
        );
        assert!(matches!(
            started,
            ParsedProviderLine::Event(ProviderEvent::ToolCallStarted { .. })
        ));

        let completed = parse_gemini_stream_line(
            r#"{"type":"tool_result","tool_id":"tool-1","status":"success"}"#,
        );
        assert!(matches!(
            completed,
            ParsedProviderLine::Event(ProviderEvent::ToolCallCompleted { .. })
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
    fn gemini_build_turn_args_use_native_plan_mode() {
        let provider = HeadlessCliProviderAdapter::new(
            "gemini",
            "Gemini",
            "Gemini",
            "gemini",
            Capabilities {
                streaming: true,
                mcp: false,
                tools: true,
                vision: false,
                resumable: false,
                experimental: true,
            },
            true,
            HeadlessCliKind::Gemini,
        );

        let plan_args = provider.build_turn_args(None, None, "ship it", Some(true));
        assert!(plan_args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "plan"]));

        let execute_args = provider.build_turn_args(None, None, "ship it", Some(false));
        assert!(execute_args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "yolo"]));
    }
}
