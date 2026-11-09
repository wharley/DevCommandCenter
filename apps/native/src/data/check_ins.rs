//! The agents' check-in, with no model (Compozy's "no item, no model
//! turn"): what the 5-minute search already brings — pull requests that ask
//! for the person's review, issues assigned to them — and the tasks whose
//! turn is open with no sign of life for `STALLED_MINUTES`. Each new item is
//! a card in the team room; whether an agent wakes for it is the triggers'
//! call (`agent_triggers`). The first read of a kind only sets what is
//! already there, so a first run does not flood the room. Kept in the
//! native app's own file (`check-ins.json`), cards for `KEEP_DAYS`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};

/// A turn open this long with no event is stalled.
pub const STALLED_MINUTES: i64 = 20;
const KEEP_DAYS: i64 = 7;
/// What was seen is forgotten after this, so it can come back later.
const SEEN_DAYS: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckInKind {
    /// A pull request asks for the person's review.
    ReviewRequested,
    /// An issue was assigned to the person.
    IssueAssigned,
    /// A task's turn is open with no sign of life.
    TaskStalled,
}

/// One thing the check-in found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckIn {
    /// What makes it the same thing next time.
    pub key: String,
    pub kind: CheckInKind,
    pub title: String,
    /// `acme/app #12`, or the stalled task's project.
    pub place: String,
    pub url: Option<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
    /// When it was found.
    pub at: String,
    pub unread: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    /// The kinds read at least once.
    started: BTreeSet<CheckInKind>,
    /// Keys seen, with when.
    seen: BTreeMap<String, String>,
    cards: Vec<CheckIn>,
}

#[derive(Default)]
pub struct CheckIns {
    path: Option<PathBuf>,
    stored: Stored,
}

impl Global for CheckIns {}

fn older_than(at: &str, now: DateTime<Utc>, days: i64) -> bool {
    DateTime::parse_from_rfc3339(at)
        .map(|at| now - at.with_timezone(&Utc) > Duration::days(days))
        .unwrap_or(true)
}

impl CheckIns {
    pub fn init(cx: &mut App) {
        cx.set_global(Self::load(Some(crate::prefs::native_file(
            "check-ins.json",
        ))));
    }

    /// `None`: kept in memory only (tests).
    pub fn load(path: Option<PathBuf>) -> Self {
        let stored = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self { path, stored }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(raw) = serde_json::to_string(&self.stored) {
            let _ = std::fs::write(path, raw);
        }
    }

    /// What this read of `kind` found: the new items become cards and come
    /// back; the first read of a kind only marks what is there as seen.
    pub fn take(
        &mut self,
        kind: CheckInKind,
        found: Vec<CheckIn>,
        now: DateTime<Utc>,
    ) -> Vec<CheckIn> {
        let first = self.stored.started.insert(kind);
        let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut fresh = Vec::new();
        for mut item in found {
            if self.stored.seen.contains_key(&item.key) {
                continue;
            }
            self.stored.seen.insert(item.key.clone(), stamp.clone());
            if first {
                continue;
            }
            item.at = stamp.clone();
            item.unread = true;
            fresh.push(item);
        }
        self.stored.cards.extend(fresh.iter().cloned());
        self.stored
            .cards
            .retain(|card| !older_than(&card.at, now, KEEP_DAYS));
        self.stored
            .seen
            .retain(|_, at| !older_than(at, now, SEEN_DAYS));
        if first || !fresh.is_empty() {
            self.save();
        }
        fresh
    }

    pub fn cards(&self) -> &[CheckIn] {
        &self.stored.cards
    }

    pub fn unread(&self) -> usize {
        self.stored.cards.iter().filter(|card| card.unread).count()
    }

    pub fn mark_read(&mut self, key: &str) {
        let mut changed = false;
        for card in self.stored.cards.iter_mut().filter(|card| card.key == key) {
            changed |= std::mem::replace(&mut card.unread, false);
        }
        if changed {
            self.save();
        }
    }

    pub fn mark_all_read(&mut self) {
        if self.unread() > 0 {
            for card in &mut self.stored.cards {
                card.unread = false;
            }
            self.save();
        }
    }
}

