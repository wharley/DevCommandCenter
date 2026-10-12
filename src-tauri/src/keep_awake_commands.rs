//! Wires the keep-awake controller (`dev_command_center_tauri::keep_awake`)
//! into the app: settings commands, the session event bus and the menu bar.
use std::path::Path;
use std::sync::Arc;

use dcc_core::ports::EventBus;
use dcc_tauri::state::SessionCommandState;
use dev_command_center_tauri::keep_awake::{
    self, HoldReason, KeepAwake, KeepAwakeMode, KeepAwakeStatus, TurnSource,
};
use tauri::{AppHandle, Emitter, State};

/// Tells every window the hold changed; they read `keep_awake_status`.
pub const CHANGED_EVENT: &str = "keep-awake-changed";

pub struct KeepAwakeState {
    pub controller: Arc<KeepAwake>,
    // The runtime keeps only a weak reference to its buses.
    _bus: Arc<dyn EventBus>,
}

pub fn start(
    app: &AppHandle,
    sessions: &SessionCommandState,
    app_data_dir: &Path,
) -> KeepAwakeState {
    let controller = KeepAwake::new(
        keep_awake::system_power(),
        Some(app_data_dir.to_path_buf()),
        keep_awake::load_mode(app_data_dir),
        keep_awake::HAND_OFF_GRACE,
    );
    let handle = app.clone();
    controller.set_listener(Box::new(move |reason| {
        crate::menu_bar::set_keep_awake(&handle, reason.map(HoldReason::label));
        let _ = handle.emit(CHANGED_EVENT, ());
    }));
    let bus = controller.event_bus();
    if let Err(error) = sessions.register_event_bus(&bus) {
        eprintln!("[DCC][keep-awake] session events unavailable: {error}");
    }
    let source: Arc<dyn TurnSource> = Arc::new(sessions.clone());
    tauri::async_runtime::spawn(controller.clone().run(source));
    KeepAwakeState {
        controller,
        _bus: bus,
    }
}

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
