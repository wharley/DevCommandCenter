use tauri::{AppHandle, State};

use dcc_core::{
	application::{
		abort_run as run_abort_run, resume_session as run_resume_session,
		send_turn as run_send_turn, start_thread as run_start_thread, AbortRunInput,
		AbortRunOutput, ResumeSessionInput, ResumeSessionOutput, SendTurnInput,
		SendTurnOutput, StartThreadInput, StartThreadOutput,
	},
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
	let prompt = input.prompt.clone();
	let output = run_send_turn(&*state, &*state, &*state, input)
		.await
		.map_err(|error| error.to_string())?;
	state
		.attach_provider_session(&output.session)
		.await
		.map_err(|error| error.to_string())?;
	state
		.set_active_turn(&output.session.id, Some(output.turn.id.0.clone()))
		.await
		.map_err(|error| error.to_string())?;
	if let Err(error) = state
		.send_provider_input(&output.session.id, prompt)
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
