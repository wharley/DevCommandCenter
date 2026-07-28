use std::{collections::HashMap, fs, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::{self, BoxStream};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, Mutex},
};
use uuid::Uuid;

use dcc_core::{
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

use crate::claude_mcp::{
    parse_claude_mcp_status_snapshot, write_initial_mcp_configuration, CLAUDE_MCP_RUNTIME_VERSION,
};
use crate::common::{
    apply_cli_spawn_environment, augmented_path, now_iso, parse_provider_stream_line,
    ParsedProviderLine, ProviderStreamState,
};

#[derive(Clone)]
pub struct ClaudeSdkSidecarAdapter {
    pub id: ProviderId,
    pub label: String,
    pub description: String,
    pub capabilities: Capabilities,
    pub stable: bool,
    runtime: Arc<ProviderRuntimeState>,
}

#[derive(Default)]
struct ProviderRuntimeState {
    sessions: Mutex<HashMap<String, Arc<SessionRuntime>>>,
    account_usage: Mutex<HashMap<String, ProviderAccountUsage>>,
}

struct SessionRuntime {
    handle: SessionHandle,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    events_tx: broadcast::Sender<ProviderEvent>,
}

fn claude_reset_time(value: &Value) -> Option<String> {
    let raw_timestamp = value.as_i64()?;
    let timestamp = if raw_timestamp > 10_000_000_000 {
        raw_timestamp / 1_000
    } else {
        raw_timestamp
    };
    DateTime::<Utc>::from_timestamp(timestamp, 0).map(|value| value.to_rfc3339())
}

fn parse_claude_rate_limit_window(value: &Value) -> Option<ProviderUsageWindow> {
    if value.get("type").and_then(Value::as_str) != Some("rate_limit_event") {
        return None;
    }
    let info = value.get("rate_limit_info")?;
    let id = info
        .get("rateLimitType")
        .or_else(|| info.get("rate_limit_type"))
        .and_then(Value::as_str)
        .unwrap_or("subscription");
    let status = info.get("status").and_then(Value::as_str);
    let used_percent = match info.get("utilization").and_then(Value::as_f64) {
        Some(value) => {
            if value <= 1.0 {
                value * 100.0
            } else {
                value
            }
        }
        None if status == Some("rejected") => 100.0,
        // An allowed event may omit utilization. Treating that as 0% used made
        // DCC display a convincing but incorrect "100% remaining" value.
        None => return None,
    }
    .clamp(0.0, 100.0);
    let is_exhausted = status == Some("rejected") || used_percent >= 100.0;

    Some(ProviderUsageWindow {
        id: id.to_string(),
        used_percent,
        remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
        resets_at: info
            .get("resetsAt")
            .or_else(|| info.get("resets_at"))
            .and_then(claude_reset_time),
        window_duration_minutes: match id {
            "five_hour" => Some(300),
            "seven_day" | "seven_day_opus" | "seven_day_sonnet" => Some(10_080),
            _ => None,
        },
        is_exhausted,
    })
}

fn parse_claude_oauth_usage_window(id: &str, value: &Value) -> Option<ProviderUsageWindow> {
    let used_percent = value.get("utilization")?.as_f64()?.clamp(0.0, 100.0);
    Some(ProviderUsageWindow {
        id: id.to_string(),
        used_percent,
        remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
        resets_at: value
            .get("resets_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        window_duration_minutes: match id {
            "five_hour" => Some(300),
            "seven_day" | "seven_day_opus" | "seven_day_sonnet" => Some(10_080),
            _ => None,
        },
        is_exhausted: used_percent >= 100.0,
    })
}

fn parse_claude_oauth_account_usage(value: &Value) -> Result<ProviderAccountUsage> {
    let mut windows = Vec::new();
    for id in [
        "five_hour",
        "seven_day",
        "seven_day_opus",
        "seven_day_sonnet",
    ] {
        if let Some(window) = value
            .get(id)
            .and_then(|value| parse_claude_oauth_usage_window(id, value))
        {
            windows.push(window);
        }
    }

    if windows.is_empty() {
        return Err(CoreError::Provider(
            "Claude usage API returned no usage windows".to_string(),
        ));
    }

    Ok(ProviderAccountUsage {
        provider_id: ProviderId("claude_code".to_string()),
        state: ProviderAccountUsageState::Available,
        windows,
        plan_type: None,
        updated_at: Utc::now().to_rfc3339(),
        is_cached: false,
    })
}

fn claude_credentials_path(runtime: Option<&ProviderRuntimeConfig>) -> Option<PathBuf> {
    if let Some(config_dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        let path = PathBuf::from(config_dir);
        if !path.as_os_str().is_empty() {
            return Some(path.join(".credentials.json"));
        }
    }

    let home = runtime
        .and_then(|config| config.home_path.as_deref())
        .and_then(|path| crate::common::resolve_runtime_home_path(Some(path)))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))?;
    Some(home.join(".claude").join(".credentials.json"))
}

