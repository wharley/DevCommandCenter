use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock, RwLock,
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
    application::{compose_fallback_prompt_for_provider, PromptInjectionOptions},
    domain::provider::{
        Capabilities, HealthStatus, McpSupportLevel, ProviderApprovalPolicy, ProviderDescriptor,
        ProviderEvent, ProviderId, ProviderModelDescriptor, SessionHandle, TurnControlSupport,
    },
    domain::session::SessionId,
    ports::{
        provider::{
            ProviderPermissionRequest, ProviderPermissionResponse, ProviderUserInputOption,
            ProviderUserInputQuestion, ProviderUserInputResponse,
        },
        Input, Provider, ProviderRuntimeConfig, ProviderTurnInput, SessionConfig,
    },
    CoreError, Result,
};

use crate::common::{append_tool_instructions, augmented_path};

const PROVIDER_ID: &str = "antigravity";
const PROVIDER_LABEL: &str = "Antigravity";
const AUTH_METHOD: &str = "oauth-personal";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const AUTH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PROMPT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_PENDING_INTERACTIONS: usize = 64;
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;

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

struct NativePermission {
    rpc_id: Value,
    options: HashMap<String, String>,
}

struct UserInputPermission {
    rpc_id: Value,
    options: HashMap<String, String>,
}

enum PendingInteraction {
    Native(NativePermission),
    UserInput(UserInputPermission),
}

struct SessionRuntime {
    handle: SessionHandle,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    native_session_id: Mutex<Option<String>>,
    pending: Arc<Mutex<HashMap<u64, PendingResponse>>>,
    interactions: Mutex<HashMap<String, PendingInteraction>>,
    next_id: AtomicU64,
    events_tx: broadcast::Sender<ProviderEvent>,
    reasoning_active: Mutex<bool>,
    approval_policy: Mutex<Option<ProviderApprovalPolicy>>,
    allowed_roots: Vec<PathBuf>,
}

