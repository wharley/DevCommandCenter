use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use dcc_core::ports::provider::{ProviderPermissionRequest, ProviderPermissionResponse};
use dcc_core::{
    application::{
        resolve_session_mcp_servers, run_provider_computer_use_conformance,
        run_provider_mcp_conformance, ResolveSessionMcpInput,
    },
    domain::{
        mcp::{
            McpBinding, McpBindingId, McpBindingScope, McpDefinition, McpDefinitionId,
            McpDefinitionOwnership, McpRuntimeState, McpSecretBinding, McpSecretReferenceId,
            McpSecretTarget, McpTransport, McpTransportKind, McpTrust, McpTrustDecision,
            McpTrustFingerprint,
        },
        project::ProjectId,
        provider::{HealthStatus, ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
        workspace::WorkspaceId,
    },
    ports::{
        ComputerUseConformanceAdapter, ComputerUseConformanceAdapterError,
        ComputerUseConformanceAdapterResult, ComputerUseConformanceObservation,
        ComputerUseConformanceStep, Input, McpConformanceAdapter, McpConformanceAdapterError,
        McpConformanceAdapterResult, McpConformanceObservation, McpConformanceStep,
        McpConformanceUnavailableKind, McpRepo, Provider, ProviderMcpServerConfig,
        ProviderMcpTransport, ProviderTurnInput, SessionConfig, MCP_CONFORMANCE_ECHO_VALUE,
    },
};
use dcc_infra::{credential_store::InMemoryCredentialStore, mcp_db::SqliteMcpRepo};
use dcc_mcp_fixture::COMPUTER_USE_IMAGE_LABELS;
use dcc_providers::{claude_code, codex, cursor};
use futures::{stream::BoxStream, StreamExt};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

const STEP_TIMEOUT: Duration = Duration::from_secs(120);
const SERVER_NAME: &str = "dcc-conformance-fixture";

type ProviderStream = BoxStream<'static, dcc_core::Result<ProviderEvent>>;

#[derive(Default)]
struct TurnObservation {
    statuses: Vec<dcc_core::domain::mcp::McpRuntimeStatus>,
    text: String,
    assistant_messages: HashMap<String, String>,
    permission_resolutions: Vec<(String, String)>,
    completed_actions: Vec<String>,
    failed: bool,
}

impl TurnObservation {
    fn observe_text(&mut self, event: &ProviderEvent) {
        match event {
            ProviderEvent::TextDelta { content } => self.text.push_str(content),
            ProviderEvent::AssistantMessageDelta { id, content } => {
                self.assistant_messages
                    .entry(id.clone())
                    .or_default()
                    .push_str(content);
            }
            ProviderEvent::AssistantMessageCompleted {
                id,
                content: Some(content),
                ..
            } => {
                // Match the production consumer: the completed snapshot replaces
                // partial text, which may be incomplete or have been revised.
                self.assistant_messages.insert(id.clone(), content.clone());
            }
            _ => {}
        }
    }

    fn contains_text(&self, expected: &str) -> bool {
        self.text.contains(expected)
            || self
                .assistant_messages
                .values()
                .any(|text| text.contains(expected))
    }

    fn contains_text_ascii_case_insensitive(&self, expected: &str) -> bool {
        self.text
            .to_ascii_uppercase()
            .contains(&expected.to_ascii_uppercase())
            || self.assistant_messages.values().any(|text| {
                text.to_ascii_uppercase()
                    .contains(&expected.to_ascii_uppercase())
            })
    }
}

struct ProviderMcpConformanceAdapter<P> {
    provider: P,
    provider_id: ProviderId,
    provider_label: &'static str,
    definition_id: McpDefinitionId,
    fixture_binary: PathBuf,
    workspace: PathBuf,
    model: Option<String>,
    session_sequence: u64,
    handle: Option<SessionHandle>,
    events: Option<ProviderStream>,
    http_fixture: Option<Child>,
    http_endpoint: Option<String>,
    attached: bool,
    server_unavailable: bool,
    credential_unavailable: bool,
    pending_mutation: Option<ProviderPermissionRequest>,
    active_tool_calls: HashMap<String, String>,
    mutation_denied: bool,
    mutation_completed: bool,
    computer_use_image_case: usize,
    computer_use_hold_start_file: PathBuf,
    computer_use_cancel_sent: bool,
}

impl<P> ProviderMcpConformanceAdapter<P>
where
    P: Provider,
{
    fn new(
        provider: P,
        provider_id: &str,
        provider_label: &'static str,
        model_env: &str,
        fixture_binary: PathBuf,
        workspace: PathBuf,
    ) -> Self {
        let computer_use_hold_start_file = workspace.join("computer-use-hold-started");
        Self {
            provider,
            provider_id: ProviderId(provider_id.to_string()),
            provider_label,
            definition_id: McpDefinitionId(format!("{provider_id}-conformance-fixture")),
            fixture_binary,
            workspace,
            model: std::env::var(model_env)
                .ok()
                .filter(|value| !value.trim().is_empty()),
            session_sequence: 0,
            handle: None,
            events: None,
            http_fixture: None,
            http_endpoint: None,
            attached: false,
            server_unavailable: false,
            credential_unavailable: false,
            pending_mutation: None,
            active_tool_calls: HashMap::new(),
            mutation_denied: false,
            mutation_completed: false,
            computer_use_image_case: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default() as usize
                + std::process::id() as usize)
                % COMPUTER_USE_IMAGE_LABELS.len(),
            computer_use_hold_start_file,
            computer_use_cancel_sent: false,
        }
    }

    async fn cleanup_runtime(&mut self) -> McpConformanceAdapterResult<()> {
        self.events = None;
        if let Some(handle) = self.handle.take() {
            self.provider
                .cancel(&handle)
                .await
                .map_err(|_| McpConformanceAdapterError::Lifecycle)?;
        }
        self.pending_mutation = None;
        self.active_tool_calls.clear();
        Ok(())
    }

    async fn stop_http_fixture(&mut self) -> McpConformanceAdapterResult<()> {
        if let Some(mut child) = self.http_fixture.take() {
            match child
                .try_wait()
                .map_err(|_| McpConformanceAdapterError::Lifecycle)?
            {
                Some(_) => {}
                None => {
                    child
                        .start_kill()
                        .map_err(|_| McpConformanceAdapterError::Lifecycle)?;
                    timeout(Duration::from_secs(2), child.wait())
                        .await
                        .map_err(|_| McpConformanceAdapterError::Lifecycle)?
                        .map_err(|_| McpConformanceAdapterError::Lifecycle)?;
                }
            }
        }
        self.http_endpoint = None;
        Ok(())
    }

    async fn reset(&mut self) -> McpConformanceAdapterResult<()> {
        self.cleanup_runtime().await?;
        self.stop_http_fixture().await?;
        self.computer_use_image_case =
            (self.computer_use_image_case + 1) % COMPUTER_USE_IMAGE_LABELS.len();
        let _ = std::fs::remove_file(&self.computer_use_hold_start_file);
        self.computer_use_cancel_sent = false;
        self.attached = false;
        self.server_unavailable = false;
        self.credential_unavailable = false;
        self.mutation_denied = false;
        self.mutation_completed = false;
        Ok(())
    }

    async fn start_http_fixture(&mut self) -> McpConformanceAdapterResult<()> {
        self.stop_http_fixture().await?;
        let mut command = Command::new(&self.fixture_binary);
        command
            .args(["http", "--bind", "127.0.0.1:0", "--image-case"])
            .arg(self.computer_use_image_case.to_string())
            .arg("--hold-start-file")
            .arg(&self.computer_use_hold_start_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|_| McpConformanceAdapterError::Attachment)?;
        let stderr = child
            .stderr
            .take()
            .ok_or(McpConformanceAdapterError::Attachment)?;
        let mut lines = BufReader::new(stderr).lines();
        let line = timeout(Duration::from_secs(5), lines.next_line())
            .await
            .map_err(|_| McpConformanceAdapterError::Attachment)?
            .map_err(|_| McpConformanceAdapterError::Attachment)?
            .ok_or(McpConformanceAdapterError::Attachment)?;
        let endpoint = line
            .strip_prefix("DCC_MCP_FIXTURE_URL=")
            .filter(|value| value.starts_with("http://127.0.0.1:"))
            .ok_or(McpConformanceAdapterError::Attachment)?
            .to_string();
        self.http_fixture = Some(child);
        self.http_endpoint = Some(endpoint);
        Ok(())
    }

    async fn attach_fixture(
        &mut self,
        transport: &McpTransportKind,
    ) -> McpConformanceAdapterResult<()> {
        self.cleanup_runtime().await?;
        self.server_unavailable = false;
        if transport == &McpTransportKind::Http {
            self.start_http_fixture().await?;
        }
        self.attached = true;
        Ok(())
    }

    fn server_config(
        &self,
        transport: &McpTransportKind,
    ) -> McpConformanceAdapterResult<ProviderMcpServerConfig> {
        if !self.attached {
            return Err(McpConformanceAdapterError::Attachment);
        }
        let projected_transport = match transport {
            McpTransportKind::Stdio => ProviderMcpTransport::Stdio {
                executable: if self.server_unavailable {
                    self.workspace
                        .join("unavailable-dcc-mcp-fixture")
                        .to_string_lossy()
                        .to_string()
                } else {
                    self.fixture_binary.to_string_lossy().to_string()
                },
                args: vec![
                    "stdio".to_string(),
                    "--image-case".to_string(),
                    self.computer_use_image_case.to_string(),
                    "--hold-start-file".to_string(),
                    self.computer_use_hold_start_file
                        .to_string_lossy()
                        .to_string(),
                ],
                cwd: None,
                environment: Vec::new(),
            },
            McpTransportKind::Http => ProviderMcpTransport::Http {
                url: self
                    .http_endpoint
                    .clone()
                    .unwrap_or_else(|| "http://127.0.0.1:9/mcp".to_string()),
                headers: Vec::new(),
            },
        };
        Ok(ProviderMcpServerConfig {
            definition_id: self.definition_id.clone(),
            server_name: SERVER_NAME.to_string(),
            transport: projected_transport,
            oauth_state: None,
            tool_policies: Vec::new(),
        })
    }

    async fn prepare_session(
        &mut self,
        servers: Vec<ProviderMcpServerConfig>,
    ) -> McpConformanceAdapterResult<()> {
        self.cleanup_runtime().await?;
        self.session_sequence += 1;
        let session_id = SessionId(format!(
            "{}-mcp-conformance-{}",
            self.provider_id.0, self.session_sequence
        ));
        let handle = self
            .provider
            .prepare_session(SessionConfig {
                workspace_id: WorkspaceId(format!("{}-mcp-conformance", self.provider_id.0)),
                session_id,
                model: self.model.clone(),
                working_directory: Some(self.workspace.to_string_lossy().to_string()),
                additional_working_directories: Vec::new(),
                provider_runtime: None,
                mcp_servers: servers,
            })
            .await
            .map_err(|_| McpConformanceAdapterError::ProviderSession)?;
        let events = self.provider.stream_events(&handle);
        self.handle = Some(handle);
        self.events = Some(events);
        Ok(())
    }

    async fn send_turn(&mut self, prompt: &str) -> McpConformanceAdapterResult<()> {
        let handle = self
            .handle
            .as_ref()
            .ok_or(McpConformanceAdapterError::ProviderSession)?;
        self.provider
            .send_input(
                handle,
                Input::Turn(ProviderTurnInput {
                    prompt: prompt.to_string(),
                    tool_instructions: Some(
                        "Use only the explicitly requested DCC fixture tool. Do not use shell, file, web, or any other tool."
                            .to_string(),
                    ),
                    plan_mode: Some(false),
                    effort: Some("low".to_string()),
                    fast_mode: Some(true),
                    approval_policy: None,
                }),
            )
            .await
            .map_err(|_| McpConformanceAdapterError::ProviderSession)
    }

    async fn next_event(&mut self) -> McpConformanceAdapterResult<ProviderEvent> {
        let events = self
            .events
            .as_mut()
            .ok_or(McpConformanceAdapterError::ProviderSession)?;
        timeout(STEP_TIMEOUT, events.next())
            .await
            .map_err(|_| McpConformanceAdapterError::Unavailable)?
            .ok_or(McpConformanceAdapterError::ProviderSession)?
            .map_err(|_| McpConformanceAdapterError::Protocol)
    }

    async fn resolve_permission(
        &mut self,
        request_id: String,
        behavior: &str,
    ) -> McpConformanceAdapterResult<()> {
        let handle = self
            .handle
            .as_ref()
            .ok_or(McpConformanceAdapterError::ProviderSession)?;
        self.provider
            .send_input(
                handle,
                Input::PermissionResponse(ProviderPermissionResponse {
                    request_id,
                    behavior: behavior.to_string(),
                }),
            )
            .await
            .map_err(|_| McpConformanceAdapterError::PermissionBoundary)
    }

    fn observe_event(&mut self, event: &ProviderEvent, observation: &mut TurnObservation) {
        observation.observe_text(event);
        match event {
            ProviderEvent::McpRuntimeStatusSnapshot { statuses } => {
                observation.statuses = statuses.clone();
            }
            ProviderEvent::ToolCallStarted { id, action, .. } => {
                self.active_tool_calls.insert(id.clone(), action.clone());
            }
            ProviderEvent::ToolCallCompleted { id, .. } => {
                if let Some(action) = self.active_tool_calls.remove(id) {
                    observation.completed_actions.push(action);
                }
            }
            ProviderEvent::PermissionResolved { id, behavior, .. } => {
                observation
                    .permission_resolutions
                    .push((id.clone(), behavior.clone()));
            }
            ProviderEvent::Failed { .. } => observation.failed = true,
            _ => {}
        }
    }

    async fn finish_turn(
        &mut self,
        allow_echo: bool,
    ) -> McpConformanceAdapterResult<TurnObservation> {
        let mut observation = TurnObservation::default();
        loop {
            let event = self.next_event().await?;
            self.observe_event(&event, &mut observation);
            match event {
                ProviderEvent::PermissionRequested { request, .. } => {
                    let allow = allow_echo
                        && (tool_matches(&request.tool_name, "fixture_echo")
                            || tool_matches(&request.tool_name, "fixture_image"));
                    self.resolve_permission(
                        request.request_id,
                        if allow { "allow" } else { "deny" },
                    )
                    .await?;
                }
                ProviderEvent::Completed { .. } => return Ok(observation),
                ProviderEvent::Failed { .. } => return Ok(observation),
                _ => {}
            }
        }
    }

    async fn run_turn(
        &mut self,
        prompt: &str,
        allow_echo: bool,
    ) -> McpConformanceAdapterResult<TurnObservation> {
        self.send_turn(prompt).await?;
        self.finish_turn(allow_echo).await
    }

    async fn begin_mutation(&mut self) -> McpConformanceAdapterResult<ProviderPermissionRequest> {
        self.send_turn(
            "Call the DCC-projected MCP tool fixture.mutate exactly once with label \
             \"must-not-run\" and changeTools true. Request permission and wait. Do not use any \
             other tool.",
        )
        .await?;
        loop {
            let event = self.next_event().await?;
            let mut observation = TurnObservation::default();
            self.observe_event(&event, &mut observation);
            match event {
                ProviderEvent::PermissionRequested { request, .. }
                    if tool_matches(&request.tool_name, "fixture_mutate") =>
                {
                    self.pending_mutation = Some(request.clone());
                    return Ok(request);
                }
                ProviderEvent::PermissionRequested { request, .. } => {
                    self.resolve_permission(request.request_id, "deny").await?;
                }
                ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. } => {
                    return Err(McpConformanceAdapterError::PermissionBoundary);
                }
                _ => {}
            }
        }
    }

    async fn deny_mutation(&mut self) -> McpConformanceAdapterResult<()> {
        let request = self
            .pending_mutation
            .take()
            .ok_or(McpConformanceAdapterError::PermissionBoundary)?;
        self.resolve_permission(request.request_id.clone(), "deny")
            .await?;
        let observation = self.finish_turn(false).await?;
        self.mutation_denied = observation
            .permission_resolutions
            .iter()
            .any(|(id, behavior)| id == &request.request_id && behavior == "deny");
        self.mutation_completed = observation
            .completed_actions
            .iter()
            .any(|action| tool_matches(action, "fixture_mutate"));
        if !self.mutation_denied || self.mutation_completed {
            return Err(McpConformanceAdapterError::PermissionBoundary);
        }
        Ok(())
    }

    async fn confirm_disabled(&mut self) -> McpConformanceAdapterResult<bool> {
        self.prepare_session(Vec::new()).await?;
        let observation = self
            .run_turn("Do not call tools. Reply exactly DCC_DISABLED.", false)
            .await?;
        Ok(!observation.failed && observation.statuses.is_empty())
    }

    async fn confirm_server_failure(
        &mut self,
        transport: &McpTransportKind,
    ) -> McpConformanceAdapterResult<bool> {
        let server = self.server_config(transport)?;
        self.prepare_session(vec![server]).await?;
        let observation = self
            .run_turn("Do not call tools. Reply exactly DCC_SERVER_CHECK.", false)
            .await?;
        Ok(observation.statuses.iter().any(|status| {
            status.definition_id == self.definition_id && status.state == McpRuntimeState::Failed
        }))
    }

    async fn missing_credential_fails_closed(
        &mut self,
        transport: &McpTransportKind,
    ) -> McpConformanceAdapterResult<bool> {
        if !self.credential_unavailable {
            return Ok(false);
        }
        let db_path = self.workspace.join(format!(
            "missing-credential-{}.sqlite",
            transport_label(transport)
        ));
        let repo =
            SqliteMcpRepo::open(&db_path).map_err(|_| McpConformanceAdapterError::Lifecycle)?;
        let secret_reference = McpSecretReferenceId(format!(
            "credential:{}-conformance-canary",
            self.provider_id.0
        ));
        let (definition_transport, target) = match transport {
            McpTransportKind::Stdio => (
                McpTransport::Stdio {
                    executable: self.fixture_binary.to_string_lossy().to_string(),
                    args: vec!["stdio".to_string()],
                    cwd: None,
                },
                McpSecretTarget::EnvironmentVariable {
                    name: "DCC_MCP_FIXTURE_TOKEN".to_string(),
                },
            ),
            McpTransportKind::Http => (
                McpTransport::Http {
                    url: "http://127.0.0.1:9/mcp".to_string(),
                },
                McpSecretTarget::HttpHeader {
                    name: "X-DCC-Fixture-Token".to_string(),
                },
            ),
        };
        let mut definition = McpDefinition {
            id: self.definition_id.clone(),
            display_name: format!("{} conformance fixture", self.provider_label),
            transport: definition_transport,
            secret_refs: vec![McpSecretBinding {
                target,
                secret_ref: secret_reference,
            }],
            enabled: true,
            ownership: McpDefinitionOwnership::DccManaged,
            trust: McpTrust {
                current_fingerprint: McpTrustFingerprint("0".repeat(64)),
                decision: McpTrustDecision::Untrusted,
            },
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };
        definition.synchronize_trust_fingerprint();
        definition.trust.decision = McpTrustDecision::Trusted {
            fingerprint: definition.trust.current_fingerprint.clone(),
            trusted_at: "2026-07-28T00:00:00Z".to_string(),
        };
        repo.save_mcp_definition(&definition)
            .await
            .map_err(|_| McpConformanceAdapterError::Lifecycle)?;
        repo.save_mcp_binding(&McpBinding {
            id: McpBindingId(format!("{}-conformance-global", self.provider_id.0)),
            definition_id: definition.id,
            scope: McpBindingScope::Global,
            enabled: true,
            provider_exclusions: Vec::new(),
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        })
        .await
        .map_err(|_| McpConformanceAdapterError::Lifecycle)?;

        let result = resolve_session_mcp_servers(
            &repo,
            &InMemoryCredentialStore::default(),
            &ResolveSessionMcpInput {
                provider_id: self.provider_id.clone(),
                project_id: ProjectId(format!("{}-conformance", self.provider_id.0)),
                session_id: SessionId(format!("{}-conformance", self.provider_id.0)),
            },
        )
        .await;
        drop(repo);
        let _ = std::fs::remove_file(db_path);

        Ok(matches!(
            result,
            Err(error) if error.to_string() == "provider error: MCP credential resolution failed"
        ))
    }

    async fn wait_for_computer_use_tools(&mut self) -> ComputerUseConformanceAdapterResult<()> {
        // Codex publishes an asynchronous MCP connection snapshot after
        // thread/start; sending the image prompt before it arrives can make
        // the model report that the fixture tool is unavailable. Claude's
        // SDK exposes readiness through its first turn rather than this
        // status event, so its native tool path must be exercised directly.
        if self.provider_id.0 != "codex" {
            return Ok(());
        }
        timeout(Duration::from_secs(30), async {
            loop {
                let event = self
                    .next_event()
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::ProviderSession)?;
                if let ProviderEvent::McpRuntimeStatusSnapshot { statuses } = event {
                    let Some(status) = statuses
                        .into_iter()
                        .find(|status| status.definition_id == self.definition_id)
                    else {
                        continue;
                    };
                    if status.state == McpRuntimeState::Failed {
                        return Err(ComputerUseConformanceAdapterError::Attachment);
                    }
                    let has_image = status
                        .tools
                        .iter()
                        .any(|tool| tool_matches(&tool.name, "fixture_image"));
                    let has_hold = status
                        .tools
                        .iter()
                        .any(|tool| tool_matches(&tool.name, "fixture_hold"));
                    if status.state == McpRuntimeState::Connected && has_image && has_hold {
                        return Ok(());
                    }
                }
            }
        })
        .await
        .map_err(|_| ComputerUseConformanceAdapterError::Unavailable)?
    }

    async fn begin_computer_use_hold(&mut self) -> ComputerUseConformanceAdapterResult<()> {
        self.send_turn(
            "Call the DCC MCP tool fixture.hold exactly once. Do not call any other tool.",
        )
        .await
        .map_err(|_| ComputerUseConformanceAdapterError::ProviderSession)?;

        let mut hold_call_started = false;
        loop {
            if hold_call_started && self.computer_use_hold_start_file.exists() {
                return Ok(());
            }
            let event = tokio::select! {
                result = self.next_event() => Some(result.map_err(|_| ComputerUseConformanceAdapterError::ProviderSession)?),
                _ = tokio::time::sleep(Duration::from_millis(50)) => None,
            };
            let Some(event) = event else {
                continue;
            };
            match event {
                ProviderEvent::PermissionRequested { request, .. } => {
                    let allow = tool_matches(&request.tool_name, "fixture_hold");
                    self.resolve_permission(
                        request.request_id,
                        if allow { "allow" } else { "deny" },
                    )
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::ProviderSession)?;
                }
                ProviderEvent::ToolCallStarted { action, .. }
                    if tool_matches(&action, "fixture_hold") =>
                {
                    hold_call_started = true;
                }
                ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. } => {
                    return Err(ComputerUseConformanceAdapterError::ToolNotRequested);
                }
                _ => {}
            }
        }
    }
}

