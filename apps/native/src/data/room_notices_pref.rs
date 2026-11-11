//! "Avisos das salas" (Settings › General): whether an agent asking, done
//! or stopped in a room is told while the person looks elsewhere
//! (`room_notices`). On unless turned off; kept in the native app's own
//! `room-notices.json`, like the cleanup notice's switch.

use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    enabled: bool,
}

impl Default for Stored {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub struct RoomNoticesPref {
    path: Option<PathBuf>,
    enabled: bool,
}

impl Global for RoomNoticesPref {}

fn load(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Stored>(&raw).ok())
        .unwrap_or_default()
        .enabled
}

impl RoomNoticesPref {
    /// Read once at start (tests keep the default, on, and save nothing).
    pub fn init(cx: &mut App) {
        let path = (!cfg!(test)).then(|| crate::prefs::native_file("room-notices.json"));
        let enabled = path.as_deref().is_none_or(load);
        cx.set_global(Self { path, enabled });
    }

    pub fn enabled(cx: &App) -> bool {
        cx.try_global::<Self>().is_none_or(|pref| pref.enabled)
    }

    pub fn set(enabled: bool, cx: &mut App) -> std::io::Result<()> {
        let path = cx.try_global::<Self>().and_then(|pref| pref.path.clone());
        cx.set_global(Self {
            path: path.clone(),
            enabled,
        });
        let Some(path) = path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            path,
            serde_json::to_string(&Stored { enabled }).unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_are_on_unless_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("room-notices.json");
        assert!(load(&path), "no file: on");
        std::fs::write(&path, r#"{"enabled":false}"#).unwrap();
        assert!(!load(&path));
        std::fs::write(&path, "not json").unwrap();
        assert!(load(&path));
    }
}