fn oauth_token_from_credentials(raw: &str) -> Option<String> {
    serde_json::from_str::<Value>(raw)
        .ok()?
        .pointer("/claudeAiOauth/accessToken")?
        .as_str()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

#[cfg(target_os = "macos")]
async fn claude_oauth_token_from_keychain() -> Option<String> {
    let mut command = Command::new("security");
    command.args([
        "find-generic-password",
        "-w",
        "-s",
        "Claude Code-credentials",
    ]);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    oauth_token_from_credentials(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(target_os = "macos"))]
async fn claude_oauth_token_from_keychain() -> Option<String> {
    None
}

async fn claude_oauth_access_token(runtime: Option<&ProviderRuntimeConfig>) -> Option<String> {
    if let Some(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
    {
        return Some(token);
    }

    if let Some(token) = claude_credentials_path(runtime)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| oauth_token_from_credentials(&raw))
    {
        return Some(token);
    }

    claude_oauth_token_from_keychain().await
}

async fn fetch_claude_account_usage(
    runtime: Option<&ProviderRuntimeConfig>,
) -> Result<Option<ProviderAccountUsage>> {
    let Some(token) = claude_oauth_access_token(runtime).await else {
        return Ok(None);
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| {
            CoreError::Provider(format!("failed to build Claude usage client: {error}"))
        })?;
    let response = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| CoreError::Provider(format!("Claude usage request failed: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CoreError::Provider(format!(
            "Claude usage request returned HTTP {status}"
        )));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|error| CoreError::Provider(format!("invalid Claude usage response: {error}")))?;
    parse_claude_oauth_account_usage(&value).map(Some)
}

fn provider_runtime_cache_key(runtime: Option<&ProviderRuntimeConfig>) -> String {
    let home = runtime
        .and_then(|config| config.home_path.as_deref())
        .unwrap_or_default();
    let shadow_home = runtime
        .and_then(|config| config.shadow_home_path.as_deref())
        .unwrap_or_default();
    format!("{home}\n{shadow_home}")
}

async fn cache_claude_account_usage(state: &ProviderRuntimeState, cache_key: &str, value: &Value) {
    let Some(window) = parse_claude_rate_limit_window(value) else {
        return;
    };
    let mut cache = state.account_usage.lock().await;
    let usage = cache
        .entry(cache_key.to_string())
        .or_insert_with(|| ProviderAccountUsage {
            provider_id: ProviderId("claude_code".to_string()),
            state: ProviderAccountUsageState::Available,
            windows: Vec::new(),
            plan_type: None,
            updated_at: Utc::now().to_rfc3339(),
            is_cached: true,
        });
    if let Some(existing) = usage.windows.iter_mut().find(|item| item.id == window.id) {
        *existing = window;
    } else {
        usage.windows.push(window);
    }
    usage.updated_at = Utc::now().to_rfc3339();
}

