//! When the Reporter speaks up about the space completed tasks take
//! (`cleanup-alert.ts`): only the limit is the person's to choose. Kept in
//! the native app's own file (`cleanup-alert.json`, the desktop keeps it in
//! the WebView), next to the recap's reading marker.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const DEFAULT_THRESHOLD_GB: u32 = 5;
pub const MIN_THRESHOLD_GB: u32 = 1;
pub const MAX_THRESHOLD_GB: u32 = 1000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanupAlert {
    pub enabled: bool,
    pub threshold_gb: u32,
    /// The total when the person closed the bubble.
    pub dismissed_at_bytes: Option<u64>,
}

impl Default for CleanupAlert {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_gb: DEFAULT_THRESHOLD_GB,
            dismissed_at_bytes: None,
        }
    }
}

/// `clampCleanupThresholdGb`: a typed limit, rounded and kept in range.
pub fn clamp_threshold(typed: &str) -> u32 {
    match typed.trim().parse::<f64>() {
        Ok(value) if value.is_finite() => {
            (value
                .round()
                .clamp(MIN_THRESHOLD_GB as f64, MAX_THRESHOLD_GB as f64)) as u32
        }
        _ => DEFAULT_THRESHOLD_GB,
    }
}

impl CleanupAlert {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .map(|mut alert| {
                alert.threshold_gb = alert.threshold_gb.clamp(MIN_THRESHOLD_GB, MAX_THRESHOLD_GB);
                alert
            })
            .unwrap_or_default()
    }

    /// `setCleanupAlertSettings`: the switch and the limit; a dismissal stays.
    pub fn save_settings(path: &Path, enabled: bool, threshold_gb: u32) -> std::io::Result<()> {
        let mut alert = Self::load(path);
        alert.enabled = enabled;
        alert.threshold_gb = threshold_gb;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            path,
            serde_json::to_string(&alert).unwrap_or_else(|_| "{}".into()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_clamped_and_kept() {
        assert_eq!(clamp_threshold("7.4"), 7);
        assert_eq!(clamp_threshold("0"), MIN_THRESHOLD_GB);
        assert_eq!(clamp_threshold("5000"), MAX_THRESHOLD_GB);
        assert_eq!(clamp_threshold("abc"), DEFAULT_THRESHOLD_GB);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cleanup-alert.json");
        assert_eq!(CleanupAlert::load(&path), CleanupAlert::default());
        CleanupAlert::save_settings(&path, false, 12).unwrap();
        let saved = CleanupAlert::load(&path);
        assert!(!saved.enabled);
        assert_eq!(saved.threshold_gb, 12);
    }
}
