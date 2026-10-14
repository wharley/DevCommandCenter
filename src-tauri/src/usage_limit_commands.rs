//! Wires the usage-limit resume scheduler (`dcc_tauri::usage_limit_resume`)
//! into the app: the global preference, each task's choice, the manual
//! "Continuar" of a task stopped by its limit, and the scheduler itself.
use std::sync::Arc;

use dcc_core::domain::session::SessionId;
use dcc_core::ports::EventBus;
use dcc_tauri::state::SessionCommandState;
use dcc_tauri::usage_limit_resume::{UsageLimitScheduler, UsageLimitSettings};
use tauri::State;

pub struct UsageLimitState {
    pub scheduler: Arc<UsageLimitScheduler>,
    // The runtime keeps only a weak reference to its buses.
    _bus: Arc<dyn EventBus>,
}

pub fn start(sessions: &SessionCommandState) -> UsageLimitState {
    let scheduler = UsageLimitScheduler::new(sessions.clone());
    let bus = scheduler.event_bus();
    if let Err(error) = sessions.register_event_bus(&bus) {
        eprintln!("[DCC][usage-limit] session events unavailable: {error}");
    }
    tauri::async_runtime::spawn(scheduler.clone().run());
    UsageLimitState {
        scheduler,
        _bus: bus,
    }
}

#[tauri::command]
pub fn usage_limit_settings(state: State<'_, SessionCommandState>) -> UsageLimitSettings {
    state.usage_limit_settings()
}

#[tauri::command]
pub fn usage_limit_set_settings(
    state: State<'_, SessionCommandState>,
    settings: UsageLimitSettings,
) -> Result<UsageLimitSettings, String> {
    state
        .set_usage_limit_settings(settings)
        .map_err(|error| error.to_string())
}

/// Whether this task continues by itself after a limit: its own choice, or
/// the global preference.
#[tauri::command]
pub fn session_usage_limit_auto_resume(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<bool, String> {
    state
        .session_auto_resume_enabled(&SessionId(session_id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn session_set_usage_limit_auto_resume(
    state: State<'_, SessionCommandState>,
    session_id: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .set_session_auto_resume(&SessionId(session_id), enabled)
        .await
        .map_err(|error| error.to_string())
}

/// "Continuar" / "Tentar de novo": sends the continuation turn now.
#[tauri::command]
pub async fn session_resume_after_usage_limit(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<String, String> {
    state
        .resume_after_usage_limit(&SessionId(session_id), false)
        .await
        .map(|turn_id| turn_id.0)
        .map_err(|error| error.to_string())
}

/// "Continuar com a conta …": moves the task to another account of its
/// provider (`None` = system login) and sends the continuation turn.
#[tauri::command]
pub async fn session_resume_after_usage_limit_on_account(
    state: State<'_, SessionCommandState>,
    session_id: String,
    account_id: Option<String>,
) -> Result<String, String> {
    state
        .resume_after_usage_limit_on_account(&SessionId(session_id), account_id)
        .await
        .map(|turn_id| turn_id.0)
        .map_err(|error| error.to_string())
}
