//! The global shortcuts' side of the API while this app hosts the backend
//! (`DesktopShortcuts`, the desktop's `quick_composer` and Appshots): the
//! quick composer's launch journal and key, and the Appshots key
//! (`data::appshot_key`), each registered here and kept in its own file.

use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;

use dcc_server::quick_composer_journal::{self as journal, QuickComposerState};
use global_hotkey::hotkey::{HotKey, Modifiers};
use serde_json::{Value, json};

use crate::app::global_keys::{self, Request};
use crate::data::appshot_key::SavedKey;

pub struct HostShortcuts {
    journal: QuickComposerState,
    appshots: SavedKey,
    /// The saved Appshots key did not register (another app holds it).
    appshots_error: Mutex<bool>,
}

/// Appshots need macOS 14 (ScreenCaptureKit).
fn appshots_supported() -> bool {
    cfg!(target_os = "macos") && dcc_server::computer_use_commands::appshot_status().0
}

fn register(key: HotKey) -> bool {
    global_keys::request(|reply| Request::Register(key, reply)).is_ok()
}

impl HostShortcuts {
    /// Opens both and registers the saved keys once the UI runs.
    pub fn open(app_data: &Path) -> Result<Self, String> {
        let shortcuts = Self {
            journal: QuickComposerState::open(app_data)?,
            appshots: SavedKey::open(&app_data.join("appshots")),
            appshots_error: Mutex::new(false),
        };
        if let Some(key) = saved_key(shortcuts.journal.shortcut()) {
            let registered = register(key);
            if registered {
                global_keys::set_quick_composer(Some(&key));
            }
            if let Ok(mut error) = shortcuts.journal.shortcut_error.lock() {
                *error = !registered;
            }
        }
        if appshots_supported()
            && let Some(key) = saved_key(shortcuts.appshots.shortcut())
        {
            let registered = register(key);
            if registered {
                global_keys::set_appshots(Some(&key));
            }
            if let Ok(mut error) = shortcuts.appshots_error.lock() {
                *error = !registered;
            }
        }
        Ok(shortcuts)
    }
}

fn saved_key(shortcut: Option<String>) -> Option<HotKey> {
    shortcut.and_then(|key| HotKey::from_str(&key).ok())
}

/// `quick_composer_set_shortcut`: Command, Control or Option with a key.
fn parse(shortcut: Option<&str>) -> Result<Option<HotKey>, String> {
    let key = shortcut
        .map(HotKey::from_str)
        .transpose()
        .map_err(|_| "shortcutInvalid".to_string())?;
    if key.as_ref().is_some_and(|key| {
        !key.mods
            .intersects(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT)
    }) {
        return Err("shortcutInvalid".into());
    }
    Ok(key)
}

/// Moves a global key from `old` to `key`: the new one registered first,
/// then kept (`persist`), then the old one let go (unless it never
/// registered, `failed`). A failed save lets the new one go again.
fn swap(
    old: Option<HotKey>,
    key: Option<HotKey>,
    failed: bool,
    persist: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if let Some(key) = key {
        global_keys::request(|reply| Request::Register(key, reply))
            .map_err(|_| "shortcutUnavailable".to_string())?;
    }
    if let Err(failure) = persist() {
        if let Some(key) = key {
            let _ = global_keys::request(|reply| Request::Unregister(key, reply));
        }
        return Err(failure);
    }
    if !failed && let Some(old) = old {
        let _ = global_keys::request(|reply| Request::Unregister(old, reply));
    }
    Ok(())
}

impl dcc_server::http_api::DesktopShortcuts for HostShortcuts {
    fn quick_composer(&self) -> Result<Value, String> {
        serde_json::to_value(journal::status(&self.journal)?).map_err(|error| error.to_string())
    }

    fn set_quick_composer(&self, shortcut: Option<String>) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("unsupported".into());
        }
        let key = parse(shortcut.as_deref())?;
        let mut saved = self.journal.saved.lock().map_err(|e| e.to_string())?;
        let mut error = self
            .journal
            .shortcut_error
            .lock()
            .map_err(|e| e.to_string())?;
        let old = saved_key(saved.shortcut.clone());
        if key == old && !*error {
            return Ok(());
        }
        let mut next = saved.clone();
        next.shortcut = shortcut;
        swap(old, key, *error, || self.journal.persist(&next))?;
        global_keys::set_quick_composer(key.as_ref());
        *saved = next;
        *error = false;
        Ok(())
    }

    fn toggle_quick_composer(&self) -> Result<(), String> {
        if dcc_server::http_api::native_quick_composer().toggle() {
            Ok(())
        } else {
            Err("unavailable".into())
        }
    }

    fn begin_quick_launch(&self, launch: Value) -> Result<Value, String> {
        let launch = serde_json::from_value(launch).map_err(|error| error.to_string())?;
        let begun = journal::begin(&self.journal, launch)?;
        serde_json::to_value(begun).map_err(|error| error.to_string())
    }

    fn checkpoint_quick_launch(&self, launch: Value) -> Result<Value, String> {
        let launch = serde_json::from_value(launch).map_err(|error| error.to_string())?;
        let saved = journal::checkpoint(&self.journal, launch)?;
        serde_json::to_value(saved).map_err(|error| error.to_string())
    }

    fn appshots(&self) -> Result<Value, String> {
        let error = self.appshots_error.lock().map_err(|e| e.to_string())?;
        Ok(json!({
            "supported": appshots_supported(),
            "shortcut": self.appshots.shortcut(),
            "shortcutError": *error,
        }))
    }

    fn set_appshots(&self, shortcut: Option<String>) -> Result<(), String> {
        if !appshots_supported() {
            return Err("unsupported".into());
        }
        let key = parse(shortcut.as_deref())?;
        let mut error = self.appshots_error.lock().map_err(|e| e.to_string())?;
        let old = saved_key(self.appshots.shortcut());
        if key == old && !*error {
            return Ok(());
        }
        swap(old, key, *error, || {
            self.appshots.persist(shortcut.as_deref())
        })?;
        global_keys::set_appshots(key.as_ref());
        *error = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn a_key_needs_command_control_or_option() {
        assert!(parse(Some("Super+Shift+KeyC")).unwrap().is_some());
        assert!(
            parse(Some(crate::data::appshot_key::DEFAULT))
                .unwrap()
                .is_some()
        );
        assert!(parse(None).unwrap().is_none());
        assert_eq!(parse(Some("Shift+KeyC")).unwrap_err(), "shortcutInvalid");
        assert_eq!(parse(Some("Banana")).unwrap_err(), "shortcutInvalid");
    }
}
