//! When each snooze choice ends, in the person's local time
//! (`workspace-snooze.ts` `snoozeUntil`).

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnoozeOption {
    Hour,
    Tomorrow,
    Monday,
}

impl SnoozeOption {
    pub const ALL: [Self; 3] = [Self::Hour, Self::Tomorrow, Self::Monday];

    /// `sidebar.snooze.*`
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Hour => "sidebar.snooze.hour",
            Self::Tomorrow => "sidebar.snooze.tomorrow",
            Self::Monday => "sidebar.snooze.monday",
        }
    }
}

/// An hour from now; tomorrow 09:00; next Monday 09:00 (a week on, on a Monday).
pub fn snooze_until(option: SnoozeOption, now: DateTime<Local>) -> DateTime<Local> {
    if option == SnoozeOption::Hour {
        return now + Duration::hours(1);
    }
    let nine = NaiveTime::from_hms_opt(9, 0, 0).expect("valid time");
    let days = match option {
        SnoozeOption::Tomorrow => 1,
        _ => {
            let from_sunday = now.weekday().num_days_from_sunday() as i64;
            match (8 - from_sunday) % 7 {
                0 => 7,
                days => days,
            }
        }
    };
    let date = now.date_naive() + Duration::days(days);
    Local
        .from_local_datetime(&date.and_time(nine))
        .earliest()
        .unwrap_or(now + Duration::days(days))
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
    fn snooze_choices_match_the_desktop() {
        let thursday = local("2026-10-22 15:30");
        assert_eq!(
            snooze_until(SnoozeOption::Hour, thursday),
            local("2026-10-22 16:30")
        );
        assert_eq!(
            snooze_until(SnoozeOption::Tomorrow, thursday),
            local("2026-10-23 09:00")
        );
        assert_eq!(
            snooze_until(SnoozeOption::Monday, thursday),
            local("2026-10-26 09:00")
        );
        let monday = local("2026-10-26 08:00");
        assert_eq!(
            snooze_until(SnoozeOption::Monday, monday),
            local("2026-11-02 09:00")
        );
    }
}
