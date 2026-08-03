use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tokio::time::sleep;

use dcc_core::{
    application::{
        abort_run as run_abort_run, active_turn_for_steer, approve_plan as run_approve_plan,
        close_session as run_close_session, list_turn_queue as run_list_turn_queue,
        queue_turn as run_queue_turn, record_plan_handoff as run_record_plan_handoff,
        record_turn_steer, remove_queued_turn as run_remove_queued_turn,
        reorder_turn_queue as run_reorder_turn_queue, restore_session as run_restore_session,
        resume_session as run_resume_session, send_turn as run_send_turn,
        start_thread as run_start_thread, AbortRunInput, AbortRunOutput, ApprovePlanInput,
        ApprovePlanOutput, CloseSessionInput, CloseSessionOutput, QueueTurnInput,
        RecordPlanHandoffInput, RecordPlanHandoffOutput, RemoveQueuedTurnInput,
        ReorderTurnQueueInput, RestoreSessionInput, RestoreSessionOutput, ResumeSessionInput,
        ResumeSessionOutput, SendTurnInput, SendTurnOutput, StartThreadInput, StartThreadOutput,
        SteerTurnInput, SteerTurnOutput,
    },
    domain::{
        mcp::{McpDefinitionId, McpErrorCategory, McpRuntimeState, McpRuntimeStatus},
        provider::McpOauthSupport,
        session::{
            QueuedTurn, SessionEventRecord, SessionId, SessionProjection, SessionSearchResult,
            WorkspaceSessionSummary,
        },
        thread::Thread,
        workspace::{Workspace, WorkspaceId},
    },
    ports::{
        provider::ProviderPermissionResponse, provider::ProviderUserInputAnswer,
        provider::ProviderUserInputResponse, Input, ProviderRuntimeConfig, ProviderTurnInput,
        SessionEventRepo, SessionRepo, ThreadRepo, WorkspaceRepo,
    },
};
use dcc_infra::db::SqliteWorkspaceRepo;

