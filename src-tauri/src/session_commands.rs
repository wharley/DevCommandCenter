use tauri::{AppHandle, State};

use dcc_core::application::{
    AbortRunInput, AbortRunOutput, ApprovePlanInput, ApprovePlanOutput, CloseSessionInput,
    CloseSessionOutput, RecordPlanHandoffInput, RecordPlanHandoffOutput, RestoreSessionInput,
    RestoreSessionOutput, ResumeSessionInput, ResumeSessionOutput, SendTurnInput, SendTurnOutput,
    StartThreadInput, StartThreadOutput,
};
use dcc_core::domain::session::{SessionEventRecord, SessionSearchResult, WorkspaceSessionSummary};
use dcc_tauri::{
    commands::session_commands::{
        self as session_command_impl, ListMcpRuntimeStatusesInput, ListMcpRuntimeStatusesOutput,
        RespondToPermissionRequestInput, RespondToPermissionRequestOutput, RespondToUserInputInput,
        RespondToUserInputOutput, SearchSessionsInput, StartMcpOauthInput, StartMcpOauthOutput,
    },
    state::SessionCommandState,
};

#[tauri::command]
pub async fn start_thread(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: StartThreadInput,
) -> Result<StartThreadOutput, String> {
    session_command_impl::start_thread(state, app, input).await
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
