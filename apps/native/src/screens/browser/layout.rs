//! Where the Browser sits (`browser-layout.ts`): beside the conversation
//! when both columns stay useful, over it in narrower windows; its width
//! is kept per task.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `DEFAULT_BROWSER_SURFACE_WIDTH` and its bounds.
pub const DEFAULT_WIDTH: f32 = 520.0;
pub const MIN_WIDTH: f32 = 360.0;
pub const MAX_WIDTH: f32 = 840.0;
/// `MIN_BROWSER_CHAT_WIDTH`
pub const MIN_CHAT_WIDTH: f32 = 520.0;
/// `BROWSER_SPLITTER_WIDTH`
pub const SPLITTER_WIDTH: f32 = 6.0;
/// `BROWSER_SPLIT_BREAKPOINT`
pub const SPLIT_BREAKPOINT: f32 = MIN_CHAT_WIDTH + MIN_WIDTH + SPLITTER_WIDTH;
/// The splitter's arrow keys move it this much.
pub const KEY_STEP: f32 = 16.0;

/// `clampBrowserSurfaceWidth`
pub fn clamp_width(width: f32) -> f32 {
    let width = if width.is_finite() {
        width
    } else {
        DEFAULT_WIDTH
    };
    width.round().clamp(MIN_WIDTH, MAX_WIDTH)
}

/// `clampBrowserSurfaceWidthForContainer`: the conversation keeps 520px.
pub fn clamp_width_for(width: f32, container: f32) -> f32 {
    let container = if container.is_finite() {
        container
    } else {
        SPLIT_BREAKPOINT
    };
    let room = (container - MIN_CHAT_WIDTH - SPLITTER_WIDTH)
        .floor()
        .clamp(MIN_WIDTH, MAX_WIDTH);
    clamp_width(width).min(room)
}

/// `shouldSplitBrowser`
pub fn should_split(container: f32) -> bool {
    container.is_finite() && container >= SPLIT_BREAKPOINT
}

/// The width each task last gave it (`dcc.browserSurfaceWidth.v1:<task>`),
/// in `browser-widths.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widths {
    by_task: BTreeMap<String, f32>,
    path: Option<PathBuf>,
}

impl Widths {
    pub fn default_path() -> PathBuf {
        crate::prefs::native_file("browser-widths.json")
    }

    pub fn load(path: &Path) -> Self {
        let by_task = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, f32>>(&bytes).ok())
            .unwrap_or_default();
        Self {
            by_task,
            path: Some(path.to_path_buf()),
        }
    }

    /// `readBrowserSurfaceWidth`
    pub fn get(&self, task: &str) -> f32 {
        self.by_task
            .get(task)
            .copied()
            .map(clamp_width)
            .unwrap_or(DEFAULT_WIDTH)
    }

    /// `persistBrowserSurfaceWidth`
    pub fn set(&mut self, task: &str, width: f32) {
        self.by_task.insert(task.to_string(), clamp_width(width));
        let Some(path) = &self.path else {
            return;
        };
        let saved = serde_json::to_vec_pretty(&self.by_task)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let temporary = path.with_extension("json.tmp");
                std::fs::write(&temporary, json)?;
                std::fs::rename(temporary, path)
            });
        if let Err(error) = saved {
            eprintln!("[dcc-native] could not save the browser width: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_or_takeover_at_the_breakpoint() {
        assert!(!should_split(SPLIT_BREAKPOINT - 1.0));
        assert!(should_split(SPLIT_BREAKPOINT));
        assert!(!should_split(f32::NAN));
    }

    #[test]
    fn widths_are_clamped_like_the_desktop() {
        assert_eq!(clamp_width(1.0), MIN_WIDTH);
        assert_eq!(clamp_width(9999.0), MAX_WIDTH);
        assert_eq!(clamp_width(f32::NAN), DEFAULT_WIDTH);
        assert_eq!(clamp_width_for(800.0, 900.0), 374.0);
        assert_eq!(clamp_width_for(9999.0, 2000.0), MAX_WIDTH);
    }

    #[test]
    fn a_width_per_task_survives_a_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("browser-widths.json");
        let mut widths = Widths::load(&path);
        widths.set("workspace/one", 611.4);
        let again = Widths::load(&path);
        assert_eq!(again.get("workspace/one"), 611.0);
        assert_eq!(again.get("workspace/two"), DEFAULT_WIDTH);
    }
}