impl ClaudeSdkSidecarAdapter {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        capabilities: Capabilities,
        stable: bool,
    ) -> Self {
        Self {
            id: ProviderId(id.into()),
            label: label.into(),
            description: description.into(),
            capabilities,
            stable,
            runtime: Arc::new(ProviderRuntimeState::default()),
        }
    }

    fn sidecar_name() -> &'static str {
        if cfg!(windows) {
            "dcc-claude-sidecar.exe"
        } else {
            "dcc-claude-sidecar"
        }
    }

    fn repo_root_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = Vec::new();

        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.clone());
            if let Some(parent) = cwd.parent() {
                candidates.push(parent.to_path_buf());
            }
        }

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        candidates.push(manifest_dir.clone());
        if let Some(parent) = manifest_dir.parent() {
            candidates.push(parent.to_path_buf());
            if let Some(grandparent) = parent.parent() {
                candidates.push(grandparent.to_path_buf());
            }
        }

        candidates
    }

    fn script_path(&self) -> Option<PathBuf> {
        for base in self.repo_root_candidates() {
            let candidate = base.join("sidecar").join("src").join("index.mjs");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    fn repo_vendor_claude_bin_path(&self) -> Option<PathBuf> {
        let binary_name = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        for base in self.repo_root_candidates() {
            let candidate = base
                .join("sidecar")
                .join("dist")
                .join("vendor")
                .join("claude-code")
                .join(binary_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    fn bundled_sidecar_path(&self) -> Option<PathBuf> {
        if let Ok(path) = std::env::var("DCC_CLAUDE_SIDECAR_PATH") {
            let candidate = PathBuf::from(path);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        let exe = std::env::current_exe().ok()?;
        let exe_dir = exe.parent()?;
        let candidate = exe_dir.join(Self::sidecar_name());
        if candidate.is_file() {
            return Some(candidate);
        }
        None
    }

    fn vendor_claude_bin_path(&self) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let contents_dir = exe.parent()?.parent()?;
        let resources_dir = contents_dir.join("Resources");
        let name = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        let candidate = resources_dir.join("vendor").join("claude-code").join(name);
        if candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    }

    fn base_command(&self, extra_args: &[&str]) -> Result<Command> {
        if let Some(script_path) = self.script_path() {
            let mut command = Command::new("node");
            command.arg(script_path);
            command.args(extra_args);
            command.env("PATH", augmented_path());
            if let Some(repo_vendor_claude) = self.repo_vendor_claude_bin_path() {
                command.env("DCC_CLAUDE_CODE_BIN_PATH", repo_vendor_claude);
            }
            return Ok(command);
        }

        if let Some(sidecar_binary) = self.bundled_sidecar_path() {
            let mut command = Command::new(sidecar_binary);
            command.args(extra_args);
            command.env("PATH", augmented_path());
            if let Some(claude_bin_path) = self
                .vendor_claude_bin_path()
                .or_else(|| self.repo_vendor_claude_bin_path())
            {
                command.env("DCC_CLAUDE_CODE_BIN_PATH", claude_bin_path);
            }
            return Ok(command);
        }

        Err(CoreError::Provider(
            "Claude sidecar not found. Expected sidecar/src/index.mjs in dev or bundled dcc-claude-sidecar next to the app executable.".to_string(),
        ))
    }

    fn binary_command(&self, extra_args: &[&str]) -> Result<Command> {
        self.base_command(extra_args)
    }

    fn interactive_command(&self) -> Result<Command> {
        let mut command = self.base_command(&[])?;
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        Ok(command)
    }

    async fn start_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let account_usage_key = provider_runtime_cache_key(cfg.provider_runtime.as_ref());
        let mut command = self.interactive_command()?;
        apply_cli_spawn_environment(&mut command, &self.id.0, &cfg)?;
        let additional_directories = serde_json::to_string(&cfg.additional_working_directories)
            .map_err(|error| CoreError::Provider(error.to_string()))?;
        command.env("DCC_ADDITIONAL_DIRECTORIES", additional_directories);
        if let Some(ref working_directory) = cfg.working_directory {
            let cwd = PathBuf::from(working_directory);
            if !working_directory.trim().is_empty() {
                command.current_dir(cwd);
            }
        }

        let mut child = command.spawn().map_err(|error| {
            CoreError::Provider(format!("failed to spawn Claude sidecar: {error}"))
        })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            CoreError::Provider("Claude sidecar did not expose stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CoreError::Provider("Claude sidecar did not expose stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CoreError::Provider("Claude sidecar did not expose stderr".to_string())
        })?;
        if let Err(error) = write_initial_mcp_configuration(&mut stdin, &cfg.mcp_servers).await {
            let _ = child.start_kill();
            return Err(error);
        }

        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id,
            handle_id: Uuid::new_v4().to_string(),
        };
        let runtime_provider_id = handle.provider_id.clone();
        let runtime_session_id = handle.session_id.clone();
        let session_key = handle.session_id.0.clone();
        let (events_tx, _) = broadcast::channel(64);
        let runtime = Arc::new(SessionRuntime {
            handle: handle.clone(),
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(child),
            events_tx: events_tx.clone(),
        });

        self.runtime
            .sessions
            .lock()
            .await
            .insert(session_key.clone(), runtime.clone());

        let runtime_for_task = runtime.clone();
        let runtime_state = Arc::clone(&self.runtime);
        tokio::spawn(async move {
            let _ = runtime_for_task
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });

            let stderr_task = tokio::spawn(async move {
                let mut output = String::new();
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(line.trim_end());
                }
                output
            });

            let mut stream_state = ProviderStreamState::default();
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let content = line.trim_end().to_string();
                if content.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(&content) {
                    cache_claude_account_usage(&runtime_state, &account_usage_key, &value).await;
                    if let Some(snapshot) = parse_claude_mcp_status_snapshot(
                        &value,
                        &runtime_provider_id,
                        &runtime_session_id,
                    ) {
                        match snapshot {
                            Ok(statuses) => {
                                let _ = runtime_for_task
                                    .events_tx
                                    .send(ProviderEvent::McpRuntimeStatusSnapshot { statuses });
                            }
                            Err(_) => {
                                let _ = runtime_for_task.events_tx.send(ProviderEvent::Failed {
                                    message: "invalid Claude MCP status payload".to_string(),
                                    at: now_iso(),
                                });
                            }
                        }
                        continue;
                    }
                }
                match parse_provider_stream_line(&content, &mut stream_state) {
                    ParsedProviderLine::Event(event) => {
                        let _ = runtime_for_task.events_tx.send(event);
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
                let mut child = runtime_for_task.child.lock().await;
                child.wait().await
            };

            let at = now_iso();
            match exit_result {
                Ok(exit) if exit.success() => {}
                Ok(exit) => {
                    let message = if stderr_output.trim().is_empty() {
                        format!("Claude sidecar exited with status {exit}")
                    } else {
                        stderr_output.trim().to_string()
                    };
                    let _ = runtime_for_task
                        .events_tx
                        .send(ProviderEvent::Failed { message, at });
                }
                Err(error) => {
                    let _ = runtime_for_task.events_tx.send(ProviderEvent::Failed {
                        message: format!("failed to wait for Claude sidecar: {error}"),
                        at,
                    });
                }
            }

            runtime_state.sessions.lock().await.remove(&session_key);
        });

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

    fn sidecar_effort(effort: Option<&str>) -> Option<&'static str> {
        match effort.map(str::trim).filter(|value| !value.is_empty()) {
            Some("minimal") => Some("low"),
            Some("balanced") | Some("medium") => Some("medium"),
            Some("low") => Some("low"),
            Some("high") => Some("high"),
            Some("xhigh") => Some("xhigh"),
            Some("max") | Some("ultrathink") => Some("max"),
            Some(_) | None => None,
        }
    }
}

