//! When the team room shows how much space the completed tasks take
//! (`cleanup-alert.ts`): only the switch and the limit are the person's to
//! choose (Settings › General). Kept in the native app's own file
//! (`cleanup-alert.json`), next to the recap's reading marker.

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
    /// The total when the person dismissed the card.
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

/// The limits Settings offers, in gigabytes.
pub const THRESHOLD_CHOICES: [u32; 5] = [2, 5, 10, 20, 50];

const GB: u64 = 1024 * 1024 * 1024;

impl CleanupAlert {
    /// Whether `total` bytes of completed tasks call for the card: past the
    /// limit, and a gigabyte past where the person last dismissed it.
    pub fn shows(&self, total: u64) -> bool {
        self.enabled
            && total >= u64::from(self.threshold_gb) * GB
            && self
                .dismissed_at_bytes
                .is_none_or(|dismissed| total >= dismissed.saturating_add(GB))
    }

    /// "Dispensar": quiet until the total grows again.
    pub fn dismiss(path: &Path, total: u64) -> std::io::Result<Self> {
        let mut alert = Self::load(path);
        alert.dismissed_at_bytes = Some(total);
        alert.write(path)?;
        Ok(alert)
    }

    fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            path,
            serde_json::to_string(self).unwrap_or_else(|_| "{}".into()),
        )
    }

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
        alert.write(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_shows_past_the_limit_until_dismissed_and_again_when_it_grows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cleanup-alert.json");
        assert_eq!(CleanupAlert::load(&path), CleanupAlert::default());
        let alert = CleanupAlert::default();
        assert!(!alert.shows(4 * GB));
        assert!(alert.shows(5 * GB));
        let alert = CleanupAlert::dismiss(&path, 6 * GB).unwrap();
        assert!(!alert.shows(6 * GB + GB / 2));
        assert!(alert.shows(7 * GB));
        CleanupAlert::save_settings(&path, false, 12).unwrap();
        let saved = CleanupAlert::load(&path);
        assert!(!saved.enabled && !saved.shows(100 * GB));
        assert_eq!(saved.threshold_gb, 12);
        // A dismissal stays when the settings change.
        assert_eq!(saved.dismissed_at_bytes, Some(6 * GB));
    }
}
