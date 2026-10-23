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

/// date-fns `formatDistanceToNow(date, { addSuffix: true })` in the two
/// locales the desktop ships: "há 5 minutos", "about 2 hours ago".
pub fn distance_to_now(at_ms: i64, now_ms: i64, english: bool) -> String {
    let seconds = (now_ms - at_ms) as f64 / 1000.0;
    let future = seconds < 0.0;
    let seconds = seconds.abs();
    let minutes = (seconds / 60.0).round() as i64;
    let words = |pt_one: &str, pt_other: &str, en_one: &str, en_other: &str, count: i64| {
        let template = match (english, count == 1) {
            (false, true) => pt_one,
            (false, false) => pt_other,
            (true, true) => en_one,
            (true, false) => en_other,
        };
        template.replace("{}", &count.to_string())
    };
    let text = if minutes < 1 {
        words("menos de um minuto", "", "less than a minute", "", 1)
    } else if minutes < 45 {
        words("1 minuto", "{} minutos", "1 minute", "{} minutes", minutes)
    } else if minutes < 1440 {
        let hours = ((minutes as f64) / 60.0).round().max(1.0) as i64;
        words(
            "cerca de 1 hora",
            "cerca de {} horas",
            "about 1 hour",
            "about {} hours",
            hours,
        )
    } else if minutes < 2520 {
        words("1 dia", "", "1 day", "", 1)
    } else if minutes < 43_200 {
        let days = ((minutes as f64) / 1440.0).round() as i64;
        words("1 dia", "{} dias", "1 day", "{} days", days)
    } else if minutes < 86_400 {
        let months = ((minutes as f64) / 43_200.0).round() as i64;
        words(
            "cerca de 1 mês",
            "cerca de {} meses",
            "about 1 month",
            "about {} months",
            months,
        )
    } else {
        let months = minutes / 43_200;
        if months < 12 {
            let nearest = ((minutes as f64) / 43_200.0).round().max(1.0) as i64;
            words("1 mês", "{} meses", "1 month", "{} months", nearest)
        } else {
            let years = months / 12;
            match months % 12 {
                0..=2 => words(
                    "cerca de 1 ano",
                    "cerca de {} anos",
                    "about 1 year",
                    "about {} years",
                    years,
                ),
                3..=8 => words(
                    "mais de 1 ano",
                    "mais de {} anos",
                    "over 1 year",
                    "over {} years",
                    years,
                ),
                _ => words(
                    "quase 1 ano",
                    "quase {} anos",
                    "almost 1 year",
                    "almost {} years",
                    years + 1,
                ),
            }
        }
    };
    match (english, future) {
        (false, false) => format!("há {text}"),
        (false, true) => format!("em {text}"),
        (true, false) => format!("{text} ago"),
        (true, true) => format!("in {text}"),
    }
}

#[cfg(test)]
mod distance_tests {
    use super::distance_to_now;

    #[test]
    fn reads_like_date_fns() {
        let now = 1_000_000_000_000;
        let ago = |seconds: i64| distance_to_now(now - seconds * 1000, now, false);
        assert_eq!(ago(10), "há menos de um minuto");
        assert_eq!(ago(60), "há 1 minuto");
        assert_eq!(ago(5 * 60), "há 5 minutos");
        assert_eq!(ago(50 * 60), "há cerca de 1 hora");
        assert_eq!(ago(3 * 3600), "há cerca de 3 horas");
        assert_eq!(ago(30 * 3600), "há 1 dia");
        assert_eq!(ago(4 * 86_400), "há 4 dias");
        assert_eq!(distance_to_now(now - 120_000, now, true), "2 minutes ago");
    }
}
