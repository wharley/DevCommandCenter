//! The native app's own UI preferences (`native-ui.json`), kept next to the
//! shared database. The desktop keeps the same values in its WebView storage.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Locale;
use crate::screens::shell::layout::{DEFAULT_SIDEBAR_WIDTH, clamp_sidebar_width};
use crate::theme::ThemePreference;

const FILE_NAME: &str = "native-ui.json";
/// Overrides where preferences live (tests, a second profile).
const DATA_DIR_ENV: &str = "DCC_NATIVE_DATA_DIR";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub theme: ThemePreference,
    pub locale: Locale,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    /// "Em espera" and "Concluídos" open in the rail.
    pub waiting_open: bool,
    pub completed_open: bool,
    /// The code column next to the sidebar (`dcc.codeColumnOpen`, closed by
    /// default) and its list (`dcc.codeColumnTab`).
    pub code_column_open: bool,
    pub code_column_tab: crate::screens::code_column::CodeColumnTab,
    /// The terminal dock's height (`dcc-workbench-terminal-dock-height-v1`).
    pub terminal_height: f32,
    /// "Abrir no editor"'s pick (`dcc-preferred-editor`).
    pub preferred_editor: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            locale: Locale::PtBr,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            sidebar_collapsed: false,
            waiting_open: false,
            completed_open: false,
            code_column_open: false,
            code_column_tab: crate::screens::code_column::CodeColumnTab::Files,
            terminal_height: crate::screens::terminal::dock::DEFAULT_HEIGHT,
            preferred_editor: dcc_infra::editor_launch::EDITORS[0].id.to_string(),
        }
    }
}

impl Prefs {
    /// Reads the file; anything missing or unreadable falls back to the defaults.
    pub fn load(path: &Path) -> Self {
        let mut prefs: Self = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        prefs.sidebar_width = clamp_sidebar_width(prefs.sidebar_width);
        // `getStoredPreferredEditor`: anything unknown is Cursor.
        if dcc_infra::editor_launch::editor(&prefs.preferred_editor).is_none() {
            prefs.preferred_editor = dcc_infra::editor_launch::EDITORS[0].id.to_string();
        }
        prefs
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(temporary, path)
    }
}

/// Same folder as the desktop app's data (`com.devcommandcenter.app`, or `.dev` in debug builds).
pub fn default_path() -> PathBuf {
    data_dir().join(FILE_NAME)
}

/// Another file of this app's own, next to its preferences.
pub fn native_file(name: &str) -> PathBuf {
    data_dir().join(name)
}

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return PathBuf::from(dir);
    }
    let identifier = if cfg!(debug_assertions) {
        "com.devcommandcenter.app.dev"
    } else {
        "com.devcommandcenter.app"
    };
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(identifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_the_desktop_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let prefs = Prefs::load(&dir.path().join(FILE_NAME));
        assert_eq!(prefs, Prefs::default());
        assert_eq!(prefs.sidebar_width, 336.0);
    }

    #[test]
    fn round_trips_and_clamps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(FILE_NAME);
        let prefs = Prefs {
            theme: ThemePreference::Dark,
            sidebar_width: 900.0,
            sidebar_collapsed: true,
            ..Prefs::default()
        };
        prefs.save(&path).unwrap();
        let loaded = Prefs::load(&path);
        assert_eq!(loaded.theme, ThemePreference::Dark);
        assert!(loaded.sidebar_collapsed);
        assert_eq!(loaded.sidebar_width, 520.0);
    }

    #[test]
    fn a_broken_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, b"{ not json").unwrap();
        assert_eq!(Prefs::load(&path), Prefs::default());
    }
}