impl SessionRuntime {
    async fn request(&self, method: &str, params: Value, wait: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        if let Err(error) = self
            .write(json!({
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
        let response = match timeout(wait, rx).await {
            Ok(response) => response,
            Err(_) => {
                self.pending.lock().await.remove(&id);
                return Err(CoreError::Provider(format!(
                    "Antigravity ACP {method} timed out"
                )));
            }
        };
        response
            .map_err(|_| CoreError::Provider(format!("Antigravity ACP {method} was cancelled")))?
            .map_err(|message| CoreError::Provider(format!("Antigravity ACP {method}: {message}")))
    }

    async fn write(&self, message: Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(message.to_string().as_bytes())
            .await
            .map_err(|_| channel_error())?;
        stdin.write_all(b"\n").await.map_err(|_| channel_error())?;
        stdin.flush().await.map_err(|_| channel_error())
    }

    async fn result(&self, id: &Value, result: Value) -> Result<()> {
        if !valid_rpc_id(id) {
            return Err(CoreError::InvalidInput(
                "Antigravity ACP request ID is invalid".to_string(),
            ));
        }
        self.write(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
            .await
    }

    async fn error(&self, id: &Value, code: i64, message: &str) -> Result<()> {
        if !valid_rpc_id(id) {
            return Ok(());
        }
        self.write(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }))
        .await
    }

    async fn prompt(&self, turn: ProviderTurnInput) -> Result<()> {
        let session_id =
            self.native_session_id.lock().await.clone().ok_or_else(|| {
                CoreError::Provider("Antigravity ACP session is unavailable".into())
            })?;
        let approval_policy = turn.approval_policy.unwrap_or(ProviderApprovalPolicy::Ask);
        self.set_approval_mode(&session_id, approval_policy).await?;
        let prompt = append_tool_instructions(
            compose_fallback_prompt_for_provider(
                PROVIDER_ID,
                &turn.prompt,
                turn.plan_mode,
                turn.effort.as_deref(),
                turn.fast_mode,
                PromptInjectionOptions {
                    plan: true,
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
            .write(json!({
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
        let events = self.events_tx.clone();
        let pending = self.pending.clone();
        tokio::spawn(async move {
            let at = now_iso();
            match timeout(PROMPT_TIMEOUT, rx).await {
                Ok(Ok(Ok(_))) => {
                    let _ = events.send(ProviderEvent::Completed { at });
                }
                Ok(Ok(Err(message))) => {
                    let _ = events.send(ProviderEvent::Failed { message, at });
                }
                Ok(Err(_)) => {
                    let _ = events.send(ProviderEvent::Failed {
                        message: "Antigravity ACP prompt was cancelled".into(),
                        at,
                    });
                }
                Err(_) => {
                    pending.lock().await.remove(&id);
                    let _ = events.send(ProviderEvent::Failed {
                        message: "Antigravity ACP prompt timed out".into(),
                        at,
                    });
                }
            }
        });
        Ok(())
    }

    async fn set_approval_mode(
        &self,
        session_id: &str,
        policy: ProviderApprovalPolicy,
    ) -> Result<()> {
        if *self.approval_policy.lock().await == Some(policy) {
            return Ok(());
        }
        self.request(
            "session/set_config_option",
            json!({
                "sessionId": session_id,
                "configId": "mode",
                "value": antigravity_permission_mode(policy),
            }),
            REQUEST_TIMEOUT,
        )
        .await?;
        *self.approval_policy.lock().await = Some(policy);
        Ok(())
    }

    async fn resolve_permission(&self, response: ProviderPermissionResponse) -> Result<()> {
        let interaction = self
            .interactions
            .lock()
            .await
            .remove(&response.request_id)
            .ok_or_else(|| {
                CoreError::InvalidInput("Antigravity permission is not pending".into())
            })?;
        match interaction {
            PendingInteraction::Native(permission) => {
                let selected = permission.options.get(&response.behavior).cloned();
                let result = selected
                    .map(|option_id| json!({ "outcome": { "outcome": "selected", "optionId": option_id } }))
                    .unwrap_or_else(|| json!({ "outcome": { "outcome": "cancelled" } }));
                self.result(&permission.rpc_id, result).await?;
            }
            PendingInteraction::UserInput(permission) => {
                self.result(
                    &permission.rpc_id,
                    json!({ "outcome": { "outcome": "cancelled" } }),
                )
                .await?;
            }
        }
        let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
            id: response.request_id,
            behavior: response.behavior,
            at: now_iso(),
        });
        Ok(())
    }

    async fn resolve_user_input(&self, response: ProviderUserInputResponse) -> Result<()> {
        let interaction = self
            .interactions
            .lock()
            .await
            .remove(&response.request_id)
            .ok_or_else(|| CoreError::InvalidInput("Antigravity question is not pending".into()))?;
        let PendingInteraction::UserInput(permission) = interaction else {
            return Err(CoreError::InvalidInput(
                "Antigravity interaction is not a question".into(),
            ));
        };
        let answer = response
            .answers
            .first()
            .map(|answer| answer.answer.as_str())
            .unwrap_or_default();
        let option = permission.options.get(answer).cloned();
        let result = option
            .map(|option_id| json!({ "outcome": { "outcome": "selected", "optionId": option_id } }))
            .unwrap_or_else(|| json!({ "outcome": { "outcome": "cancelled" } }));
        self.result(&permission.rpc_id, result).await?;
        let _ = self.events_tx.send(ProviderEvent::UserInputResolved {
            id: response.request_id,
            answers: response.answers,
            at: now_iso(),
        });
        Ok(())
    }
}

#[derive(Default)]
struct AdapterState {
    sessions: RwLock<HashMap<String, Arc<SessionRuntime>>>,
}

#[derive(Clone, Default)]
pub struct AntigravityAcpAdapter {
    state: Arc<AdapterState>,
}

impl AntigravityAcpAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    async fn runtime(&self, session_id: &SessionId) -> Option<Arc<SessionRuntime>> {
        self.state
            .sessions
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&session_id.0)
            .cloned()
    }

    async fn start(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        // One DCC Antigravity instance owns one credential profile. Serialize
        // startup so two windows cannot race the OAuth token file.
        let _startup_guard = startup_gate().lock().await;
        if !cfg.mcp_servers.is_empty() {
            return Err(CoreError::InvalidInput(
                "Antigravity does not yet accept DCC-projected MCP servers".into(),
            ));
        }
        if !cfg.additional_working_directories.is_empty() {
            return Err(CoreError::InvalidInput(
                "Antigravity ACP does not advertise additional workspace directories".into(),
            ));
        }
        let runtime_config = cfg.provider_runtime.clone().unwrap_or_default();
        let executable = resolve_executable(&runtime_config)?;
        let harness = sibling_harness(&executable)?;
        let profile = resolve_profile(&runtime_config)?;
        prepare_profile(&profile)?;
        let cwd = cfg
            .working_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                CoreError::InvalidInput("Antigravity requires a working directory".into())
            })?;
        let allowed_roots = canonical_roots(&cwd, &cfg.additional_working_directories)?;

        let mut command = Command::new(&executable);
        #[cfg(target_os = "linux")]
        command.arg("--uid=");
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&cwd)
            .kill_on_drop(true)
            .env("PATH", augmented_path())
            .env("GEMINI_HOME", &profile)
            .env("AGY_ACP_FORCE_FILE_STORAGE", "1")
            .env("ANTIGRAVITY_HARNESS_PATH", &harness)
            .env("PYTHONUNBUFFERED", "1")
            .env("ELECTRON_RUN_AS_NODE", "1");
        for key in [
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "GOOGLE_CLOUD_PROJECT",
            "GOOGLE_CLOUD_LOCATION",
            "GOOGLE_CLOUD_QUOTA_PROJECT",
            "GOOGLE_GENAI_USE_VERTEXAI",
            "GCLOUD_PROJECT",
            "CLOUDSDK_CORE_PROJECT",
            "AGY_ACP_CCPA_PROJECT",
            "AGY_ACP_ENABLE_OAUTH",
            "BROWSER",
        ] {
            command.env_remove(key);
        }

        let mut child = command.spawn().map_err(|error| {
            CoreError::Provider(format!(
                "failed to start the official Antigravity ACP agent: {error}"
            ))
        })?;
        let stdin = child.stdin.take().ok_or_else(channel_error)?;
        let stdout = child.stdout.take().ok_or_else(channel_error)?;
        let stderr = child.stderr.take().ok_or_else(channel_error)?;
        let handle = SessionHandle {
            provider_id: ProviderId(PROVIDER_ID.into()),
            session_id: cfg.session_id.clone(),
            handle_id: Uuid::new_v4().to_string(),
        };
        let (events_tx, _) = broadcast::channel(256);
        let runtime = Arc::new(SessionRuntime {
            handle: handle.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            native_session_id: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            interactions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            events_tx,
            reasoning_active: Mutex::new(false),
            approval_policy: Mutex::new(None),
            allowed_roots,
        });
        let key = cfg.session_id.0.clone();
        self.state
            .sessions
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.clone(), runtime.clone());
        spawn_reader(runtime.clone(), stdout, stderr, self.state.clone(), key);
        if let Err(error) = self.handshake(&runtime, &cfg).await {
            self.state
                .sessions
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&cfg.session_id.0);
            let _ = runtime.child.lock().await.start_kill();
            return Err(error);
        }
        Ok(handle)
    }

    async fn handshake(&self, runtime: &Arc<SessionRuntime>, cfg: &SessionConfig) -> Result<()> {
        let initialized = runtime
            .request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientInfo": { "name": "dcc", "version": env!("CARGO_PKG_VERSION") },
                    "clientCapabilities": {
                        "fs": { "readTextFile": true, "writeTextFile": true },
                        "terminal": false,
                    },
                }),
                REQUEST_TIMEOUT,
            )
            .await?;
        let name = initialized
            .pointer("/agentInfo/name")
            .and_then(Value::as_str);
        if name != Some("antigravity-acp") {
            return Err(CoreError::Provider(
                "the selected executable is not the official Antigravity ACP agent".into(),
            ));
        }
        let advertised = initialized
            .get("authMethods")
            .and_then(Value::as_array)
            .is_some_and(|methods| {
                methods
                    .iter()
                    .any(|method| method.get("id").and_then(Value::as_str) == Some(AUTH_METHOD))
            });
        if !advertised {
            return Err(CoreError::Provider(
                "Antigravity ACP does not advertise personal Google sign-in".into(),
            ));
        }
        runtime
            .request(
                "authenticate",
                json!({ "methodId": AUTH_METHOD }),
                AUTH_TIMEOUT,
            )
            .await
            .map_err(|error| {
                CoreError::Provider(format!(
                    "Antigravity Google sign-in failed. Complete the browser sign-in and try again: {error}"
                ))
            })?;
        let cwd = cfg.working_directory.as_deref().unwrap_or_default();
        let created = runtime
            .request(
                "session/new",
                json!({
                    "cwd": cwd,
                    "mcpServers": [],
                }),
                REQUEST_TIMEOUT,
            )
            .await?;
        let session_id = created
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CoreError::Provider("Antigravity session/new omitted sessionId".into()))?
            .to_string();
        *runtime.native_session_id.lock().await = Some(session_id.clone());
        let models = extract_models(&created);
        if !models.is_empty() {
            *model_cache()
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = models;
        }
        select_model(runtime, &session_id, cfg.model.as_deref(), &created).await
    }

    /// Runs the same authenticated handshake used by real sessions, then
    /// retires the probe. Credentials remain in the isolated profile while no
    /// setup process is left behind.
    pub async fn authenticate_account(
        &self,
        cfg: SessionConfig,
    ) -> Result<Vec<ProviderModelDescriptor>> {
        let session_id = cfg.session_id.clone();
        let handle = self.start(cfg).await?;
        let models = model_cache()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(runtime) = self.runtime(&handle.session_id).await {
            cancel_interactions(&runtime).await;
            let _ = runtime.child.lock().await.start_kill();
        }
        self.state
            .sessions
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&session_id.0);
        Ok(models)
    }
}

