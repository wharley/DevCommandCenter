//! Native quick-entry surface and a durable, compare-and-swap launch journal.
//! The renderer uses the existing workspace/session commands; ambiguous launches
//! remain reviewable and are never automatically replayed after a restart.
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, State, Webview};

pub const LABEL: &str = "quick-composer";
const DEFAULT_SHORTCUT: &str = "Super+Shift+KeyC";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub id: String,
    pub revision: u32,
    pub phase: String,
    pub request: serde_json::Value,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    shortcut: Option<String>,
    launch: Option<Launch>,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            shortcut: Some(DEFAULT_SHORTCUT.into()),
            launch: None,
        }
    }
}
pub struct QuickComposerState {
    root: PathBuf,
    saved: Mutex<Saved>,
    shortcut_error: Mutex<bool>,
}
impl QuickComposerState {
    fn persist(&self, saved: &Saved) -> Result<(), String> {
        let bytes = serde_json::to_vec(saved).map_err(|e| e.to_string())?;
        let temporary = self.root.join("state.tmp");
        fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        // Sync before publication: no creation/send can precede this checkpoint.
        fs::File::open(&temporary)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(temporary, self.root.join("state.json")).map_err(|e| e.to_string())
    }
}

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    supported: bool,
    shortcut: Option<String>,
    shortcut_error: bool,
    launch: Option<Launch>,
}
#[tauri::command]
pub fn quick_composer_status(
    window: Webview,
    state: State<QuickComposerState>,
) -> Result<Status, String> {
    trusted(&window)?;
    let saved = state.saved.lock().map_err(|e| e.to_string())?;
    Ok(Status {
        supported: cfg!(target_os = "macos"),
        shortcut: saved.shortcut.clone(),
        shortcut_error: *state.shortcut_error.lock().map_err(|e| e.to_string())?,
        launch: saved.launch.clone(),
    })
}

