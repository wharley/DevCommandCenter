//! Timestamps as the rail reads them: ISO strings from the database, local
//! calendar days, and the compact "2h 5min" elapsed format.

use chrono::{DateTime, Local, TimeZone};

/// `Date.parse` for the RFC 3339 stamps the backend writes.
pub fn parse_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|time| time.timestamp_millis())
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Midnight of the local day `ms` falls in.
pub fn start_of_local_day(ms: i64) -> i64 {
    let Some(time) = Local.timestamp_millis_opt(ms).single() else {
        return ms;
    };
    let midnight = time.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map_or(ms, |day| day.timestamp_millis())
}

/// Unit suffixes, from `sidebar.activityTime.units`.
#[derive(Clone, Debug)]
pub struct ElapsedUnits {
    pub second: String,
    pub minute: String,
    pub hour: String,
    pub day: String,
    pub month: String,
    pub months: String,
}

/// `formatCompactElapsedTime`
pub fn compact_elapsed(elapsed_ms: i64, units: &ElapsedUnits) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let seconds = (elapsed_ms / 1000).max(0);
    if seconds < MINUTE {
        return format!("{seconds}{}", units.second);
    }
    let minutes = seconds / MINUTE;
    if minutes < 60 {
        return format!("{minutes}{}", units.minute);
    }
    if seconds < DAY {
        let (hours, rest) = (minutes / 60, minutes % 60);
        return if rest > 0 {
            format!("{hours}{} {rest}{}", units.hour, units.minute)
        } else {
            format!("{hours}{}", units.hour)
        };
    }
    let days = seconds / DAY;
    if days < 30 {
        let hours = (seconds % DAY) / HOUR;
        return if hours > 0 {
            format!("{days}{} {hours}{}", units.day, units.hour)
        } else {
            format!("{days}{}", units.day)
        };
    }
    let (months, rest) = (days / 30, days % 30);
    let label = if months == 1 {
        &units.month
    } else {
        &units.months
    };
    if rest > 0 {
        format!("{months}{label} {rest}{}", units.day)
    } else {
        format!("{months}{label}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> ElapsedUnits {
        ElapsedUnits {
            second: "s".into(),
            minute: "min".into(),
            hour: "h".into(),
            day: "d".into(),
            month: "mês".into(),
            months: "meses".into(),
        }
    }

    #[test]
    fn elapsed_matches_the_desktop_format() {
        let u = units();
        assert_eq!(compact_elapsed(-5, &u), "0s");
        assert_eq!(compact_elapsed(42_000, &u), "42s");
        assert_eq!(compact_elapsed(5 * 60_000, &u), "5min");
        assert_eq!(compact_elapsed(2 * 3_600_000, &u), "2h");
        assert_eq!(compact_elapsed(2 * 3_600_000 + 5 * 60_000, &u), "2h 5min");
        assert_eq!(compact_elapsed(3 * 86_400_000 + 4 * 3_600_000, &u), "3d 4h");
        assert_eq!(compact_elapsed(31 * 86_400_000, &u), "1mês 1d");
        assert_eq!(compact_elapsed(60 * 86_400_000, &u), "2meses");
    }

    #[test]
    fn parses_backend_stamps() {
        assert_eq!(parse_ms("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(parse_ms("not a date"), None);
    }

    #[test]
    fn local_days_step_back_by_calendar_day() {
        let now = parse_ms("2026-10-23T15:00:00Z").unwrap();
        let today = start_of_local_day(now);
        assert!(today <= now && now - today < 86_400_000);
        let yesterday = start_of_local_day(today - 1);
        assert!(today - yesterday >= 23 * 3_600_000 && today - yesterday <= 25 * 3_600_000);
    }
}
