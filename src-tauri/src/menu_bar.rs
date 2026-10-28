//! The menu bar on this branch: the native app draws the item and its panel
//! (the snapshot is `dcc_server::menu_bar_snapshot`); the desktop keeps
//! hiding its window on close, showing it again, and the ☕ hand-off.
use tauri::{AppHandle, Manager};

pub const LABEL: &str = "menu-bar";

/// Present while this app keeps the menu bar behaviour: closing its window
/// hides it (the native app draws the item and keeps using this backend).
pub struct MenuBarState;

pub fn show_main(app: &AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or("Main window unavailable")?;
    main.show().map_err(|e| e.to_string())?;
    main.unminimize().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())
}
pub fn setup(app: &AppHandle) -> Result<(), String> {
    // On this branch the native app owns the menu bar item and its panel
    // (the snapshot is the host's); the desktop keeps hiding its window on
    // close so the native app keeps its backend.
    #[cfg(target_os = "macos")]
    app.manage(MenuBarState);
    let _ = app;
    Ok(())
}
/// The ☕ next to the menu bar icon while DCC keeps the Mac awake; the
/// tooltip says why. Safe before `setup`: the native side applies it when
/// the item is created.
pub fn set_keep_awake(app: &AppHandle, reason: Option<String>) {
    #[cfg(target_os = "macos")]
    {
        let tooltip = reason.and_then(|reason| {
            std::ffi::CString::new(format!("Dev Command Center · Mac acordado: {reason}")).ok()
        });
        let _ = app.run_on_main_thread(move || unsafe {
            native::dcc_menu_bar_set_keep_awake(
                tooltip.as_ref().map_or(std::ptr::null(), |tooltip| tooltip.as_ptr()),
            );
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, reason);
}
pub fn shutdown(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    unsafe {
        native::dcc_menu_bar_destroy();
    }
    if let Some(panel) = app.get_webview_window(LABEL) {
        let _ = panel.destroy();
    }
}
#[cfg(target_os = "macos")]
mod native {
    extern "C" {
        pub fn dcc_menu_bar_destroy();
        pub fn dcc_menu_bar_set_keep_awake(tooltip: *const std::ffi::c_char);
    }
}
