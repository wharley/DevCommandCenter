//! User-initiated window attachments. No MCP route and no Computer Use grant.
//! Preview bytes stay in memory; only explicitly attached captures go to disk.
use crate::computer_use_commands::{self as native, ComputerTarget};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

const MAX_IMAGES: usize = 6;
const MAX_PREVIEWS: usize = 24;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const PREVIEW_TTL: Duration = Duration::from_secs(10 * 60);
const DEFAULT_SHORTCUT: &str = "CommandOrControl+Shift+9";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAppshot {
    id: String,
    draft_key: String,
    path: String,
}

#[derive(Serialize, Deserialize)]
struct SavedState {
    shortcut: Option<String>,
    pending: Vec<PendingAppshot>,
}
impl Default for SavedState {
    fn default() -> Self {
        Self {
            shortcut: Some(DEFAULT_SHORTCUT.into()),
            pending: vec![],
        }
    }
}

struct Preview {
    id: String,
    draft_key: String,
    target: ComputerTarget,
    bytes: Vec<u8>,
    created: Instant,
}

struct Inner {
    saved: SavedState,
    previews: Vec<Preview>,
    active: Option<(String, String)>, // owner, draft key
    shortcut_error: bool,
    capturing: bool,
}
pub struct AppshotState {
    root: PathBuf,
    inner: Mutex<Inner>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppshotStatus {
    supported: bool,
    screen_recording: bool,
    shortcut: Option<String>,
    shortcut_error: bool,
    targets: Vec<ComputerTarget>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppshotPreview {
    id: String,
    data_url: String,
}

fn main_only(window: &tauri::Webview) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("unavailable".into())
    }
}
fn valid_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 1024 {
        Err("unavailable".into())
    } else {
        Ok(())
    }
}