#[async_trait]
impl Provider for AntigravityAcpAdapter {
    fn id(&self) -> ProviderId {
        ProviderId(PROVIDER_ID.into())
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        self.start(cfg).await
    }

    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()> {
        let runtime = self
            .runtime(&handle.session_id)
            .await
            .ok_or_else(|| CoreError::Provider("Antigravity ACP runtime is unavailable".into()))?;
        match input {
            Input::Text(prompt) => {
                runtime
                    .prompt(ProviderTurnInput {
                        prompt,
                        tool_instructions: None,
                        plan_mode: None,
                        effort: None,
                        fast_mode: None,
                        approval_policy: None,
                    })
                    .await
            }
            Input::Turn(turn) => runtime.prompt(turn).await,
            Input::PermissionResponse(response) => runtime.resolve_permission(response).await,
            Input::UserInputResponse(response) => runtime.resolve_user_input(response).await,
        }
    }

    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
        let runtime = self
            .state
            .sessions
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&handle.session_id.0)
            .cloned();
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
            .runtime(&handle.session_id)
            .await
            .ok_or_else(|| CoreError::Provider("Antigravity ACP runtime is unavailable".into()))?;
        cancel_interactions(&runtime).await;
        if let Some(session_id) = runtime.native_session_id.lock().await.clone() {
            runtime
                .write(json!({
                    "jsonrpc": "2.0",
                    "method": "session/cancel",
                    "params": { "sessionId": session_id },
                }))
                .await?;
        }
        Ok(())
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        self.runtime(previous)
            .await
            .map(|runtime| runtime.handle.clone())
            .ok_or_else(|| CoreError::Provider("Antigravity ACP runtime is unavailable".into()))
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        match resolve_executable(&ProviderRuntimeConfig::default()).and_then(|path| sibling_harness(&path).map(|_| path)) {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(_) => Ok(HealthStatus::Degraded {
                reason: "The official Antigravity ACP runtime is not on PATH; configure its executable path before starting a session.".into(),
            }),
        }
    }

    async fn discover_models(&self) -> Result<Option<Vec<ProviderModelDescriptor>>> {
        Ok(Some(
            model_cache()
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        ))
    }
}

