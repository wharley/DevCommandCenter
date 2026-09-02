use std::{collections::HashSet, path::PathBuf, process::Stdio, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{self, BoxStream};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    sync::{broadcast, watch, Mutex},
};

use dcc_core::{
    application::{compose_fallback_prompt_for_provider, PromptInjectionOptions},
    domain::{
        provider::{
            Capabilities, HealthStatus, McpSupportLevel, ProviderDescriptor, ProviderEvent,
            ProviderId, ProviderModelDescriptor, SessionHandle,
        },
        session::{AssistantMessagePhase, SessionId},
    },
    ports::{Input, Provider, SessionConfig},
    CoreError, Result,
};

use crate::common::{append_tool_instructions, experimental_cli_capabilities};
use crate::cursor_acp::CursorBridgeProvider;

const PROVIDER_LABEL: &str = "Cursor";
const PROVIDER_DESCRIPTION: &str =
    "Cursor Agent CLI provider with CLI-native session resume and stream-json event parsing.";
const CURSOR_MODEL_DESCRIPTION: &str = "Model reported by Cursor Agent CLI for this account.";
const CURSOR_AUTODETECT_MODEL_ID: &str = "auto";

#[derive(Clone)]
pub struct CursorProvider {
    id: ProviderId,
    binary: String,
    capabilities: Capabilities,
    runtime: Arc<ProviderRuntimeState>,
}

#[derive(Default)]
struct ProviderRuntimeState {
    sessions: Mutex<std::collections::HashMap<String, Arc<CursorSessionRuntime>>>,
}

struct CursorSessionRuntime {
    handle: SessionHandle,
    chat_id: String,
    model: Option<String>,
    cwd: PathBuf,
    additional_directories: Vec<PathBuf>,
    events_tx: broadcast::Sender<ProviderEvent>,
    active_turn: Mutex<Option<ActiveCursorTurn>>,
}

struct ActiveCursorTurn {
    cancel_tx: watch::Sender<bool>,
}

#[derive(Debug, Default)]
struct CursorCommandResult {
    stdout: String,
    stderr: String,
    code: i32,
}

#[derive(Debug, Default)]
struct CursorStreamState {
    assistant_message_id: Option<String>,
    assistant_message_started: bool,
}

pub fn adapter() -> CursorBridgeProvider {
    let capabilities = cursor_capabilities();
    CursorBridgeProvider::new(
        "cursor-agent",
        capabilities.clone(),
        CursorProvider::new("cursor", "cursor-agent", capabilities),
    )
}

fn cursor_capabilities() -> Capabilities {
    let mut capabilities = experimental_cli_capabilities();
    capabilities.mcp_support = McpSupportLevel::NativeConfig;
    capabilities.supports_multi_root = true;
    capabilities.plan_mode_support = dcc_core::domain::provider::TurnControlSupport::Native;
    capabilities.supports_dynamic_models = true;
    capabilities
}

pub fn descriptor(
    health: HealthStatus,
    discovered_models: Vec<ProviderModelDescriptor>,
) -> ProviderDescriptor {
    let mut models = vec![ProviderModelDescriptor {
        id: CURSOR_AUTODETECT_MODEL_ID.to_string(),
        label: "Auto".to_string(),
        description: "Use Cursor's recommended model for this account.".to_string(),
        recommended: true,
        effort_levels: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
    }];

    let mut seen = HashSet::from([CURSOR_AUTODETECT_MODEL_ID.to_string()]);
    for model in discovered_models {
        let id = model.id.trim().to_string();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        models.push(ProviderModelDescriptor {
            id,
            label: model.label,
            description: model.description,
            recommended: false,
            effort_levels: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
        });
    }

    ProviderDescriptor {
        id: ProviderId("cursor".to_string()),
        label: PROVIDER_LABEL.to_string(),
        description: PROVIDER_DESCRIPTION.to_string(),
        models,
        capabilities: cursor_capabilities(),
        health,
        enabled: true,
        availability_generation: 0,
        stable: false,
    }
}

