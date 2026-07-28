use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};

use dcc_core::{
    application::{
        abort_run as run_abort_run, approve_plan as run_approve_plan,
        close_session as run_close_session, record_plan_handoff as run_record_plan_handoff,
        restore_session as run_restore_session, resume_session as run_resume_session,
        send_turn as run_send_turn, send_turn_selection_differs_from_session,
        start_thread as run_start_thread, AbortRunInput, AbortRunOutput, ApprovePlanInput,
        ApprovePlanOutput, CloseSessionInput, CloseSessionOutput, RecordPlanHandoffInput,
        RecordPlanHandoffOutput, RestoreSessionInput, RestoreSessionOutput, ResumeSessionInput,
        ResumeSessionOutput, SendTurnInput, SendTurnOutput, StartThreadInput, StartThreadOutput,
    },
    domain::{
        mcp::McpRuntimeStatus,
        session::{SessionEventRecord, SessionId, SessionSearchResult, WorkspaceSessionSummary},
    },
    ports::{
        provider::ProviderPermissionResponse, provider::ProviderUserInputAnswer,
        provider::ProviderUserInputResponse, Input, ProviderTurnInput, SessionEventRepo,
    },
};

use crate::state::SessionCommandState;

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
    if let Some(session) = state
        .peek_session(&input.session_id)
        .await
        .map_err(|error| error.to_string())?
    {
        if send_turn_selection_differs_from_session(&session, &input) {
            let _ = state.cancel_provider_session(&input.session_id).await;
        }
    }

    let provider_turn_input = ProviderTurnInput {
        prompt: input.prompt.clone(),
        tool_instructions: input.tool_instructions.clone(),
        plan_mode: input.plan_mode,
        effort: input.effort.clone(),
        fast_mode: input.fast_mode,
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

    if let Err(error) = state.attach_provider_session(&output.session).await {
        return Err(abort_turn(error.to_string()).await);
    }

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