/// A running turn's sign of life, as `PrWatch` reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    pub workspace_id: String,
    pub session_id: String,
    pub title: String,
    pub project: String,
    pub project_id: String,
    /// When its turn started; one stall per turn.
    pub started_at: String,
    pub last_event_at: Option<String>,
}

/// The running turns with no event for `STALLED_MINUTES`.
pub fn stalled(running: &[Running], now: DateTime<Utc>) -> Vec<CheckIn> {
    running
        .iter()
        .filter(|turn| {
            turn.last_event_at
                .as_deref()
                .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                .is_some_and(|at| {
                    now - at.with_timezone(&Utc) >= Duration::minutes(STALLED_MINUTES)
                })
        })
        .map(|turn| CheckIn {
            key: format!("stalled|{}|{}", turn.session_id, turn.started_at),
            kind: CheckInKind::TaskStalled,
            title: turn.title.clone(),
            place: turn.project.clone(),
            url: None,
            workspace_id: Some(turn.workspace_id.clone()),
            session_id: Some(turn.session_id.clone()),
            project_id: Some(turn.project_id.clone()),
            at: String::new(),
            unread: true,
        })
        .collect()
}

/// A pull request or an issue of the search as a check-in item.
pub fn asked(kind: CheckInKind, item: &crate::data::pr_hub::Asked) -> CheckIn {
    let tag = match kind {
        CheckInKind::ReviewRequested => "review",
        _ => "issue",
    };
    CheckIn {
        key: format!("{tag}|{}", item.url),
        kind,
        title: item.title.clone(),
        place: format!("{} #{}", item.repository, item.number),
        url: Some(item.url.clone()),
        workspace_id: None,
        session_id: None,
        project_id: item.project_id.clone(),
        at: String::new(),
        unread: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn issue(url: &str) -> CheckIn {
        asked(
            CheckInKind::IssueAssigned,
            &crate::data::pr_hub::Asked {
                url: url.into(),
                number: 4,
                repository: "acme/app".into(),
                title: "Bug no frete".into(),
                ..Default::default()
            },
        )
    }

    #[test]
    fn the_first_read_only_sets_what_is_there_then_only_new_items_are_cards() {
        let mut checks = CheckIns::load(None);
        let now = at("2026-11-08T10:00:00Z");
        assert!(
            checks
                .take(CheckInKind::IssueAssigned, vec![issue("u/1")], now)
                .is_empty()
        );
        let fresh = checks.take(
            CheckInKind::IssueAssigned,
            vec![issue("u/1"), issue("u/2")],
            now,
        );
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].place, "acme/app #4");
        assert_eq!(checks.unread(), 1);
        checks.mark_read("issue|u/2");
        assert_eq!(checks.unread(), 0);
        // A card older than a week goes.
        checks.take(
            CheckInKind::IssueAssigned,
            Vec::new(),
            at("2026-11-20T10:00:00Z"),
        );
        assert!(checks.cards().is_empty());
    }

    #[test]
    fn a_turn_with_no_event_for_twenty_minutes_is_stalled_once() {
        let turn = |last: Option<&str>| Running {
            workspace_id: "w-1".into(),
            session_id: "s-1".into(),
            title: "Checkout".into(),
            project: "Loja".into(),
            project_id: "p".into(),
            started_at: "2026-11-08T09:00:00Z".into(),
            last_event_at: last.map(str::to_string),
        };
        let now = at("2026-11-08T10:00:00Z");
        assert!(stalled(&[turn(Some("2026-11-08T09:50:00Z"))], now).is_empty());
        assert!(stalled(&[turn(None)], now).is_empty());
        let found = stalled(&[turn(Some("2026-11-08T09:39:00Z"))], now);
        assert_eq!(found[0].key, "stalled|s-1|2026-11-08T09:00:00Z");
        let mut checks = CheckIns::load(None);
        checks.take(CheckInKind::TaskStalled, Vec::new(), now);
        assert_eq!(
            checks
                .take(CheckInKind::TaskStalled, found.clone(), now)
                .len(),
            1
        );
        assert!(checks.take(CheckInKind::TaskStalled, found, now).is_empty());
    }
}
