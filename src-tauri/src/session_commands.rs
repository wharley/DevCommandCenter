use tauri::{AppHandle, Manager, State};

use dcc_core::application::{
    AbortRunInput, AbortRunOutput, ApprovePlanInput, ApprovePlanOutput, CloseSessionInput,
    CloseSessionOutput, QueueTurnInput, RecordPlanHandoffInput, RecordPlanHandoffOutput,
    RemoveQueuedTurnInput, ReorderTurnQueueInput, RestoreSessionInput, RestoreSessionOutput,
    ResumeSessionInput, ResumeSessionOutput, SendTurnInput, SendTurnOutput, StartThreadInput,
    StartThreadOutput, SteerTurnInput, SteerTurnOutput,
};
use dcc_core::domain::session::{
    QueuedTurn, SessionEventRecord, SessionSearchResult, WorkspaceSessionSummary,
};
use dcc_core::domain::usage::{UsageDashboard, UsageDashboardInput};
use dcc_tauri::{
    commands::session_commands::{
        self as session_command_impl, AiMemoryOutboxStatusOutput, AiMemoryQueryHit,
        AiMemoryQueryInput, AiMemorySourceActionInput, AiMemorySourceActionOutput,
        AiMemorySyncInput, AiMemorySyncOutput, ApplyTaskTitleInput, ApplyTaskTitleOutput,
        ExecuteGuardedUndoInput, ExecuteGuardedUndoOutput, InheritSessionObjectiveInput,
        InterruptNativeSubagentInput, LastTurnReviewInput, ListMcpRuntimeStatusesInput,
        ListMcpRuntimeStatusesOutput, NativeSubagentControlOutput, PrepareGuardedUndoInput,
        PrepareGuardedUndoOutput, PrepareTurnOutput, RespondToPermissionRequestInput,
        RespondToPermissionRequestOutput, RespondToUserInputInput, RespondToUserInputOutput,
        RunPullRequestReviewAgentInput, RunPullRequestReviewAgentOutput, SearchSessionsInput,
        SessionLiveSnapshot, SessionObjectiveOutput, SetSessionObjectiveInput, StartMcpOauthInput,
        StartMcpOauthOutput, SteerNativeSubagentInput, TransitionSessionObjectiveInput,
        TurnReviewFileDiffInput, TurnReviewFileDiffOutput, TurnReviewSummary, WaitMcpOauthInput,
        WaitMcpOauthOutput,
    },
    state::SessionCommandState,
};
use dev_command_center_tauri::ai_memory_sidecar::{
    AiMemorySettingsInput, AiMemorySettingsOutput, AiMemorySidecarStatus,
};

#[tauri::command]
pub async fn prepare_guarded_undo(
    state: State<'_, SessionCommandState>,
    input: PrepareGuardedUndoInput,
) -> Result<PrepareGuardedUndoOutput, String> {
    session_command_impl::prepare_guarded_undo(state, input).await
}

#[tauri::command]
pub async fn execute_guarded_undo(
    state: State<'_, SessionCommandState>,
    input: ExecuteGuardedUndoInput,
) -> Result<ExecuteGuardedUndoOutput, String> {
    session_command_impl::execute_guarded_undo(state, input).await
}

#[tauri::command]
pub async fn start_thread(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: StartThreadInput,
) -> Result<StartThreadOutput, String> {
    session_command_impl::start_thread(state, app, input).await
}

#[tauri::command]
pub async fn run_pull_request_review_agent(
    state: State<'_, SessionCommandState>,
    input: RunPullRequestReviewAgentInput,
) -> Result<RunPullRequestReviewAgentOutput, String> {
    session_command_impl::run_pull_request_review_agent(state, input).await
}

#[tauri::command]
pub async fn sync_session_to_ai_memory(
    state: State<'_, SessionCommandState>,
    input: AiMemorySyncInput,
) -> Result<AiMemorySyncOutput, String> {
    session_command_impl::sync_session_to_ai_memory(state, input).await
}