impl CursorProvider {
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

    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command
    }

    async fn run_command(
        &self,
        args: &[&str],
        current_dir: Option<&std::path::Path>,
    ) -> Result<CursorCommandResult> {
        let mut command = self.command();
        command.args(args);
        if let Some(dir) = current_dir {
            command.current_dir(dir);
        }
        let output = command.output().await.map_err(|error| {
            CoreError::Provider(format!("failed to execute {}: {}", self.binary, error))
        })?;
        Ok(CursorCommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            code: output.status.code().unwrap_or(-1),
        })
    }

    fn current_working_dir() -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    fn resolve_working_directory(cfg: &SessionConfig) -> PathBuf {
        cfg.working_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(Self::current_working_dir)
    }

    fn normalize_model_arg(model: Option<&str>) -> Option<String> {
        let trimmed = model.map(str::trim).unwrap_or_default();
        if trimmed.is_empty() {
            return None;
        }
        let normalized = trimmed.to_lowercase();
        if normalized == CURSOR_AUTODETECT_MODEL_ID
            || normalized == "default"
            || normalized == "cursor-agent"
            || normalized == "cursor-editor"
        {
            return None;
        }
        Some(trimmed.to_string())
    }

    async fn runtime_for_session(
        &self,
        session_id: &SessionId,
    ) -> Option<Arc<CursorSessionRuntime>> {
        self.runtime
            .sessions
            .lock()
            .await
            .get(&session_id.0)
            .cloned()
    }

    fn cursor_turn_args(
        &self,
        chat_id: &str,
        model: Option<&str>,
        prompt: &str,
        plan_mode: Option<bool>,
        additional_directories: &[PathBuf],
    ) -> Vec<String> {
        let mut args = vec![
            "--print".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--trust".to_string(),
            "--sandbox".to_string(),
            "enabled".to_string(),
            "--resume".to_string(),
            chat_id.to_string(),
        ];
        if plan_mode.unwrap_or(false) {
            args.push("--mode".to_string());
            args.push("plan".to_string());
        }
        if let Some(model) = Self::normalize_model_arg(model) {
            args.push("--model".to_string());
            args.push(model);
        }
        for directory in additional_directories {
            args.push("--add-dir".to_string());
            args.push(directory.to_string_lossy().to_string());
        }
        args.push(prompt.to_string());
        args
    }

    async fn spawn_cursor_turn(
        &self,
        runtime: Arc<CursorSessionRuntime>,
        prompt: String,
        plan_mode: Option<bool>,
    ) -> Result<()> {
        {
            let active_turn = runtime.active_turn.lock().await;
            if active_turn.is_some() {
                return Err(CoreError::Provider(
                    "cursor turn already active for this session".to_string(),
                ));
            }
        }

        let args = self.cursor_turn_args(
            &runtime.chat_id,
            runtime.model.as_deref(),
            &prompt,
            plan_mode,
            &runtime.additional_directories,
        );
        let cwd = runtime.cwd.clone();
        let mut command = self.command();
        command.args(&args);
        command.current_dir(&cwd);

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

        let (cancel_tx, mut cancel_rx) = watch::channel(false);
        {
            let mut active_turn = runtime.active_turn.lock().await;
            *active_turn = Some(ActiveCursorTurn {
                cancel_tx: cancel_tx.clone(),
            });
        }

        let events_tx = runtime.events_tx.clone();
        let runtime_for_task = runtime.clone();
        tokio::spawn(async move {
            let mut terminated = false;
            let _ = events_tx.send(ProviderEvent::Started { at: now_iso() });

            let stderr_task = tokio::spawn(async move { collect_stream_to_string(stderr).await });
            let mut reader = BufReader::new(stdout).lines();
            let mut stream_state = CursorStreamState::default();

            loop {
                let next_line = reader.next_line();
                tokio::pin!(next_line);
                tokio::select! {
                    line_result = &mut next_line => {
                        match line_result {
                            Ok(Some(line)) => {
                                for event in parse_cursor_stream_line(&line, &mut stream_state) {
                                    if matches!(
                                        event,
                                        ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. }
                                    ) {
                                        terminated = true;
                                    }
                                    let _ = events_tx.send(event);
                                }
                            }
                            Ok(None) => break,
                            Err(error) => {
                                terminated = true;
                                let _ = events_tx.send(ProviderEvent::Failed {
                                    message: format!("failed to read Cursor output: {error}"),
                                    at: now_iso(),
                                });
                                break;
                            }
                        }
                    }
                    cancelled = cancel_rx.changed() => {
                        if cancelled.is_ok() && *cancel_rx.borrow() {
                            terminated = true;
                            let _ = child.kill().await;
                            let _ = events_tx.send(ProviderEvent::Failed {
                                message: "Cursor turn cancelled".to_string(),
                                at: now_iso(),
                            });
                            break;
                        }
                    }
                }
            }

            let exit_result = child.wait().await;
            let stderr = stderr_task.await.unwrap_or_default();

            if !terminated {
                let at = now_iso();
                match exit_result {
                    Ok(exit) if exit.success() => {
                        let _ = events_tx.send(ProviderEvent::Completed { at });
                    }
                    Ok(exit) => {
                        let message = join_cursor_messages(
                            Some(format!("Cursor exited with status {exit}")),
                            Some(stderr.trim().to_string()),
                        );
                        let _ = events_tx.send(ProviderEvent::Failed {
                            message: message
                                .unwrap_or_else(|| format!("Cursor exited with status {exit}")),
                            at,
                        });
                    }
                    Err(error) => {
                        let message = join_cursor_messages(
                            Some(format!("failed to wait for Cursor: {error}")),
                            Some(stderr.trim().to_string()),
                        );
                        let _ = events_tx.send(ProviderEvent::Failed {
                            message: message
                                .unwrap_or_else(|| format!("failed to wait for Cursor: {error}")),
                            at,
                        });
                    }
                }
            }

            let mut active_turn = runtime_for_task.active_turn.lock().await;
            *active_turn = None;
        });

        Ok(())
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('[') => {
                while let Some(next) = chars.next() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    output
}

