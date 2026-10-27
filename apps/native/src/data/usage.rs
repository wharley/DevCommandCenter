//! "DCC Usage" (`UsageDialog`): the dashboard read straight from the
//! database (`usage_dashboard`), and the numbers as the desktop formats them
//! — `Intl.NumberFormat` compact from 10 000 on, the cost in dollars, the
//! shares in percent — in the app's language.

use std::path::Path;

use dcc_core::domain::usage::{UsageDashboard, UsageDashboardInput};
use dcc_core::ports::UsageRepo;

use crate::i18n::Locale;

/// `PROVIDER_COLORS` (`providerUsageColor`), as `0xRRGGBB`.
pub fn provider_color(provider: &str) -> u32 {
    match provider {
        "codex" => 0x34d399,
        "claude_code" => 0xfb923c,
        "gemini" => 0x60a5fa,
        "cursor" => 0xa78bfa,
        "droid" => 0x22d3ee,
        "grok" => 0xf472b6,
        _ => 0x94a3b8,
    }
}

/// `loadUsageDashboard({ periodDays, projectId })`
pub fn load(
    db_path: &Path,
    period_days: Option<u32>,
    project_id: Option<String>,
) -> anyhow::Result<UsageDashboard> {
    let repo = dcc_infra::db::SqliteSessionRepo::open(db_path)?;
    Ok(futures::executor::block_on(repo.usage_dashboard(
        &UsageDashboardInput {
            period_days,
            project_id,
        },
    ))?)
}

/// `percent`: the share of `total`, kept in 0–100.
pub fn percent(value: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (value as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
}

fn separators(locale: Locale) -> (char, char) {
    match locale {
        Locale::PtBr => ('.', ','),
        Locale::En => (',', '.'),
    }
}

/// The whole number with the language's grouping (`1.234` / `1,234`).
fn grouped(value: u64, locale: Locale) -> String {
    let (group, _) = separators(locale);
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(group);
        }
        out.push(digit);
    }
    out
}

/// `value` with at most one decimal, the trailing zero dropped.
fn one_decimal(value: f64, locale: Locale) -> String {
    let (_, decimal) = separators(locale);
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}").replace('.', &decimal.to_string())
    }
}

/// `compactNumber`: plain below 10 000, then "12,3 mil" / "12.3K".
pub fn compact(value: u64, locale: Locale) -> String {
    if value < 10_000 {
        return grouped(value, locale);
    }
    let units: [(f64, &str, &str); 4] = [
        (1e12, "\u{a0}tri", "T"),
        (1e9, "\u{a0}bi", "B"),
        (1e6, "\u{a0}mi", "M"),
        (1e3, "\u{a0}mil", "K"),
    ];
    let value = value as f64;
    for (index, (size, pt, en)) in units.iter().enumerate() {
        if value >= *size {
            let scaled = (value / size * 10.0).round() / 10.0;
            // 999 950 rounds to "1000 mil": the next unit up says it.
            if scaled >= 1000.0 && index > 0 {
                let (size, pt, en) = units[index - 1];
                return compact_with(value / size, pt, en, locale);
            }
            return compact_with(value / size, pt, en, locale);
        }
    }
    grouped(value as u64, locale)
}

fn compact_with(scaled: f64, pt: &str, en: &str, locale: Locale) -> String {
    let number = one_decimal(scaled, locale);
    match locale {
        Locale::PtBr => format!("{number}{pt}"),
        Locale::En => format!("{number}{en}"),
    }
}

/// `Intl.NumberFormat(…, { style: "currency", currency: "USD" })`.
pub fn dollars(value: f64, locale: Locale) -> String {
    let (_, decimal) = separators(locale);
    let cents = (value * 100.0).round() as i64;
    let whole = grouped(cents.unsigned_abs() / 100, locale);
    let fraction = cents.unsigned_abs() % 100;
    let sign = if cents < 0 { "-" } else { "" };
    match locale {
        Locale::PtBr => format!("{sign}US$\u{a0}{whole}{decimal}{fraction:02}"),
        Locale::En => format!("{sign}${whole}{decimal}{fraction:02}"),
    }
}

/// A calendar date (`YYYY-MM-DD`) as `toLocaleDateString()` writes it.
pub fn short_date(date: &str, locale: Locale) -> String {
    let mut parts = date.splitn(3, '-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return date.to_string();
    };
    let (month, day) = (month.trim_start_matches('0'), day.trim_start_matches('0'));
    match locale {
        Locale::PtBr => format!("{:0>2}/{:0>2}/{year}", day, month),
        Locale::En => format!("{month}/{day}/{year}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_like_intl_in_each_language() {
        assert_eq!(compact(9_999, Locale::PtBr), "9.999");
        assert_eq!(compact(9_999, Locale::En), "9,999");
        assert_eq!(compact(12_345, Locale::PtBr), "12,3\u{a0}mil");
        assert_eq!(compact(12_000, Locale::En), "12K");
        assert_eq!(compact(1_250_000, Locale::En), "1.3M");
        assert_eq!(compact(999_960, Locale::En), "1M");
        assert_eq!(compact(2_000_000_000, Locale::PtBr), "2\u{a0}bi");
    }

    #[test]
    fn money_shares_and_dates() {
        assert_eq!(dollars(1234.5, Locale::En), "$1,234.50");
        assert_eq!(dollars(0.456, Locale::PtBr), "US$\u{a0}0,46");
        assert_eq!(percent(1, 4), 25.0);
        assert_eq!(percent(1, 0), 0.0);
        assert_eq!(short_date("2026-10-05", Locale::PtBr), "05/10/2026");
        assert_eq!(short_date("2026-10-05", Locale::En), "10/5/2026");
        assert_eq!(provider_color("claude_code"), 0xfb923c);
        assert_eq!(provider_color("other"), 0x94a3b8);
    }

    #[test]
    fn an_empty_database_has_no_providers() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("database.sqlite");
        drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
        let dashboard = load(&db, Some(30), None).unwrap();
        assert!(dashboard.providers.is_empty());
        assert_eq!(dashboard.totals.turns, 0);
    }
}
