//! The global shortcuts' side of the API while this app hosts the backend
//! (`DesktopShortcuts`, the desktop's `quick_composer` and Appshots): the
//! quick composer's launch journal and key. Appshots are not in this app.

use std::path::Path;
use std::str::FromStr;

use dcc_server::quick_composer_journal::{self as journal, QuickComposerState};
use global_hotkey::hotkey::{HotKey, Modifiers};
use serde_json::{Value, json};

use crate::app::global_keys::{self, Request};

pub struct HostShortcuts {
    journal: QuickComposerState,
}

impl HostShortcuts {
    /// Opens the journal and registers the saved key once the UI runs.
    pub fn open(app_data: &Path) -> Result<Self, String> {
        let shortcuts = Self {
            journal: QuickComposerState::open(app_data)?,
        };
        if let Some(key) = shortcuts
            .journal
            .shortcut()
            .and_then(|key| HotKey::from_str(&key).ok())
        {
            let registered = global_keys::request(|reply| Request::Register(key, reply)).is_ok();
            if registered {
                global_keys::set_quick_composer(Some(&key));
            }
            if let Ok(mut error) = shortcuts.journal.shortcut_error.lock() {
                *error = !registered;
            }
        }
        Ok(shortcuts)
    }
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
        let old = saved
            .shortcut
            .as_deref()
            .and_then(|key| HotKey::from_str(key).ok());
        if key == old && !*error {
            return Ok(());
        }
        if let Some(key) = key {
            global_keys::request(|reply| Request::Register(key, reply))
                .map_err(|_| "shortcutUnavailable".to_string())?;
        }
        let mut next = saved.clone();
        next.shortcut = shortcut;
        if let Err(failure) = self.journal.persist(&next) {
            if let Some(key) = key {
                let _ = global_keys::request(|reply| Request::Unregister(key, reply));
            }
            return Err(failure);
        }
        if !*error && let Some(old) = old {
            let _ = global_keys::request(|reply| Request::Unregister(old, reply));
        }
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
        Ok(json!({ "supported": false, "shortcut": null, "shortcutError": false }))
    }

    fn set_appshots(&self, _shortcut: Option<String>) -> Result<(), String> {
        Err("unsupported".into())
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn a_key_needs_command_control_or_option() {
        assert!(parse(Some("Super+Shift+KeyC")).unwrap().is_some());
        assert!(parse(None).unwrap().is_none());
        assert_eq!(parse(Some("Shift+KeyC")).unwrap_err(), "shortcutInvalid");
        assert_eq!(parse(Some("Banana")).unwrap_err(), "shortcutInvalid");
    }
}