fn join_cursor_messages(first: Option<String>, second: Option<String>) -> Option<String> {
    let mut parts = Vec::new();
    for message in [first, second] {
        let Some(message) = message else {
            continue;
        };
        let trimmed = message.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn parse_cursor_stream_line(line: &str, state: &mut CursorStreamState) -> Vec<ProviderEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return Vec::new();
    };
    parse_cursor_stream_value(&value, state)
}

fn parse_cursor_stream_value(value: &Value, state: &mut CursorStreamState) -> Vec<ProviderEvent> {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    let at = now_iso();

    match kind {
        "assistant" => parse_cursor_assistant_message(value, state, at),
        "tool_call" => parse_cursor_tool_call(value, at).into_iter().collect(),
        "result" => parse_cursor_result(value, state, at),
        "system" | "user" => Vec::new(),
        _ => Vec::new(),
    }
}

fn parse_cursor_assistant_message(
    value: &Value,
    state: &mut CursorStreamState,
    at: String,
) -> Vec<ProviderEvent> {
    let Some(message) = value.get("message").and_then(Value::as_object) else {
        return Vec::new();
    };
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut delta = String::new();
    for item in content {
        let Some(content_type) = item.get("type").and_then(Value::as_str) else {
            continue;
        };
        if content_type == "text" {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                delta.push_str(text);
            }
        }
    }
    if delta.is_empty() {
        return Vec::new();
    }
    let id = state
        .assistant_message_id
        .get_or_insert_with(|| "cursor:assistant:0".to_string())
        .clone();
    let mut events = Vec::new();
    if !state.assistant_message_started {
        // `assistant_message_id` is created on the first delta and retained for
        // the whole Cursor result. Cursor documents the terminal `result` as
        // the concatenation of these deltas, even when tools interleave.
        state.assistant_message_started = true;
        events.push(ProviderEvent::AssistantMessageStarted {
            id: id.clone(),
            phase: AssistantMessagePhase::Unknown,
            at,
        });
    }
    events.push(ProviderEvent::AssistantMessageDelta { id, content: delta });
    events
}

