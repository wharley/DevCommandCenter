use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dcc_core::{
    application::{resolve_session_mcp_servers, ResolveSessionMcpInput},
    domain::{
        mcp::{
            McpBinding, McpBindingId, McpBindingScope, McpDefinition, McpDefinitionId,
            McpDefinitionOwnership, McpRuntimeState, McpSecretBinding, McpSecretReferenceId,
            McpSecretTarget, McpTransport, McpTrust, McpTrustDecision, McpTrustFingerprint,
        },
        project::ProjectId,
        provider::{ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
        workspace::WorkspaceId,
    },
    ports::{
        CredentialStore, Input, McpRepo, Provider, ProviderMcpServerConfig, ProviderTurnInput,
        SecretValue, SessionConfig,
    },
};
use dcc_infra::{credential_store::InMemoryCredentialStore, mcp_db::SqliteMcpRepo};
use dcc_providers::{claude_code, codex};
use futures::{stream::BoxStream, StreamExt};
use tokio::time::timeout;
use url::Url;

const STEP_TIMEOUT: Duration = Duration::from_secs(180);
const FIGMA_ENDPOINT: &str = "https://mcp.figma.com/mcp";
const FIGMA_TOOL: &str = "get_design_context";
const FIGMA_SUCCESS: &str = "DCC_FIGMA_READ_ONLY_OK";
const GARU_PACKAGE: &str = "@garuhq/mcp";
const GARU_PACKAGE_VERSION: &str = "0.17.0";
const GARU_TOOL: &str = "list_charges";
const GARU_SUCCESS: &str = "DCC_GARU_READ_ONLY_OK";
const GARU_EXECUTION_ACK: &str = "I_UNDERSTAND_THIS_RUNS_PINNED_THIRD_PARTY_CODE";

type ProviderStream = BoxStream<'static, dcc_core::Result<ProviderEvent>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokeFailure {
    Configuration,
    Registry,
    ProviderSession,
    Attachment,
    PermissionBoundary,
    UnexpectedTool,
    ToolExecution,
    ResponseContract,
    Timeout,
    Cleanup,
}

struct SmokeSpec {
    definition_id: &'static str,
    display_name: &'static str,
    transport: McpTransport,
    secret: Option<(&'static str, SecretValue)>,
    expected_tool: &'static str,
    prompt: String,
    success_sentinel: &'static str,
}

struct ExactSentinel {
    expected: &'static str,
    observed: String,
    invalid: bool,
}

impl ExactSentinel {
    fn new(expected: &'static str) -> Self {
        Self {
            expected,
            observed: String::with_capacity(expected.len()),
            invalid: false,
        }
    }

    fn push(&mut self, content: &str) {
        if self.invalid || self.observed.len().saturating_add(content.len()) > self.expected.len() {
            self.invalid = true;
            self.observed.clear();
            return;
        }
        self.observed.push_str(content);
        if !self.expected.starts_with(&self.observed) {
            self.invalid = true;
            self.observed.clear();
        }
    }

