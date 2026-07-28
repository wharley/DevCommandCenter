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
        resolve_session_mcp_servers, run_provider_mcp_conformance, ResolveSessionMcpInput,
    },
    domain::{
        mcp::{
            McpBinding, McpBindingId, McpBindingScope, McpDefinition, McpDefinitionId,
            McpDefinitionOwnership, McpRuntimeState, McpSecretBinding, McpSecretReferenceId,
            McpSecretTarget, McpTransport, McpTransportKind, McpTrust, McpTrustDecision,
            McpTrustFingerprint,
        },
        project::ProjectId,
        provider::{ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
        workspace::WorkspaceId,
    },
    ports::{
        Input, McpConformanceAdapter, McpConformanceAdapterError, McpConformanceAdapterResult,
        McpConformanceObservation, McpConformanceStep, McpConformanceUnavailableKind, McpRepo,
        Provider, ProviderMcpServerConfig, ProviderMcpTransport, ProviderTurnInput, SessionConfig,
        MCP_CONFORMANCE_ECHO_VALUE,
    },
};
use dcc_infra::{credential_store::InMemoryCredentialStore, mcp_db::SqliteMcpRepo};
use dcc_providers::{claude_code, claude_sdk_sidecar::ClaudeSdkSidecarAdapter};
use futures::{stream::BoxStream, StreamExt};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

const STEP_TIMEOUT: Duration = Duration::from_secs(120);
const DEFINITION_ID: &str = "claude-conformance-fixture";
const SERVER_NAME: &str = "dcc-conformance-fixture";

type ProviderStream = BoxStream<'static, dcc_core::Result<ProviderEvent>>;

#[derive(Default)]
struct TurnObservation {
    statuses: Vec<dcc_core::domain::mcp::McpRuntimeStatus>,
    text: String,
    permission_resolutions: Vec<(String, String)>,
    completed_actions: Vec<String>,
    failed: bool,
}

struct ClaudeMcpConformanceAdapter {
    provider: ClaudeSdkSidecarAdapter,
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
}

impl ClaudeMcpConformanceAdapter {
    fn new(fixture_binary: PathBuf, workspace: PathBuf) -> Self {
        Self {
            provider: claude_code::adapter(),
            fixture_binary,
            workspace,
            model: std::env::var("DCC_CLAUDE_CONFORMANCE_MODEL")
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
            .args(["http", "--bind", "127.0.0.1:0"])
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
                args: vec!["stdio".to_string()],
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
            definition_id: McpDefinitionId(DEFINITION_ID.to_string()),
            server_name: SERVER_NAME.to_string(),
            transport: projected_transport,
        })
    }

    async fn prepare_session(
        &mut self,
        servers: Vec<ProviderMcpServerConfig>,
    ) -> McpConformanceAdapterResult<()> {
        self.cleanup_runtime().await?;
        self.session_sequence += 1;
        let session_id = SessionId(format!("claude-mcp-conformance-{}", self.session_sequence));
        let handle = self
            .provider
            .prepare_session(SessionConfig {
                workspace_id: WorkspaceId("claude-mcp-conformance".to_string()),
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
        match event {
            ProviderEvent::McpRuntimeStatusSnapshot { statuses } => {
                observation.statuses = statuses.clone();
            }
            ProviderEvent::TextDelta { content } => observation.text.push_str(content),
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
                    let allow = allow_echo && tool_matches(&request.tool_name, "fixture_echo");
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
        Ok(observation.failed
            && observation.statuses.iter().any(|status| {
                status.definition_id.0 == DEFINITION_ID && status.state == McpRuntimeState::Failed
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
        let secret_reference =
            McpSecretReferenceId("credential:claude-conformance-canary".to_string());
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
            id: McpDefinitionId(DEFINITION_ID.to_string()),
            display_name: "Claude conformance fixture".to_string(),
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
            id: McpBindingId("claude-conformance-global".to_string()),
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
                provider_id: ProviderId("claude_code".to_string()),
                project_id: ProjectId("claude-conformance".to_string()),
                session_id: SessionId("claude-conformance".to_string()),
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
}

#[async_trait]
impl McpConformanceAdapter for ClaudeMcpConformanceAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId("claude_code".to_string())
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
                    .find(|status| status.definition_id.0 == DEFINITION_ID)
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
                if observation.failed || !observation.text.contains(MCP_CONFORMANCE_ECHO_VALUE) {
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

fn test_workspace() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "dcc-claude-mcp-conformance-{}-{nonce}",
        std::process::id()
    ))
}

fn require_explicit_opt_in() {
    assert_eq!(
        std::env::var("DCC_RUN_CLAUDE_MCP_CONFORMANCE")
            .ok()
            .as_deref(),
        Some("1"),
        "set DCC_RUN_CLAUDE_MCP_CONFORMANCE=1 after authenticating Claude Code"
    );
}

#[tokio::test]
#[ignore = "requires explicit opt-in and an authenticated Claude Code account"]
async fn authenticated_claude_bridge_passes_the_shared_harness() {
    require_explicit_opt_in();
    let workspace = test_workspace();
    std::fs::create_dir_all(&workspace).expect("create isolated conformance workspace");
    let mut adapter = ClaudeMcpConformanceAdapter::new(fixture_binary(), workspace.clone());

    let evidence = run_provider_mcp_conformance(&mut adapter)
        .await
        .expect("Claude bridge conformance");
    evidence
        .validate_for_provider(
            &ProviderId("claude_code".to_string()),
            adapter
                .provider
                .dcc_mcp_projection_version()
                .expect("Claude projection version"),
        )
        .expect("version-bound Claude evidence");

    adapter.reset().await.expect("final Claude cleanup");
    let _ = std::fs::remove_dir_all(workspace);
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

#[tokio::test]
async fn missing_credentials_fail_before_the_claude_runtime_for_both_transports() {
    let workspace = test_workspace();
    std::fs::create_dir_all(&workspace).expect("create isolated credential workspace");
    let mut adapter = ClaudeMcpConformanceAdapter::new(fixture_binary(), workspace.clone());
    adapter.attached = true;
    adapter.credential_unavailable = true;

    for transport in [McpTransportKind::Stdio, McpTransportKind::Http] {
        assert!(
            adapter
                .missing_credential_fails_closed(&transport)
                .await
                .expect("categorical missing-credential result"),
            "{transport:?} did not fail before provider attachment"
        );
    }

    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn test_workspace_is_bounded_to_the_system_temporary_directory() {
    assert!(test_workspace().starts_with(Path::new(&std::env::temp_dir())));
}