pub fn descriptor(
    health: HealthStatus,
    discovered: Vec<ProviderModelDescriptor>,
) -> ProviderDescriptor {
    let mut models = vec![ProviderModelDescriptor {
        id: "default".into(),
        label: "Account default".into(),
        description: "Use the default model offered to this Google account.".into(),
        recommended: true,
        effort_levels: Vec::new(),
    }];
    let mut seen = HashSet::from(["default".to_string()]);
    models.extend(
        discovered
            .into_iter()
            .filter(|model| seen.insert(model.id.clone())),
    );
    ProviderDescriptor {
        id: ProviderId(PROVIDER_ID.into()),
        label: PROVIDER_LABEL.into(),
        description: "Google's official Antigravity agent through ACP, with isolated credentials and DCC-owned approvals.".into(),
        models,
        capabilities: capabilities(),
        health,
        enabled: true,
        availability_generation: 0,
        stable: false,
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        streaming: true,
        supports_steering: false,
        supports_native_subagent_steering: false,
        supports_native_subagent_interrupt: false,
        mcp_support: McpSupportLevel::Unsupported,
        mcp_oauth_support: dcc_core::domain::provider::McpOauthSupport::Unsupported,
        tools: true,
        vision: false,
        resumable: false,
        experimental: true,
        can_be_delegation_target: true,
        can_request_delegation: false,
        supports_read_only_delegation: true,
        supports_edit_delegation: true,
        supports_multi_root: false,
        approval_policies: vec![
            ProviderApprovalPolicy::Ask,
            ProviderApprovalPolicy::Auto,
            ProviderApprovalPolicy::FullAccess,
        ],
        supports_runtime_home: true,
        supports_runtime_binary: true,
        supports_shadow_home: false,
        supports_subagent_concurrency: false,
        supports_account_usage: false,
        plan_mode_support: TurnControlSupport::PromptFallback,
        fast_mode_support: TurnControlSupport::PromptFallback,
        supports_dynamic_models: true,
        supports_compaction_command: false,
    }
}

const fn antigravity_permission_mode(policy: ProviderApprovalPolicy) -> &'static str {
    match policy {
        ProviderApprovalPolicy::Ask => "default",
        ProviderApprovalPolicy::Auto => "auto_edit",
        ProviderApprovalPolicy::FullAccess => "yolo",
    }
}

fn startup_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

fn model_cache() -> &'static RwLock<Vec<ProviderModelDescriptor>> {
    static MODELS: OnceLock<RwLock<Vec<ProviderModelDescriptor>>> = OnceLock::new();
    MODELS.get_or_init(|| RwLock::new(Vec::new()))
}

fn resolve_profile(runtime: &ProviderRuntimeConfig) -> Result<PathBuf> {
    let value = runtime
        .home_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoreError::Provider("Antigravity requires an isolated runtime home".into())
        })?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(CoreError::InvalidInput(
            "Antigravity runtime home must be an absolute path".into(),
        ));
    }
    Ok(path)
}

fn prepare_profile(profile: &Path) -> Result<()> {
    let acp = profile.join("antigravity-acp");
    fs::create_dir_all(&acp).map_err(|error| {
        CoreError::Provider(format!("could not create the Antigravity profile: {error}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for directory in [profile, acp.as_path()] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|error| {
                CoreError::Provider(format!(
                    "could not protect the Antigravity profile: {error}"
                ))
            })?;
        }
    }
    let settings = acp.join("settings.json");
    fs::write(&settings, b"{\"auth\":{\"type\":\"oauth-personal\"}}\n").map_err(|error| {
        CoreError::Provider(format!("could not configure Antigravity sign-in: {error}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&settings, fs::Permissions::from_mode(0o600)).map_err(|error| {
            CoreError::Provider(format!(
                "could not protect the Antigravity profile settings: {error}"
            ))
        })?;
    }
    Ok(())
}

fn resolve_executable(runtime: &ProviderRuntimeConfig) -> Result<PathBuf> {
    if let Some(value) = runtime
        .binary_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(value);
        if !path.is_absolute() || !is_executable_file(&path) {
            return Err(CoreError::Provider(
                "the configured Antigravity executable path is invalid".into(),
            ));
        }
        return Ok(path);
    }
    let name = if cfg!(windows) {
        "agy_acp_server.exe"
    } else {
        "agy_acp_server.par"
    };
    std::env::split_paths(&augmented_path())
        .map(|dir| dir.join(name))
        .find(|path| is_executable_file(path))
        .ok_or_else(|| {
            CoreError::Provider(
                "official Antigravity ACP runtime not found; install it or set its executable path"
                    .into(),
            )
        })
}

fn sibling_harness(executable: &Path) -> Result<PathBuf> {
    let name = if cfg!(windows) {
        "localharness_external.exe"
    } else {
        "localharness_external"
    };
    let path = executable
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(name);
    if !is_executable_file(&path) {
        return Err(CoreError::Provider(format!(
            "Antigravity requires {name} beside the ACP executable"
        )));
    }
    Ok(path)
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn canonical_roots(cwd: &Path, additional: &[String]) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for value in
        std::iter::once(cwd.to_string_lossy().to_string()).chain(additional.iter().cloned())
    {
        let path = fs::canonicalize(&value).map_err(|error| {
            CoreError::InvalidInput(format!("Antigravity workspace root is invalid: {error}"))
        })?;
        if !roots.contains(&path) {
            roots.push(path);
        }
    }
    Ok(roots)
}