    fn matches(&self) -> bool {
        !self.invalid && self.observed == self.expected
    }
}

struct RealServiceSmokeRunner<P> {
    provider: P,
    provider_id: ProviderId,
    workspace: PathBuf,
    model: Option<String>,
    definition_id: McpDefinitionId,
    expected_tool: &'static str,
    handle: Option<SessionHandle>,
    events: Option<ProviderStream>,
}

impl<P> RealServiceSmokeRunner<P>
where
    P: Provider,
{
    fn new(
        provider: P,
        provider_id: &str,
        model_env: &str,
        workspace: PathBuf,
        definition_id: McpDefinitionId,
        expected_tool: &'static str,
    ) -> Self {
        Self {
            provider,
            provider_id: ProviderId(provider_id.to_string()),
            workspace,
            model: bounded_optional_env(model_env, 256),
            definition_id,
            expected_tool,
            handle: None,
            events: None,
        }
    }

    async fn prepare_session(
        &mut self,
        servers: Vec<ProviderMcpServerConfig>,
    ) -> Result<(), SmokeFailure> {
        let handle = self
            .provider
            .prepare_session(SessionConfig {
                workspace_id: WorkspaceId(format!("{}-real-service-smoke", self.provider_id.0)),
                session_id: SessionId(format!("{}-real-service-smoke", self.provider_id.0)),
                model: self.model.clone(),
                working_directory: Some(self.workspace.to_string_lossy().to_string()),
                additional_working_directories: Vec::new(),
                provider_runtime: None,
                mcp_servers: servers,
            })
            .await
            .map_err(|_| SmokeFailure::ProviderSession)?;
        self.events = Some(self.provider.stream_events(&handle));
        self.handle = Some(handle);
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<(), SmokeFailure> {
        self.events = None;
        if let Some(handle) = self.handle.take() {
            self.provider
                .cancel(&handle)
                .await
                .map_err(|_| SmokeFailure::Cleanup)?;
        }
        Ok(())
    }

    async fn send_turn(&self, prompt: &str) -> Result<(), SmokeFailure> {
        let handle = self.handle.as_ref().ok_or(SmokeFailure::ProviderSession)?;
        self.provider
            .send_input(
                handle,
                Input::Turn(ProviderTurnInput {
                    prompt: prompt.to_string(),
                    tool_instructions: Some(
                        "Use only the explicitly named DCC-projected MCP tool. Never use shell, \
                         files, web, or another tool. Never quote or summarize tool output."
                            .to_string(),
                    ),
                    plan_mode: Some(false),
                    effort: Some("low".to_string()),
                    fast_mode: Some(true),
                }),
            )
            .await
            .map_err(|_| SmokeFailure::ProviderSession)
    }

    async fn next_event(&mut self) -> Result<ProviderEvent, SmokeFailure> {
        let events = self.events.as_mut().ok_or(SmokeFailure::ProviderSession)?;
        timeout(STEP_TIMEOUT, events.next())
            .await
            .map_err(|_| SmokeFailure::Timeout)?
            .ok_or(SmokeFailure::ProviderSession)?
            .map_err(|_| SmokeFailure::ProviderSession)
    }

    async fn resolve_permission(
        &self,
        request_id: String,
        allow: bool,
    ) -> Result<(), SmokeFailure> {
        let handle = self.handle.as_ref().ok_or(SmokeFailure::ProviderSession)?;
        self.provider
            .send_input(
                handle,
                Input::PermissionResponse(dcc_core::ports::provider::ProviderPermissionResponse {
                    request_id,
                    behavior: if allow { "allow" } else { "deny" }.to_string(),
                }),
            )
            .await
            .map_err(|_| SmokeFailure::PermissionBoundary)
    }

    async fn confirm_inventory(&mut self) -> Result<(), SmokeFailure> {
        self.send_turn("Do not call tools. Reply exactly DCC_REAL_SERVICE_READY.")
            .await?;
        let mut sentinel = ExactSentinel::new("DCC_REAL_SERVICE_READY.");
        let mut connected = false;
        loop {
            match self.next_event().await? {
                ProviderEvent::McpRuntimeStatusSnapshot { statuses } => {
                    connected = statuses.iter().any(|status| {
                        status.definition_id == self.definition_id
                            && status.state == McpRuntimeState::Connected
                            && status
                                .tools
                                .iter()
                                .any(|tool| tool.name == self.expected_tool)
                    });
                }
                ProviderEvent::TextDelta { content } => sentinel.push(&content),
                ProviderEvent::PermissionRequested { request, .. } => {
                    self.resolve_permission(request.request_id, false).await?;
                    return Err(SmokeFailure::UnexpectedTool);
                }
                ProviderEvent::ToolCallStarted { .. } => {
                    return Err(SmokeFailure::UnexpectedTool);
                }
                ProviderEvent::Failed { .. } => return Err(SmokeFailure::Attachment),
                ProviderEvent::Completed { .. } => {
                    return if connected && sentinel.matches() {
                        Ok(())
                    } else {
                        Err(SmokeFailure::Attachment)
                    };
                }
                _ => {}
            }
        }
    }

    async fn call_read_only_tool(
        &mut self,
        prompt: &str,
        success_sentinel: &'static str,
    ) -> Result<(), SmokeFailure> {
        self.send_turn(prompt).await?;
        let mut sentinel = ExactSentinel::new(success_sentinel);
        let mut active_calls = HashMap::<String, bool>::new();
        let mut expected_started = false;
        let mut expected_completed = false;
        let mut expected_permission = false;
        loop {
            match self.next_event().await? {
                ProviderEvent::TextDelta { content } => sentinel.push(&content),
                ProviderEvent::ToolCallStarted { id, action, .. } => {
                    let expected = provider_tool_matches(&action, self.expected_tool);
                    expected_started |= expected;
                    active_calls.insert(id, expected);
                    if !expected {
                        return Err(SmokeFailure::UnexpectedTool);
                    }
                }
                ProviderEvent::PermissionRequested { request, .. } => {
                    let expected = provider_tool_matches(&request.tool_name, self.expected_tool);
                    expected_permission |= expected;
                    self.resolve_permission(request.request_id, expected)
                        .await?;
                    if !expected {
                        return Err(SmokeFailure::UnexpectedTool);
                    }
                }
                ProviderEvent::ToolCallCompleted { id, .. } => {
                    if active_calls.remove(&id) == Some(true) {
                        expected_completed = true;
                    }
                }
                ProviderEvent::ToolCallFailed { id, .. } => {
                    if active_calls.remove(&id) == Some(true) {
                        return Err(SmokeFailure::ToolExecution);
                    }
                }
                ProviderEvent::Failed { .. } => return Err(SmokeFailure::ToolExecution),
                ProviderEvent::Completed { .. } => {
                    return if expected_started
                        && expected_completed
                        && expected_permission
                        && sentinel.matches()
                    {
                        Ok(())
                    } else {
                        Err(SmokeFailure::ResponseContract)
                    };
                }
                _ => {}
            }
        }
    }

    async fn run(
        &mut self,
        servers: Vec<ProviderMcpServerConfig>,
        prompt: &str,
        success_sentinel: &'static str,
    ) -> Result<(), SmokeFailure> {
        self.prepare_session(servers).await?;
        self.confirm_inventory().await?;
        self.call_read_only_tool(prompt, success_sentinel).await
    }
}

fn provider_tool_matches(provider_name: &str, expected_tool: &str) -> bool {
    provider_name == expected_tool
        || provider_name
            .rsplit_once("__")
            .is_some_and(|(_, suffix)| suffix == expected_tool)
}

fn bounded_optional_env(name: &str, max_chars: usize) -> Option<String> {
    std::env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()
            && trimmed.chars().count() <= max_chars
            && !trimmed.chars().any(char::is_control))
        .then(|| trimmed.to_string())
    })
}

