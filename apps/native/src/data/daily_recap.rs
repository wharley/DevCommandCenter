//! The Reporter's recap window (`daily-recap.ts`): a new recap opens at the
//! first look after 4am local time and covers everything since the previous
//! one ended, at most a week. A reading marker of this app, kept in
//! `daily-recap.json`; the first window it opens counts as read, like the
//! other markers here, so a first run does not light the Reporter up.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

const RECAP_HOUR: u32 = 4;
const MAX_WINDOW_DAYS: i64 = 7;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecapWindow {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyRecap {
    pub current: Option<RecapWindow>,
    /// `to` of the window the person last looked at.
    pub seen_to: Option<String>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

/// The latest 4am local time at or before `now`.
pub fn recap_boundary(now: DateTime<Local>) -> DateTime<Local> {
    let four = NaiveTime::from_hms_opt(RECAP_HOUR, 0, 0).expect("valid time");
    let today = Local
        .from_local_datetime(&now.date_naive().and_time(four))
        .earliest()
        .unwrap_or(now);
    if today > now {
        today - Duration::days(1)
    } else {
        today
    }
}

fn iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl DailyRecap {
    pub fn load(path: &Path, now: DateTime<Local>) -> Self {
        let stored: Option<Self> = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let first_run = stored.is_none();
        let mut recap = stored.unwrap_or_default();
        recap.path = Some(path.to_path_buf());
        if recap.refresh(now) && first_run {
            recap.seen_to = recap.current.as_ref().map(|window| window.to.clone());
            recap.save();
        }
        recap
    }

    /// `nextRecapState`: opens a new window once the current one ended before
    /// today's 4am; returns whether it did.
    pub fn refresh(&mut self, now: DateTime<Local>) -> bool {
        let boundary = recap_boundary(now).with_timezone(&Utc);
        let current_to = self
            .current
            .as_ref()
            .and_then(|window| DateTime::parse_from_rfc3339(&window.to).ok())
            .map(|to| to.with_timezone(&Utc));
        if current_to.is_some_and(|to| to >= boundary) {
            return false;
        }
        let now = now.with_timezone(&Utc);
        let previous_end = current_to.unwrap_or(boundary - Duration::days(1));
        let from = previous_end.max(now - Duration::days(MAX_WINDOW_DAYS));
        self.current = Some(RecapWindow {
            from: iso(from),
            to: iso(now),
        });
        self.save();
        true
    }

    /// `isRecapUnread`
    pub fn is_unread(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|window| self.seen_to.as_deref() != Some(window.to.as_str()))
    }

    /// `markDailyRecapSeen`: the Reporter's page shows it; returns whether
    /// it was unread.
    pub fn mark_seen(&mut self) -> bool {
        let Some(to) = self.current.as_ref().map(|window| window.to.clone()) else {
            return false;
        };
        if self.seen_to.as_deref() == Some(to.as_str()) {
            return false;
        }
        self.seen_to = Some(to);
        self.save();
        true
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(text: &str) -> DateTime<Local> {
        Local
            .from_local_datetime(
                &chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap(),
            )
            .unwrap()
    }

    #[test]
    fn the_boundary_is_the_last_4am() {
        assert_eq!(
            recap_boundary(local("2026-10-23 15:00")),
            local("2026-10-23 04:00")
        );
        assert_eq!(
            recap_boundary(local("2026-10-23 03:00")),
            local("2026-10-22 04:00")
        );
    }

    #[test]
    fn a_new_window_opens_after_4am_and_the_first_counts_as_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daily-recap.json");
        let mut recap = DailyRecap::load(&path, local("2026-10-23 09:00"));
        assert!(!recap.is_unread(), "the first window is the baseline");
        assert!(
            !recap.refresh(local("2026-10-23 20:00")),
            "same day: same window"
        );
        assert!(recap.refresh(local("2026-10-24 08:00")));
        assert!(recap.is_unread());
        let window = recap.current.clone().unwrap();
        assert_eq!(
            DateTime::parse_from_rfc3339(&window.from)
                .unwrap()
                .with_timezone(&Local),
            local("2026-10-23 09:00")
        );
        let mut reloaded = DailyRecap::load(&path, local("2026-10-24 09:00"));
        assert!(reloaded.is_unread(), "a stored window is not a first run");
        assert!(reloaded.mark_seen());
        assert!(!reloaded.mark_seen(), "once");
        assert!(!DailyRecap::load(&path, local("2026-10-24 10:00")).is_unread());
    }
}
