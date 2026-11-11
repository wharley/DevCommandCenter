//! A resident agent's routines (Monocode's habits, Tucano's calendar): days
//! of the week, a time and a request; at that time the agent gets the
//! request in its own conversation and, when there is nothing to report,
//! answers a fixed word the team room keeps quiet. A routine missed while
//! the app was closed runs once when it opens, within `CATCH_UP_HOURS`. An
//! agent may propose a routine; it runs only after the person creates it.
//! Times are the person's local time.

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};
use specta::Type;

pub const MAX_ROUTINES: usize = 5;
pub const CATCH_UP_HOURS: i64 = 12;
pub const MAX_ROUTINE_PROMPT_CHARS: usize = 2_000;
pub const PROPOSE_ROUTINE_TOOL: &str = "dcc_propose_routine";
/// How a routine's turn opens in the agent's conversation.
pub const ROUTINE_MARK: &str = "[DCC] Routine";
/// The whole answer of a routine with nothing to report.
pub const QUIET_ANSWER: &str = "NOTHING_TO_REPORT";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: String,
    pub agent_id: String,
    /// 0 = Monday … 6 = Sunday.
    pub days: Vec<u8>,
    pub hour: u8,
    pub minute: u8,
    pub prompt: String,
    pub enabled: bool,
    /// Proposed by the agent, waiting for the person.
    pub proposed: bool,
    pub last_run_at: Option<String>,
    pub created_at: String,
}

/// What the person (or the agent proposing it) writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoutineDraft {
    pub days: Vec<u8>,
    pub hour: u8,
    pub minute: u8,
    pub prompt: String,
}

impl RoutineDraft {
    pub fn normalized(mut self) -> Result<Self, String> {
        self.days.sort_unstable();
        self.days.dedup();
        if self.days.is_empty() || self.days.iter().any(|day| *day > 6) {
            return Err("a routine needs at least one day, 0 (Monday) to 6 (Sunday)".into());
        }
        if self.hour > 23 || self.minute > 59 {
            return Err("a routine's time is hour 0–23 and minute 0–59".into());
        }
        self.prompt = self.prompt.trim().to_string();
        let chars = self.prompt.chars().count();
        if chars == 0 || chars > MAX_ROUTINE_PROMPT_CHARS {
            return Err(format!(
                "a routine's request is 1 to {MAX_ROUTINE_PROMPT_CHARS} characters"
            ));
        }
        Ok(self)
    }
}

fn parse(at: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&Local))
}

fn at_time(day: chrono::NaiveDate, hour: u8, minute: u8) -> Option<DateTime<Local>> {
    let time = NaiveTime::from_hms_opt(u32::from(hour), u32::from(minute), 0)?;
    Local.from_local_datetime(&day.and_time(time)).earliest()
}

impl Routine {
    fn scheduled(&self, day: chrono::NaiveDate) -> Option<DateTime<Local>> {
        let weekday = day.weekday().num_days_from_monday() as u8;
        self.days
            .contains(&weekday)
            .then(|| at_time(day, self.hour, self.minute))
            .flatten()
    }

    /// The time it is due now: its latest time at or before `now`, within
    /// the catch-up, after its last run (or its creation).
    pub fn due(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        if !self.enabled || self.proposed {
            return None;
        }
        let since = self
            .last_run_at
            .as_deref()
            .and_then(parse)
            .or_else(|| parse(&self.created_at))?;
        (0..=1)
            .filter_map(|back| self.scheduled(now.date_naive() - Duration::days(back)))
            .filter(|at| *at <= now && now - *at <= Duration::hours(CATCH_UP_HOURS))
            .filter(|at| *at > since)
            .max()
    }

    /// When it runs next, from `now` on.
    pub fn next_run(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        if !self.enabled || self.proposed {
            return None;
        }
        (0..=7)
            .filter_map(|ahead| self.scheduled(now.date_naive() + Duration::days(ahead)))
            .find(|at| *at > now)
    }
}

/// What the agent gets at the routine's time.
pub fn routine_prompt(routine: &Routine) -> String {
    format!(
        "{ROUTINE_MARK} ({:02}:{:02}): {}\n\nThis is a routine the person scheduled, not a message they just wrote. Do what it asks with your tools; to change code, open a task with dcc_open_task (it starts, or waits as a card, as the person set for you). If there is nothing worth the person's attention, answer exactly {QUIET_ANSWER} and nothing else.",
        routine.hour, routine.minute, routine.prompt
    )
}

