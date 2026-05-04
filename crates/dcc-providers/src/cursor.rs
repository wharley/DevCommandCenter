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
    domain::{
        provider::{
            Capabilities, HealthStatus, ProviderDescriptor, ProviderEvent, ProviderId,
            ProviderModelDescriptor, SessionHandle,
        },
        session::SessionId,
    },
    ports::{Input, Provider, SessionConfig},
    CoreError, Result,
};

use crate::common::experimental_cli_capabilities;

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

pub fn adapter() -> CursorProvider {
    CursorProvider::new("cursor", "cursor-agent", experimental_cli_capabilities())
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
        });
    }

    ProviderDescriptor {
        id: ProviderId("cursor".to_string()),
        label: PROVIDER_LABEL.to_string(),
        description: PROVIDER_DESCRIPTION.to_string(),
        models,
        capabilities: experimental_cli_capabilities(),
        health,
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

    fn cursor_turn_args(&self, chat_id: &str, model: Option<&str>, prompt: &str) -> Vec<String> {
        let mut args = vec![
            "--print".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--trust".to_string(),
            "--resume".to_string(),
            chat_id.to_string(),
        ];
        if let Some(model) = Self::normalize_model_arg(model) {
            args.push("--model".to_string());
            args.push(model);
        }
        args.push(prompt.to_string());
        args
    }

    async fn spawn_cursor_turn(
        &self,
        runtime: Arc<CursorSessionRuntime>,
        prompt: String,
    ) -> Result<()> {
        {
            let active_turn = runtime.active_turn.lock().await;
            if active_turn.is_some() {
                return Err(CoreError::Provider(
                    "cursor turn already active for this session".to_string(),
                ));
            }
        }

        let args = self.cursor_turn_args(&runtime.chat_id, runtime.model.as_deref(), &prompt);
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

            loop {
                let next_line = reader.next_line();
                tokio::pin!(next_line);
                tokio::select! {
                    line_result = &mut next_line => {
                        match line_result {
                            Ok(Some(line)) => {
                                if let Some(event) = parse_cursor_stream_line(&line) {
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

fn parse_cursor_stream_line(line: &str) -> Option<ProviderEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value = serde_json::from_str::<Value>(trimmed).ok()?;
    parse_cursor_stream_value(&value)
}

fn parse_cursor_stream_value(value: &Value) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    let at = now_iso();

    match kind {
        "assistant" => parse_cursor_assistant_message(value),
        "tool_call" => parse_cursor_tool_call(value, at),
        "result" => Some(parse_cursor_result(value, at)),
        "system" | "user" => None,
        _ => None,
    }
}

fn parse_cursor_assistant_message(value: &Value) -> Option<ProviderEvent> {
    let message = value.get("message")?.as_object()?;
    let content = message.get("content")?.as_array()?;
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
        return None;
    }
    Some(ProviderEvent::TextDelta { content: delta })
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

fn parse_cursor_result(value: &Value, at: String) -> ProviderEvent {
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
        return ProviderEvent::Failed { message, at };
    }
    ProviderEvent::Completed { at }
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

fn parse_cursor_models(raw: &str) -> Vec<String> {
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

fn collect_model_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_model_strings(item, output);
            }
        }
        Value::Object(object) => {
            for key in ["models", "recommendedModels", "data", "items"] {
                if let Some(child) = object.get(key) {
                    collect_model_strings(child, output);
                }
            }
            if let Some(model) = object.get("model").and_then(Value::as_str) {
                output.push(model.trim().to_string());
            }
            if let Some(id) = object.get("id").and_then(Value::as_str) {
                output.push(id.trim().to_string());
            }
            if let Some(slug) = object.get("slug").and_then(Value::as_str) {
                output.push(slug.trim().to_string());
            }
            if let Some(name) = object.get("name").and_then(Value::as_str) {
                output.push(name.trim().to_string());
            }
        }
        Value::String(string) => {
            output.push(string.trim().to_string());
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

fn parse_cursor_model_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start_matches(['-', '*', '•', ' ']).trim();
    if trimmed.is_empty() {
        return None;
    }

    for delimiter in ['\t', '|'] {
        if let Some((left, _)) = trimmed.split_once(delimiter) {
            let candidate = left.trim();
            if !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }
    }

    if let Some((left, _)) = trimmed.split_once("  ") {
        let candidate = left.trim();
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }

    Some(trimmed.to_string())
}

fn parse_cursor_models_to_descriptors(raw: &str) -> Vec<ProviderModelDescriptor> {
    let mut seen = HashSet::from([CURSOR_AUTODETECT_MODEL_ID.to_string()]);
    let mut models = vec![ProviderModelDescriptor {
        id: CURSOR_AUTODETECT_MODEL_ID.to_string(),
        label: "Auto".to_string(),
        description: "Use Cursor's recommended model for this account.".to_string(),
        recommended: true,
    }];

    for model in parse_cursor_models(raw) {
        let trimmed = model.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.to_lowercase();
        if normalized == CURSOR_AUTODETECT_MODEL_ID || !seen.insert(trimmed.clone()) {
            continue;
        }
        models.push(ProviderModelDescriptor {
            id: trimmed.clone(),
            label: trimmed,
            description: CURSOR_MODEL_DESCRIPTION.to_string(),
            recommended: false,
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

        let Input::Text(prompt) = input;
        self.spawn_cursor_turn(runtime, prompt).await
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
            match receiver.recv().await {
                Ok(event) => Some((Ok(event), receiver)),
                Err(broadcast::error::RecvError::Closed) => None,
                Err(broadcast::error::RecvError::Lagged(_)) => Some((
                    Ok(ProviderEvent::Failed {
                        message: "provider event stream lagged".to_string(),
                        at: now_iso(),
                    }),
                    receiver,
                )),
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
        let assistant = parse_cursor_stream_line(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"},{"type":"text","text":" world"}]},"session_id":"sess"}"#,
        );
        match assistant {
            Some(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "Hello world");
            }
            other => panic!("expected text delta, got {other:?}"),
        }

        let tool_started = parse_cursor_stream_line(
            r#"{"type":"tool_call","subtype":"started","call_id":"call_1","tool_call":{"readToolCall":{"args":{"path":"README.md"}}},"session_id":"sess"}"#,
        );
        match tool_started {
            Some(ProviderEvent::ToolCallStarted {
                id,
                action,
                command,
                file,
                ..
            }) => {
                assert_eq!(id, "call_1");
                assert_eq!(action, "Read");
                assert_eq!(file.as_deref(), Some("README.md"));
                assert_eq!(command.as_deref(), Some("README.md"));
            }
            other => panic!("expected tool call start, got {other:?}"),
        }

        let tool_completed = parse_cursor_stream_line(
            r#"{"type":"tool_call","subtype":"completed","call_id":"call_1","tool_call":{"readToolCall":{"args":{"path":"README.md"},"result":{"success":{"content":"hello"}}}},"session_id":"sess"}"#,
        );
        match tool_completed {
            Some(ProviderEvent::ToolCallCompleted { id, .. }) => {
                assert_eq!(id, "call_1");
            }
            other => panic!("expected tool call completed, got {other:?}"),
        }

        let result = parse_cursor_stream_line(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"done","session_id":"sess"}"#,
        );
        match result {
            Some(ProviderEvent::Completed { .. }) => {}
            other => panic!("expected result completion, got {other:?}"),
        }
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
			"Available models\n- claude-4-sonnet-thinking   Balanced default\n- o3   Best reasoning",
		);
        assert!(text_models
            .iter()
            .any(|model| model.id == "claude-4-sonnet-thinking"));
        assert!(text_models.iter().any(|model| model.id == "o3"));
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
}