use crate::state::SessionCommandState;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunPullRequestReviewAgentInput {
    pub working_directory: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub provider_runtime: Option<ProviderRuntimeConfig>,
    pub prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunPullRequestReviewAgentOutput {
    pub response: String,
}

#[tauri::command]
pub async fn run_pull_request_review_agent(
    state: State<'_, SessionCommandState>,
    input: RunPullRequestReviewAgentInput,
) -> Result<RunPullRequestReviewAgentOutput, String> {
    let working_directory = input.working_directory.trim();
    let root = std::path::Path::new(working_directory);
    if working_directory.is_empty() || !root.is_absolute() || !root.is_dir() {
        return Err("Pull request repository directory is invalid.".to_string());
    }
    let provider_id = input.provider_id.trim();
    if provider_id.is_empty() {
        return Err("Select a provider for the review.".to_string());
    }
    let prompt = input.prompt.trim();
    if prompt.is_empty() || prompt.len() > 120_000 {
        return Err("Pull request review context is empty or too large.".to_string());
    }
    let response = state
        .run_ephemeral_read_only_turn(
            working_directory.to_string(),
            provider_id.to_string(),
            input.model,
            input.provider_runtime,
            prompt.to_string(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(RunPullRequestReviewAgentOutput { response })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RespondToUserInputInput {
    pub session_id: String,
    pub request_id: String,
    #[serde(default)]
    pub answers: Vec<ProviderUserInputAnswer>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RespondToUserInputOutput {
    pub ok: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RespondToPermissionRequestInput {
    pub session_id: String,
    pub request_id: String,
    pub behavior: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RespondToPermissionRequestOutput {
    pub ok: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchSessionsInput {
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyTaskTitleInput {
    pub workspace_id: String,
    pub session_id: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyTaskTitleOutput {
    pub applied: bool,
    pub workspace: Workspace,
    pub thread: Thread,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMcpRuntimeStatusesInput {
    pub session_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMcpRuntimeStatusesOutput {
    pub statuses: Vec<McpRuntimeStatus>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartMcpOauthInput {
    pub session_id: String,
    pub definition_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartMcpOauthOutput {
    pub authorization_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpTurnPreflightState {
    Ready,
    AuthenticationRequired {
        #[serde(rename = "definitionId")]
        definition_id: McpDefinitionId,
        #[serde(rename = "authorizationUrl")]
        authorization_url: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrepareTurnOutput {
    pub preflight: McpTurnPreflightState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WaitMcpOauthInput {
    pub session_id: String,
    pub definition_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WaitMcpOauthOutput {
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum McpPreflightReadiness {
    Ready,
    Attaching,
    AuthenticationRequired(McpDefinitionId),
    Failed(String),
}

const MCP_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_OAUTH_COMPLETION_TIMEOUT: Duration = Duration::from_secs(300);
const MCP_PREFLIGHT_POLL_INTERVAL: Duration = Duration::from_millis(100);

fn classify_mcp_preflight(statuses: &[McpRuntimeStatus]) -> McpPreflightReadiness {
    if let Some(status) = statuses.iter().find(|status| {
        status.state == McpRuntimeState::Failed
            && status
                .bounded_error
                .as_ref()
                .is_some_and(|error| error.category == McpErrorCategory::Authentication)
    }) {
        return McpPreflightReadiness::AuthenticationRequired(status.definition_id.clone());
    }
    if let Some(status) = statuses
        .iter()
        .find(|status| status.state == McpRuntimeState::Failed)
    {
        return McpPreflightReadiness::Failed(
            status
                .bounded_error
                .as_ref()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| "MCP provider attachment failed".to_string()),
        );
    }
    if statuses
        .iter()
        .any(|status| status.state == McpRuntimeState::NeedsTrust)
    {
        return McpPreflightReadiness::Failed(
            "MCP integration requires trust approval before the turn".to_string(),
        );
    }
    if statuses
        .iter()
        .any(|status| status.state == McpRuntimeState::Unsupported)
    {
        return McpPreflightReadiness::Failed(
            "MCP integration is unsupported by this provider runtime".to_string(),
        );
    }
    if statuses.iter().any(|status| {
        matches!(
            status.state,
            McpRuntimeState::ProbingServer
                | McpRuntimeState::ServerReachable
                | McpRuntimeState::AttachingProvider
        )
    }) {
        return McpPreflightReadiness::Attaching;
    }
    McpPreflightReadiness::Ready
}

async fn wait_for_mcp_preflight(
    state: &SessionCommandState,
    session_id: &SessionId,
) -> Result<McpPreflightReadiness, String> {
    let deadline = Instant::now() + MCP_PREFLIGHT_TIMEOUT;
    loop {
        let readiness = classify_mcp_preflight(
            &state
                .list_mcp_runtime_statuses(session_id)
                .map_err(|error| error.to_string())?,
        );
        if readiness != McpPreflightReadiness::Attaching {
            return Ok(readiness);
        }
        if Instant::now() >= deadline {
            return Err("MCP provider attachment timed out before the turn".to_string());
        }
        sleep(MCP_PREFLIGHT_POLL_INTERVAL).await;
    }
}

fn default_search_limit() -> usize {
    40
}

#[tauri::command]
pub async fn start_thread(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: StartThreadInput,
) -> Result<StartThreadOutput, String> {
    state
        .validate_start_thread_scope(&input)
        .await
        .map_err(|error| error.to_string())?;
    let output = run_start_thread(&*state, &*state, &*state, &*state, input)
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = state.attach_provider_session(&output.session).await {
        eprintln!("[DCC] provider session attach failed: {}", error);
    }
    Ok(output)
}

#[tauri::command]
pub async fn send_turn(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: SendTurnInput,
) -> Result<SendTurnOutput, String> {
    let session = state
        .prepare_provider_session_for_turn(&input)
        .await
        .map_err(|error| error.to_string())?;
    if state
        .session_mcp_oauth_support(&session.id)
        .map_err(|error| error.to_string())?
        == McpOauthSupport::InteractivePreflight
    {
        match wait_for_mcp_preflight(&state, &session.id).await? {
            McpPreflightReadiness::Ready => {}
            McpPreflightReadiness::AuthenticationRequired(_) => {
                return Err("MCP authentication must complete before sending the turn".to_string());
            }
            McpPreflightReadiness::Failed(message) => return Err(message),
            McpPreflightReadiness::Attaching => unreachable!("bounded wait resolves attaching"),
        }
    }

    let provider_turn_input = ProviderTurnInput {
        prompt: input.prompt.clone(),
        tool_instructions: input.tool_instructions.clone(),
        plan_mode: input.plan_mode,
        effort: input.effort.clone(),
        fast_mode: input.fast_mode,
        approval_policy: input.approval_policy,
    };
    let output = run_send_turn(&*state, &*state, &*state, input)
        .await
        .map_err(|error| error.to_string())?;

    // Turn is now recorded in the event store. Any failure from here must emit
    // TurnAborted so the UI does not get stuck on session.turn.started.
    let turn_id = output.turn.id.clone();
    let session_id = output.session.id.clone();

    let abort_turn = |reason: String| {
        let state = &state;
        let session_id = session_id.clone();
        let turn_id = turn_id.clone();
        async move {
            let _ = state
                .emit_turn_aborted(&session_id, &turn_id, Some(reason.clone()))
                .await;
            reason
        }
    };

    if let Err(error) = state
        .set_active_turn(&output.session.id, Some(turn_id.0.clone()))
        .await
    {
        return Err(abort_turn(error.to_string()).await);
    }

    if let Err(error) = state
        .send_provider_input(&output.session.id, Input::Turn(provider_turn_input))
        .await
    {
        let _ = state.set_active_turn(&output.session.id, None).await;
        return Err(abort_turn(error.to_string()).await);
    }

    Ok(output)
}

#[tauri::command]
pub async fn steer_turn(
    state: State<'_, SessionCommandState>,
    input: SteerTurnInput,
) -> Result<SteerTurnOutput, String> {
    let (_, turn_id) = active_turn_for_steer(&*state, &*state, &input)
        .await
        .map_err(|error| error.to_string())?;
    state
        .steer_provider_turn(&input.session_id, input.prompt.trim())
        .await
        .map_err(|error| error.to_string())?;
    record_turn_steer(&*state, &*state, &*state, input, turn_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn queue_turn(
    state: State<'_, SessionCommandState>,
    input: QueueTurnInput,
) -> Result<QueuedTurn, String> {
    let history = SessionEventRepo::list_events_by_session(&*state, &input.turn.session_id)
        .await
        .map_err(|error| error.to_string())?;
    let projection =
        SessionProjection::fold(&history).ok_or_else(|| "session history is empty".to_string())?;
    if projection.active_turn_id.is_none() {
        return Err("follow-ups can only be queued while a turn is active".to_string());
    }
    run_queue_turn(&*state, &*state, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_turn_queue(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<Vec<QueuedTurn>, String> {
    run_list_turn_queue(&*state, &SessionId(session_id))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_queued_turn(
    state: State<'_, SessionCommandState>,
    input: RemoveQueuedTurnInput,
) -> Result<Vec<QueuedTurn>, String> {
    run_remove_queued_turn(&*state, &*state, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn reorder_turn_queue(
    state: State<'_, SessionCommandState>,
    input: ReorderTurnQueueInput,
) -> Result<Vec<QueuedTurn>, String> {
    run_reorder_turn_queue(&*state, &*state, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn dispatch_next_queued_turn(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<bool, String> {
    state
        .dispatch_next_queued_turn(&SessionId(session_id))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn abort_run(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: AbortRunInput,
) -> Result<AbortRunOutput, String> {
    let output = run_abort_run(&*state, &*state, &*state, input)
        .await
        .map_err(|error| error.to_string())?;
    let _ = state.cancel_provider_session(&output.session.id).await;
    Ok(output)
}

#[tauri::command]
pub async fn resume_session(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: ResumeSessionInput,
) -> Result<ResumeSessionOutput, String> {
    let output = run_resume_session(&*state, &*state, &*state, input)
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = state.attach_provider_session(&output.session).await {
        eprintln!("[DCC] provider session attach failed: {}", error);
    }
    Ok(output)
}

#[tauri::command]
pub async fn close_session(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: CloseSessionInput,
) -> Result<CloseSessionOutput, String> {
    let _ = state.cancel_provider_session(&input.session_id).await;
    run_close_session(&*state, &*state, &*state, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn restore_session(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: RestoreSessionInput,
) -> Result<RestoreSessionOutput, String> {
    run_restore_session(&*state, &*state, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_thread_events(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    session_id: String,
) -> Result<Vec<SessionEventRecord>, String> {
    let session_id = dcc_core::domain::session::SessionId(session_id);
    SessionEventRepo::list_events_by_session(&*state, &session_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_mcp_runtime_statuses(
    state: State<'_, SessionCommandState>,
    input: ListMcpRuntimeStatusesInput,
) -> Result<ListMcpRuntimeStatusesOutput, String> {
    let session_id = SessionId(input.session_id.trim().to_string());
    if session_id.0.is_empty() {
        return Err("sessionId is required".to_string());
    }
    if state
        .peek_session(&session_id)
        .await
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("session not found".to_string());
    }

    let statuses = state
        .list_mcp_runtime_statuses(&session_id)
        .map_err(|error| error.to_string())?;
    Ok(ListMcpRuntimeStatusesOutput { statuses })
}

#[tauri::command]
pub async fn prepare_turn(
    state: State<'_, SessionCommandState>,
    input: SendTurnInput,
) -> Result<PrepareTurnOutput, String> {
    let session = state
        .prepare_provider_session_for_turn(&input)
        .await
        .map_err(|error| error.to_string())?;
    if state
        .session_mcp_oauth_support(&session.id)
        .map_err(|error| error.to_string())?
        != McpOauthSupport::InteractivePreflight
    {
        return Ok(PrepareTurnOutput {
            preflight: McpTurnPreflightState::Ready,
        });
    }

    match wait_for_mcp_preflight(&state, &session.id).await? {
        McpPreflightReadiness::Ready => Ok(PrepareTurnOutput {
            preflight: McpTurnPreflightState::Ready,
        }),
        McpPreflightReadiness::AuthenticationRequired(definition_id) => {
            let result = state
                .start_mcp_oauth(&session.id, &definition_id)
                .await
                .map_err(|error| error.to_string())?;
            Ok(PrepareTurnOutput {
                preflight: McpTurnPreflightState::AuthenticationRequired {
                    definition_id,
                    authorization_url: result.authorization_url,
                },
            })
        }
        McpPreflightReadiness::Failed(message) => Err(message),
        McpPreflightReadiness::Attaching => unreachable!("bounded wait resolves attaching"),
    }
}

#[tauri::command]
pub async fn wait_mcp_oauth(
    state: State<'_, SessionCommandState>,
    input: WaitMcpOauthInput,
) -> Result<WaitMcpOauthOutput, String> {
    let session_id = SessionId(input.session_id.trim().to_string());
    let definition_id = McpDefinitionId(input.definition_id.trim().to_string());
    if session_id.0.is_empty() || definition_id.0.is_empty() {
        return Err("sessionId and definitionId are required".to_string());
    }
    if state
        .session_mcp_oauth_support(&session_id)
        .map_err(|error| error.to_string())?
        != McpOauthSupport::InteractivePreflight
    {
        return Err("provider does not expose interactive MCP OAuth preflight".to_string());
    }

    let deadline = Instant::now() + MCP_OAUTH_COMPLETION_TIMEOUT;
    loop {
        let statuses = state
            .list_mcp_runtime_statuses(&session_id)
            .map_err(|error| error.to_string())?;
        let status = statuses
            .iter()
            .find(|status| status.definition_id == definition_id)
            .ok_or_else(|| "MCP integration is no longer attached to this session".to_string())?;
        match status.state {
            McpRuntimeState::Connected => {
                return Ok(WaitMcpOauthOutput { connected: true });
            }
            McpRuntimeState::Failed
                if status
                    .bounded_error
                    .as_ref()
                    .is_some_and(|error| error.category == McpErrorCategory::Authentication) => {}
            McpRuntimeState::ProbingServer
            | McpRuntimeState::ServerReachable
            | McpRuntimeState::AttachingProvider => {}
            McpRuntimeState::Failed => {
                return Err(status
                    .bounded_error
                    .as_ref()
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| "MCP provider attachment failed".to_string()));
            }
            McpRuntimeState::Disabled => {
                return Err("MCP integration was disabled during authentication".to_string());
            }
            McpRuntimeState::NeedsTrust => {
                return Err(
                    "MCP integration requires trust approval during authentication".to_string(),
                );
            }
            McpRuntimeState::Unsupported => {
                return Err("MCP integration became unsupported during authentication".to_string());
            }
        }
        if Instant::now() >= deadline {
            return Err("MCP OAuth authentication timed out".to_string());
        }
        sleep(MCP_PREFLIGHT_POLL_INTERVAL).await;
    }
}

#[tauri::command]
pub async fn start_mcp_oauth(
    state: State<'_, SessionCommandState>,
    input: StartMcpOauthInput,
) -> Result<StartMcpOauthOutput, String> {
    let session_id = SessionId(input.session_id.trim().to_string());
    let definition_id = McpDefinitionId(input.definition_id.trim().to_string());
    if session_id.0.is_empty() || definition_id.0.is_empty() {
        return Err("sessionId and definitionId are required".to_string());
    }
    let status = state
        .list_mcp_runtime_statuses(&session_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|status| status.definition_id == definition_id)
        .ok_or_else(|| "MCP integration is not attached to this session".to_string())?;
    let requires_authentication = status.state == McpRuntimeState::Failed
        && status
            .bounded_error
            .as_ref()
            .is_some_and(|error| error.category == McpErrorCategory::Authentication);
    if !requires_authentication {
        return Err("MCP integration does not require authentication".to_string());
    }

    let result = state
        .start_mcp_oauth(&session_id, &definition_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(StartMcpOauthOutput {
        authorization_url: result.authorization_url,
    })
}

#[tauri::command]
pub async fn approve_plan(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: ApprovePlanInput,
) -> Result<ApprovePlanOutput, String> {
    run_approve_plan(&*state, &*state, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn record_plan_handoff(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: RecordPlanHandoffInput,
) -> Result<RecordPlanHandoffOutput, String> {
    run_record_plan_handoff(&*state, &*state, input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_workspace_sessions(
    state: State<'_, SessionCommandState>,
    _workspace_id: String,
) -> Result<Vec<WorkspaceSessionSummary>, String> {
    let workspace_id = dcc_core::domain::workspace::WorkspaceId(_workspace_id);
    state
        .list_workspace_sessions(&workspace_id)
        .map_err(|error| error.to_string())
}

fn normalize_task_title(title: &str) -> Result<String, String> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err("task title cannot be empty".to_string());
    }
    if title.chars().count() > 120 {
        return Err("task title cannot exceed 120 characters".to_string());
    }
    Ok(title)
}

fn workspace_accepts_automatic_title(workspace: &Workspace) -> bool {
    let Some(name) = workspace.name.as_deref().map(str::trim) else {
        return true;
    };
    name.is_empty()
        || name.eq_ignore_ascii_case("new task")
        || name.eq_ignore_ascii_case("nova tarefa")
}

#[tauri::command]
pub async fn apply_task_title(
    state: State<'_, SessionCommandState>,
    input: ApplyTaskTitleInput,
) -> Result<ApplyTaskTitleOutput, String> {
    apply_task_title_to_state(&state, input).await
}

async fn apply_task_title_to_state(
    state: &SessionCommandState,
    input: ApplyTaskTitleInput,
) -> Result<ApplyTaskTitleOutput, String> {
    let title = normalize_task_title(&input.title)?;

    let workspace_id = WorkspaceId(input.workspace_id);
    let session_id = SessionId(input.session_id);
    let session = SessionRepo::get_session(state, &session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("session not found: {}", session_id.0))?;
    if session.workspace_id != workspace_id {
        return Err("session does not belong to the requested workspace".to_string());
    }

    // Task names belong to the durable workspace store. The session command state
    // keeps a separate repository for sessions and threads, so open the workspace
    // repository explicitly instead of using its compatibility trait adapter.
    let workspace_repo =
        SqliteWorkspaceRepo::open(state.db_path()).map_err(|error| error.to_string())?;
    let mut workspace = WorkspaceRepo::get_workspace(&workspace_repo, &workspace_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("workspace not found: {}", workspace_id.0))?;
    let mut thread = ThreadRepo::find_thread_by_session_id(state, &session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("thread not found for session: {}", session_id.0))?;

    let applied = workspace_accepts_automatic_title(&workspace);
    if applied {
        workspace.name = Some(title.clone());
        workspace.updated_at = chrono::Utc::now().to_rfc3339();
        thread.title = title;
        // Persist the thread first. If the workspace write fails, the workspace
        // remains untitled and a later first-turn retry can safely finish both.
        ThreadRepo::save_thread(state, &thread)
            .await
            .map_err(|error| error.to_string())?;
        WorkspaceRepo::save_workspace(&workspace_repo, &workspace)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(ApplyTaskTitleOutput {
        applied,
        workspace,
        thread,
    })
}

#[tauri::command]
pub async fn search_sessions(
    state: State<'_, SessionCommandState>,
    input: SearchSessionsInput,
) -> Result<Vec<SessionSearchResult>, String> {
    state
        .search_sessions(&input.query, input.limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn respond_to_user_input(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: RespondToUserInputInput,
) -> Result<RespondToUserInputOutput, String> {
    let session_id = dcc_core::domain::session::SessionId(input.session_id);
    state
        .send_provider_input(
            &session_id,
            Input::UserInputResponse(ProviderUserInputResponse {
                request_id: input.request_id,
                answers: input.answers,
            }),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(RespondToUserInputOutput { ok: true })
}

#[tauri::command]
pub async fn respond_to_permission_request(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: RespondToPermissionRequestInput,
) -> Result<RespondToPermissionRequestOutput, String> {
    let session_id = dcc_core::domain::session::SessionId(input.session_id);
    state
        .send_provider_input(
            &session_id,
            Input::PermissionResponse(ProviderPermissionResponse {
                request_id: input.request_id,
                behavior: input.behavior,
            }),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(RespondToPermissionRequestOutput { ok: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::{provider::ProviderId, session::SessionId};

    fn status(
        definition_id: &str,
        state: McpRuntimeState,
        error: Option<(McpErrorCategory, &str)>,
    ) -> McpRuntimeStatus {
        McpRuntimeStatus {
            definition_id: McpDefinitionId(definition_id.to_string()),
            provider_id: ProviderId("codex".to_string()),
            provider_version: "codex@test".to_string(),
            session_id: SessionId("session-1".to_string()),
            state,
            tools: Vec::new(),
            checked_at: "2026-07-30T00:00:00Z".to_string(),
            bounded_error: error.map(|(category, message)| {
                dcc_core::domain::mcp::McpRuntimeError::bounded(category, message)
            }),
        }
    }

    #[test]
    fn automatic_title_normalizes_whitespace_and_rejects_empty_input() {
        assert_eq!(
            normalize_task_title("  Corrigir   login\n do checkout ").unwrap(),
            "Corrigir login do checkout"
        );
        assert!(normalize_task_title("   ").is_err());
    }

    #[test]
    fn automatic_title_never_overwrites_a_manual_workspace_name() {
        let mut workspace = Workspace {
            id: WorkspaceId("workspace-1".to_string()),
            project_id: dcc_core::domain::project::ProjectId("project-1".to_string()),
            name: None,
            root_path: "/tmp/project".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/worktree".to_string()),
            source: None,
            state: dcc_core::domain::workspace::WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-07-31T00:00:00Z".to_string(),
            updated_at: "2026-07-31T00:00:00Z".to_string(),
        };
        assert!(workspace_accepts_automatic_title(&workspace));

        workspace.name = Some("Nome escolhido".to_string());
        assert!(!workspace_accepts_automatic_title(&workspace));

        workspace.name = Some("Nova tarefa".to_string());
        assert!(workspace_accepts_automatic_title(&workspace));
    }

    #[tokio::test]
    async fn automatic_title_persists_in_the_workspace_repository() {
        let db_path = std::env::temp_dir().join(format!(
            "dcc-automatic-task-title-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let workspace_repo = SqliteWorkspaceRepo::open(&db_path).expect("open workspace repo");
        let workspace = Workspace {
            id: WorkspaceId("workspace-title-test".to_string()),
            project_id: dcc_core::domain::project::ProjectId("project-title-test".to_string()),
            name: Some("Nova tarefa".to_string()),
            root_path: "/tmp/project-title-test".to_string(),
            base_branch: "main".to_string(),
            worktree_path: Some("/tmp/worktree-title-test".to_string()),
            source: None,
            state: dcc_core::domain::workspace::WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-08-01T00:00:00Z".to_string(),
            updated_at: "2026-08-01T00:00:00Z".to_string(),
        };
        WorkspaceRepo::save_workspace(&workspace_repo, &workspace)
            .await
            .expect("save workspace");

        let state = SessionCommandState::new_headless(db_path.clone(), std::env::temp_dir());
        let started = run_start_thread(
            &state,
            &state,
            &state,
            &state,
            StartThreadInput {
                workspace_id: workspace.id.clone(),
                additional_workspace_ids: Vec::new(),
                project_id: workspace.project_id.clone(),
                provider_id: "codex".to_string(),
                model: None,
                provider_runtime: None,
                working_directory_override: None,
                title: Some("Nova tarefa".to_string()),
            },
        )
        .await
        .expect("start thread");

        let titled = apply_task_title_to_state(
            &state,
            ApplyTaskTitleInput {
                workspace_id: workspace.id.0.clone(),
                session_id: started.session.id.0,
                title: "Corrigir título da sidebar".to_string(),
            },
        )
        .await
        .expect("apply task title");
        assert!(titled.applied);

        let persisted = WorkspaceRepo::get_workspace(&workspace_repo, &workspace.id)
            .await
            .expect("read workspace")
            .expect("workspace exists");
        assert_eq!(
            persisted.name.as_deref(),
            Some("Corrigir título da sidebar")
        );

        drop(state);
        drop(workspace_repo);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn authentication_challenge_has_priority_over_other_runtime_failures() {
        let readiness = classify_mcp_preflight(&[
            status(
                "broken",
                McpRuntimeState::Failed,
                Some((McpErrorCategory::Protocol, "protocol failed")),
            ),
            status(
                "clickup",
                McpRuntimeState::Failed,
                Some((McpErrorCategory::Authentication, "authentication required")),
            ),
        ]);

        assert_eq!(
            readiness,
            McpPreflightReadiness::AuthenticationRequired(McpDefinitionId("clickup".to_string()))
        );
    }

    #[test]
    fn preflight_waits_for_transient_attachment_states() {
        for state in [
            McpRuntimeState::ProbingServer,
            McpRuntimeState::ServerReachable,
            McpRuntimeState::AttachingProvider,
        ] {
            assert_eq!(
                classify_mcp_preflight(&[status("clickup", state, None)]),
                McpPreflightReadiness::Attaching
            );
        }
    }

    #[test]
    fn preflight_is_ready_when_every_projected_server_is_connected() {
        assert_eq!(
            classify_mcp_preflight(&[
                status("clickup", McpRuntimeState::Connected, None),
                status("linear", McpRuntimeState::Connected, None),
            ]),
            McpPreflightReadiness::Ready
        );
    }
}