fn require_opt_in(name: &str, expected: &str) -> Result<(), SmokeFailure> {
    exact_opt_in(std::env::var(name).ok().as_deref(), expected)
        .then_some(())
        .ok_or(SmokeFailure::Configuration)
}

fn exact_opt_in(value: Option<&str>, expected: &str) -> bool {
    value == Some(expected)
}

fn garu_command_args() -> Vec<String> {
    vec![
        "-y".to_string(),
        format!("{GARU_PACKAGE}@{GARU_PACKAGE_VERSION}"),
    ]
}

fn validate_figma_fixture_url(value: &str) -> Result<String, SmokeFailure> {
    if value.chars().count() > 2_048 || value.chars().any(char::is_control) {
        return Err(SmokeFailure::Configuration);
    }
    let url = Url::parse(value).map_err(|_| SmokeFailure::Configuration)?;
    let valid_host = matches!(url.host_str(), Some("figma.com" | "www.figma.com"));
    let mut segments = url.path_segments().ok_or(SmokeFailure::Configuration)?;
    let resource_kind = segments.next().ok_or(SmokeFailure::Configuration)?;
    let file_key = segments.next().ok_or(SmokeFailure::Configuration)?;
    let valid_path = matches!(resource_kind, "design" | "file")
        && !file_key.is_empty()
        && file_key.len() <= 128
        && file_key.bytes().all(|byte| byte.is_ascii_alphanumeric());
    let node_id = url
        .query_pairs()
        .find_map(|(name, value)| (name == "node-id").then(|| value.into_owned()))
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_'))
        })
        .ok_or(SmokeFailure::Configuration)?;
    if url.scheme() != "https"
        || !valid_host
        || !valid_path
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(SmokeFailure::Configuration);
    }
    Ok(format!(
        "https://www.figma.com/{resource_kind}/{file_key}/DCC-Smoke?node-id={node_id}"
    ))
}