#[async_trait]
impl<P> McpConformanceAdapter for ProviderMcpConformanceAdapter<P>
where
    P: Provider,
{
    fn provider_id(&self) -> ProviderId {
        self.provider_id.clone()
    }

    fn provider_version(&self) -> String {
        self.provider
            .dcc_mcp_projection_version()
            .unwrap_or_default()
            .to_string()
    }

    async fn execute(
        &mut self,
        transport: McpTransportKind,
        step: McpConformanceStep,
    ) -> McpConformanceAdapterResult<McpConformanceObservation> {
        match step {
            McpConformanceStep::Reset => {
                self.reset().await?;
                Ok(McpConformanceObservation::Acknowledged)
            }
            McpConformanceStep::AttachFixture
            | McpConformanceStep::AttachFixtureForServerFailure
            | McpConformanceStep::AttachFixtureForCredentialFailure => {
                self.attach_fixture(&transport).await?;
                Ok(McpConformanceObservation::Acknowledged)
            }
            McpConformanceStep::CreateSession => {
                let server = self.server_config(&transport)?;
                self.prepare_session(vec![server]).await?;
                Ok(McpConformanceObservation::SessionCreated)
            }
            McpConformanceStep::ListTools => {
                let observation = self
                    .run_turn("Do not call tools. Reply exactly DCC_TOOLS_READY.", false)
                    .await?;
                if observation.failed {
                    return Err(McpConformanceAdapterError::ProviderSession);
                }
                let status = observation
                    .statuses
                    .iter()
                    .find(|status| status.definition_id == self.definition_id)
                    .filter(|status| status.state == McpRuntimeState::Connected)
                    .ok_or(McpConformanceAdapterError::Attachment)?;
                Ok(McpConformanceObservation::ToolsVisible(
                    status.tools.iter().map(|tool| tool.name.clone()).collect(),
                ))
            }
            McpConformanceStep::CallReadOnlyTool => {
                let observation = self
                    .run_turn(
                        "Call the DCC-projected MCP tool fixture.echo exactly once with message \
                         \"dcc-conformance-echo-v1\". Then reply with only the returned text.",
                        true,
                    )
                    .await?;
                let echo_completed = observation
                    .completed_actions
                    .iter()
                    .any(|action| tool_matches(action, "fixture_echo"));
                let allow_resolved = observation
                    .permission_resolutions
                    .iter()
                    .any(|(_, behavior)| behavior == "allow");
                let expected_text = observation.contains_text(MCP_CONFORMANCE_ECHO_VALUE);
                if observation.failed || !echo_completed || !allow_resolved || !expected_text {
                    eprintln!(
                        "read-only conformance observation: provider_failed={}, \
                         echo_completed={echo_completed}, allow_resolved={allow_resolved}, \
                         expected_text={expected_text}",
                        observation.failed,
                    );
                    return Err(McpConformanceAdapterError::Protocol);
                }
                Ok(McpConformanceObservation::ReadOnlyResult(
                    MCP_CONFORMANCE_ECHO_VALUE.to_string(),
                ))
            }
            McpConformanceStep::RequestMutatingTool => {
                let request = self.begin_mutation().await?;
                Ok(McpConformanceObservation::ApprovalRequired {
                    tool_name: if tool_matches(&request.tool_name, "fixture_mutate") {
                        "fixture.mutate".to_string()
                    } else {
                        return Err(McpConformanceAdapterError::PermissionBoundary);
                    },
                })
            }
            McpConformanceStep::DenyMutatingTool => {
                self.deny_mutation().await?;
                Ok(McpConformanceObservation::MutationDenied)
            }
            McpConformanceStep::ConfirmMutationNotExecuted => {
                if self.mutation_denied && !self.mutation_completed {
                    Ok(McpConformanceObservation::MutationNotExecuted)
                } else {
                    Err(McpConformanceAdapterError::PermissionBoundary)
                }
            }
            McpConformanceStep::DisableFixture => {
                self.cleanup_runtime().await?;
                self.attached = false;
                Ok(McpConformanceObservation::Acknowledged)
            }
            McpConformanceStep::RefreshAfterDisable => {
                if self.confirm_disabled().await? {
                    Ok(McpConformanceObservation::FixtureUnavailable)
                } else {
                    Err(McpConformanceAdapterError::Lifecycle)
                }
            }
            McpConformanceStep::RemoveFixture => {
                self.cleanup_runtime().await?;
                self.stop_http_fixture().await?;
                self.attached = false;
                Ok(McpConformanceObservation::Acknowledged)
            }
            McpConformanceStep::ConfirmCleanup | McpConformanceStep::FinalCleanup => {
                self.cleanup_runtime().await?;
                self.stop_http_fixture().await?;
                self.attached = false;
                Ok(McpConformanceObservation::CleanupConfirmed)
            }
            McpConformanceStep::MakeServerUnavailable => {
                self.server_unavailable = true;
                if transport == McpTransportKind::Http {
                    self.stop_http_fixture().await?;
                }
                Ok(McpConformanceObservation::Acknowledged)
            }
            McpConformanceStep::ConfirmServerFailure => {
                if self.confirm_server_failure(&transport).await? {
                    Ok(McpConformanceObservation::FailedClosed(
                        McpConformanceUnavailableKind::Server,
                    ))
                } else {
                    Err(McpConformanceAdapterError::Unavailable)
                }
            }
            McpConformanceStep::ResetAfterServerFailure => {
                self.reset().await?;
                Ok(McpConformanceObservation::Acknowledged)
            }
            McpConformanceStep::MakeCredentialUnavailable => {
                self.credential_unavailable = true;
                Ok(McpConformanceObservation::Acknowledged)
            }
            McpConformanceStep::ConfirmCredentialFailure => {
                if self.missing_credential_fails_closed(&transport).await? {
                    Ok(McpConformanceObservation::FailedClosed(
                        McpConformanceUnavailableKind::Credential,
                    ))
                } else {
                    Err(McpConformanceAdapterError::Unavailable)
                }
            }
        }
    }
}