#[tauri::command]
pub async fn query_ai_memory(
    state: State<'_, SessionCommandState>,
    input: AiMemoryQueryInput,
) -> Result<Vec<AiMemoryQueryHit>, String> {
    session_command_impl::query_ai_memory(state, input).await
}

#[tauri::command]
pub fn ai_memory_recovered_sources(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<Vec<AiMemoryQueryHit>, String> {
    session_command_impl::ai_memory_recovered_sources(state, session_id)
}

#[tauri::command]
pub fn ai_memory_source_actions(
    state: State<'_, SessionCommandState>,
    limit: Option<usize>,
) -> Result<Vec<AiMemorySourceActionOutput>, String> {
    session_command_impl::ai_memory_source_actions(state, limit)
}

#[tauri::command]
pub fn ai_memory_source_action_save(
    state: State<'_, SessionCommandState>,
    input: AiMemorySourceActionInput,
) -> Result<AiMemorySourceActionOutput, String> {
    session_command_impl::ai_memory_source_action_save(state, input)
}

#[tauri::command]
pub fn ai_memory_export_status(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<Option<AiMemoryOutboxStatusOutput>, String> {
    session_command_impl::ai_memory_export_status(state, session_id)
}

#[tauri::command]
pub async fn ai_memory_outbox_list(
    state: State<'_, SessionCommandState>,
    limit: Option<usize>,
) -> Result<Vec<AiMemoryOutboxStatusOutput>, String> {
    let entries = state
        .list_ai_memory_exports(limit.unwrap_or(50))
        .map_err(|error| error.to_string())?;
    let mut output = Vec::with_capacity(entries.len());
    for entry in entries {
        let event_count =
            dcc_core::ports::SessionEventRepo::list_events_by_session(&*state, &entry.session_id)
                .await
                .map(|events| events.len())
                .unwrap_or_default();
        output.push(AiMemoryOutboxStatusOutput {
            session_id: entry.session_id.0,
            attempts: entry.attempts,
            next_attempt_at: entry.next_attempt_at,
            last_error: entry.last_error,
            event_count,
        });
    }
    Ok(output)
}

#[tauri::command]
pub async fn ai_memory_outbox_retry(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<Option<AiMemoryOutboxStatusOutput>, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() || session_id.len() > 200 {
        return Err("sessionId is required".to_string());
    }
    let session_id = dcc_core::domain::session::SessionId(session_id.to_string());
    state
        .retry_ai_memory_export(&session_id)
        .await
        .map_err(|error| error.to_string())?;
    state
        .ai_memory_export_status(&session_id)
        .map(|status| {
            status.map(|entry| AiMemoryOutboxStatusOutput {
                session_id: entry.session_id.0,
                attempts: entry.attempts,
                next_attempt_at: entry.next_attempt_at,
                last_error: entry.last_error,
                event_count: 0,
            })
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn ai_memory_checkpoint(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<Option<AiMemoryOutboxStatusOutput>, String> {
    session_command_impl::ai_memory_checkpoint(state, session_id).await
}

#[tauri::command]
pub fn ai_memory_sidecar_status(
    sidecar: State<'_, dev_command_center_tauri::ai_memory_sidecar::AiMemorySidecar>,
) -> Result<AiMemorySidecarStatus, String> {
    Ok(sidecar.status())
}

#[tauri::command]
pub async fn ai_memory_settings_load(app: AppHandle) -> Result<AiMemorySettingsOutput, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    dev_command_center_tauri::ai_memory_sidecar::AiMemorySidecar::read_settings(&app_data_dir).await
}

#[tauri::command]
pub async fn ai_memory_settings_save(
    app: AppHandle,
    input: AiMemorySettingsInput,
) -> Result<AiMemorySettingsOutput, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    dev_command_center_tauri::ai_memory_sidecar::AiMemorySidecar::save_settings(
        &app_data_dir,
        input,
    )
    .await
}

#[tauri::command]
pub async fn apply_task_title(
    state: State<'_, SessionCommandState>,
    input: ApplyTaskTitleInput,
) -> Result<ApplyTaskTitleOutput, String> {
    session_command_impl::apply_task_title(state, input).await
}

#[tauri::command]
pub async fn send_turn(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: SendTurnInput,
) -> Result<SendTurnOutput, String> {
    session_command_impl::send_turn(state, app, input).await
}

#[tauri::command]
pub async fn steer_turn(
    state: State<'_, SessionCommandState>,
    input: SteerTurnInput,
) -> Result<SteerTurnOutput, String> {
    session_command_impl::steer_turn(state, input).await
}

#[tauri::command]
pub async fn steer_native_subagent(
    state: State<'_, SessionCommandState>,
    input: SteerNativeSubagentInput,
) -> Result<NativeSubagentControlOutput, String> {
    session_command_impl::steer_native_subagent(state, input).await
}

#[tauri::command]
pub async fn interrupt_native_subagent(
    state: State<'_, SessionCommandState>,
    input: InterruptNativeSubagentInput,
) -> Result<NativeSubagentControlOutput, String> {
    session_command_impl::interrupt_native_subagent(state, input).await
}

#[tauri::command]
pub async fn queue_turn(
    state: State<'_, SessionCommandState>,
    input: QueueTurnInput,
) -> Result<QueuedTurn, String> {
    session_command_impl::queue_turn(state, input).await
}

#[tauri::command]
pub async fn list_turn_queue(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<Vec<QueuedTurn>, String> {
    session_command_impl::list_turn_queue(state, session_id).await
}

#[tauri::command]
pub async fn get_session_objective(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<SessionObjectiveOutput, String> {
    session_command_impl::get_session_objective(state, session_id).await
}

#[tauri::command]
pub async fn set_session_objective(
    state: State<'_, SessionCommandState>,
    input: SetSessionObjectiveInput,
) -> Result<SessionObjectiveOutput, String> {
    session_command_impl::set_session_objective(state, input).await
}

#[tauri::command]
pub async fn transition_session_objective(
    state: State<'_, SessionCommandState>,
    input: TransitionSessionObjectiveInput,
) -> Result<SessionObjectiveOutput, String> {
    session_command_impl::transition_session_objective(state, input).await
}

#[tauri::command]
pub async fn inherit_session_objective(
    state: State<'_, SessionCommandState>,
    input: InheritSessionObjectiveInput,
) -> Result<SessionObjectiveOutput, String> {
    session_command_impl::inherit_session_objective(state, input).await
}

#[tauri::command]
pub async fn clear_session_objective(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<SessionObjectiveOutput, String> {
    session_command_impl::clear_session_objective(state, session_id).await
}

#[tauri::command]
pub async fn remove_queued_turn(
    state: State<'_, SessionCommandState>,
    input: RemoveQueuedTurnInput,
) -> Result<Vec<QueuedTurn>, String> {
    session_command_impl::remove_queued_turn(state, input).await
}

#[tauri::command]
pub async fn reorder_turn_queue(
    state: State<'_, SessionCommandState>,
    input: ReorderTurnQueueInput,
) -> Result<Vec<QueuedTurn>, String> {
    session_command_impl::reorder_turn_queue(state, input).await
}

#[tauri::command]
pub async fn dispatch_next_queued_turn(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<bool, String> {
    session_command_impl::dispatch_next_queued_turn(state, session_id).await
}

#[tauri::command]
pub async fn prepare_turn(
    state: State<'_, SessionCommandState>,
    input: SendTurnInput,
) -> Result<PrepareTurnOutput, String> {
    session_command_impl::prepare_turn(state, input).await
}

#[tauri::command]
pub async fn wait_mcp_oauth(
    state: State<'_, SessionCommandState>,
    input: WaitMcpOauthInput,
) -> Result<WaitMcpOauthOutput, String> {
    session_command_impl::wait_mcp_oauth(state, input).await
}

#[tauri::command]
pub async fn abort_run(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: AbortRunInput,
) -> Result<AbortRunOutput, String> {
    session_command_impl::abort_run(state, app, input).await
}

#[tauri::command]
pub async fn resume_session(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: ResumeSessionInput,
) -> Result<ResumeSessionOutput, String> {
    session_command_impl::resume_session(state, app, input).await
}

#[tauri::command]
pub async fn close_session(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: CloseSessionInput,
) -> Result<CloseSessionOutput, String> {
    session_command_impl::close_session(state, app, input).await
}

#[tauri::command]
pub async fn restore_session(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: RestoreSessionInput,
) -> Result<RestoreSessionOutput, String> {
    session_command_impl::restore_session(state, app, input).await
}

#[tauri::command]
pub async fn list_thread_events(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    session_id: String,
) -> Result<Vec<SessionEventRecord>, String> {
    session_command_impl::list_thread_events(state, app, session_id).await
}

#[tauri::command]
pub async fn session_live_snapshot(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<SessionLiveSnapshot, String> {
    session_command_impl::session_live_snapshot(state, session_id).await
}

#[tauri::command]
pub async fn last_turn_review(
    state: State<'_, SessionCommandState>,
    input: LastTurnReviewInput,
) -> Result<Option<TurnReviewSummary>, String> {
    session_command_impl::last_turn_review(state, input).await
}

#[tauri::command]
pub async fn turn_review_file_diff(
    state: State<'_, SessionCommandState>,
    input: TurnReviewFileDiffInput,
) -> Result<TurnReviewFileDiffOutput, String> {
    session_command_impl::turn_review_file_diff(state, input).await
}

#[tauri::command]
pub async fn list_mcp_runtime_statuses(
    state: State<'_, SessionCommandState>,
    input: ListMcpRuntimeStatusesInput,
) -> Result<ListMcpRuntimeStatusesOutput, String> {
    session_command_impl::list_mcp_runtime_statuses(state, input).await
}

#[tauri::command]
pub async fn start_mcp_oauth(
    state: State<'_, SessionCommandState>,
    input: StartMcpOauthInput,
) -> Result<StartMcpOauthOutput, String> {
    session_command_impl::start_mcp_oauth(state, input).await
}

#[tauri::command]
pub async fn approve_plan(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: ApprovePlanInput,
) -> Result<ApprovePlanOutput, String> {
    session_command_impl::approve_plan(state, app, input).await
}

#[tauri::command]
pub async fn record_plan_handoff(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: RecordPlanHandoffInput,
) -> Result<RecordPlanHandoffOutput, String> {
    session_command_impl::record_plan_handoff(state, app, input).await
}

#[tauri::command]
pub async fn list_workspace_sessions(
    state: State<'_, SessionCommandState>,
    workspace_id: String,
) -> Result<Vec<WorkspaceSessionSummary>, String> {
    session_command_impl::list_workspace_sessions(state, workspace_id).await
}

#[tauri::command]
pub async fn search_sessions(
    state: State<'_, SessionCommandState>,
    input: SearchSessionsInput,
) -> Result<Vec<SessionSearchResult>, String> {
    session_command_impl::search_sessions(state, input).await
}

#[tauri::command]
pub async fn usage_dashboard(
    state: State<'_, SessionCommandState>,
    input: UsageDashboardInput,
) -> Result<UsageDashboard, String> {
    session_command_impl::usage_dashboard(state, input).await
}

#[tauri::command]
pub async fn respond_to_user_input(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: RespondToUserInputInput,
) -> Result<RespondToUserInputOutput, String> {
    session_command_impl::respond_to_user_input(state, app, input).await
}

#[tauri::command]
pub async fn respond_to_permission_request(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: RespondToPermissionRequestInput,
) -> Result<RespondToPermissionRequestOutput, String> {
    session_command_impl::respond_to_permission_request(state, app, input).await
}