fn parse_cursor_tool_call(value: &Value, at: String) -> Option<ProviderEvent> {
    let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
    let call_id = value
        .get("call_id")
        .or_else(|| value.get("tool_call_id"))
        .and_then(Value::as_str)
        .unwrap_or("tool-call")
        .to_string();
    let tool_call = value.get("tool_call")?.as_object()?;
    let (tool_name, tool_value) = tool_call.iter().next()?;

    match subtype {
        "started" => {
            let (action, command, file) = parse_cursor_tool_call_metadata(tool_name, tool_value);
            Some(ProviderEvent::ToolCallStarted {
                id: call_id,
                action,
                command,
                file,
                at,
            })
        }
        "completed" => Some(ProviderEvent::ToolCallCompleted { id: call_id, at }),
        "failed" => Some(ProviderEvent::ToolCallFailed {
            id: call_id,
            reason: extract_cursor_tool_call_failure(tool_value),
            at,
        }),
        _ => None,
    }
}

fn parse_cursor_tool_call_metadata(
    tool_name: &str,
    tool_value: &Value,
) -> (String, Option<String>, Option<String>) {
    let action = humanize_cursor_tool_name(tool_name);
    let args = tool_value
        .get("args")
        .or_else(|| tool_value.get("input"))
        .or_else(|| tool_value.get("parameters"))
        .and_then(Value::as_object);

    let command = args
        .and_then(|args| {
            args.get("command")
                .or_else(|| args.get("query"))
                .or_else(|| args.get("text"))
                .or_else(|| args.get("fileText"))
                .or_else(|| args.get("path"))
        })
        .and_then(Value::as_str)
        .map(str::to_string);

    let file = args
        .and_then(|args| {
            args.get("path")
                .or_else(|| args.get("filePath"))
                .or_else(|| args.get("file"))
                .or_else(|| args.get("name"))
        })
        .and_then(Value::as_str)
        .map(str::to_string);

    (action, command, file)
}

fn extract_cursor_tool_call_failure(tool_value: &Value) -> Option<String> {
    let result = tool_value.get("result")?.as_object()?;
    if let Some(error) = result.get("error").and_then(Value::as_object) {
        if let Some(message) = error.get("message").and_then(Value::as_str) {
            return Some(message.to_string());
        }
    }
    result
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn humanize_cursor_tool_name(tool_name: &str) -> String {
    let name = tool_name
        .trim()
        .trim_end_matches("ToolCall")
        .trim_end_matches("toolCall")
        .trim_end_matches("Call");
    let lowered = name.to_lowercase();
    if lowered.contains("read") {
        "Read".to_string()
    } else if lowered.contains("write") {
        "Write".to_string()
    } else if lowered.contains("shell")
        || lowered.contains("terminal")
        || lowered.contains("command")
    {
        "Shell".to_string()
    } else if lowered.contains("mcp") {
        "MCP".to_string()
    } else if lowered.is_empty() {
        "Tool".to_string()
    } else {
        to_title_case_words(name)
    }
}

fn to_title_case_words(value: &str) -> String {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_cursor_result(
    value: &Value,
    state: &mut CursorStreamState,
    at: String,
) -> Vec<ProviderEvent> {
    let is_error = value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if is_error {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.get("result").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| "Cursor reported an error".to_string());
        return vec![ProviderEvent::Failed { message, at }];
    }
    let mut events = Vec::new();
    if let Some(content) = value
        .get("result")
        .and_then(Value::as_str)
        .filter(|content| !content.is_empty())
        .map(str::to_string)
    {
        events.push(ProviderEvent::AssistantMessageCompleted {
            id: state
                .assistant_message_id
                .take()
                .unwrap_or_else(|| "cursor:assistant:0".to_string()),
            phase: AssistantMessagePhase::Unknown,
            content: Some(content),
            model: None,
            at: at.clone(),
        });
        state.assistant_message_started = false;
    }
    events.push(ProviderEvent::Completed { at });
    events
}

async fn collect_stream_to_string<R>(reader: R) -> String
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut output = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(trimmed);
    }
    output
}