fn authorized_existing_path(runtime: &SessionRuntime, raw: &str) -> Result<PathBuf> {
    let canonical = fs::canonicalize(raw)
        .map_err(|_| CoreError::InvalidInput("Antigravity file path does not exist".into()))?;
    if runtime
        .allowed_roots
        .iter()
        .any(|root| canonical.starts_with(root))
    {
        Ok(canonical)
    } else {
        Err(CoreError::InvalidInput(
            "Antigravity file path is outside the authorized workspace".into(),
        ))
    }
}

fn authorized_write_path(runtime: &SessionRuntime, raw: &str) -> Result<PathBuf> {
    let path = PathBuf::from(raw);
    if path.exists() {
        return authorized_existing_path(runtime, raw);
    }
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::InvalidInput("Antigravity file path is invalid".into()))?;
    let parent = fs::canonicalize(parent)
        .map_err(|_| CoreError::InvalidInput("Antigravity file parent does not exist".into()))?;
    if !runtime
        .allowed_roots
        .iter()
        .any(|root| parent.starts_with(root))
    {
        return Err(CoreError::InvalidInput(
            "Antigravity file path is outside the authorized workspace".into(),
        ));
    }
    let name = path
        .file_name()
        .ok_or_else(|| CoreError::InvalidInput("Antigravity file path is invalid".into()))?;
    Ok(parent.join(name))
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
            let mut stderr = stderr;
            let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        });
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("Open the following link to authenticate")
            {
                continue;
            }
            let Ok(message) = serde_json::from_str::<Incoming>(trimmed) else {
                continue;
            };
            if let Some(id) = message.id.as_ref().and_then(Value::as_u64) {
                if message.result.is_some() || message.error.is_some() {
                    if let Some(sender) = runtime.pending.lock().await.remove(&id) {
                        let response = match (message.result, message.error) {
                            (Some(result), _) => Ok(result),
                            (_, Some(error)) => Err(rpc_error_message(&error)),
                            _ => Err("empty response".into()),
                        };
                        let _ = sender.send(response);
                    }
                    continue;
                }
            }
            if let (Some(id), Some(method)) = (message.id.as_ref(), message.method.as_deref()) {
                let params = message.params.as_ref().unwrap_or(&Value::Null);
                handle_reverse_request(&runtime, id, method, params).await;
                continue;
            }
            if let (Some(method), Some(params)) =
                (message.method.as_deref(), message.params.as_ref())
            {
                for event in notification_events(&runtime, method, params).await {
                    let _ = runtime.events_tx.send(event);
                }
            }
        }
        for (_, sender) in runtime.pending.lock().await.drain() {
            let _ = sender.send(Err("Antigravity ACP process stopped".into()));
        }
        cancel_interactions(&runtime).await;
        let _ = runtime.child.lock().await.start_kill();
        let exit = runtime.child.lock().await.wait().await;
        let _ = stderr_task.await;
        if !matches!(exit, Ok(status) if status.success()) {
            let _ = runtime.events_tx.send(ProviderEvent::Failed {
                message: "Antigravity ACP process stopped unexpectedly".into(),
                at: now_iso(),
            });
        }
        state
            .sessions
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&session_key);
    });
}

async fn handle_reverse_request(
    runtime: &Arc<SessionRuntime>,
    id: &Value,
    method: &str,
    params: &Value,
) {
    match method {
        "fs/read_text_file" => handle_read(runtime, id, params).await,
        "fs/write_text_file" => handle_write(runtime, id, params).await,
        "session/request_permission" => handle_permission(runtime, id, params).await,
        _ => {
            let _ = runtime
                .error(id, -32601, "ACP method is not supported by DCC")
                .await;
        }
    }
}

async fn handle_read(runtime: &Arc<SessionRuntime>, id: &Value, params: &Value) {
    let Some(raw) = params.get("path").and_then(Value::as_str) else {
        let _ = runtime.error(id, -32602, "Missing file path").await;
        return;
    };
    let result = authorized_existing_path(runtime, raw).and_then(|path| {
        let metadata =
            fs::metadata(&path).map_err(|error| CoreError::Provider(error.to_string()))?;
        if metadata.len() > MAX_FILE_BYTES as u64 {
            return Err(CoreError::InvalidInput(
                "Antigravity file is too large".into(),
            ));
        }
        fs::read_to_string(path).map_err(|error| CoreError::Provider(error.to_string()))
    });
    match result {
        Ok(content) => {
            let _ = runtime.result(id, json!({ "content": content })).await;
        }
        Err(error) => {
            let _ = runtime.error(id, -32000, &error.to_string()).await;
        }
    }
}

