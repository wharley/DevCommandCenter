//! Native quick-entry surface and a durable, compare-and-swap launch journal.
//! The renderer uses the existing workspace/session commands; ambiguous launches
//! remain reviewable and are never automatically replayed after a restart.
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State, Webview};

pub use dev_command_center_tauri::quick_composer_journal::{
    begin, checkpoint, status, Launch, QuickComposerState, Status,
};

pub const LABEL: &str = "quick-composer";

fn trusted(window: &Webview) -> Result<(), String> {
    if matches!(window.label(), "main" | LABEL) {
        Ok(())
    } else {
        Err("unavailable".into())
    }
}
fn panel_only(window: &Webview) -> Result<(), String> {
    if window.label() == LABEL {
        Ok(())
    } else {
        Err("unavailable".into())
    }
}

#[tauri::command]
pub fn quick_composer_status(
    window: Webview,
    state: State<QuickComposerState>,
) -> Result<Status, String> {
    trusted(&window)?;
    status(&state)
}

#[tauri::command]
pub fn quick_composer_begin(
    window: Webview,
    state: State<QuickComposerState>,
    launch: Launch,
) -> Result<Launch, String> {
    panel_only(&window)?;
    begin(&state, launch)
}

/// `quick_composer_begin` without the window check (the native app's quick
/// composer, over the loopback API).
#[tauri::command]
pub fn quick_composer_checkpoint(
    window: Webview,
    state: State<QuickComposerState>,
    launch: Launch,
) -> Result<Launch, String> {
    panel_only(&window)?;
    checkpoint(&state, launch)
}

/// `quick_composer_checkpoint` without the window check.
#[tauri::command]
pub fn quick_composer_set_shortcut(
    window: Webview,
    app: AppHandle,
    shortcut: Option<String>,
) -> Result<(), String> {
    trusted(&window)?;
    set_shortcut(&app, shortcut)
}

/// `quick_composer_set_shortcut` without the window check.
pub fn set_shortcut(app: &AppHandle, shortcut: Option<String>) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, shortcut);
        Err("unsupported".into())
    }
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};
        let key = shortcut
            .as_deref()
            .map(str::parse::<Shortcut>)
            .transpose()
            .map_err(|_| "shortcutInvalid")?;
        if key.as_ref().is_some_and(|k| {
            !k.mods
                .intersects(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT)
        }) {
            return Err("shortcutInvalid".into());
        }
        let state = app.state::<QuickComposerState>();
        let mut saved = state.saved.lock().map_err(|e| e.to_string())?;
        let old = saved
            .shortcut
            .as_deref()
            .and_then(|s| s.parse::<Shortcut>().ok());
        if key == old && !*state.shortcut_error.lock().map_err(|e| e.to_string())? {
            return Ok(());
        }
        // Never claim an already registered Appshots key, nor unregister it.
        if let Some(key) = key {
            if app.global_shortcut().is_registered(key) {
                return Err("shortcutUnavailable".into());
            }
            app.global_shortcut()
                .register(key)
                .map_err(|_| "shortcutUnavailable")?;
        }
        let mut next = saved.clone();
        next.shortcut = shortcut;
        if let Err(error) = state.persist(&next) {
            if let Some(key) = key {
                let _ = app.global_shortcut().unregister(key);
            }
            return Err(error);
        }
        let old_was_registered = !*state.shortcut_error.lock().map_err(|e| e.to_string())?;
        if old_was_registered {
            if let Some(old) = old {
                if app.global_shortcut().unregister(old).is_err() {
                    if let Some(key) = key {
                        let _ = app.global_shortcut().unregister(key);
                    }
                    state.persist(&saved)?;
                    return Err("shortcutUnavailable".into());
                }
            }
        }
        *saved = next;
        *state.shortcut_error.lock().map_err(|e| e.to_string())? = false;
        Ok(())
    }
}

#[tauri::command]
pub fn quick_composer_toggle(window: Webview, app: AppHandle) -> Result<(), String> {
    trusted(&window)?;
    toggle(&app)
}
/// On this branch the native app draws the quick composer: the key and
/// "Abrir" reach it while it listens.
pub fn toggle(app: &AppHandle) -> Result<(), String> {
    let _ = app;
    if dev_command_center_tauri::http_api::native_quick_composer().toggle() {
        Ok(())
    } else {
        Err("unavailable".into())
    }
}
#[tauri::command]
pub fn quick_composer_hide(window: Webview, app: AppHandle) -> Result<(), String> {
    panel_only(&window)?;
    #[cfg(target_os = "macos")]
    app.run_on_main_thread(|| unsafe { native::dcc_quick_composer_hide() })
        .map_err(|e| e.to_string())?;
    let _ = app;
    Ok(())
}
#[tauri::command]
pub fn quick_composer_open_main(window: Webview, app: AppHandle) -> Result<(), String> {
    panel_only(&window)?;
    let launch = app
        .state::<QuickComposerState>()
        .saved
        .lock()
        .map_err(|e| e.to_string())?
        .launch
        .clone();
    if let Some(main) = app.get_webview_window("main") {
        main.show().map_err(|e| e.to_string())?;
        main.unminimize().map_err(|e| e.to_string())?;
        main.set_focus().map_err(|e| e.to_string())?;
        main.emit("quick-composer-open-task", launch)
            .map_err(|e| e.to_string())?;
    }
    quick_composer_hide(window, app)
}

#[cfg(target_os = "macos")]
pub fn handles_shortcut(
    app: &AppHandle,
    shortcut: &tauri_plugin_global_shortcut::Shortcut,
) -> bool {
    app.try_state::<QuickComposerState>().is_some_and(|state| {
        let Ok(saved) = state.saved.lock() else {
            return false;
        };
        let Ok(error) = state.shortcut_error.lock() else {
            return false;
        };
        !*error
            && saved
                .shortcut
                .as_deref()
                .and_then(|s| s.parse::<tauri_plugin_global_shortcut::Shortcut>().ok())
                .as_ref()
                == Some(shortcut)
    })
}

pub fn setup(app: &AppHandle, app_data: PathBuf) -> Result<(), String> {
    app.manage(QuickComposerState::open(&app_data)?);
    // The panel itself is the native app's on this branch; the desktop keeps
    // the key and the journal.
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let state = app.state::<QuickComposerState>();
        let saved = state.saved.lock().map_err(|e| e.to_string())?;
        if let Some(shortcut) = &saved.shortcut {
            *state.shortcut_error.lock().map_err(|e| e.to_string())? =
                app.global_shortcut().register(shortcut.as_str()).is_err();
        }
    }
    Ok(())
}

pub fn shutdown(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    unsafe {
        native::dcc_quick_composer_destroy();
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}
#[cfg(target_os = "macos")]
mod native {
    extern "C" {
        pub fn dcc_quick_composer_hide();
        pub fn dcc_quick_composer_destroy();
    }
}
