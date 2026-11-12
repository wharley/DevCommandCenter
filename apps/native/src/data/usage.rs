//! "DCC Usage" (`UsageDialog`): the usage of Claude Code and Codex read
//! from their local logs through the incremental index beside DCC's
//! database (`dcc_infra::usage_logs`), and the numbers as the desktop
//! formats them — `Intl.NumberFormat` compact from 10 000 on, the cost in
//! dollars, the shares in percent — in the app's language.

use std::path::Path;

use dcc_core::domain::usage_logs::UsageRows;
use dcc_infra::usage_logs::{DccHints, RefreshStats, Resolver, Sources, UsageIndex};

use crate::i18n::Locale;

/// The runtimes' colors (`claude_code` and `codex` of `PROVIDER_COLORS`).
pub fn runtime_color(runtime: &str) -> u32 {
    match runtime {
        "codex" => 0x34d399,
        "claude" => 0xfb923c,
        _ => 0x94a3b8,
    }
}

/// The legend's colors in order; the gathered rest is gray.
const GROUP_COLORS: [u32; 6] = [0x60a5fa, 0xfb923c, 0x34d399, 0xa78bfa, 0xf472b6, 0x22d3ee];

/// The color of the legend's `index`-th group, `others` gray.
pub fn group_color(index: usize, others: bool) -> u32 {
    if others {
        0x94a3b8
    } else {
        GROUP_COLORS[index % GROUP_COLORS.len()]
    }
}

/// The first millisecond the period can need (a day of slack for the
/// time zone; the dashboard cuts at local midnight).
pub fn since_ms(period_days: Option<u32>, now_ms: i64) -> Option<i64> {
    period_days.map(|days| now_ms - (i64::from(days) + 1) * 86_400_000)
}

/// Reads what is new in the logs into the index, reporting `(done,
/// total)` files, then the period's rows. `sources` replaces this
/// machine's log folders (tests).
pub fn index_and_load(
    db_path: &Path,
    sources: Option<Sources>,
    since: Option<i64>,
    progress: &mut dyn FnMut(usize, usize),
) -> anyhow::Result<(RefreshStats, UsageRows)> {
    let (sources, hints) = match sources {
        Some(sources) => (sources, DccHints::default()),
        None => dcc_infra::usage_logs::discover(Some(db_path)),
    };
    let mut index = UsageIndex::open(&dcc_infra::usage_logs::index_path(db_path))?;
    let stats = index.refresh(&sources, &mut Resolver::new(hints), progress)?;
    Ok((stats, index.load_rows(since)?))
}

/// The period's rows as the index has them, without reading the logs.
pub fn load_rows(db_path: &Path, since: Option<i64>) -> anyhow::Result<UsageRows> {
    let index = UsageIndex::open(&dcc_infra::usage_logs::index_path(db_path))?;
    Ok(index.load_rows(since)?)
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

/// Dollars short: "US$ 22,4 mil" / "$22.4K" from 10 000 on.
pub fn dollars_compact(value: f64, locale: Locale) -> String {
    if value < 10_000.0 {
        return dollars(value, locale);
    }
    let amount = compact(value.round() as u64, locale);
    match locale {
        Locale::PtBr => format!("US$\u{a0}{amount}"),
        Locale::En => format!("${amount}"),
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
        assert_eq!(runtime_color("claude"), 0xfb923c);
        assert_eq!(group_color(7, true), 0x94a3b8);
        assert_eq!(
            dollars_compact(22_400.0, Locale::PtBr),
            "US$\u{a0}22,4\u{a0}mil"
        );
        assert_eq!(dollars_compact(16_200.0, Locale::En), "$16.2K");
        assert_eq!(dollars_compact(12.5, Locale::En), "$12.50");
        assert_eq!(compact(88_200_000_000, Locale::PtBr), "88,2\u{a0}bi");
        assert_eq!(compact(226_500_000, Locale::PtBr), "226,5\u{a0}mi");
    }

    #[test]
    fn an_empty_machine_indexes_to_no_rows() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("database.sqlite");
        let rows = load_rows(&db, since_ms(Some(30), 0)).unwrap();
        assert!(rows.rows.is_empty());
        assert_eq!(since_ms(Some(7), 8 * 86_400_000), Some(0));
        assert_eq!(since_ms(None, 1), None);
    }
}
