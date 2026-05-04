use tauri::{AppHandle, State};

use dcc_core::{
    application::{
        abort_run as run_abort_run, compose_wire_prompt_for_provider,
        resume_session as run_resume_session, send_turn as run_send_turn,
        send_turn_selection_differs_from_session, start_thread as run_start_thread, AbortRunInput,
        AbortRunOutput, ResumeSessionInput, ResumeSessionOutput, SendTurnInput, SendTurnOutput,
        StartThreadInput, StartThreadOutput,
    },
    domain::session::{SessionEventRecord, WorkspaceSessionSummary},
    ports::SessionEventRepo,
};

use crate::state::SessionCommandState;

#[tauri::command]
pub async fn start_thread(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: StartThreadInput,
) -> Result<StartThreadOutput, String> {
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
        .map_err(|error| error.to_string())?
    {
        if send_turn_selection_differs_from_session(&session, &input) {
            let _ = state.cancel_provider_session(&input.session_id).await;
        }
    }

    let prompt_for_wire = input.prompt.clone();
    let plan_mode = input.plan_mode;
    let effort = input.effort.clone();
    let fast_mode = input.fast_mode;
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

    let wire_prompt = compose_wire_prompt_for_provider(
        &output.session.provider_id,
        &prompt_for_wire,
        plan_mode,
        effort.as_deref(),
        fast_mode,
    );

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
        .send_provider_input(&output.session.id, wire_prompt)
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
pub async fn list_workspace_sessions(
    state: State<'_, SessionCommandState>,
    _workspace_id: String,
) -> Result<Vec<WorkspaceSessionSummary>, String> {
    let workspace_id = dcc_core::domain::workspace::WorkspaceId(_workspace_id);
    state
        .list_workspace_sessions(&workspace_id)
        .map_err(|error| error.to_string())
}
