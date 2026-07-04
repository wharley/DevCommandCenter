use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};

use async_trait::async_trait;
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
}

struct SessionRuntime {
    handle: SessionHandle,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    events_tx: broadcast::Sender<ProviderEvent>,
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
        let mut command = self.interactive_command()?;
        apply_cli_spawn_environment(&mut command, &self.id.0, &cfg)?;
        if let Some(ref working_directory) = cfg.working_directory {
            let cwd = PathBuf::from(working_directory);
            if !working_directory.trim().is_empty() {
                command.current_dir(cwd);
            }
        }

        let mut child = command.spawn().map_err(|error| {
            CoreError::Provider(format!("failed to spawn Claude sidecar: {error}"))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            CoreError::Provider("Claude sidecar did not expose stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CoreError::Provider("Claude sidecar did not expose stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CoreError::Provider("Claude sidecar did not expose stderr".to_string())
        })?;

        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id,
            handle_id: Uuid::new_v4().to_string(),
        };
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
}
