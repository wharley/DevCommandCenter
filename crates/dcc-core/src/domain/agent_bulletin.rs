//! The agents' weekly bulletin (Tucano's "Boletim"): per agent, what the
//! last complete week held — answers, tasks it worked in, turns stopped,
//! tokens and the cost the providers reported, the triggers that woke it —
//! next to the week before, computed from what DCC recorded, with no model.
//! A week starts on Monday at midnight, in the person's local time.

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// One agent's week.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWeek {
    pub agent_id: String,
    /// Turns that finished.
    pub answers: u32,
    /// Turns stopped before the end.
    pub stopped: u32,
    /// The person's tasks it worked in (its own conversation aside).
    pub tasks: u32,
    pub tokens: u64,
    pub cost_usd: Option<f64>,
    /// Answers the person marked 👍 and 👎 in the team room.
    #[serde(default)]
    pub useful: u32,
    #[serde(default)]
    pub not_useful: u32,
}

/// A week: its first instant and the next week's, as stored timestamps compare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Week {
    /// `2026-11-02`, its Monday: the bulletin's reading marker.
    pub key: String,
    pub from: String,
    pub to: String,
    pub monday: DateTime<Local>,
}

fn stamp(at: DateTime<Local>) -> String {
    at.with_timezone(&Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The week `weeks_back` before the one `now` is in (1: the last complete one).
pub fn week_before(now: DateTime<Local>, weeks_back: i64) -> Week {
    let days = i64::from(now.weekday().num_days_from_monday());
    let this_monday = (now.date_naive() - Duration::days(days)).and_time(NaiveTime::MIN);
    let monday = this_monday - Duration::weeks(weeks_back);
    let start = Local.from_local_datetime(&monday).earliest().unwrap_or(now);
    let end = Local
        .from_local_datetime(&(monday + Duration::weeks(1)))
        .earliest()
        .unwrap_or(now);
    Week {
        key: monday.date().to_string(),
        from: stamp(start),
        to: stamp(end),
        monday: start,
    }
}

/// How much `now` moved from `before`, in percent; none without a base.
pub fn trend(now: u64, before: u64) -> Option<i64> {
    (before > 0).then(|| ((now as f64 - before as f64) / before as f64 * 100.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_complete_week_runs_monday_to_monday() {
        // A Wednesday.
        let now = Local.with_ymd_and_hms(2026, 11, 11, 15, 0, 0).unwrap();
        let last = week_before(now, 1);
        assert_eq!(last.key, "2026-11-02");
        let before = week_before(now, 2);
        assert_eq!(before.key, "2026-10-26");
        assert_eq!(before.to, last.from);
        assert_eq!(week_before(now, 0).key, "2026-11-09");
        // On a Monday the week before is the one that just ended.
        let monday = Local.with_ymd_and_hms(2026, 11, 9, 9, 0, 0).unwrap();
        assert_eq!(week_before(monday, 1).key, "2026-11-02");
        assert_eq!(trend(130, 100), Some(30));
        assert_eq!(trend(50, 100), Some(-50));
        assert_eq!(trend(5, 0), None);
    }
}
