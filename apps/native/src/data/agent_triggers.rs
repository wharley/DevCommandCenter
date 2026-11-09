//! What wakes a resident agent by itself, as the person set it on the
//! agent's page: every trigger starts off and is turned on per agent, and
//! the runs it starts have a ceiling — `DAILY_RUNS` a day for the agent and
//! `TARGET_RUNS` a day for one pull request or task — so a busy day does not
//! turn into an open tab of paid runs. Kept in the native app's own file
//! (`agent-triggers.json`), with the runs of the last two weeks.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};

/// Runs a day for one agent, every trigger together.
pub const DAILY_RUNS: usize = 5;
/// Runs a day for one pull request or task.
pub const TARGET_RUNS: usize = 2;
/// Runs older than this are forgotten: two weeks, as the bulletin compares
/// the last week with the one before.
const KEEP_DAYS: i64 = 14;

/// Something that wakes an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    /// A pull request the person reviewed got commits or replies.
    PrUpdated,
    /// The checks of a task's pull request failed.
    CiFailed,
    /// Someone reviewed or commented on a task's pull request.
    ReviewReceived,
    /// A pull request asks for the person's review (the check-in).
    ReviewRequested,
    /// An issue was assigned to the person (the check-in).
    IssueAssigned,
    /// A task's turn is open with no sign of life (the check-in).
    TaskStalled,
    /// A routine's time came (`routines`); not a switch, and not counted in
    /// the daily ceiling, only in the brake (`agent_brake`).
    Routine,
}

impl Trigger {
    pub const ALL: [Trigger; 6] = [
        Trigger::PrUpdated,
        Trigger::ReviewRequested,
        Trigger::CiFailed,
        Trigger::ReviewReceived,
        Trigger::IssueAssigned,
        Trigger::TaskStalled,
    ];

    /// `agents.triggers.<id>.*`
    pub fn id(self) -> &'static str {
        match self {
            Self::PrUpdated => "prUpdated",
            Self::CiFailed => "ciFailed",
            Self::ReviewReceived => "reviewReceived",
            Self::ReviewRequested => "reviewRequested",
            Self::IssueAssigned => "issueAssigned",
            Self::TaskStalled => "taskStalled",
            Self::Routine => "routine",
        }
    }

    /// The pull request hub's review is the Reviewer's own job; the others
    /// wake any agent in its conversation.
    pub fn reviewer_only(self) -> bool {
        matches!(self, Self::PrUpdated | Self::ReviewRequested)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub agent_id: String,
    pub trigger: Trigger,
    /// The pull request's URL or the task's id.
    pub target: String,
    pub at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct Stored {
    /// The triggers turned on, by agent.
    on: BTreeMap<String, Vec<Trigger>>,
    runs: Vec<Run>,
    /// The failing commit each pull request was last looked at for, by
    /// `<agent>|<url>`: one failure wakes the agent once, a new push again.
    failures: BTreeMap<String, String>,
    /// The latest review activity seen on each pull request, by
    /// `<agent>|<url>`: only what comes after it wakes the agent.
    received: BTreeMap<String, String>,
    /// "Pausar todos": no trigger wakes anyone.
    pub(super) paused: bool,
    /// The hours no trigger wakes anyone (`agent_quiet`).
    pub(super) quiet: Option<super::agent_quiet::QuietHours>,
    /// When the person last talked to each agent (`agent_brake`).
    pub(super) attended: BTreeMap<String, String>,
}

#[derive(Default)]
pub struct AgentTriggers {
    path: Option<PathBuf>,
    pub(super) stored: Stored,
}

impl Global for AgentTriggers {}

/// Why a trigger does not run now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    Off,
    /// "Pausar todos".
    Paused,
    /// Inside the quiet hours.
    Quiet,
    /// Too many runs in a row without the person (`agent_brake`).
    Unattended,
    DailyLimit,
    TargetLimit,
}

fn local_day(at: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|time| time.with_timezone(&Local).date_naive())
}

impl AgentTriggers {
    pub fn init(cx: &mut App) {
        cx.set_global(Self::load(crate::prefs::native_file("agent-triggers.json")));
    }