/// `Seg, Qua · 09:00` / `Mon, Wed · 09:00`; every day: `Todo dia`.
pub fn schedule_label(routine: &Routine, english: bool) -> String {
    const PT: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let names = if english { EN } else { PT };
    let days = match routine.days.as_slice() {
        [0, 1, 2, 3, 4, 5, 6] => (if english { "Every day" } else { "Todo dia" }).to_string(),
        [0, 1, 2, 3, 4] => (if english { "Weekdays" } else { "Dias úteis" }).to_string(),
        days => days
            .iter()
            .filter_map(|day| names.get(*day as usize))
            .copied()
            .collect::<Vec<_>>()
            .join(", "),
    };
    format!("{days} · {:02}:{:02}", routine.hour, routine.minute)
}

/// Whether an answer is a routine's "nothing to report".
pub fn is_quiet(answer: &str) -> bool {
    answer.trim().trim_matches('`') == QUIET_ANSWER
}

/// The request of a routine's turn, when `text` is one.
pub fn routine_request(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(ROUTINE_MARK)?;
    let first = rest.lines().next()?;
    first.split_once("): ").map(|(_, request)| request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn routine(days: Vec<u8>, last: Option<&str>) -> Routine {
        Routine {
            id: "r".into(),
            agent_id: "qa".into(),
            days,
            hour: 9,
            minute: 0,
            prompt: "O que travou no Checkout?".into(),
            enabled: true,
            proposed: false,
            last_run_at: last.map(str::to_string),
            created_at: Local
                .with_ymd_and_hms(2026, 11, 1, 0, 0, 0)
                .unwrap()
                .to_rfc3339(),
        }
    }

    #[test]
    fn a_routine_is_due_once_at_its_time_and_catches_up_within_twelve_hours() {
        // Monday 9h.
        let monday = Local.with_ymd_and_hms(2026, 11, 9, 9, 30, 0).unwrap();
        let every_monday = routine(vec![0], None);
        let due = every_monday.due(monday).unwrap();
        assert_eq!((due.hour(), due.minute()), (9, 0));
        let ran = routine(vec![0], Some(&due.to_rfc3339()));
        assert!(ran.due(monday).is_none(), "once");
        // Missed by more than twelve hours: skipped until next time.
        let too_late = Local.with_ymd_and_hms(2026, 11, 9, 22, 0, 0).unwrap();
        assert!(every_monday.due(too_late).is_none());
        // Not its day.
        let tuesday = Local.with_ymd_and_hms(2026, 11, 10, 10, 0, 0).unwrap();
        assert!(routine(vec![0], Some(&due.to_rfc3339()))
            .due(tuesday)
            .is_none());
        // A proposed or switched-off routine never runs.
        let mut proposed = routine(vec![0], None);
        proposed.proposed = true;
        assert!(proposed.due(monday).is_none());
        let next = ran.next_run(monday).unwrap();
        assert_eq!(next.date_naive().to_string(), "2026-11-16");
    }

    #[test]
    fn a_draft_is_checked_and_a_routines_turn_reads_back() {
        let draft = RoutineDraft {
            days: vec![4, 0, 0],
            hour: 9,
            minute: 30,
            prompt: "  Resuma a semana  ".into(),
        }
        .normalized()
        .unwrap();
        assert_eq!(draft.days, [0, 4]);
        assert_eq!(draft.prompt, "Resuma a semana");
        let bad = |days: Vec<u8>, hour: u8| {
            RoutineDraft {
                days,
                hour,
                minute: 0,
                prompt: "x".into(),
            }
            .normalized()
            .is_err()
        };
        assert!(bad(vec![], 9) && bad(vec![7], 9) && bad(vec![0], 24));
        let prompt = routine_prompt(&routine(vec![0], None));
        assert_eq!(routine_request(&prompt), Some("O que travou no Checkout?"));
        assert!(prompt.contains(QUIET_ANSWER));
        assert!(is_quiet(" NOTHING_TO_REPORT \n") && is_quiet("`NOTHING_TO_REPORT`"));
        assert!(!is_quiet("Nada travou, mas o CI está lento."));
        assert_eq!(routine_request("oi"), None);
    }
}
