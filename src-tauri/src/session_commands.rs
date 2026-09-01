use tauri::{AppHandle, State};

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
        self as session_command_impl, ApplyTaskTitleInput, ApplyTaskTitleOutput,
        ExecuteGuardedUndoInput, ExecuteGuardedUndoOutput, InterruptNativeSubagentInput,
        LastTurnReviewInput, ListMcpRuntimeStatusesInput, ListMcpRuntimeStatusesOutput,
        NativeSubagentControlOutput, PrepareGuardedUndoInput, PrepareGuardedUndoOutput,
        PrepareTurnOutput, RespondToPermissionRequestInput, RespondToPermissionRequestOutput,
        RespondToUserInputInput, RespondToUserInputOutput, RunPullRequestReviewAgentInput,
        RunPullRequestReviewAgentOutput, SearchSessionsInput, SessionLiveSnapshot,
        StartMcpOauthInput, StartMcpOauthOutput, SteerNativeSubagentInput, TurnReviewFileDiffInput,
        TurnReviewFileDiffOutput, TurnReviewSummary, WaitMcpOauthInput, WaitMcpOauthOutput,
    },
    state::SessionCommandState,
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