fn validate_launch(launch: &Launch) -> Result<(), String> {
    if uuid::Uuid::parse_str(&launch.id).is_err()
        || launch.revision != 0
        || launch.phase != "pending"
        || launch.workspace_id.is_some()
        || launch.session_id.is_some()
        || launch.error.is_some()
    {
        return Err("Invalid launch".into());
    }
    let prompt = launch
        .request
        .get("turn")
        .and_then(|v| v.get("rawPrompt"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if prompt.trim().is_empty() || prompt.len() > 256_000 {
        return Err("Invalid prompt".into());
    }
    if !matches!(
        launch.request.get("isolationMode").and_then(|v| v.as_str()),
        Some("localDirect" | "protectedWorktree")
    ) {
        return Err("Invalid environment".into());
    }
    if serde_json::to_vec(&launch.request)
        .map_err(|e| e.to_string())?
        .len()
        > 300_000
    {
        return Err("Launch too large".into());
    }
    Ok(())
}

#[tauri::command]
pub fn quick_composer_begin(
    window: Webview,
    state: State<QuickComposerState>,
    launch: Launch,
) -> Result<Launch, String> {
    panel_only(&window)?;
    validate_launch(&launch)?;
    let mut saved = state.saved.lock().map_err(|e| e.to_string())?;
    if let Some(previous) = &saved.launch {
        if previous.id == launch.id {
            return Ok(previous.clone());
        }
        if !matches!(previous.phase.as_str(), "completed" | "failed") {
            return Err("A launch is already pending".into());
        }
        // Keep the original prompt and partial task links available for recovery.
        fs::write(
            state.root.join(format!("{}.json", previous.id)),
            serde_json::to_vec(previous).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    let mut next = saved.clone();
    next.launch = Some(launch.clone());
    state.persist(&next)?;
    *saved = next;
    Ok(launch)
}

fn transition(current: &Launch, next: &Launch) -> Result<(), String> {
    if current.id != next.id
        || next.revision != current.revision + 1
        || current.request != next.request
    {
        return Err("Stale launch".into());
    }
    let allowed = matches!(
        (current.phase.as_str(), next.phase.as_str()),
        ("pending", "creating")
            | ("creating", "workspaceReady")
            | ("workspaceReady", "starting")
            | ("starting", "sessionReady")
            | ("sessionReady", "sending")
            | ("sending", "completed")
    );
    if (!allowed && next.phase != "failed")
        || matches!(current.phase.as_str(), "completed" | "failed")
    {
        return Err("Invalid launch transition".into());
    }
    if current.workspace_id.is_some() && current.workspace_id != next.workspace_id
        || current.session_id.is_some() && current.session_id != next.session_id
    {
        return Err("Launch scope changed".into());
    }
    if matches!(
        next.phase.as_str(),
        "workspaceReady" | "starting" | "sessionReady" | "sending" | "completed"
    ) && next.workspace_id.is_none()
        || matches!(
            next.phase.as_str(),
            "sessionReady" | "sending" | "completed"
        ) && next.session_id.is_none()
    {
        return Err("Missing launch scope".into());
    }
    Ok(())
}
#[tauri::command]
pub fn quick_composer_checkpoint(
    window: Webview,
    state: State<QuickComposerState>,
    launch: Launch,
) -> Result<Launch, String> {
    panel_only(&window)?;
    let mut saved = state.saved.lock().map_err(|e| e.to_string())?;
    let current = saved.launch.as_ref().ok_or("Missing launch")?;
    transition(current, &launch)?;
    let mut next = saved.clone();
    next.launch = Some(launch.clone());
    state.persist(&next)?;
    *saved = next;
    Ok(launch)
}

#[tauri::command]
pub fn quick_composer_set_shortcut(
    window: Webview,
    app: AppHandle,
    shortcut: Option<String>,
) -> Result<(), String> {
    trusted(&window)?;
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
pub fn toggle(app: &AppHandle) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("unsupported".into())
    }
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        app.run_on_main_thread(move || {
            if unsafe { native::dcc_quick_composer_toggle() } {
                let _ = handle.emit_to(LABEL, "quick-composer-shown", ());
            }
        })
        .map_err(|e| e.to_string())
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
    let root = app_data.join("quick-composer");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let saved = match fs::read(root.join("state.json")) {
        Ok(bytes) => serde_json::from_slice::<Saved>(&bytes).map_err(|e| e.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Saved::default(),
        Err(error) => return Err(error.to_string()),
    };
    app.manage(QuickComposerState {
        root,
        saved: Mutex::new(saved),
        shortcut_error: Mutex::new(false),
    });
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let panel = tauri::WebviewWindowBuilder::new(
            app,
            LABEL,
            tauri::WebviewUrl::App("index.html?quick-composer=1".into()),
        )
        .title("DCC — Quick composer")
        .inner_size(720., 440.)
        .visible(false)
        .focused(false)
        .resizable(false)
        .skip_taskbar(true)
        .on_navigation(|url| crate::quick_composer::local_navigation(url))
        .build()
        .map_err(|e| e.to_string())?;
        let pointer = panel.ns_window().map_err(|e| e.to_string())?;
        if !unsafe { native::dcc_quick_composer_create(pointer) } {
            return Err("Could not create quick composer panel".into());
        }
        let state = app.state::<QuickComposerState>();
        let saved = state.saved.lock().map_err(|e| e.to_string())?;
        if let Some(shortcut) = &saved.shortcut {
            *state.shortcut_error.lock().map_err(|e| e.to_string())? =
                app.global_shortcut().register(shortcut.as_str()).is_err();
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn local_navigation(url: &url::Url) -> bool {
    let packaged = url.scheme() == "tauri" && url.host_str() == Some("localhost");
    let dev = cfg!(debug_assertions)
        && url.scheme() == "http"
        && url.host_str() == Some("localhost")
        && url.port() == Some(5173);
    (packaged || dev) && matches!(url.path(), "/" | "/index.html")
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
        pub fn dcc_quick_composer_create(window: *mut std::ffi::c_void) -> bool;
        pub fn dcc_quick_composer_toggle() -> bool;
        pub fn dcc_quick_composer_hide();
        pub fn dcc_quick_composer_destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn launch() -> Launch {
        Launch {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            phase: "pending".into(),
            request: serde_json::json!({ "isolationMode": "localDirect", "turn": { "rawPrompt": "Fix this" } }),
            workspace_id: None,
            session_id: None,
            error: None,
        }
    }
    #[test]
    fn cannot_replay_or_change_a_launch_scope() {
        let current = launch();
        let mut next = current.clone();
        next.phase = "creating".into();
        assert!(transition(&current, &next).is_err());
        next.revision = 1;
        assert!(transition(&current, &next).is_ok());
        assert!(transition(&next, &next).is_err());
        next.request["isolationMode"] = "protectedWorktree".into();
        assert!(transition(&current, &next).is_err());
    }
    #[test]
    fn checkpoints_require_task_and_session_identity() {
        let mut current = launch();
        current.phase = "creating".into();
        let mut next = current.clone();
        next.revision = 1;
        next.phase = "workspaceReady".into();
        assert!(transition(&current, &next).is_err());
        next.workspace_id = Some("workspace".into());
        assert!(transition(&current, &next).is_ok());
        current = next.clone();
        next.revision += 1;
        next.phase = "starting".into();
        next.workspace_id = Some("another".into());
        assert!(transition(&current, &next).is_err());
    }
    #[test]
    fn journal_survives_reopen_and_validates_requests() {
        let root = tempfile::tempdir().unwrap();
        let state = QuickComposerState {
            root: root.path().into(),
            saved: Mutex::new(Saved::default()),
            shortcut_error: Mutex::new(false),
        };
        let request = launch();
        validate_launch(&request).unwrap();
        let saved = Saved {
            shortcut: None,
            launch: Some(request.clone()),
        };
        state.persist(&saved).unwrap();
        let recovered: Saved =
            serde_json::from_slice(&fs::read(root.path().join("state.json")).unwrap()).unwrap();
        assert_eq!(recovered.launch.unwrap().id, request.id);
        let mut invalid = request;
        invalid.request["turn"]["rawPrompt"] = " ".into();
        assert!(validate_launch(&invalid).is_err());
    }
}