#[async_trait]
impl<P> ComputerUseConformanceAdapter for ProviderMcpConformanceAdapter<P>
where
    P: Provider,
{
    fn provider_id(&self) -> ProviderId {
        self.provider_id.clone()
    }

    fn provider_version(&self) -> String {
        self.provider
            .dcc_mcp_projection_version()
            .unwrap_or_default()
            .to_string()
    }

    async fn execute(
        &mut self,
        transport: McpTransportKind,
        step: ComputerUseConformanceStep,
    ) -> ComputerUseConformanceAdapterResult<ComputerUseConformanceObservation> {
        match step {
            ComputerUseConformanceStep::Reset => {
                self.reset()
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::Lifecycle)?;
                Ok(ComputerUseConformanceObservation::Acknowledged)
            }
            ComputerUseConformanceStep::AttachFixture => {
                self.attach_fixture(&transport)
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::Attachment)?;
                Ok(ComputerUseConformanceObservation::Acknowledged)
            }
            ComputerUseConformanceStep::CreateSession => {
                let server = self
                    .server_config(&transport)
                    .map_err(|_| ComputerUseConformanceAdapterError::Attachment)?;
                self.prepare_session(vec![server])
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::ProviderSession)?;
                self.wait_for_computer_use_tools().await?;
                Ok(ComputerUseConformanceObservation::SessionCreated)
            }
            ComputerUseConformanceStep::InspectImage => {
                let expected = COMPUTER_USE_IMAGE_LABELS[self.computer_use_image_case];
                let prompt = "Call the DCC MCP tool fixture.image exactly once. Inspect the attached image and reply only with the exact eight-digit code printed inside it.";
                let observation = self
                    .run_turn(prompt, true)
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::ProviderSession)?;
                let image_completed = observation
                    .completed_actions
                    .iter()
                    .any(|action| tool_matches(action, "fixture_image"));
                let image_allowed = observation
                    .permission_resolutions
                    .iter()
                    .any(|(_, behavior)| behavior == "allow");
                let expected_label_found =
                    observation.contains_text_ascii_case_insensitive(expected);
                if observation.failed || !image_completed || !image_allowed || !expected_label_found
                {
                    eprintln!(
                        "Computer Use image gate: provider_failed={}, image_completed={}, image_allowed={}, expected_label_found={expected_label_found}",
                        observation.failed, image_completed, image_allowed,
                    );
                    return Err(ComputerUseConformanceAdapterError::ImageResult);
                }
                Ok(ComputerUseConformanceObservation::ImageUnderstood)
            }
            ComputerUseConformanceStep::StartInterruptibleOperation => {
                self.begin_computer_use_hold().await?;
                Ok(ComputerUseConformanceObservation::Acknowledged)
            }
            ComputerUseConformanceStep::InterruptTurn => {
                let handle = self
                    .handle
                    .as_ref()
                    .ok_or(ComputerUseConformanceAdapterError::ProviderSession)?
                    .clone();
                self.provider
                    .cancel(&handle)
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::Interruption)?;
                self.computer_use_cancel_sent = true;
                Ok(ComputerUseConformanceObservation::InterruptRequested)
            }
            ComputerUseConformanceStep::ConfirmTurnStopped => {
                if !self.computer_use_cancel_sent {
                    return Err(ComputerUseConformanceAdapterError::Interruption);
                }
                let terminal_event = timeout(Duration::from_secs(15), async {
                    let events = self
                        .events
                        .as_mut()
                        .ok_or(ComputerUseConformanceAdapterError::ProviderSession)?;
                    loop {
                        match events.next().await {
                            Some(Ok(
                                ProviderEvent::Completed { .. } | ProviderEvent::Failed { .. },
                            ))
                            | Some(Err(_))
                            | None => return Ok::<bool, ComputerUseConformanceAdapterError>(true),
                            Some(Ok(_)) => {}
                        }
                    }
                })
                .await
                .is_ok_and(|result| result.is_ok_and(|terminal| terminal));
                let marker = std::fs::read(&self.computer_use_hold_start_file)
                    .map_err(|_| ComputerUseConformanceAdapterError::Interruption)?;
                let handle = self
                    .handle
                    .as_ref()
                    .ok_or(ComputerUseConformanceAdapterError::ProviderSession)?;
                let stopped_session_rejected_turn = if terminal_event {
                    false
                } else {
                    self.provider
                        .send_input(
                            handle,
                            Input::Turn(ProviderTurnInput {
                                prompt: "Reply with DCC_STOP_CHECK".to_string(),
                                tool_instructions: None,
                                plan_mode: Some(false),
                                effort: Some("low".to_string()),
                                fast_mode: Some(true),
                                approval_policy: None,
                            }),
                        )
                        .await
                        .is_err()
                };
                if (!terminal_event && !stopped_session_rejected_turn)
                    || marker.as_slice() == b"completed"
                {
                    eprintln!(
                        "Computer Use interruption gate: terminal_event={terminal_event}, stopped_session_rejected_turn={stopped_session_rejected_turn}, fixture_marker={}",
                        String::from_utf8_lossy(&marker),
                    );
                    return Err(ComputerUseConformanceAdapterError::Interruption);
                }
                Ok(ComputerUseConformanceObservation::TurnStopped)
            }
            ComputerUseConformanceStep::FinalCleanup => {
                if self.computer_use_cancel_sent {
                    if let Some(handle) = self.handle.take() {
                        // Some adapters already tear down the runtime on the
                        // first cancel; others keep the session available.
                        // A best-effort second cancel cleans up the latter.
                        let _ = self.provider.cancel(&handle).await;
                    }
                    self.events = None;
                } else {
                    self.cleanup_runtime()
                        .await
                        .map_err(|_| ComputerUseConformanceAdapterError::Lifecycle)?;
                }
                self.stop_http_fixture()
                    .await
                    .map_err(|_| ComputerUseConformanceAdapterError::Lifecycle)?;
                self.attached = false;
                self.computer_use_cancel_sent = false;
                Ok(ComputerUseConformanceObservation::CleanupConfirmed)
            }
        }
    }
}