    pub fn load(path: PathBuf) -> Self {
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            stored,
        }
    }

    pub(super) fn save(&self) {
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

    pub fn is_on(&self, agent_id: &str, trigger: Trigger) -> bool {
        self.stored
            .on
            .get(agent_id)
            .is_some_and(|on| on.contains(&trigger))
    }

    /// Whether the agent has any trigger on (its mascot keeps an eye open).
    pub fn any_on(&self, agent_id: &str) -> bool {
        self.stored
            .on
            .get(agent_id)
            .is_some_and(|on| !on.is_empty())
    }

    pub fn set(&mut self, agent_id: &str, trigger: Trigger, on: bool) {
        let triggers = self.stored.on.entry(agent_id.to_string()).or_default();
        triggers.retain(|candidate| *candidate != trigger);
        if on {
            triggers.push(trigger);
            triggers.sort();
        }
        if triggers.is_empty() {
            self.stored.on.remove(agent_id);
        }
        // Turned on again: what came while it was off is not news.
        if on && trigger == Trigger::ReviewReceived {
            let prefix = format!("{agent_id}|");
            self.stored
                .received
                .retain(|key, _| !key.starts_with(&prefix));
        }
        self.save();
    }

    /// The agent's runs today, in the person's day.
    pub fn runs_today(&self, agent_id: &str, now: DateTime<Utc>) -> usize {
        let today = now.with_timezone(&Local).date_naive();
        self.stored
            .runs
            .iter()
            .filter(|run| run.agent_id == agent_id && local_day(&run.at) == Some(today))
            .filter(|run| run.trigger != Trigger::Routine)
            .count()
    }

    /// Whether `trigger` may wake the agent for `target` now.
    pub fn check(
        &self,
        agent_id: &str,
        trigger: Trigger,
        target: &str,
        now: DateTime<Utc>,
    ) -> Result<(), Held> {
        if !self.is_on(agent_id, trigger) {
            return Err(Held::Off);
        }
        self.quiet_check(now)?;
        self.brake_check(agent_id)?;
        if self.runs_today(agent_id, now) >= DAILY_RUNS {
            return Err(Held::DailyLimit);
        }
        let today = now.with_timezone(&Local).date_naive();
        let for_target = self
            .stored
            .runs
            .iter()
            .filter(|run| {
                run.agent_id == agent_id
                    && run.target == target
                    && local_day(&run.at) == Some(today)
            })
            .count();
        if for_target >= TARGET_RUNS {
            return Err(Held::TargetLimit);
        }
        Ok(())
    }

    /// The checks failing on `sha` were not looked at yet for this agent.
    pub fn is_new_failure(&self, agent_id: &str, url: &str, sha: &str) -> bool {
        self.stored
            .failures
            .get(&format!("{agent_id}|{url}"))
            .is_none_or(|seen| seen != sha)
    }

    /// A review activity seen on `url`: true when it came after the one
    /// seen before. The first sight only sets where it starts, so old
    /// reviews never wake the agent.
    pub fn advance_received(&mut self, agent_id: &str, url: &str, latest: &str) -> bool {
        let key = format!("{agent_id}|{url}");
        let news = match self.stored.received.get(&key) {
            None => false,
            Some(seen) => match (
                DateTime::parse_from_rfc3339(latest),
                DateTime::parse_from_rfc3339(seen),
            ) {
                (Ok(latest), Ok(seen)) => latest > seen,
                _ => false,
            },
        };
        if news || !self.stored.received.contains_key(&key) {
            self.stored.received.insert(key, latest.to_string());
            self.save();
        }
        news
    }

    /// The failure on `sha` is handled (looked at, or held by a ceiling).
    pub fn mark_failure(&mut self, agent_id: &str, url: &str, sha: &str) {
        self.stored
            .failures
            .insert(format!("{agent_id}|{url}"), sha.to_string());
        self.save();
    }

    /// Pull requests no longer failing are forgotten, so failing again later
    /// is news.
    pub fn keep_failures(&mut self, agent_id: &str, failing: &[&str]) {
        let before = self.stored.failures.len();
        self.stored.failures.retain(|key, _| {
            key.split_once('|')
                .is_none_or(|(agent, url)| agent != agent_id || failing.contains(&url))
        });
        if self.stored.failures.len() != before {
            self.save();
        }
    }

    /// The runs of the last week, oldest first.
    pub fn runs(&self) -> &[Run] {
        &self.stored.runs
    }

    /// A run started; runs older than a week go.
    pub fn record(&mut self, agent_id: &str, trigger: Trigger, target: &str, now: DateTime<Utc>) {
        let cutoff = now - Duration::days(KEEP_DAYS);
        self.stored.runs.retain(|run| {
            DateTime::parse_from_rfc3339(&run.at).is_ok_and(|at| at.with_timezone(&Utc) > cutoff)
        });
        self.stored.runs.push(Run {
            agent_id: agent_id.to_string(),
            trigger,
            target: target.to_string(),
            at: now.to_rfc3339(),
        });
        self.save();
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

    #[test]
    fn a_trigger_is_off_until_turned_on_and_runs_stop_at_the_ceilings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-triggers.json");
        let mut triggers = AgentTriggers::load(path.clone());
        let now = at("2026-11-06T15:00:00Z");
        let check = |triggers: &AgentTriggers, target: &str, now| {
            triggers.check("rev", Trigger::PrUpdated, target, now)
        };
        assert_eq!(check(&triggers, "pr-1", now), Err(Held::Off));
        assert!(!triggers.any_on("rev"));

        triggers.set("rev", Trigger::PrUpdated, true);
        assert!(triggers.any_on("rev"));
        assert_eq!(check(&triggers, "pr-1", now), Ok(()));
        for _ in 0..TARGET_RUNS {
            triggers.record("rev", Trigger::PrUpdated, "pr-1", now);
        }
        assert_eq!(check(&triggers, "pr-1", now), Err(Held::TargetLimit));
        for index in TARGET_RUNS..DAILY_RUNS {
            triggers.record("rev", Trigger::PrUpdated, &format!("pr-{index}"), now);
        }
        assert_eq!(check(&triggers, "pr-new", now), Err(Held::DailyLimit));
        // Another agent has its own ceiling.
        triggers.set("other", Trigger::PrUpdated, true);
        assert_eq!(
            triggers.check("other", Trigger::PrUpdated, "pr-1", now),
            Ok(())
        );

        // The next day starts over; the setting and the runs are kept.
        let tomorrow = now + Duration::days(1);
        let again = AgentTriggers::load(path.clone());
        assert!(again.is_on("rev", Trigger::PrUpdated));
        assert_eq!(again.runs_today("rev", now), DAILY_RUNS);
        assert_eq!(
            again.check("rev", Trigger::PrUpdated, "pr-1", tomorrow),
            Ok(())
        );

        // Two weeks later the old runs are gone.
        let mut later = again;
        later.record("rev", Trigger::PrUpdated, "pr-9", now + Duration::days(15));
        assert_eq!(later.stored.runs.len(), 1);

        // A failure is news once per commit, and again after it recovers.
        assert!(later.is_new_failure("rev", "pr-1", "abc"));
        later.mark_failure("rev", "pr-1", "abc");
        assert!(!later.is_new_failure("rev", "pr-1", "abc"));
        assert!(later.is_new_failure("rev", "pr-1", "def"), "a new push");
        assert!(later.is_new_failure("other", "pr-1", "abc"), "per agent");
        later.keep_failures("rev", &["pr-2"]);
        assert!(later.is_new_failure("rev", "pr-1", "abc"), "it recovered");

        // Reviews: the first sight is where it starts; only later ones wake.
        assert!(!later.advance_received("rev", "pr-1", "2026-11-06T10:00:00Z"));
        assert!(!later.advance_received("rev", "pr-1", "2026-11-06T10:00:00Z"));
        assert!(later.advance_received("rev", "pr-1", "2026-11-06T11:00:00Z"));
        assert!(!later.advance_received("rev", "pr-1", "2026-11-06T11:00:00Z"));
        // Turned on again: it starts over, so the time off is not news.
        later.set("rev", Trigger::ReviewReceived, true);
        assert!(!later.advance_received("rev", "pr-1", "2026-11-06T12:00:00Z"));
        later.set("rev", Trigger::ReviewReceived, false);

        later.set("rev", Trigger::PrUpdated, false);
        assert!(!AgentTriggers::load(path).any_on("rev"));
    }
}