fn absolute_executable(value: &str) -> Result<PathBuf, SmokeFailure> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(SmokeFailure::Configuration);
    }
    let path = std::fs::canonicalize(path).map_err(|_| SmokeFailure::Configuration)?;
    let valid_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "npx" | "npx.cmd" | "npx-cli.js"));
    (path.is_file() && valid_name)
        .then_some(path)
        .ok_or(SmokeFailure::Configuration)
}

fn figma_spec(run_variable: &str) -> Result<SmokeSpec, SmokeFailure> {
    require_opt_in(run_variable, "1")?;
    let fixture_url = bounded_optional_env("DCC_FIGMA_MCP_FIXTURE_URL", 2_048)
        .ok_or(SmokeFailure::Configuration)
        .and_then(|value| validate_figma_fixture_url(&value))?;
    Ok(SmokeSpec {
        definition_id: "figma-real-service-smoke",
        display_name: "Figma real-service smoke",
        transport: McpTransport::Http {
            url: FIGMA_ENDPOINT.to_string(),
        },
        secret: None,
        expected_tool: FIGMA_TOOL,
        prompt: format!(
            "Call the DCC-projected Figma MCP tool {FIGMA_TOOL} exactly once for this disposable \
             Figma node URL: {fixture_url}. Do not call any write, create, update, upload, or \
             mapping tool. Do not quote or summarize the result. After the tool completes, reply \
             exactly {FIGMA_SUCCESS}"
        ),
        success_sentinel: FIGMA_SUCCESS,
    })
}

fn garu_spec(run_variable: &str) -> Result<SmokeSpec, SmokeFailure> {
    require_opt_in(run_variable, "1")?;
    require_opt_in("DCC_GARU_MCP_DEDICATED_TEST_ACCOUNT", "1")?;
    require_opt_in("DCC_GARU_MCP_ALLOW_PINNED_EXECUTION", GARU_EXECUTION_ACK)?;
    let npx = bounded_optional_env("DCC_GARU_MCP_NPX", 1_024)
        .ok_or(SmokeFailure::Configuration)
        .and_then(|value| absolute_executable(&value))?;
    let api_key = std::env::var("DCC_GARU_MCP_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(SmokeFailure::Configuration)
        .and_then(|value| {
            SecretValue::new(value.into_bytes()).map_err(|_| SmokeFailure::Configuration)
        })?;
    Ok(SmokeSpec {
        definition_id: "garu-real-service-smoke",
        display_name: "Garu real-service smoke",
        transport: McpTransport::Stdio {
            executable: npx.to_string_lossy().to_string(),
            args: garu_command_args(),
            cwd: None,
        },
        secret: Some(("GARU_API_KEY", api_key)),
        expected_tool: GARU_TOOL,
        prompt: format!(
            "Call the DCC-projected Garu MCP tool {GARU_TOOL} exactly once using the smallest \
             supported page size and no mutation. Never call create, update, delete, charge, \
             payment, or refund tools. Do not quote or summarize the result. After the tool \
             completes, reply exactly {GARU_SUCCESS}"
        ),
        success_sentinel: GARU_SUCCESS,
    })
}

async fn resolve_smoke_server(
    spec: SmokeSpec,
    provider_id: &str,
    workspace: &Path,
) -> Result<(ProviderMcpServerConfig, String, &'static str), SmokeFailure> {
    let db_path = workspace.join("real-service-smoke.sqlite");
    let repo = SqliteMcpRepo::open(&db_path).map_err(|_| SmokeFailure::Registry)?;
    let credentials = InMemoryCredentialStore::default();
    let secret_refs = if let Some((name, secret)) = spec.secret {
        let reference = McpSecretReferenceId("credential:real-service-smoke".to_string());
        credentials
            .store_secret(&reference, secret)
            .await
            .map_err(|_| SmokeFailure::Registry)?;
        vec![McpSecretBinding {
            target: McpSecretTarget::EnvironmentVariable {
                name: name.to_string(),
            },
            secret_ref: reference,
        }]
    } else {
        Vec::new()
    };
    let mut definition = McpDefinition {
        id: McpDefinitionId(spec.definition_id.to_string()),
        display_name: spec.display_name.to_string(),
        transport: spec.transport,
        secret_refs,
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
        .map_err(|_| SmokeFailure::Registry)?;
    repo.save_mcp_binding(&McpBinding {
        id: McpBindingId(format!("{provider_id}-real-service-smoke")),
        definition_id: definition.id.clone(),
        scope: McpBindingScope::Global,
        enabled: true,
        provider_exclusions: Vec::new(),
        created_at: "2026-07-28T00:00:00Z".to_string(),
        updated_at: "2026-07-28T00:00:00Z".to_string(),
    })
    .await
    .map_err(|_| SmokeFailure::Registry)?;
    let servers = resolve_session_mcp_servers(
        &repo,
        &credentials,
        &ResolveSessionMcpInput {
            provider_id: ProviderId(provider_id.to_string()),
            project_id: ProjectId("real-service-smoke".to_string()),
            session_id: SessionId(format!("{provider_id}-real-service-smoke")),
        },
    )
    .await
    .map_err(|_| SmokeFailure::Registry)?;
    let [server]: [ProviderMcpServerConfig; 1] =
        servers.try_into().map_err(|_| SmokeFailure::Registry)?;
    Ok((server, spec.prompt, spec.success_sentinel))
}

fn test_workspace(target: &str, provider: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "dcc-{target}-{provider}-real-service-smoke-{}-{nonce}",
        std::process::id()
    ))
}