async fn handle_write(runtime: &Arc<SessionRuntime>, id: &Value, params: &Value) {
    let Some(raw) = params.get("path").and_then(Value::as_str) else {
        let _ = runtime.error(id, -32602, "Missing file path").await;
        return;
    };
    let Some(content) = params.get("content").and_then(Value::as_str) else {
        let _ = runtime.error(id, -32602, "Missing file content").await;
        return;
    };
    if content.len() > MAX_FILE_BYTES {
        let _ = runtime.error(id, -32602, "File content is too large").await;
        return;
    }
    let result = authorized_write_path(runtime, raw).and_then(|path| {
        // The official agent gates writes through session/request_permission
        // according to the negotiated mode before calling this ACP method.
        // Keep containment authoritative here without prompting twice.
        fs::write(path, content).map_err(|error| {
            CoreError::Provider(format!("Antigravity could not write the file: {error}"))
        })
    });
    match result {
        Ok(()) => {
            let _ = runtime.result(id, json!({})).await;
        }
        Err(error) => {
            let _ = runtime.error(id, -32000, &error.to_string()).await;
        }
    }
}

async fn handle_permission(runtime: &Arc<SessionRuntime>, id: &Value, params: &Value) {
    let tool = params.get("toolCall").unwrap_or(&Value::Null);
    let tool_call_id = tool
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let options = parse_options(params);
    if tool_call_id.starts_with("interaction_") {
        let request_id = Uuid::new_v4().to_string();
        let choices = params
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                let id = option.get("optionId")?.as_str()?.to_string();
                let label = option
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string();
                Some((label, id))
            })
            .collect::<HashMap<_, _>>();
        let question = ProviderUserInputQuestion {
            id: tool_call_id.to_string(),
            header: "Antigravity".into(),
            question: tool
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Choose an option")
                .to_string(),
            options: choices
                .keys()
                .map(|label| ProviderUserInputOption {
                    label: label.clone(),
                    description: label.clone(),
                })
                .collect(),
        };
        if insert_interaction(
            runtime,
            request_id.clone(),
            PendingInteraction::UserInput(UserInputPermission {
                rpc_id: id.clone(),
                options: choices,
            }),
        )
        .await
        {
            let _ = runtime.events_tx.send(ProviderEvent::UserInputRequested {
                id: request_id,
                questions: vec![question],
                at: now_iso(),
            });
        } else {
            let _ = runtime
                .result(id, json!({ "outcome": { "outcome": "cancelled" } }))
                .await;
        }
        return;
    }
    if let Some(selected) =
        automatic_permission_option(*runtime.approval_policy.lock().await, &options)
    {
        let result = selected
            .map(|option_id| json!({ "outcome": { "outcome": "selected", "optionId": option_id } }))
            .unwrap_or_else(|| json!({ "outcome": { "outcome": "cancelled" } }));
        let _ = runtime.result(id, result).await;
        return;
    }
    let request_id = Uuid::new_v4().to_string();
    if !insert_interaction(
        runtime,
        request_id.clone(),
        PendingInteraction::Native(NativePermission {
            rpc_id: id.clone(),
            options,
        }),
    )
    .await
    {
        let _ = runtime
            .result(id, json!({ "outcome": { "outcome": "cancelled" } }))
            .await;
        return;
    }
    let raw_input = tool.get("rawInput").or_else(|| tool.get("input"));
    let command = raw_input
        .and_then(|value| value.get("command").or_else(|| value.get("CommandLine")))
        .and_then(Value::as_str)
        .map(str::to_string);
    let _ = runtime.events_tx.send(ProviderEvent::PermissionRequested {
        request: ProviderPermissionRequest {
            request_id,
            tool_name: tool
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string(),
            title: tool
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            description: None,
            command,
            file: None,
        },
        at: now_iso(),
    });
}

fn parse_options(params: &Value) -> HashMap<String, String> {
    let mut options = HashMap::new();
    for option in params
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = option.get("optionId").and_then(Value::as_str) else {
            continue;
        };
        match option.get("kind").and_then(Value::as_str) {
            Some("allow_once") => {
                options.insert("allow".into(), id.into());
            }
            Some("allow_always") => {
                options.insert("allow_always".into(), id.into());
            }
            Some("reject_once") => {
                options.insert("deny".into(), id.into());
            }
            _ => {}
        }
    }
    options
}

fn automatic_permission_option(
    policy: Option<ProviderApprovalPolicy>,
    options: &HashMap<String, String>,
) -> Option<Option<String>> {
    (policy == Some(ProviderApprovalPolicy::FullAccess)).then(|| {
        options
            .get("allow")
            .or_else(|| options.get("allow_always"))
            .cloned()
    })
}

async fn insert_interaction(
    runtime: &Arc<SessionRuntime>,
    id: String,
    pending: PendingInteraction,
) -> bool {
    let mut interactions = runtime.interactions.lock().await;
    if interactions.len() >= MAX_PENDING_INTERACTIONS {
        false
    } else {
        interactions.insert(id, pending);
        true
    }
}

async fn cancel_interactions(runtime: &Arc<SessionRuntime>) {
    let pending = runtime
        .interactions
        .lock()
        .await
        .drain()
        .collect::<Vec<_>>();
    for (id, interaction) in pending {
        let rpc_id = match interaction {
            PendingInteraction::Native(value) => value.rpc_id,
            PendingInteraction::UserInput(value) => value.rpc_id,
        };
        let _ = runtime
            .result(&rpc_id, json!({ "outcome": { "outcome": "cancelled" } }))
            .await;
        let _ = runtime.events_tx.send(ProviderEvent::PermissionResolved {
            id,
            behavior: "cancelled".into(),
            at: now_iso(),
        });
    }
}