async fn run_cursor_about_command(
    cursor_binary: &str,
    cwd: &std::path::Path,
) -> Result<CursorCommandResult> {
    let mut command = Command::new(cursor_binary);
    command.arg("about").args(["--format", "json"]);
    command.current_dir(cwd);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = command.output().await.map_err(|error| {
        CoreError::Provider(format!("failed to execute {cursor_binary}: {error}"))
    })?;

    Ok(CursorCommandResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code().unwrap_or(-1),
    })
}

fn parse_cursor_about_result(result: &CursorCommandResult) -> CursorAboutResult {
    let json_payload = parse_cursor_about_json_payload(&result.stdout);
    if let Some(json_payload) = json_payload {
        let user_email = json_payload
            .get("userEmail")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_string);
        if let Some(email) = user_email {
            let lower_email = email.to_lowercase();
            if lower_email == "not logged in"
                || lower_email.contains("login required")
                || lower_email.contains("authentication required")
            {
                return CursorAboutResult {
					auth: CursorAuth {
						status: "unauthenticated".to_string(),
						_email: None,
					},
					message: Some("Cursor Agent is not authenticated. Run `cursor-agent login` and try again.".to_string()),
				};
            }

            return CursorAboutResult {
                auth: CursorAuth {
                    status: "authenticated".to_string(),
                    _email: Some(email),
                },
                message: None,
            };
        }

        if result.code == 0 {
            return CursorAboutResult {
                auth: CursorAuth {
                    status: "unknown".to_string(),
                    _email: None,
                },
                message: None,
            };
        }
    }

    let combined = join_cursor_messages(
        Some(result.stdout.trim().to_string()),
        Some(result.stderr.trim().to_string()),
    );
    CursorAboutResult {
        auth: CursorAuth {
            status: "unknown".to_string(),
            _email: None,
        },
        message: combined,
    }
}

fn parse_cursor_about_json_payload(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str::<Value>(trimmed).ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiscoveredCursorModel {
    id: String,
    label: String,
}

fn parse_cursor_models(raw: &str) -> Vec<DiscoveredCursorModel> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        let mut models = Vec::new();
        collect_model_strings(&value, &mut models);
        return models;
    }

    let mut models = Vec::new();
    for line in strip_ansi(raw).lines() {
        let cleaned = line.trim();
        if cleaned.is_empty() || is_cursor_model_header(cleaned) {
            continue;
        }
        if let Some(model) = parse_cursor_model_line(cleaned) {
            models.push(model);
        }
    }
    models
}

