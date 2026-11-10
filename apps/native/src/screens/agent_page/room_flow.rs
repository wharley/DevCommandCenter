//! What sits between the conversation's rows, as data: a day separator where
//! the local day changes, and which answers start a group (the same agent
//! again within five minutes with nothing in between draws no face or name).
//! Worked out when the rows are built (`room_rows`), never per frame.

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};

use super::room_rows::Row;
use crate::data::team_room::Speaker;

/// How long an agent's next answer still joins its last one.
const GROUP_WINDOW_MINUTES: i64 = 5;

fn when(row: &Row) -> Option<DateTime<Utc>> {
    let at = match row {
        Row::Line { line, .. } => &line.at,
        Row::Card { card, .. } => &card.at,
        Row::CheckIn { check, .. } => &check.at,
        _ => return None,
    };
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// `rows` with a day separator before the first row of each later local day,
/// and each agent line told whether it starts a group.
pub(crate) fn arrange(rows: Vec<Row>) -> Vec<Row> {
    let mut out = Vec::with_capacity(rows.len() + 4);
    let mut day: Option<NaiveDate> = None;
    // The agent and time of the row just before, when it was an answer.
    let mut before: Option<(String, DateTime<Utc>)> = None;
    for mut row in rows {
        let at = when(&row);
        if let Some(at) = at {
            let local = at.with_timezone(&Local).date_naive();
            if day.is_some_and(|day| day != local) {
                out.push(Row::Day { day: local });
                before = None;
            }
            day = Some(local);
        }
        let mut answer = None;
        if let Row::Line { line, first, .. } = &mut row
            && line.speaker == Speaker::Agent
        {
            *first = !before.as_ref().is_some_and(|(agent, last)| {
                *agent == line.agent_id
                    && at.is_some_and(|at| {
                        let gap = at - *last;
                        gap >= Duration::zero() && gap <= Duration::minutes(GROUP_WINDOW_MINUTES)
                    })
            });
            answer = at.map(|at| (line.agent_id.clone(), at));
        }
        before = answer;
        out.push(row);
    }
    out
}

/// "Hoje", "Ontem" or the date, as `today` sees it: `(key, date)` where a
/// key means the app's word for it and none the date to write.
pub(crate) fn nearness(day: NaiveDate, today: NaiveDate) -> Option<&'static str> {
    match (today - day).num_days() {
        0 => Some("agents.room.today"),
        1 => Some("agents.room.yesterday"),
        _ => None,
    }
}

/// The date of a day that is neither today nor yesterday: no year within
/// this one (`10 de nov.`, `Nov 10`), else with it.
pub(crate) fn date_text(day: NaiveDate, today: NaiveDate, english: bool) -> String {
    use chrono::Datelike;
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const PT: [&str; 12] = [
        "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.",
        "dez.",
    ];
    let month = day.month0() as usize;
    let year = day.year() != today.year();
    match (english, year) {
        (true, false) => format!("{} {}", EN[month], day.day()),
        (true, true) => format!("{} {}, {}", EN[month], day.day(), day.year()),
        (false, false) => format!("{} de {}", day.day(), PT[month]),
        (false, true) => format!("{} de {} de {}", day.day(), PT[month], day.year()),
    }
}

/// The answer's time of day (`10:05`, `10:05 AM`), in the local day.
pub(crate) fn clock(at: &str, english: bool) -> String {
    use chrono::Timelike;
    let Ok(at) = DateTime::parse_from_rfc3339(at) else {
        return at.to_string();
    };
    let at = at.with_timezone(&Local);
    if english {
        let (pm, hour) = at.hour12();
        format!("{hour}:{:02} {}", at.minute(), if pm { "PM" } else { "AM" })
    } else {
        format!("{:02}:{:02}", at.hour(), at.minute())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::data::team_room::RoomLine;

    fn at(day: u32, hour: u32, minute: u32) -> String {
        Local
            .with_ymd_and_hms(2026, 11, day, hour, minute, 0)
            .unwrap()
            .to_rfc3339()
    }

    fn line(index: usize, agent: &str, speaker: Speaker, at: String) -> Row {
        Row::Line {
            index,
            line: RoomLine {
                key: format!("k{index}"),
                agent_id: agent.into(),
                session_id: "s".into(),
                speaker,
                text: "oi".into(),
                at,
                receipt: None,
                tasks: Vec::new(),
                steps: Default::default(),
            },
            first: true,
            said: None,
            steps: None,
        }
    }

    fn shape(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Day { day } => format!("day {day}"),
                Row::Line { index, first, .. } => {
                    format!("{index}{}", if *first { "*" } else { "" })
                }
                _ => "?".into(),
            })
            .collect()
    }

    #[test]
    fn answers_of_the_same_agent_within_five_minutes_group() {
        let rows = arrange(vec![
            line(0, "dev", Speaker::Agent, at(10, 9, 0)),
            line(1, "dev", Speaker::Agent, at(10, 9, 5)),
            line(2, "dev", Speaker::Agent, at(10, 9, 11)),
            line(3, "rev", Speaker::Agent, at(10, 9, 12)),
            line(4, "dev", Speaker::Agent, at(10, 9, 13)),
        ]);
        assert_eq!(shape(&rows), ["0*", "1", "2*", "3*", "4*"]);
    }

    #[test]
    fn the_person_between_answers_breaks_the_group() {
        let rows = arrange(vec![
            line(0, "dev", Speaker::Agent, at(10, 9, 0)),
            line(1, "dev", Speaker::Person, at(10, 9, 1)),
            line(2, "dev", Speaker::Agent, at(10, 9, 2)),
            line(3, "dev", Speaker::Agent, at(10, 9, 3)),
        ]);
        assert_eq!(shape(&rows), ["0*", "1*", "2*", "3"]);
    }

    #[test]
    fn a_new_local_day_gets_a_separator_and_starts_a_group() {
        let rows = arrange(vec![
            line(0, "dev", Speaker::Agent, at(9, 23, 58)),
            line(1, "dev", Speaker::Agent, at(10, 0, 1)),
            line(2, "dev", Speaker::Agent, at(10, 0, 2)),
        ]);
        assert_eq!(shape(&rows), ["0*", "day 2026-11-10", "1*", "2"]);
    }

    #[test]
    fn the_first_row_has_no_separator_above_it() {
        let rows = arrange(vec![line(0, "dev", Speaker::Agent, at(10, 9, 0))]);
        assert_eq!(shape(&rows), ["0*"]);
    }

    #[test]
    fn days_read_as_today_yesterday_or_the_date() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        let day = |m, d, y| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(nearness(today, today), Some("agents.room.today"));
        assert_eq!(
            nearness(day(11, 9, 2026), today),
            Some("agents.room.yesterday")
        );
        assert_eq!(nearness(day(11, 8, 2026), today), None);
        assert_eq!(date_text(day(11, 8, 2026), today, false), "8 de nov.");
        assert_eq!(date_text(day(11, 8, 2026), today, true), "Nov 8");
        assert_eq!(
            date_text(day(12, 25, 2025), today, false),
            "25 de dez. de 2025"
        );
        assert_eq!(date_text(day(12, 25, 2025), today, true), "Dec 25, 2025");
    }

    #[test]
    fn the_clock_is_the_local_time_of_day() {
        assert_eq!(clock(&at(10, 14, 5), false), "14:05");
        assert_eq!(clock(&at(10, 14, 5), true), "2:05 PM");
        assert_eq!(clock("not a date", false), "not a date");
    }
}