fn tool_matches(provider_name: &str, normalized_suffix: &str) -> bool {
    provider_name == normalized_suffix.replace('_', ".")
        || provider_name.ends_with(normalized_suffix)
        || provider_name
            .replace(['.', '-'], "_")
            .ends_with(normalized_suffix)
}

fn transport_label(transport: &McpTransportKind) -> &'static str {
    match transport {
        McpTransportKind::Stdio => "stdio",
        McpTransportKind::Http => "http",
    }
}

fn fixture_binary() -> PathBuf {
    std::fs::canonicalize(env!("CARGO_BIN_EXE_dcc-mcp-fixture")).expect("canonical fixture binary")
}

fn test_workspace(provider_id: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "dcc-{provider_id}-mcp-conformance-{}-{nonce}",
        std::process::id()
    ))
}

fn claude_adapter(workspace: PathBuf) -> ProviderMcpConformanceAdapter<impl Provider> {
    ProviderMcpConformanceAdapter::new(
        claude_code::adapter(),
        "claude_code",
        "Claude",
        "DCC_CLAUDE_CONFORMANCE_MODEL",
        fixture_binary(),
        workspace,
    )
}

fn codex_adapter(workspace: PathBuf) -> ProviderMcpConformanceAdapter<impl Provider> {
    ProviderMcpConformanceAdapter::new(
        codex::adapter(),
        "codex",
        "Codex",
        "DCC_CODEX_CONFORMANCE_MODEL",
        fixture_binary(),
        workspace,
    )
}

