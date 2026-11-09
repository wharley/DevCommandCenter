//! The weekly bulletin's reading marker (`dcc_core::domain::agent_bulletin`):
//! the Monday of the last week the person opened. A new week's bulletin is
//! unread until its card opens in the team room; the first week seen counts
//! as read, like the other markers, so a first run lights nothing up. Kept
//! in `weekly-bulletin.json`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use dcc_core::domain::agent_bulletin::{Week, week_before};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WeeklyBulletin {
    /// The Monday (`2026-11-02`) of the last week opened.
    pub seen: Option<String>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl WeeklyBulletin {
    pub fn load(path: &Path, now: DateTime<Local>) -> Self {
        let stored = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok());
        let mut bulletin = stored.clone().unwrap_or_default();
        bulletin.path = Some(path.to_path_buf());
        if stored.is_none() {
            bulletin.seen = Some(week_before(now, 1).key);
            bulletin.save();
        }
        bulletin
    }

    /// The last complete week, the one the card shows.
    pub fn week(now: DateTime<Local>) -> Week {
        week_before(now, 1)
    }

    pub fn is_unread(&self, now: DateTime<Local>) -> bool {
        self.seen.as_deref() != Some(Self::week(now).key.as_str())
    }

    /// Its card opened; true when it was unread.
    pub fn mark_seen(&mut self, now: DateTime<Local>) -> bool {
        let key = Self::week(now).key;
        if self.seen.as_deref() == Some(key.as_str()) {
            return false;
        }
        self.seen = Some(key);
        self.save();
        true
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn a_new_week_is_unread_until_its_card_opens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weekly-bulletin.json");
        let wednesday = Local.with_ymd_and_hms(2026, 11, 11, 15, 0, 0).unwrap();
        let mut bulletin = WeeklyBulletin::load(&path, wednesday);
        assert!(
            !bulletin.is_unread(wednesday),
            "the first week is the baseline"
        );
        let next_monday = Local.with_ymd_and_hms(2026, 11, 16, 9, 0, 0).unwrap();
        assert!(bulletin.is_unread(next_monday));
        assert!(bulletin.mark_seen(next_monday));
        assert!(!bulletin.mark_seen(next_monday));
        assert!(!WeeklyBulletin::load(&path, next_monday).is_unread(next_monday));
    }
}