#[async_trait]
impl Provider for ClaudeSdkSidecarAdapter {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    fn dcc_mcp_projection_version(&self) -> Option<&str> {
        Some(CLAUDE_MCP_RUNTIME_VERSION)
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        self.start_runtime(cfg).await
    }

    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()> {
        let runtime = self
            .runtime_for_session(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no runtime for session {} on provider {}",
                    handle.session_id.0, self.label
                ))
            })?;

        match input {
            Input::Text(text) => {
                let mut stdin = runtime.stdin.lock().await;
                let stream = stdin.as_mut().ok_or_else(|| {
                    CoreError::Provider(format!(
                        "stdin closed for session {} on provider {}",
                        handle.session_id.0, self.label
                    ))
                })?;
                let payload = json!({
                    "type": "input",
                    "prompt": text,
                });
                let serialized = serde_json::to_string(&payload).map_err(|error| {
                    CoreError::Provider(format!("failed to encode Claude sidecar input: {error}"))
                })?;
                stream
                    .write_all(serialized.as_bytes())
                    .await
                    .map_err(|error| {
                        CoreError::Provider(format!(
                            "failed to write Claude sidecar input: {error}"
                        ))
                    })?;
                stream.write_all(b"\n").await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to terminate Claude sidecar input: {error}"
                    ))
                })?;
                stream.flush().await.map_err(|error| {
                    CoreError::Provider(format!("failed to flush Claude sidecar input: {error}"))
                })?;
            }
            Input::Turn(turn) => {
                let mut stdin = runtime.stdin.lock().await;
                let stream = stdin.as_mut().ok_or_else(|| {
                    CoreError::Provider(format!(
                        "stdin closed for session {} on provider {}",
                        handle.session_id.0, self.label
                    ))
                })?;
                let payload = json!({
                    "type": "input",
                    "prompt": turn.prompt,
                    "toolInstructions": turn.tool_instructions,
                    "planMode": turn.plan_mode,
                    "effort": Self::sidecar_effort(turn.effort.as_deref()),
                    "fastMode": turn.fast_mode,
                });
                let serialized = serde_json::to_string(&payload).map_err(|error| {
                    CoreError::Provider(format!("failed to encode Claude sidecar input: {error}"))
                })?;
                stream
                    .write_all(serialized.as_bytes())
                    .await
                    .map_err(|error| {
                        CoreError::Provider(format!(
                            "failed to write Claude sidecar input: {error}"
                        ))
                    })?;
                stream.write_all(b"\n").await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to terminate Claude sidecar input: {error}"
                    ))
                })?;
                stream.flush().await.map_err(|error| {
                    CoreError::Provider(format!("failed to flush Claude sidecar input: {error}"))
                })?;
            }
            Input::UserInputResponse(response) => {
                let mut stdin = runtime.stdin.lock().await;
                let stream = stdin.as_mut().ok_or_else(|| {
                    CoreError::Provider(format!(
                        "stdin closed for session {} on provider {}",
                        handle.session_id.0, self.label
                    ))
                })?;
                let payload = json!({
                    "type": "user_input_response",
                    "requestId": response.request_id,
                    "answers": response.answers,
                });
                let serialized = serde_json::to_string(&payload).map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to encode Claude sidecar user input response: {error}"
                    ))
                })?;
                stream
                    .write_all(serialized.as_bytes())
                    .await
                    .map_err(|error| {
                        CoreError::Provider(format!(
                            "failed to write Claude sidecar user input response: {error}"
                        ))
                    })?;
                stream.write_all(b"\n").await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to terminate Claude sidecar user input response: {error}"
                    ))
                })?;
                stream.flush().await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to flush Claude sidecar user input response: {error}"
                    ))
                })?;
            }
            Input::PermissionResponse(response) => {
                let mut stdin = runtime.stdin.lock().await;
                let stream = stdin.as_mut().ok_or_else(|| {
                    CoreError::Provider(format!(
                        "stdin closed for session {} on provider {}",
                        handle.session_id.0, self.label
                    ))
                })?;
                let payload = json!({
                    "type": "permission_response",
                    "requestId": response.request_id,
                    "behavior": response.behavior,
                });
                let serialized = serde_json::to_string(&payload).map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to encode Claude sidecar permission response: {error}"
                    ))
                })?;
                stream
                    .write_all(serialized.as_bytes())
                    .await
                    .map_err(|error| {
                        CoreError::Provider(format!(
                            "failed to write Claude sidecar permission response: {error}"
                        ))
                    })?;
                stream.write_all(b"\n").await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to terminate Claude sidecar permission response: {error}"
                    ))
                })?;
                stream.flush().await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to flush Claude sidecar permission response: {error}"
                    ))
                })?;
            }
        }

        Ok(())
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
                    handle.session_id.0, self.label
                ))
            })?;

        {
            let mut stdin = runtime.stdin.lock().await;
            *stdin = None;
        }

        let mut child = runtime.child.lock().await;
        child.kill().await.map_err(|error| {
            CoreError::Provider(format!("failed to cancel Claude sidecar: {error}"))
        })?;

        Ok(())
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        let runtime = self.runtime_for_session(previous).await.ok_or_else(|| {
            CoreError::Provider(format!(
                "no resumable runtime for session {} on provider {}",
                previous.0, self.label
            ))
        })?;

        Ok(runtime.handle.clone())
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        let mut auth_command = self.binary_command(&["--auth-status"])?;
        match auth_command.output().await {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Ok(value) = serde_json::from_str::<Value>(&stdout) {
                    if value.get("loggedIn").and_then(Value::as_bool) == Some(false) {
                        return Ok(HealthStatus::Unhealthy {
                            reason: "Claude Code is not authenticated. Run `claude auth login`."
                                .to_string(),
                        });
                    }
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let reason = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    format!("Claude auth check exited with status {}", output.status)
                };
                return Ok(HealthStatus::Degraded { reason });
            }
            Err(error) => {
                return Ok(HealthStatus::Unhealthy {
                    reason: format!("failed to execute Claude auth check: {error}"),
                });
            }
        }

        let mut version_command = self.binary_command(&["--version"])?;
        match version_command.output().await {
            Ok(output) if output.status.success() => Ok(HealthStatus::Healthy),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let reason = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    format!(
                        "Claude sidecar version check exited with status {}",
                        output.status
                    )
                };
                Ok(HealthStatus::Degraded { reason })
            }
            Err(error) => Ok(HealthStatus::Unhealthy {
                reason: format!("failed to execute Claude sidecar version check: {error}"),
            }),
        }
    }

    async fn account_usage(
        &self,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<Option<ProviderAccountUsage>> {
        let cache_key = provider_runtime_cache_key(runtime);
        let cached_usage = self
            .runtime
            .account_usage
            .lock()
            .await
            .get(&cache_key)
            .cloned();

        match fetch_claude_account_usage(runtime).await {
            Ok(Some(usage)) => {
                self.runtime
                    .account_usage
                    .lock()
                    .await
                    .insert(cache_key, usage.clone());
                Ok(Some(usage))
            }
            Ok(None) => Ok(Some(cached_usage.unwrap_or_else(|| ProviderAccountUsage {
                provider_id: self.id.clone(),
                state: ProviderAccountUsageState::AwaitingActivity,
                windows: Vec::new(),
                plan_type: None,
                updated_at: Utc::now().to_rfc3339(),
                is_cached: true,
            }))),
            Err(error) => {
                if let Some(mut usage) = cached_usage {
                    usage.is_cached = true;
                    Ok(Some(usage))
                } else {
                    Err(error)
                }
            }
        }
    }
}