fn cursor_adapter(workspace: PathBuf) -> ProviderMcpConformanceAdapter<impl Provider> {
    ProviderMcpConformanceAdapter::new(
        cursor::adapter(),
        "cursor",
        "Cursor",
        "DCC_CURSOR_CONFORMANCE_MODEL",
        fixture_binary(),
        workspace,
    )
}

fn require_explicit_opt_in(variable: &str, instruction: &str) {
    assert_eq!(
        std::env::var(variable).ok().as_deref(),
        Some("1"),
        "{instruction}"
    );
}

async fn run_authenticated_gate<P>(
    mut adapter: ProviderMcpConformanceAdapter<P>,
    workspace: PathBuf,
) where
    P: Provider,
{
    let provider_id = adapter.provider_id.clone();
    match adapter
        .provider
        .healthcheck()
        .await
        .expect("provider preflight healthcheck")
    {
        HealthStatus::Healthy => {}
        HealthStatus::Degraded { reason } | HealthStatus::Unhealthy { reason } => {
            panic!("provider preflight failed: {reason}");
        }
    }
    adapter
        .provider
        .refresh_runtime_metadata()
        .await
        .expect("provider runtime metadata refresh");
    let provider_version = adapter
        .provider
        .dcc_mcp_projection_version()
        .expect("provider must expose an audited MCP projection version")
        .to_string();
    let evidence = run_provider_mcp_conformance(&mut adapter)
        .await
        .expect("provider bridge conformance");
    evidence
        .validate_for_provider(&provider_id, &provider_version)
        .expect("version-bound provider evidence");

    adapter.reset().await.expect("final provider cleanup");
    let _ = std::fs::remove_dir_all(workspace);
}

