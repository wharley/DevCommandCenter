use tauri::{AppHandle, State};

use dcc_core::application::{
    AbortRunInput, AbortRunOutput, ResumeSessionInput, ResumeSessionOutput, SendTurnInput,
    SendTurnOutput, StartThreadInput, StartThreadOutput,
};
use dcc_core::domain::session::{SessionEventRecord, WorkspaceSessionSummary};
use dcc_tauri::{
    commands::session_commands::{
        self as session_command_impl, RespondToPermissionRequestInput,
        RespondToPermissionRequestOutput, RespondToUserInputInput, RespondToUserInputOutput,
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
pub async fn list_thread_events(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    session_id: String,
) -> Result<Vec<SessionEventRecord>, String> {
    session_command_impl::list_thread_events(state, app, session_id).await
}

#[tauri::command]
pub async fn list_workspace_sessions(
    state: State<'_, SessionCommandState>,
    workspace_id: String,
) -> Result<Vec<WorkspaceSessionSummary>, String> {
    session_command_impl::list_workspace_sessions(state, workspace_id).await
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