async fn notification_events(
    runtime: &Arc<SessionRuntime>,
    method: &str,
    params: &Value,
) -> Vec<ProviderEvent> {
    if method != "session/update"
        || params.get("sessionId").and_then(Value::as_str)
            != runtime.native_session_id.lock().await.as_deref()
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
                    label: Some("Thinking".into()),
                    at,
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
                .or_else(|| update.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string(),
            command: update_command(update),
            file: update_file(update),
            at,
        }],
        "tool_call_update" => {
            let id = update_id(update, "tool");
            match update.get("status").and_then(Value::as_str).unwrap_or("") {
                "completed" | "complete" | "success" => {
                    vec![ProviderEvent::ToolCallCompleted { id, at }]
                }
                "failed" | "error" => vec![ProviderEvent::ToolCallFailed {
                    id,
                    reason: Some("Antigravity tool call failed".into()),
                    at,
                }],
                _ => update_text(update)
                    .map(|content| ProviderEvent::ToolCallDelta { id, content })
                    .into_iter()
                    .collect(),
            }
        }
        "available_commands_update" | "config_option_update" => Vec::new(),
        _ => Vec::new(),
    }
}

fn extract_models(value: &Value) -> Vec<ProviderModelDescriptor> {
    let Some(config) = value
        .get("configOptions")
        .and_then(Value::as_array)
        .and_then(|options| {
            options
                .iter()
                .find(|option| option.get("id").and_then(Value::as_str) == Some("model"))
        })
    else {
        return Vec::new();
    };
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    collect_model_options(config.get("options"), &mut models, &mut seen);
    models
}

fn collect_model_options(
    value: Option<&Value>,
    models: &mut Vec<ProviderModelDescriptor>,
    seen: &mut HashSet<String>,
) {
    for option in value.and_then(Value::as_array).into_iter().flatten() {
        if let Some(nested) = option.get("options") {
            collect_model_options(Some(nested), models, seen);
            continue;
        }
        let Some(id) = option
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if seen.insert(id.to_string()) {
            models.push(ProviderModelDescriptor {
                id: id.to_string(),
                label: option
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_string(),
                description: "Model available to the authenticated Antigravity account.".into(),
                recommended: false,
                effort_levels: Vec::new(),
            });
        }
    }
}

async fn select_model(
    runtime: &Arc<SessionRuntime>,
    session_id: &str,
    requested: Option<&str>,
    setup: &Value,
) -> Result<()> {
    let Some(requested) = requested
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default")
    else {
        return Ok(());
    };
    let models = extract_models(setup);
    if !models.iter().any(|model| model.id == requested) {
        return Err(CoreError::Provider(format!(
            "Antigravity model '{requested}' is unavailable for this Google account"
        )));
    }
    runtime
        .request(
            "session/set_config_option",
            json!({ "sessionId": session_id, "configId": "model", "value": requested }),
            REQUEST_TIMEOUT,
        )
        .await?;
    Ok(())
}

