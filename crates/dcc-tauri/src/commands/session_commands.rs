use tauri::{AppHandle, State};

use dcc_core::{
	application::{
		abort_run as run_abort_run,
		compose_wire_prompt_for_provider,
		resume_session as run_resume_session,
		send_turn as run_send_turn, send_turn_selection_differs_from_session,
		start_thread as run_start_thread, AbortRunInput, AbortRunOutput, ResumeSessionInput,
		ResumeSessionOutput, SendTurnInput, SendTurnOutput, StartThreadInput, StartThreadOutput,
	},
	domain::session::SessionEventRecord,
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
	let wire_prompt = compose_wire_prompt_for_provider(
		&output.session.provider_id,
		&prompt_for_wire,
		plan_mode,
		effort.as_deref(),
		fast_mode,
	);
	state
		.attach_provider_session(&output.session)
		.await
		.map_err(|error| error.to_string())?;
	state
		.set_active_turn(&output.session.id, Some(output.turn.id.0.clone()))
		.await
		.map_err(|error| error.to_string())?;
	if let Err(error) = state
		.send_provider_input(&output.session.id, wire_prompt)
		.await
	{
		let _ = state.set_active_turn(&output.session.id, None).await;
		return Err(error.to_string());
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