async fn run_smoke<P>(
    provider: P,
    provider_id: &str,
    model_env: &str,
    spec: SmokeSpec,
) -> Result<(), SmokeFailure>
where
    P: Provider,
{
    let workspace = test_workspace(spec.definition_id, provider_id);
    std::fs::create_dir_all(&workspace).map_err(|_| SmokeFailure::Configuration)?;
    let definition_id = McpDefinitionId(spec.definition_id.to_string());
    let expected_tool = spec.expected_tool;
    let resolved = resolve_smoke_server(spec, provider_id, &workspace).await;
    let result = match resolved {
        Ok((server, prompt, success_sentinel)) => {
            let mut runner = RealServiceSmokeRunner::new(
                provider,
                provider_id,
                model_env,
                workspace.clone(),
                definition_id,
                expected_tool,
            );
            let result = runner.run(vec![server], &prompt, success_sentinel).await;
            let cleanup = runner.cleanup().await;
            result.and(cleanup)
        }
        Err(error) => Err(error),
    };
    std::fs::remove_dir_all(&workspace).map_err(|_| SmokeFailure::Cleanup)?;
    result
}

#[tokio::test]
#[ignore = "requires explicit opt-in, provider-native Figma OAuth, and a disposable read-only node"]
async fn authenticated_claude_figma_read_only_smoke() {
    let spec = figma_spec("DCC_RUN_CLAUDE_FIGMA_MCP_SMOKE")
        .expect("invalid categorical Figma smoke configuration");
    run_smoke(
        claude_code::adapter(),
        "claude_code",
        "DCC_CLAUDE_MCP_SMOKE_MODEL",
        spec,
    )
    .await
    .expect("Claude Figma read-only smoke failed categorically");
}

#[tokio::test]
#[ignore = "requires explicit opt-in, a negotiated codex-cli MCP runtime, provider-native Figma OAuth, and a disposable read-only node"]
async fn authenticated_codex_figma_read_only_smoke() {
    let spec = figma_spec("DCC_RUN_CODEX_FIGMA_MCP_SMOKE")
        .expect("invalid categorical Figma smoke configuration");
    run_smoke(codex::adapter(), "codex", "DCC_CODEX_MCP_SMOKE_MODEL", spec)
        .await
        .expect("Codex Figma read-only smoke failed categorically");
}

