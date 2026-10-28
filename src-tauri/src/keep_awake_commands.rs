//! The keep-awake commands for the React app; the controller is the host's
//! (`dcc_server::host::keep_awake`).
use dev_command_center_tauri::keep_awake::{KeepAwakeMode, KeepAwakeStatus};
use tauri::State;

pub use dev_command_center_tauri::host::keep_awake::{KeepAwakeState, CHANGED_EVENT};

#[tauri::command]
pub fn keep_awake_status(state: State<'_, KeepAwakeState>) -> KeepAwakeStatus {
    state.controller.status()
}

#[tauri::command]
pub fn keep_awake_set_mode(
    state: State<'_, KeepAwakeState>,
    mode: KeepAwakeMode,
) -> Result<KeepAwakeStatus, String> {
    state.controller.set_mode(mode)
}