async fn run_authenticated_computer_use_gate<P>(
    mut adapter: ProviderMcpConformanceAdapter<P>,
    workspace: PathBuf,
) where
    P: Provider,
{
    let provider_id = adapter.provider_id.clone();
    match adapter
        .provider
        .healthcheck()
        .await
        .expect("provider preflight healthcheck")
    {
        HealthStatus::Healthy => {}
        HealthStatus::Degraded { reason } | HealthStatus::Unhealthy { reason } => {
            panic!("provider preflight failed: {reason}");
        }
    }
    adapter
        .provider
        .refresh_runtime_metadata()
        .await
        .expect("provider runtime metadata refresh");
    let provider_version = adapter
        .provider
        .dcc_mcp_projection_version()
        .expect("provider must expose an audited runtime version")
        .to_string();
    let evidence = run_provider_computer_use_conformance(&mut adapter)
        .await
        .expect("provider Computer Use conformance");
    evidence
        .validate_for_provider(&provider_id, &provider_version)
        .expect("version-bound Computer Use evidence");
    println!(
        "COMPUTER_USE_CONFORMANCE_EVIDENCE={}",
        serde_json::to_string(&evidence).expect("serialize non-sensitive evidence")
    );

    adapter.reset().await.expect("final provider cleanup");
    let _ = std::fs::remove_dir_all(workspace);
}