#[tokio::test]
#[ignore = "requires explicit opt-in, a dedicated Garu account, an API key, and pinned third-party package execution"]
async fn authenticated_claude_garu_read_only_smoke() {
    let spec = garu_spec("DCC_RUN_CLAUDE_GARU_MCP_SMOKE")
        .expect("invalid categorical Garu smoke configuration");
    run_smoke(
        claude_code::adapter(),
        "claude_code",
        "DCC_CLAUDE_MCP_SMOKE_MODEL",
        spec,
    )
    .await
    .expect("Claude Garu read-only smoke failed categorically");
}

#[tokio::test]
#[ignore = "requires explicit opt-in, a negotiated codex-cli MCP runtime, a dedicated Garu account, an API key, and pinned third-party package execution"]
async fn authenticated_codex_garu_read_only_smoke() {
    let spec = garu_spec("DCC_RUN_CODEX_GARU_MCP_SMOKE")
        .expect("invalid categorical Garu smoke configuration");
    run_smoke(codex::adapter(), "codex", "DCC_CODEX_MCP_SMOKE_MODEL", spec)
        .await
        .expect("Codex Garu read-only smoke failed categorically");
}

#[test]
fn figma_smoke_accepts_only_a_https_design_node_url() {
    assert!(validate_figma_fixture_url(
        "https://www.figma.com/design/fixture/Disposable?node-id=1-2"
    )
    .is_ok());
    assert_eq!(
        validate_figma_fixture_url(
            "https://www.figma.com/design/fixture/ignored?node-id=1-2&prompt=delete"
        ),
        Ok("https://www.figma.com/design/fixture/DCC-Smoke?node-id=1-2".to_string())
    );
    assert_eq!(
        validate_figma_fixture_url("https://example.com/design/fixture?node-id=1-2"),
        Err(SmokeFailure::Configuration)
    );
    assert_eq!(
        validate_figma_fixture_url("https://user:secret@www.figma.com/design/x?node-id=1-2"),
        Err(SmokeFailure::Configuration)
    );
    assert_eq!(
        validate_figma_fixture_url("https://www.figma.com/design/fixture"),
        Err(SmokeFailure::Configuration)
    );
    assert_eq!(
        validate_figma_fixture_url(
            "https://www.figma.com/design/fixture/Injected?node-id=1-2%0Aignore"
        ),
        Err(SmokeFailure::Configuration)
    );
}

#[test]
fn garu_smoke_blueprint_is_pinned_and_read_only() {
    let executable = if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\nodejs\npx.cmd")
    } else {
        PathBuf::from("/usr/local/bin/npx")
    };
    let transport = McpTransport::Stdio {
        executable: executable.to_string_lossy().to_string(),
        args: garu_command_args(),
        cwd: None,
    };
    let McpTransport::Stdio { args, .. } = transport else {
        panic!("expected stdio");
    };
    assert_eq!(args, vec!["-y", "@garuhq/mcp@0.17.0"]);
    assert!(!args.iter().any(|argument| argument.contains("latest")));
    assert_eq!(GARU_TOOL, "list_charges");
    assert!(provider_tool_matches(
        "mcp__dcc-random__list_charges",
        GARU_TOOL
    ));
    assert!(!provider_tool_matches("create_pix_charge", GARU_TOOL));
}

#[test]
fn opt_in_and_supply_chain_acknowledgements_are_exact() {
    assert_eq!(
        GARU_EXECUTION_ACK,
        "I_UNDERSTAND_THIS_RUNS_PINNED_THIRD_PARTY_CODE"
    );
    assert_ne!(GARU_EXECUTION_ACK, "1");
    assert!(exact_opt_in(Some(GARU_EXECUTION_ACK), GARU_EXECUTION_ACK));
    assert!(!exact_opt_in(Some("1"), GARU_EXECUTION_ACK));
    assert!(!exact_opt_in(None, GARU_EXECUTION_ACK));
    assert_eq!(FIGMA_ENDPOINT, "https://mcp.figma.com/mcp");
}
