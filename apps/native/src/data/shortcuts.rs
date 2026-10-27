//! The desktop's global shortcuts (`appshots-api.ts`, `quick-composer/api.ts`):
//! the quick composer's and Appshots' keys as Tauri writes them
//! (`Super+Shift+KeyK`), read and set over the loopback API, a pressed
//! combination turned into one (`shortcutFromEvent`) and shown as glyphs.

use gpui::Keystroke;
use serde::Deserialize;
use serde_json::json;

use super::desktop_api::{ApiError, DesktopApi};

/// What either card needs from its status.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub supported: bool,
    pub shortcut: Option<String>,
    #[serde(default)]
    pub shortcut_error: bool,
}

/// Which global shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Global {
    QuickComposer,
    Appshots,
}

impl Global {
    fn path(self) -> &'static str {
        match self {
            Global::QuickComposer => "/api/v1/quick-composer",
            Global::Appshots => "/api/v1/appshots",
        }
    }
}

/// `shortcutFromEvent`: Command, Control or Option with a letter, digit,
/// F1–F24 or Space; anything else is not a shortcut.
pub fn from_keystroke(keystroke: &Keystroke) -> Option<String> {
    let modifiers = &keystroke.modifiers;
    if !modifiers.platform && !modifiers.control && !modifiers.alt {
        return None;
    }
    let key = keystroke.key.as_str();
    let code = match key {
        "space" => "Space".to_string(),
        _ if key.len() == 1 && key.chars().all(|c| c.is_ascii_lowercase()) => {
            format!("Key{}", key.to_ascii_uppercase())
        }
        _ if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) => format!("Digit{key}"),
        _ => {
            let number: u8 = key.strip_prefix('f')?.parse().ok()?;
            if !(1..=24).contains(&number) {
                return None;
            }
            format!("F{number}")
        }
    };
    let parts: Vec<&str> = [
        modifiers.platform.then_some("Super"),
        modifiers.control.then_some("Control"),
        modifiers.alt.then_some("Alt"),
        modifiers.shift.then_some("Shift"),
    ]
    .into_iter()
    .flatten()
    .collect();
    Some(format!("{}+{code}", parts.join("+")))
}

/// The key as the card shows it: `⌘ ⇧ K`.
pub fn display(shortcut: &str) -> String {
    shortcut
        .replace("CommandOrControl", "⌘")
        .replace("Super", "⌘")
        .replace("Control", "⌃")
        .replace("Alt", "⌥")
        .replace("Shift", "⇧")
        .replace("Key", "")
        .replace("Digit", "")
        .replace('+', " ")
}

/// `appshotErrorKey`: the desktop's codes, else "unavailable".
pub fn error_key(error: &str) -> String {
    let known = [
        "unsupported",
        "backendOutdated",
        "permission",
        "unavailable",
        "captureFailed",
        "windowUnavailable",
        "saveFailed",
        "selectionInvalid",
        "previewExpired",
        "shortcutInvalid",
        "shortcutUnavailable",
    ];
    let code = known
        .iter()
        .find(|code| error.contains(**code))
        .copied()
        .unwrap_or("unavailable");
    format!("appshots.errors.{code}")
}

impl DesktopApi {
    /// `None` when the desktop does not offer it (the card hides).
    pub fn global_shortcut(&self, which: Global) -> Option<ShortcutStatus> {
        self.get(&format!("{}/status", which.path()))
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
    }

    pub fn set_global_shortcut(
        &self,
        which: Global,
        shortcut: Option<&str>,
    ) -> Result<(), ApiError> {
        self.put(
            &format!("{}/shortcut", which.path()),
            &json!({ "shortcut": shortcut }),
        )
        .map(|_| ())
    }

    /// `quick_composer_toggle`: the desktop's quick composer window.
    pub fn toggle_quick_composer(&self) -> Result<(), ApiError> {
        self.post("/api/v1/quick-composer/toggle", &json!({}))
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Option<String> {
        from_keystroke(&Keystroke::parse(text).unwrap())
    }

    #[test]
    fn combinations_read_like_the_desktop() {
        assert_eq!(key("cmd-shift-k").as_deref(), Some("Super+Shift+KeyK"));
        assert_eq!(key("ctrl-alt-5").as_deref(), Some("Control+Alt+Digit5"));
        assert_eq!(key("cmd-f12").as_deref(), Some("Super+F12"));
        assert_eq!(key("alt-space").as_deref(), Some("Alt+Space"));
        // A modifier is required, and only those keys.
        assert_eq!(key("shift-k"), None);
        assert_eq!(key("cmd-escape"), None);
        assert_eq!(display("Super+Shift+KeyK"), "⌘ ⇧ K");
        assert_eq!(display("Control+Alt+Digit5"), "⌃ ⌥ 5");
        assert_eq!(
            error_key("shortcutUnavailable"),
            "appshots.errors.shortcutUnavailable"
        );
        assert_eq!(error_key("boom"), "appshots.errors.unavailable");
    }
}