#[tokio::test]
#[ignore = "requires explicit opt-in and an authenticated Claude Code account"]
async fn authenticated_claude_bridge_passes_the_shared_harness() {
    require_explicit_opt_in(
        "DCC_RUN_CLAUDE_MCP_CONFORMANCE",
        "set DCC_RUN_CLAUDE_MCP_CONFORMANCE=1 after authenticating Claude Code",
    );
    let workspace = test_workspace("claude");
    std::fs::create_dir_all(&workspace).expect("create isolated conformance workspace");
    run_authenticated_gate(claude_adapter(workspace.clone()), workspace).await;
}

#[tokio::test]
#[ignore = "requires explicit opt-in and an authenticated Codex account; records the negotiated codex-cli runtime"]
async fn authenticated_codex_bridge_passes_the_shared_harness() {
    require_explicit_opt_in(
        "DCC_RUN_CODEX_MCP_CONFORMANCE",
        "set DCC_RUN_CODEX_MCP_CONFORMANCE=1 after authenticating codex-cli",
    );
    let workspace = test_workspace("codex");
    std::fs::create_dir_all(&workspace).expect("create isolated conformance workspace");
    run_authenticated_gate(codex_adapter(workspace.clone()), workspace).await;
}

#[tokio::test]
#[ignore = "requires explicit opt-in, cursor-agent 2026.07.23-e383d2b, and an authenticated Cursor account"]
async fn authenticated_cursor_bridge_passes_the_shared_harness() {
    require_explicit_opt_in(
        "DCC_RUN_CURSOR_MCP_CONFORMANCE",
        "set DCC_RUN_CURSOR_MCP_CONFORMANCE=1 after authenticating cursor-agent 2026.07.23-e383d2b",
    );
    let workspace = test_workspace("cursor");
    std::fs::create_dir_all(&workspace).expect("create isolated conformance workspace");
    run_authenticated_gate(cursor_adapter(workspace.clone()), workspace).await;
}

