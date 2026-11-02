//! The Appshots global key (`appshot_commands`' shortcut): the key kept in
//! the desktop's own `appshots/state.json` (⌘⇧9 until changed, so a key set
//! in the desktop carries over), and a press: the window in front captured
//! before the DCC comes forward, saved as "Capturar janela" saves them, and
//! handed to the UI (`app::appshot_key`) for the main window's composer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde_json::{Value, json};

/// The desktop's `DEFAULT_SHORTCUT`.
pub const DEFAULT: &str = "CommandOrControl+Shift+9";

/// `state.json`: the key (`null` when turned off) and whatever else the
/// desktop keeps there, written back untouched.
pub struct SavedKey {
    folder: PathBuf,
    state: Mutex<Value>,
}

impl SavedKey {
    pub fn open(folder: &Path) -> Self {
        let state = std::fs::read(folder.join("state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({ "shortcut": DEFAULT, "pending": [] }));
        Self {
            folder: folder.to_path_buf(),
            state: Mutex::new(state),
        }
    }

    pub fn shortcut(&self) -> Option<String> {
        self.state
            .lock()
            .ok()?
            .get("shortcut")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    /// Keeps `shortcut` (`None`: off); on a failed write nothing changes.
    pub fn persist(&self, shortcut: Option<&str>) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "unavailable")?;
        let mut next = state.clone();
        next["shortcut"] = shortcut.map_or(Value::Null, Value::from);
        std::fs::create_dir_all(&self.folder).map_err(|_| "saveFailed")?;
        let bytes = serde_json::to_vec(&next).map_err(|_| "saveFailed")?;
        let temporary = self.folder.join("state.tmp");
        std::fs::write(&temporary, bytes).map_err(|_| "saveFailed")?;
        std::fs::rename(&temporary, self.folder.join("state.json")).map_err(|_| "saveFailed")?;
        *state = next;
        Ok(())
    }
}

/// A capture's file, or an `appshots.errors.*` code.
pub type Captured = Result<PathBuf, String>;

type Channel = (
    UnboundedSender<Captured>,
    Mutex<Option<UnboundedReceiver<Captured>>>,
);

fn channel() -> &'static Channel {
    static CHANNEL: OnceLock<Channel> = OnceLock::new();
    CHANNEL.get_or_init(|| {
        let (send, receive) = unbounded();
        (send, Mutex::new(Some(receive)))
    })
}

/// The captures, for the one listener on the UI.
pub fn captures() -> Option<UnboundedReceiver<Captured>> {
    channel().1.lock().ok().and_then(|mut slot| slot.take())
}

/// A press while one is still capturing does nothing (`capturing`).
static CAPTURING: AtomicBool = AtomicBool::new(false);

/// A press of the key: off its callback, the window in front to a PNG.
pub fn capture_front() {
    if CAPTURING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        let captured = front_window();
        CAPTURING.store(false, Ordering::SeqCst);
        let _ = channel().0.unbounded_send(captured);
    });
}

#[cfg(target_os = "macos")]
fn front_window() -> Captured {
    use super::appshots;
    let target = dcc_server::computer_use_commands::appshot_frontmost_target()?;
    let bytes = appshots::capture(&target)?;
    appshots::save(&appshots::folder(), &target.name, &bytes)
}

#[cfg(not(target_os = "macos"))]
fn front_window() -> Captured {
    Err("unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_starts_at_the_desktops_default_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("appshots");
        let saved = SavedKey::open(&folder);
        assert_eq!(saved.shortcut().as_deref(), Some(DEFAULT));

        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("state.json"),
            r#"{"shortcut":"Alt+KeyA","pending":[{"id":"x","draftKey":"d","path":"/p.png"}]}"#,
        )
        .unwrap();
        let saved = SavedKey::open(&folder);
        assert_eq!(saved.shortcut().as_deref(), Some("Alt+KeyA"));
        saved.persist(None).unwrap();
        assert_eq!(SavedKey::open(&folder).shortcut(), None, "turned off");
        let kept: Value =
            serde_json::from_slice(&std::fs::read(folder.join("state.json")).unwrap()).unwrap();
        assert_eq!(
            kept["pending"][0]["id"], "x",
            "the desktop's own list stays"
        );
    }
}