fn update_id(update: &Value, fallback: &str) -> String {
    update
        .get("toolCallId")
        .or_else(|| update.get("id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn update_text(update: &Value) -> Option<String> {
    update
        .get("content")
        .and_then(|content| content.get("text").or(Some(content)))
        .or_else(|| update.get("text"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn update_command(update: &Value) -> Option<String> {
    update
        .get("rawInput")
        .or_else(|| update.get("input"))
        .and_then(|input| input.get("command").or_else(|| input.get("CommandLine")))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn update_file(update: &Value) -> Option<String> {
    update
        .get("locations")
        .and_then(Value::as_array)
        .and_then(|locations| locations.first())
        .and_then(|location| location.get("path"))
        .or_else(|| update.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn rpc_error_message(error: &Value) -> String {
    error
        .get("message")
        .or_else(|| error.get("errorMessage"))
        .and_then(Value::as_str)
        .unwrap_or("request failed")
        .to_string()
}

fn valid_rpc_id(value: &Value) -> bool {
    matches!(value, Value::Number(number) if number.as_u64().is_some())
        || matches!(value, Value::String(value) if !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control))
}

fn channel_error() -> CoreError {
    CoreError::Provider("Antigravity ACP private channel failed".into())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// Probes a downloaded runtime before the managed installer makes it active.
/// This intentionally stops after `initialize`: validation must never start an
/// OAuth flow or reuse the user's Antigravity profile.
pub async fn validate_official_runtime(
    executable: &Path,
    profile: &Path,
    expected_version: &str,
) -> Result<()> {
    let harness = sibling_harness(executable)?;
    prepare_profile(profile)?;
    let mut command = Command::new(executable);
    #[cfg(target_os = "linux")]
    command.arg("--uid=");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .env("GEMINI_HOME", profile)
        .env("AGY_ACP_FORCE_FILE_STORAGE", "1")
        .env("ANTIGRAVITY_HARNESS_PATH", harness)
        .env("PYTHONUNBUFFERED", "1")
        .env("ELECTRON_RUN_AS_NODE", "1");
    let mut child = command.spawn().map_err(|error| {
        CoreError::Provider(format!(
            "downloaded Antigravity runtime could not start: {error}"
        ))
    })?;
    let mut stdin = child.stdin.take().ok_or_else(channel_error)?;
    let stdout = child.stdout.take().ok_or_else(channel_error)?;
    let request = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientInfo": { "name": "dcc-installer", "version": env!("CARGO_PKG_VERSION") },
            "clientCapabilities": {
                "fs": { "readTextFile": false, "writeTextFile": false },
                "terminal": false,
            },
        },
    });
    stdin
        .write_all(format!("{request}\n").as_bytes())
        .await
        .map_err(|_| channel_error())?;
    stdin.flush().await.map_err(|_| channel_error())?;
    let probe = async {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await.map_err(|_| channel_error())? {
            let Ok(message) = serde_json::from_str::<Incoming>(line.trim()) else {
                continue;
            };
            if message.id.as_ref().and_then(Value::as_u64) != Some(0) {
                continue;
            }
            if let Some(error) = message.error {
                return Err(CoreError::Provider(format!(
                    "downloaded Antigravity runtime rejected initialize: {}",
                    rpc_error_message(&error)
                )));
            }
            let result = message.result.ok_or_else(|| {
                CoreError::Provider(
                    "downloaded Antigravity runtime returned an empty initialize response".into(),
                )
            })?;
            let valid = result.get("protocolVersion").and_then(Value::as_u64) == Some(1)
                && result.pointer("/agentInfo/name").and_then(Value::as_str)
                    == Some("antigravity-acp")
                && result.pointer("/agentInfo/version").and_then(Value::as_str)
                    == Some(expected_version)
                && result
                    .get("authMethods")
                    .and_then(Value::as_array)
                    .is_some_and(|methods| {
                        methods.iter().any(|method| {
                            method.get("id").and_then(Value::as_str) == Some(AUTH_METHOD)
                        })
                    });
            if !valid {
                return Err(CoreError::Provider(
                    "downloaded runtime did not identify as the expected official Antigravity ACP release".into(),
                ));
            }
            return Ok(());
        }
        Err(CoreError::Provider(
            "downloaded Antigravity runtime stopped during validation".into(),
        ))
    };
    let result = timeout(Duration::from_secs(90), probe)
        .await
        .map_err(|_| CoreError::Provider("Antigravity runtime validation timed out".into()))?;
    let _ = child.start_kill();
    let _ = child.wait().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_grouped_account_models() {
        let setup = json!({
            "configOptions": [{
                "id": "model",
                "type": "select",
                "options": [
                    { "name": "Gemini", "options": [
                        { "name": "Gemini Fast", "value": "gemini-fast" },
                        { "name": "Gemini Pro", "value": "gemini-pro" }
                    ]}
                ]
            }]
        });
        let models = extract_models(&setup);
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gemini-fast", "gemini-pro"]
        );
    }

    #[test]
    fn maps_only_native_permission_kinds() {
        let options = parse_options(&json!({ "options": [
            { "optionId": "once", "kind": "allow_once" },
            { "optionId": "always", "kind": "allow_always" },
            { "optionId": "reject", "kind": "reject_once" }
        ]}));
        assert_eq!(options.get("allow").map(String::as_str), Some("once"));
        assert_eq!(
            options.get("allow_always").map(String::as_str),
            Some("always")
        );
        assert_eq!(options.get("deny").map(String::as_str), Some("reject"));
    }

    #[test]
    fn maps_dcc_approval_policies_to_official_antigravity_modes() {
        assert_eq!(
            antigravity_permission_mode(ProviderApprovalPolicy::Ask),
            "default"
        );
        assert_eq!(
            antigravity_permission_mode(ProviderApprovalPolicy::Auto),
            "auto_edit"
        );
        assert_eq!(
            antigravity_permission_mode(ProviderApprovalPolicy::FullAccess),
            "yolo"
        );
    }

    #[test]
    fn only_full_access_resolves_exceptional_permissions_automatically() {
        let options = HashMap::from([
            ("allow".to_string(), "once".to_string()),
            ("allow_always".to_string(), "always".to_string()),
        ]);
        assert_eq!(
            automatic_permission_option(Some(ProviderApprovalPolicy::Ask), &options),
            None
        );
        assert_eq!(
            automatic_permission_option(Some(ProviderApprovalPolicy::Auto), &options),
            None
        );
        assert_eq!(
            automatic_permission_option(Some(ProviderApprovalPolicy::FullAccess), &options),
            Some(Some("once".to_string()))
        );
        assert_eq!(
            automatic_permission_option(Some(ProviderApprovalPolicy::FullAccess), &HashMap::new()),
            Some(None)
        );
    }

    #[test]
    fn explicit_binary_path_fails_closed() {
        let runtime = ProviderRuntimeConfig {
            binary_path: Some("/definitely/missing/agy_acp_server.par".into()),
            ..ProviderRuntimeConfig::default()
        };
        let error = resolve_executable(&runtime).expect_err("invalid override");
        assert!(error
            .to_string()
            .contains("configured Antigravity executable"));
    }

    #[test]
    fn prepares_a_private_personal_oauth_profile() {
        let root = std::env::temp_dir().join(format!("dcc-antigravity-{}", Uuid::new_v4()));
        prepare_profile(&root).expect("prepare profile");
        let settings = root.join("antigravity-acp/settings.json");
        assert_eq!(
            fs::read_to_string(&settings).expect("settings"),
            "{\"auth\":{\"type\":\"oauth-personal\"}}\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&settings)
                    .expect("settings metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(root).expect("cleanup profile");
    }
}
