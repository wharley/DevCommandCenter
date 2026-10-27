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

/// `Intl.RelativeTimeFormat(locale, { numeric: "auto" })` by the largest
/// unit (`formatReviewTimestamp`): "agora", "há 5 minutos", "ontem".
pub fn relative_time(at_ms: i64, now_ms: i64, english: bool) -> String {
    let delta = (at_ms - now_ms) as f64 / 1000.0;
    if delta.abs() < 45.0 {
        return if english { "now" } else { "agora" }.to_string();
    }
    const UNITS: [(&str, f64); 6] = [
        ("year", 31_557_600.0),
        ("month", 2_629_800.0),
        ("week", 604_800.0),
        ("day", 86_400.0),
        ("hour", 3_600.0),
        ("minute", 60.0),
    ];
    let (unit, size) = UNITS
        .iter()
        .copied()
        .find(|(_, size)| delta.abs() >= *size)
        .unwrap_or(("minute", 60.0));
    let value = (delta / size).round() as i64;
    let words = |pt: (&str, &str), en: (&str, &str)| {
        if english {
            if value.abs() == 1 { en.0 } else { en.1 }
        } else if value.abs() == 1 {
            pt.0
        } else {
            pt.1
        }
        .to_string()
    };
    // `numeric: "auto"` names the nearest ones.
    let named = match (unit, value) {
        ("day", -1) => Some(("ontem", "yesterday")),
        ("day", 1) => Some(("amanhã", "tomorrow")),
        ("week", -1) => Some(("semana passada", "last week")),
        ("week", 1) => Some(("próxima semana", "next week")),
        ("month", -1) => Some(("mês passado", "last month")),
        ("month", 1) => Some(("próximo mês", "next month")),
        ("year", -1) => Some(("ano passado", "last year")),
        ("year", 1) => Some(("próximo ano", "next year")),
        _ => None,
    };
    if let Some((pt, en)) = named {
        return if english { en } else { pt }.to_string();
    }
    let noun = match unit {
        "year" => words(("ano", "anos"), ("year", "years")),
        "month" => words(("mês", "meses"), ("month", "months")),
        "week" => words(("semana", "semanas"), ("week", "weeks")),
        "day" => words(("dia", "dias"), ("day", "days")),
        "hour" => words(("hora", "horas"), ("hour", "hours")),
        _ => words(("minuto", "minutos"), ("minute", "minutes")),
    };
    let count = value.abs();
    match (english, value < 0) {
        (false, true) => format!("há {count} {noun}"),
        (false, false) => format!("em {count} {noun}"),
        (true, true) => format!("{count} {noun} ago"),
        (true, false) => format!("in {count} {noun}"),
    }
}

/// `Intl.RelativeTimeFormat(locale, { numeric: "auto", style: "short" })`
/// by seconds, minutes, hours, then days (`relativeTimeLabel`): "agora",
/// "há 5 min.", "há 3 h", "ontem", "há 4 dias".
pub fn short_relative(at_ms: i64, now_ms: i64, english: bool) -> String {
    let seconds = ((at_ms - now_ms) as f64 / 1000.0).round() as i64;
    let past = seconds <= 0;
    let abs = seconds.abs();
    let (value, unit) = if abs < 60 {
        (abs, "second")
    } else if abs < 3600 {
        ((abs as f64 / 60.0).round() as i64, "minute")
    } else if abs < 86_400 {
        ((abs as f64 / 3600.0).round() as i64, "hour")
    } else {
        ((abs as f64 / 86_400.0).round() as i64, "day")
    };
    match (unit, value, past) {
        ("second", 0, _) => return if english { "now" } else { "agora" }.to_string(),
        ("day", 1, true) => return if english { "yesterday" } else { "ontem" }.to_string(),
        ("day", 1, false) => return if english { "tomorrow" } else { "amanhã" }.to_string(),
        _ => {}
    }
    let word = match (unit, english) {
        ("second", false) => "seg.".to_string(),
        ("minute", false) => "min.".to_string(),
        ("hour", false) => "h".to_string(),
        ("day", false) => if value == 1 { "dia" } else { "dias" }.to_string(),
        ("second", true) => "sec.".to_string(),
        ("minute", true) => "min.".to_string(),
        ("hour", true) => "hr.".to_string(),
        _ => if value == 1 { "day" } else { "days" }.to_string(),
    };
    match (english, past) {
        (false, true) => format!("há {value} {word}"),
        (false, false) => format!("em {value} {word}"),
        (true, true) => format!("{value} {word} ago"),
        (true, false) => format!("in {value} {word}"),
    }
}

#[cfg(test)]
mod short_relative_tests {
    use super::short_relative;

    #[test]
    fn reads_like_the_short_intl_format() {
        let now = 1_000_000_000_000;
        let ago = |seconds: i64| short_relative(now - seconds * 1000, now, false);
        assert_eq!(ago(0), "agora");
        assert_eq!(ago(30), "há 30 seg.");
        assert_eq!(ago(5 * 60), "há 5 min.");
        assert_eq!(ago(3 * 3600), "há 3 h");
        assert_eq!(ago(30 * 3600), "ontem");
        assert_eq!(ago(4 * 86_400), "há 4 dias");
        assert_eq!(short_relative(now - 7_200_000, now, true), "2 hr. ago");
    }
}

#[cfg(test)]
mod relative_tests {
    use super::relative_time;

    #[test]
    fn reads_like_intl_relative_time() {
        let now = 1_000_000_000_000;
        let ago = |seconds: i64| relative_time(now - seconds * 1000, now, false);
        assert_eq!(ago(10), "agora");
        assert_eq!(ago(60), "há 1 minuto");
        assert_eq!(ago(3 * 3600), "há 3 horas");
        assert_eq!(ago(30 * 3600), "ontem");
        assert_eq!(ago(3 * 86_400), "há 3 dias");
        assert_eq!(ago(8 * 86_400), "semana passada");
        assert_eq!(relative_time(now - 7_200_000, now, true), "2 hours ago");
    }
}

/// `new Date(value).toLocaleString()` in pt-BR (`24/10/2026, 12:00:00`), or
/// the value as it came when it is not a date.
pub fn local_date_time(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%d/%m/%Y, %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

/// `new Date(value).toLocaleDateString(language)`: `26/10/2026` or
/// `10/26/2026`, in the local day; the value as it came when not a date.
pub fn local_date(value: &str, english: bool) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format(if english { "%-m/%-d/%Y" } else { "%d/%m/%Y" })
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

/// `toLocaleTimeString()` of a millisecond time (`14:03:22`, `2:03:22 PM`);
/// `None` when it is not one (`browserAuditTime`).
pub fn local_clock(at_ms: i64, english: bool) -> Option<String> {
    use chrono::Timelike;
    if at_ms < 0 {
        return None;
    }
    let at = chrono::DateTime::from_timestamp_millis(at_ms)?.with_timezone(&chrono::Local);
    Some(if english {
        let (pm, hour) = at.hour12();
        format!(
            "{hour}:{:02}:{:02} {}",
            at.minute(),
            at.second(),
            if pm { "PM" } else { "AM" }
        )
    } else {
        format!("{:02}:{:02}:{:02}", at.hour(), at.minute(), at.second())
    })
}