fn collect_model_strings(value: &Value, output: &mut Vec<DiscoveredCursorModel>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_model_strings(item, output);
            }
        }
        Value::Object(object) => {
            let id = object
                .get("id")
                .or_else(|| object.get("slug"))
                .or_else(|| object.get("model"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(id) = id {
                let label = object
                    .get("label")
                    .or_else(|| object.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(id);
                output.push(DiscoveredCursorModel {
                    id: id.to_string(),
                    label: label.to_string(),
                });
                return;
            }
            for key in ["models", "recommendedModels", "data", "items"] {
                if let Some(child) = object.get(key) {
                    collect_model_strings(child, output);
                }
            }
        }
        Value::String(string) => {
            let value = string.trim();
            if !value.is_empty() {
                output.push(DiscoveredCursorModel {
                    id: value.to_string(),
                    label: value.to_string(),
                });
            }
        }
        _ => {}
    }
}

fn is_cursor_model_header(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("usage:")
        || lower.starts_with("available models")
        || lower.starts_with("models")
        || lower.starts_with("recommended models")
        || lower.starts_with("list models")
        || lower.starts_with("---")
}

fn parse_cursor_model_line(line: &str) -> Option<DiscoveredCursorModel> {
    let trimmed = line.trim_start_matches(['-', '*', '•', ' ']).trim();
    if trimmed.is_empty() {
        return None;
    }

    for delimiter in [" - ", "\t", "|", "  "] {
        if let Some((left, right)) = trimmed.split_once(delimiter) {
            let id = left.trim();
            let label = right.trim();
            if !id.is_empty() {
                return Some(DiscoveredCursorModel {
                    id: id.to_string(),
                    label: if label.is_empty() { id } else { label }.to_string(),
                });
            }
        }
    }

    Some(DiscoveredCursorModel {
        id: trimmed.to_string(),
        label: trimmed.to_string(),
    })
}

fn parse_cursor_models_to_descriptors(raw: &str) -> Vec<ProviderModelDescriptor> {
    let mut seen = HashSet::from([CURSOR_AUTODETECT_MODEL_ID.to_string()]);
    let mut models = vec![ProviderModelDescriptor {
        id: CURSOR_AUTODETECT_MODEL_ID.to_string(),
        label: "Auto".to_string(),
        description: "Use Cursor's recommended model for this account.".to_string(),
        recommended: true,
        effort_levels: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
    }];

    for model in parse_cursor_models(raw) {
        let id = model.id.trim().to_string();
        if id.is_empty() {
            continue;
        }
        let normalized = id.to_lowercase();
        if normalized == CURSOR_AUTODETECT_MODEL_ID || !seen.insert(id.clone()) {
            continue;
        }
        let label = model.label.trim();
        models.push(ProviderModelDescriptor {
            id: id.clone(),
            label: if label.is_empty() {
                id
            } else {
                label.to_string()
            },
            description: CURSOR_MODEL_DESCRIPTION.to_string(),
            recommended: false,
            effort_levels: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
        });
    }

    models
}

#[derive(Clone, Debug)]
struct CursorAboutResult {
    auth: CursorAuth,
    message: Option<String>,
}

#[derive(Clone, Debug)]
struct CursorAuth {
    status: String,
    _email: Option<String>,
}

pub async fn discover_models() -> Vec<ProviderModelDescriptor> {
    let cwd = CursorProvider::current_working_dir();
    let result = run_cursor_models_command("cursor-agent", &cwd).await;
    match result {
        Ok(result) if result.code == 0 => parse_cursor_models_to_descriptors(&result.stdout),
        _ => vec![ProviderModelDescriptor {
            id: CURSOR_AUTODETECT_MODEL_ID.to_string(),
            label: "Auto".to_string(),
            description: "Use Cursor's recommended model for this account.".to_string(),
            recommended: true,
            effort_levels: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
        }],
    }
}

async fn run_cursor_models_command(
    cursor_binary: &str,
    cwd: &std::path::Path,
) -> Result<CursorCommandResult> {
    let mut command = Command::new(cursor_binary);
    command.arg("models");
    command.current_dir(cwd);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = command.output().await.map_err(|error| {
        CoreError::Provider(format!("failed to execute {cursor_binary}: {error}"))
    })?;

    Ok(CursorCommandResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code().unwrap_or(-1),
    })
}