#[cfg(test)]
mod account_usage_tests {
    use super::*;

    #[test]
    fn parses_claude_rate_limit_fraction_as_percent() {
        let window = parse_claude_rate_limit_window(&json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "allowed_warning",
                "rateLimitType": "five_hour",
                "utilization": 0.83,
                "resetsAt": 1_800_000_000
            }
        }))
        .expect("rate limit event should parse");

        assert_eq!(window.used_percent, 83.0);
        assert_eq!(window.remaining_percent, 17.0);
        assert_eq!(window.window_duration_minutes, Some(300));
    }

    #[test]
    fn ignores_allowed_rate_limit_event_without_utilization() {
        let window = parse_claude_rate_limit_window(&json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "allowed",
                "rateLimitType": "five_hour",
                "resetsAt": 1_800_000_000
            }
        }));

        assert!(window.is_none());
    }

    #[test]
    fn parses_live_claude_oauth_usage_windows() {
        let usage = parse_claude_oauth_account_usage(&json!({
            "five_hour": {
                "utilization": 37.0,
                "resets_at": "2026-07-22T18:00:00+00:00"
            },
            "seven_day": {
                "utilization": 26.5,
                "resets_at": "2026-07-27T18:00:00+00:00"
            },
            "seven_day_opus": null,
            "seven_day_sonnet": {
                "utilization": 1.0,
                "resets_at": null
            }
        }))
        .expect("OAuth usage response should parse");

        assert_eq!(usage.windows.len(), 3);
        assert_eq!(usage.windows[0].id, "five_hour");
        assert_eq!(usage.windows[0].used_percent, 37.0);
        assert_eq!(usage.windows[0].remaining_percent, 63.0);
        assert_eq!(usage.windows[0].window_duration_minutes, Some(300));
        assert_eq!(usage.windows[1].remaining_percent, 73.5);
        assert_eq!(usage.windows[2].remaining_percent, 99.0);
        assert!(!usage.is_cached);
    }

    #[test]
    fn reads_oauth_access_token_from_claude_credentials() {
        let token = oauth_token_from_credentials(
            r#"{"claudeAiOauth":{"accessToken":" oauth-token ","refreshToken":"secret"}}"#,
        );

        assert_eq!(token.as_deref(), Some("oauth-token"));
    }
}
