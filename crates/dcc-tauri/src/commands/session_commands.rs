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
	run_start_thread(&*state, &*state, &*state, &*state, input)
		.await
		.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn send_turn(
	state: State<'_, SessionCommandState>,
	_app: AppHandle,
	input: SendTurnInput,
) -> Result<SendTurnOutput, String> {
	run_send_turn(&*state, &*state, &*state, input)
		.await
		.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn abort_run(
	state: State<'_, SessionCommandState>,
	_app: AppHandle,
	input: AbortRunInput,
) -> Result<AbortRunOutput, String> {
	run_abort_run(&*state, &*state, &*state, input)
		.await
		.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resume_session(
	state: State<'_, SessionCommandState>,
	_app: AppHandle,
	input: ResumeSessionInput,
) -> Result<ResumeSessionOutput, String> {
	run_resume_session(&*state, &*state, &*state, input)
		.await
		.map_err(|error| error.to_string())
}