impl AppshotState {
    fn new(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|_| "saveFailed")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|_| "saveFailed")?;
        }
        let saved = match fs::read(root.join("state.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "saveFailed")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => SavedState::default(),
            Err(_) => return Err("saveFailed".into()),
        };
        Ok(Self {
            root,
            inner: Mutex::new(Inner {
                saved,
                previews: vec![],
                active: None,
                shortcut_error: false,
                capturing: false,
            }),
        })
    }

    fn persist(&self, saved: &SavedState) -> Result<(), String> {
        let bytes = serde_json::to_vec(saved).map_err(|_| "saveFailed")?;
        fs::write(self.root.join("state.tmp"), bytes).map_err(|_| "saveFailed")?;
        fs::rename(self.root.join("state.tmp"), self.root.join("state.json"))
            .map_err(|_| "saveFailed".into())
    }

    fn status(&self, include_targets: bool) -> Result<AppshotStatus, String> {
        let (supported, screen_recording) = native::appshot_status();
        let targets = if supported && screen_recording && include_targets {
            native::appshot_targets()?
        } else {
            vec![]
        };
        let inner = self.inner.lock().map_err(|_| "unavailable")?;
        Ok(AppshotStatus {
            supported,
            screen_recording,
            shortcut: inner.saved.shortcut.clone(),
            shortcut_error: inner.shortcut_error,
            targets,
        })
    }

    fn add_preview(
        &self,
        draft_key: String,
        target: ComputerTarget,
        bytes: Vec<u8>,
    ) -> Result<AppshotPreview, String> {
        if bytes.len() > MAX_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("captureFailed".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&bytes));
        let mut inner = self.inner.lock().map_err(|_| "unavailable")?;
        inner
            .previews
            .retain(|preview| preview.created.elapsed() < PREVIEW_TTL);
        if inner.previews.len() >= MAX_PREVIEWS {
            inner.previews.remove(0);
        }
        inner.previews.push(Preview {
            id: id.clone(),
            draft_key,
            target,
            bytes,
            created: Instant::now(),
        });
        Ok(AppshotPreview { id, data_url })
    }

    // Commit exactly the pixels shown in the picker, atomically as a batch.
    fn attach(&self, draft_key: &str, ids: &[String]) -> Result<(), String> {
        valid_key(draft_key)?;
        if ids.is_empty()
            || ids.len() > MAX_IMAGES
            || ids.iter().collect::<HashSet<_>>().len() != ids.len()
        {
            return Err("selectionInvalid".into());
        }
        let mut inner = self.inner.lock().map_err(|_| "unavailable")?;
        // A retried IPC response must not attach twice.
        if ids.iter().all(|id| {
            inner
                .saved
                .pending
                .iter()
                .any(|shot| &shot.id == id && shot.draft_key == draft_key)
        }) {
            return Ok(());
        }
        let previews = ids
            .iter()
            .map(|id| {
                inner
                    .previews
                    .iter()
                    .find(|preview| {
                        &preview.id == id
                            && preview.draft_key == draft_key
                            && preview.created.elapsed() < PREVIEW_TTL
                    })
                    .ok_or_else(|| "previewExpired".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut added = Vec::new();
        let written = (|| {
            for preview in previews {
                let name: String = preview
                    .target
                    .name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .take(40)
                    .collect();
                let path = self.root.join(format!("Appshot-{name}-{}.png", preview.id));
                // UUIDs are generated here, never interpreted as paths supplied by a caller.
                added.push(PendingAppshot {
                    id: preview.id.clone(),
                    draft_key: draft_key.into(),
                    path: path.to_string_lossy().into(),
                });
                fs::write(&path, &preview.bytes).map_err(|_| "saveFailed")?;
            }
            Ok::<_, String>(())
        })();
        let result = written.and_then(|()| {
            inner.saved.pending.extend(added.iter().cloned());
            self.persist(&inner.saved)
        });
        if result.is_err() {
            inner
                .saved
                .pending
                .retain(|shot| !added.iter().any(|added| added.id == shot.id));
            for shot in &added {
                let _ = fs::remove_file(&shot.path);
            }
        } else {
            inner.previews.retain(|preview| !ids.contains(&preview.id));
        }
        result
    }
}

#[tauri::command]
pub async fn appshots_status(
    window: tauri::Webview,
    app: tauri::AppHandle,
    include_targets: bool,
) -> Result<AppshotStatus, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppshotState>().status(include_targets)
    })
    .await
    .map_err(|_| "unavailable")?
}

#[tauri::command]
pub fn appshots_request_access(window: tauri::Webview) -> Result<(), String> {
    main_only(&window)?;
    if !native::appshot_status().0 {
        return Err("unsupported".into());
    }
    native::appshot_request_access();
    Ok(())
}

#[tauri::command]
pub async fn appshots_preview(
    window: tauri::Webview,
    app: tauri::AppHandle,
    draft_key: String,
    target: ComputerTarget,
) -> Result<AppshotPreview, String> {
    main_only(&window)?;
    valid_key(&draft_key)?;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = native::appshot_capture(&target)?;
        app.state::<AppshotState>()
            .add_preview(draft_key, target, bytes)
    })
    .await
    .map_err(|_| "captureFailed")?
}

#[tauri::command]
pub async fn appshots_attach(
    window: tauri::Webview,
    app: tauri::AppHandle,
    draft_key: String,
    ids: Vec<String>,
) -> Result<(), String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppshotState>().attach(&draft_key, &ids)?;
        let _ = app.emit_to("main", "appshots-ready", &draft_key);
        Ok(())
    })
    .await
    .map_err(|_| "saveFailed")?
}

#[tauri::command]
pub fn appshots_activate(
    window: tauri::Webview,
    state: tauri::State<AppshotState>,
    owner: String,
    draft_key: Option<String>,
) -> Result<(), String> {
    main_only(&window)?;
    let mut inner = state.inner.lock().map_err(|_| "unavailable")?;
    if let Some(key) = draft_key {
        valid_key(&key)?;
        inner.active = Some((owner, key));
    } else if inner
        .active
        .as_ref()
        .is_some_and(|active| active.0 == owner)
    {
        inner.active = None;
    }
    Ok(())
}