#[tokio::test]
#[ignore = "requires explicit opt-in and an authenticated Claude Code account"]
async fn authenticated_claude_passes_computer_use_conformance() {
    require_explicit_opt_in(
        "DCC_RUN_CLAUDE_COMPUTER_USE_CONFORMANCE",
        "set DCC_RUN_CLAUDE_COMPUTER_USE_CONFORMANCE=1 after authenticating Claude Code",
    );
    let workspace = test_workspace("claude-computer-use");
    std::fs::create_dir_all(&workspace).expect("create isolated conformance workspace");
    run_authenticated_computer_use_gate(claude_adapter(workspace.clone()), workspace).await;
}

#[tokio::test]
#[ignore = "requires explicit opt-in and an authenticated Codex account"]
async fn authenticated_codex_passes_computer_use_conformance() {
    require_explicit_opt_in(
        "DCC_RUN_CODEX_COMPUTER_USE_CONFORMANCE",
        "set DCC_RUN_CODEX_COMPUTER_USE_CONFORMANCE=1 after authenticating codex-cli",
    );
    let workspace = test_workspace("codex-computer-use");
    std::fs::create_dir_all(&workspace).expect("create isolated conformance workspace");
    run_authenticated_computer_use_gate(codex_adapter(workspace.clone()), workspace).await;
}

#[tokio::test]
#[ignore = "requires explicit opt-in, audited cursor-agent 2026.07.23-e383d2b, and an authenticated Cursor account"]
async fn authenticated_cursor_passes_computer_use_conformance() {
    require_explicit_opt_in(
        "DCC_RUN_CURSOR_COMPUTER_USE_CONFORMANCE",
        "set DCC_RUN_CURSOR_COMPUTER_USE_CONFORMANCE=1 after authenticating Cursor",
    );
    let workspace = test_workspace("cursor-computer-use");
    std::fs::create_dir_all(&workspace).expect("create isolated conformance workspace");
    run_authenticated_computer_use_gate(cursor_adapter(workspace.clone()), workspace).await;
}

#[test]
fn provider_tool_names_are_normalized_without_provider_heuristics() {
    assert!(tool_matches(
        "mcp__dcc-session__fixture_echo",
        "fixture_echo"
    ));
    assert!(tool_matches("fixture.echo", "fixture_echo"));
    assert!(!tool_matches(
        "mcp__dcc-session__fixture_mutate",
        "fixture_echo"
    ));
}

#[test]
fn conformance_observes_current_assistant_messages_and_reconciles_partial_text() {
    let mut observation = TurnObservation::default();
    observation.observe_text(&ProviderEvent::AssistantMessageDelta {
        id: "message".to_string(),
        content: "dcc-conformance-echo-".to_string(),
    });
    assert!(!observation.contains_text(MCP_CONFORMANCE_ECHO_VALUE));
    observation.observe_text(&ProviderEvent::AssistantMessageCompleted {
        id: "message".to_string(),
        phase: dcc_core::domain::session::AssistantMessagePhase::Unknown,
        content: Some(MCP_CONFORMANCE_ECHO_VALUE.to_string()),
        model: None,
        at: "fixture".to_string(),
    });
    assert!(observation.contains_text(MCP_CONFORMANCE_ECHO_VALUE));
    observation.observe_text(&ProviderEvent::AssistantMessageCompleted {
        id: "message".to_string(),
        phase: dcc_core::domain::session::AssistantMessagePhase::Unknown,
        content: Some("revised text without the expected value".to_string()),
        model: None,
        at: "fixture".to_string(),
    });
    assert!(!observation.contains_text(MCP_CONFORMANCE_ECHO_VALUE));
    observation.observe_text(&ProviderEvent::TextDelta {
        content: MCP_CONFORMANCE_ECHO_VALUE.to_string(),
    });
    assert!(observation.contains_text(MCP_CONFORMANCE_ECHO_VALUE));
}

#[tokio::test]
async fn missing_credentials_fail_before_each_provider_runtime_for_both_transports() {
    let claude_workspace = test_workspace("claude");
    std::fs::create_dir_all(&claude_workspace).expect("create isolated credential workspace");
    let mut claude = claude_adapter(claude_workspace.clone());
    assert_missing_credentials_fail_closed("claude_code", &mut claude).await;
    let _ = std::fs::remove_dir_all(claude_workspace);

    let codex_workspace = test_workspace("codex");
    std::fs::create_dir_all(&codex_workspace).expect("create isolated credential workspace");
    let mut codex = codex_adapter(codex_workspace.clone());
    assert_missing_credentials_fail_closed("codex", &mut codex).await;
    let _ = std::fs::remove_dir_all(codex_workspace);

    let cursor_workspace = test_workspace("cursor");
    std::fs::create_dir_all(&cursor_workspace).expect("create isolated credential workspace");
    let mut cursor = cursor_adapter(cursor_workspace.clone());
    assert_missing_credentials_fail_closed("cursor", &mut cursor).await;
    let _ = std::fs::remove_dir_all(cursor_workspace);
}

async fn assert_missing_credentials_fail_closed<P>(
    provider_id: &str,
    adapter: &mut ProviderMcpConformanceAdapter<P>,
) where
    P: Provider,
{
    adapter.attached = true;
    adapter.credential_unavailable = true;
    for transport in [McpTransportKind::Stdio, McpTransportKind::Http] {
        assert!(
            adapter
                .missing_credential_fails_closed(&transport)
                .await
                .expect("categorical missing-credential result"),
            "{provider_id} {transport:?} did not fail before provider attachment"
        );
    }
}

#[test]
fn test_workspace_is_bounded_to_the_system_temporary_directory() {
    assert!(test_workspace("claude").starts_with(Path::new(&std::env::temp_dir())));
    assert!(test_workspace("codex").starts_with(Path::new(&std::env::temp_dir())));
}