#[async_trait]
impl Provider for CursorProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let cwd = Self::resolve_working_directory(&cfg);
        let result = self.run_command(&["create-chat"], Some(&cwd)).await?;
        if result.code != 0 {
            let message = join_cursor_messages(
                Some("failed to create Cursor chat".to_string()),
                Some(result.stderr.trim().to_string()),
            )
            .unwrap_or_else(|| "failed to create Cursor chat".to_string());
            return Err(CoreError::Provider(message));
        }

        let chat_id = result
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
            .ok_or_else(|| CoreError::Provider("Cursor did not return a chat id".to_string()))?;

        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id.clone(),
            handle_id: chat_id.clone(),
        };
        let runtime = Arc::new(CursorSessionRuntime {
            handle: handle.clone(),
            chat_id,
            model: cfg.model.clone(),
            cwd,
            additional_directories: cfg
                .additional_working_directories
                .iter()
                .map(PathBuf::from)
                .collect(),
            events_tx: broadcast::channel(64).0,
            active_turn: Mutex::new(None),
        });

        self.runtime
            .sessions
            .lock()
            .await
            .insert(cfg.session_id.0.clone(), runtime);
        Ok(handle)
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

        let (prompt, plan_mode) = match input {
            Input::Text(prompt) => (prompt, None),
            Input::Turn(turn) => (
                append_tool_instructions(
                    compose_fallback_prompt_for_provider(
                        "cursor",
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
                ),
                turn.plan_mode,
            ),
            Input::UserInputResponse(_) => {
                return Err(CoreError::Provider(
                    "Cursor does not support mid-turn user input responses".to_string(),
                ));
            }
            Input::PermissionResponse(_) => {
                return Err(CoreError::Provider(
                    "Cursor does not support mid-turn permission responses".to_string(),
                ));
            }
        };
        self.spawn_cursor_turn(runtime, prompt, plan_mode).await
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
            .runtime_for_session(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no runtime for session {} on provider {}",
                    handle.session_id.0, self.binary
                ))
            })?;

        let mut active_turn = runtime.active_turn.lock().await;
        let Some(turn) = active_turn.as_ref() else {
            return Ok(());
        };
        let _ = turn.cancel_tx.send(true);
        *active_turn = None;
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
        let cwd = Self::current_working_dir();
        let about = match run_cursor_about_command(&self.binary, &cwd).await {
            Ok(result) => result,
            Err(error) => {
                return Ok(HealthStatus::Unhealthy {
                    reason: error.to_string(),
                });
            }
        };

        let parsed = parse_cursor_about_result(&about);
        match parsed.auth.status.as_str() {
            "authenticated" => Ok(HealthStatus::Healthy),
            "unauthenticated" => Ok(HealthStatus::Unhealthy {
                reason: parsed
                    .message
                    .unwrap_or_else(|| "Cursor Agent is not authenticated".to_string()),
            }),
            _ => Ok(HealthStatus::Degraded {
                reason: parsed.message.unwrap_or_else(|| {
                    "Cursor Agent authentication could not be verified".to_string()
                }),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cursor_stream_assistant_and_tool_events() {
        let mut state = CursorStreamState::default();
        let assistant = parse_cursor_stream_line(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"},{"type":"text","text":" world"}]},"session_id":"sess"}"#,
            &mut state,
        );
        assert!(matches!(
            assistant.as_slice(),
            [
                ProviderEvent::AssistantMessageStarted { id: first_id, .. },
                ProviderEvent::AssistantMessageDelta { id: second_id, content }
            ] if first_id == "cursor:assistant:0" && second_id == "cursor:assistant:0"
                && content == "Hello world"
        ));

        let tool_started = parse_cursor_stream_line(
            r#"{"type":"tool_call","subtype":"started","call_id":"call_1","tool_call":{"readToolCall":{"args":{"path":"README.md"}}},"session_id":"sess"}"#,
            &mut state,
        );
        match tool_started.as_slice() {
            [ProviderEvent::ToolCallStarted {
                id,
                action,
                command,
                file,
                ..
            }] => {
                assert_eq!(id, "call_1");
                assert_eq!(action, "Read");
                assert_eq!(file.as_deref(), Some("README.md"));
                assert_eq!(command.as_deref(), Some("README.md"));
            }
            other => panic!("expected tool call start, got {other:?}"),
        }

        let tool_completed = parse_cursor_stream_line(
            r#"{"type":"tool_call","subtype":"completed","call_id":"call_1","tool_call":{"readToolCall":{"args":{"path":"README.md"},"result":{"success":{"content":"hello"}}}},"session_id":"sess"}"#,
            &mut state,
        );
        match tool_completed.as_slice() {
            [ProviderEvent::ToolCallCompleted { id, .. }] => {
                assert_eq!(id, "call_1");
            }
            other => panic!("expected tool call completed, got {other:?}"),
        }

        let result = parse_cursor_stream_line(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"done","session_id":"sess"}"#,
            &mut state,
        );
        assert!(matches!(
            result.as_slice(),
            [
                ProviderEvent::AssistantMessageCompleted {
                    id,
                    content: Some(content),
                    ..
                },
                ProviderEvent::Completed { .. }
            ] if id == "cursor:assistant:0" && content == "done"
        ));
    }

    #[test]
    fn parses_cursor_model_output_from_json_and_plain_text() {
        let json_models =
            parse_cursor_models_to_descriptors(r#"{"models":["claude-4-sonnet-thinking","o3"]}"#);
        assert!(json_models.iter().any(|model| model.id == "auto"));
        assert!(json_models
            .iter()
            .any(|model| model.id == "claude-4-sonnet-thinking"));
        assert!(json_models.iter().any(|model| model.id == "o3"));

        let text_models = parse_cursor_models_to_descriptors(
			"Available models\nauto - Auto (current, default)\nclaude-4-sonnet-thinking - Claude 4 Sonnet Thinking\no3 - Best reasoning",
        );
        assert_eq!(
            text_models
                .iter()
                .filter(|model| model.id == "auto")
                .count(),
            1
        );
        assert!(text_models
            .iter()
            .any(|model| model.id == "claude-4-sonnet-thinking"));
        assert!(text_models.iter().any(|model| model.id == "o3"));
        assert_eq!(
            text_models
                .iter()
                .find(|model| model.id == "claude-4-sonnet-thinking")
                .map(|model| model.label.as_str()),
            Some("Claude 4 Sonnet Thinking")
        );
    }

    #[test]
    fn normalizes_placeholder_model_ids_to_auto() {
        assert_eq!(
            CursorProvider::normalize_model_arg(Some("cursor-agent")),
            None
        );
        assert_eq!(
            CursorProvider::normalize_model_arg(Some("cursor-editor")),
            None
        );
        assert_eq!(CursorProvider::normalize_model_arg(Some("auto")), None);
        assert_eq!(
            CursorProvider::normalize_model_arg(Some("claude-4-sonnet-thinking")),
            Some("claude-4-sonnet-thinking".to_string())
        );
    }

    #[test]
    fn cursor_turn_args_use_native_plan_mode() {
        let provider = CursorProvider::new("cursor", "cursor-agent", cursor_capabilities());
        let plan_args = provider.cursor_turn_args("chat-1", None, "ship it", Some(true), &[]);
        assert!(plan_args.windows(2).any(|pair| pair == ["--mode", "plan"]));

        let execute_args = provider.cursor_turn_args("chat-1", None, "ship it", Some(false), &[]);
        assert!(!execute_args
            .windows(2)
            .any(|pair| pair == ["--mode", "plan"]));
    }

    #[test]
    fn cursor_turn_args_include_each_authorized_directory() {
        let provider = CursorProvider::new("cursor", "cursor-agent", cursor_capabilities());
        let directories = vec![PathBuf::from("/tmp/api"), PathBuf::from("/tmp/web")];
        let args = provider.cursor_turn_args("chat-1", None, "ship it", Some(false), &directories);
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--add-dir", "/tmp/api"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--add-dir", "/tmp/web"]));
        assert!(args.windows(2).any(|pair| pair == ["--sandbox", "enabled"]));
    }
}