#[tauri::command]
pub fn appshots_pending(
    window: tauri::Webview,
    state: tauri::State<AppshotState>,
    draft_key: String,
) -> Result<Vec<PendingAppshot>, String> {
    main_only(&window)?;
    Ok(state
        .inner
        .lock()
        .map_err(|_| "unavailable")?
        .saved
        .pending
        .iter()
        .filter(|shot| shot.draft_key == draft_key)
        .cloned()
        .collect())
}

#[tauri::command]
pub fn appshots_acknowledge(
    window: tauri::Webview,
    state: tauri::State<AppshotState>,
    draft_key: String,
    ids: Vec<String>,
) -> Result<(), String> {
    main_only(&window)?;
    let mut inner = state.inner.lock().map_err(|_| "unavailable")?;
    let previous = inner.saved.pending.clone();
    inner
        .saved
        .pending
        .retain(|shot| shot.draft_key != draft_key || !ids.contains(&shot.id));
    if let Err(error) = state.persist(&inner.saved) {
        inner.saved.pending = previous;
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub fn appshots_set_shortcut(
    window: tauri::Webview,
    app: tauri::AppHandle,
    shortcut: Option<String>,
) -> Result<(), String> {
    main_only(&window)?;
    configure_shortcut(&app, shortcut)
}

#[cfg(target_os = "macos")]
fn configure_shortcut(app: &tauri::AppHandle, shortcut: Option<String>) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};
    let parsed = shortcut
        .as_deref()
        .map(|text| text.parse::<Shortcut>().map_err(|_| "shortcutInvalid"))
        .transpose()?;
    if parsed.as_ref().is_some_and(|key| {
        !key.mods
            .intersects(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT)
    }) {
        return Err("shortcutInvalid".into());
    }
    let state = app.state::<AppshotState>();
    let mut inner = state.inner.lock().map_err(|_| "unavailable")?;
    let previous = inner.saved.shortcut.clone();
    if previous
        .as_deref()
        .and_then(|key| key.parse::<Shortcut>().ok())
        == parsed
        && !inner.shortcut_error
    {
        return Ok(());
    }
    if let Some(key) = parsed {
        app.global_shortcut()
            .register(key)
            .map_err(|_| "shortcutUnavailable")?;
    }
    if !inner.shortcut_error {
        if let Some(old) = previous.as_deref() {
            if app.global_shortcut().unregister(old).is_err() {
                if let Some(key) = parsed {
                    let _ = app.global_shortcut().unregister(key);
                }
                return Err("shortcutUnavailable".into());
            }
        }
    }
    inner.saved.shortcut = shortcut.clone();
    if let Err(error) = state.persist(&inner.saved) {
        if let Some(key) = parsed {
            let _ = app.global_shortcut().unregister(key);
        }
        inner.shortcut_error = previous
            .as_deref()
            .is_some_and(|old| app.global_shortcut().register(old).is_err());
        inner.saved.shortcut = previous;
        return Err(error);
    }
    inner.shortcut_error = false;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn configure_shortcut(_app: &tauri::AppHandle, _shortcut: Option<String>) -> Result<(), String> {
    Err("unsupported".into())
}

pub fn setup(app: &tauri::AppHandle, root: PathBuf) -> Result<(), String> {
    app.manage(AppshotState::new(root.join("appshots"))?);
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
        app.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        capture_shortcut(app);
                    }
                })
                .build(),
        )
        .map_err(|_| "shortcutUnavailable")?;
        let state = app.state::<AppshotState>();
        let mut inner = state.inner.lock().map_err(|_| "unavailable")?;
        if native::appshot_status().0 {
            if let Some(shortcut) = &inner.saved.shortcut {
                inner.shortcut_error = app.global_shortcut().register(shortcut.as_str()).is_err();
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_shortcut(app: &tauri::AppHandle) {
    let state = app.state::<AppshotState>();
    let draft_key = {
        let Ok(mut inner) = state.inner.lock() else {
            return;
        };
        if inner.capturing {
            return;
        }
        let Some((_, key)) = &inner.active else {
            return;
        };
        let key = key.clone();
        inner.capturing = true;
        key
    };
    // Freeze the external target before showing/focusing DCC.
    let target = native::appshot_frontmost_target();
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppshotState>();
        let result = (|| {
            let target = target?;
            let bytes = native::appshot_capture(&target)?;
            let preview = state.add_preview(draft_key.clone(), target, bytes)?;
            state.attach(&draft_key, &[preview.id])
        })();
        if let Ok(mut inner) = state.inner.lock() {
            inner.capturing = false;
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        match result {
            Ok(()) => {
                let _ = app.emit_to("main", "appshots-ready", &draft_key);
            }
            Err(error) => {
                let _ = app.emit_to("main", "appshots-error", error);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn preview(state: &AppshotState, draft: &str) -> AppshotPreview {
        state
            .add_preview(
                draft.into(),
                ComputerTarget {
                    bundle_id: "test".into(),
                    name: "../Editor".into(),
                    pid: 42,
                    window_id: 1,
                    title: "window".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                b"\x89PNG\r\n\x1a\nexact captured bytes".to_vec(),
            )
            .unwrap()
    }
    #[test]
    fn previews_are_ephemeral_and_attaching_persists_exact_bytes_and_destination() {
        let root = tempfile::tempdir().unwrap();
        let state = AppshotState::new(root.path().into()).unwrap();
        let capture = preview(&state, "draft-a");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        assert!(state.attach("draft-b", &[capture.id.clone()]).is_err());
        state.attach("draft-a", &[capture.id.clone()]).unwrap();
        state.attach("draft-a", &[capture.id]).unwrap();
        let reloaded = AppshotState::new(root.path().into()).unwrap();
        let inner = reloaded.inner.lock().unwrap();
        assert_eq!(inner.saved.pending.len(), 1);
        let shot = &inner.saved.pending[0];
        assert_eq!(shot.draft_key, "draft-a");
        assert_eq!(PathBuf::from(&shot.path).parent(), Some(root.path()));
        assert_eq!(
            fs::read(&shot.path).unwrap(),
            b"\x89PNG\r\n\x1a\nexact captured bytes"
        );
    }
    #[test]
    fn invalid_batches_and_expired_previews_never_partially_attach() {
        let root = tempfile::tempdir().unwrap();
        let state = AppshotState::new(root.path().into()).unwrap();
        let capture = preview(&state, "draft");
        for ids in [
            vec![],
            vec![capture.id.clone(), "missing".into()],
            vec![capture.id.clone(); 2],
        ] {
            assert!(state.attach("draft", &ids).is_err());
        }
        state.inner.lock().unwrap().previews[0].created = Instant::now() - PREVIEW_TTL;
        assert!(state.attach("draft", &[capture.id]).is_err());
        assert!(state.inner.lock().unwrap().saved.pending.is_empty());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[test]
    fn failed_persistence_rolls_back_files_and_keeps_previews_for_retry() {
        let root = tempfile::tempdir().unwrap();
        let state = AppshotState::new(root.path().into()).unwrap();
        let capture = preview(&state, "draft");
        fs::create_dir(root.path().join("state.json")).unwrap();
        assert!(state.attach("draft", &[capture.id.clone()]).is_err());
        let inner = state.inner.lock().unwrap();
        assert!(inner.saved.pending.is_empty());
        assert_eq!(inner.previews[0].id, capture.id);
        assert!(!fs::read_dir(root.path()).unwrap().any(|file| file
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "png")));
    }
}
