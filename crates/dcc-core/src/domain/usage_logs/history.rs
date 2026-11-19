//! What each runtime used, by the day: tokens and estimated dollars, and
//! the models behind them. The HTTP API answers this to clients that draw
//! their own history (DCC Bot) instead of the dashboard's charts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Duration, NaiveDate, TimeZone};
use serde::Serialize;

use super::{EventKind, Runtime, UsageRows};
use crate::domain::pricing::{call_cost, model_price, ModelPrice};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryModel {
    pub model: String,
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDay {
    /// `yyyy-MM-dd` in the asked time zone.
    pub date: String,
    pub tokens: u64,
    /// US dollars over the priced models only.
    pub cost: f64,
    /// Largest first.
    pub models: Vec<HistoryModel>,
    /// Models used that day without a price; their tokens still count.
    pub unknown: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHistory {
    pub runtime: &'static str,
    /// Days with use, newest first.
    pub days: Vec<HistoryDay>,
}

#[derive(Default)]
struct DayAcc {
    models: HashMap<String, (u64, f64)>,
    unknown: BTreeSet<String>,
}

/// Each runtime's use for today and the `days - 1` days before it, in
/// `tz`, from the calls in `rows` (turns carry no tokens).
pub fn daily_history<Tz: TimeZone>(
    rows: &UsageRows,
    tz: &Tz,
    now_ms: i64,
    days: u32,
) -> Vec<RuntimeHistory> {
    let local = |ms: i64| {
        tz.timestamp_millis_opt(ms)
            .single()
            .map(|at| at.naive_local().date())
    };
    let Some(today) = local(now_ms) else {
        return Vec::new();
    };
    let start = today - Duration::days(i64::from(days.max(1)) - 1);
    let mut prices: HashMap<u32, Option<ModelPrice>> = HashMap::new();
    let mut per: [BTreeMap<NaiveDate, DayAcc>; 2] = Default::default();
    for row in &rows.rows {
        if row.kind != EventKind::Call || row.tokens.total() == 0 {
            continue;
        }
        let Some(date) = local(row.at_ms) else {
            continue;
        };
        if date < start || date > today {
            continue;
        }
        let name = rows.string(row.model).to_string();
        let price = *prices
            .entry(row.model)
            .or_insert_with(|| model_price(&name));
        let day = per[row.runtime.index()].entry(date).or_default();
        let cost = match price {
            Some(price) => call_cost(
                &price,
                row.tokens.input,
                row.tokens.cache_read,
                row.tokens.cache_write,
                row.tokens.output,
            ),
            None => {
                day.unknown.insert(name.clone());
                0.0
            }
        };
        let entry = day.models.entry(name).or_default();
        entry.0 += row.tokens.total();
        entry.1 += cost;
    }
    Runtime::ALL
        .into_iter()
        .zip(per)
        .filter(|(_, days)| !days.is_empty())
        .map(|(runtime, days)| RuntimeHistory {
            runtime: runtime.as_str(),
            days: days
                .into_iter()
                .rev()
                .map(|(date, acc)| day(date, acc))
                .collect(),
        })
        .collect()
}

fn day(date: NaiveDate, acc: DayAcc) -> HistoryDay {
    let mut models: Vec<HistoryModel> = acc
        .models
        .into_iter()
        .map(|(model, (tokens, cost))| HistoryModel {
            model,
            tokens,
            cost,
        })
        .collect();
    models.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.model.cmp(&b.model)));
    HistoryDay {
        date: date.format("%Y-%m-%d").to_string(),
        tokens: models.iter().map(|model| model.tokens).sum(),
        cost: models.iter().map(|model| model.cost).sum(),
        models,
        unknown: acc.unknown.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::usage_logs::{TokenBuckets, UsageEvent, Via};
    use chrono::{FixedOffset, Utc};

    fn call(rows: &mut UsageRows, at_ms: i64, runtime: Runtime, model: &str, input: u64) {
        rows.push(&UsageEvent {
            key: format!("{at_ms}-{model}-{input}"),
            at_ms,
            kind: EventKind::Call,
            runtime,
            model: model.into(),
            via: Via::Dcc,
            project: String::new(),
            agent: String::new(),
            session: "s".into(),
            subagent: false,
            tokens: TokenBuckets {
                input,
                ..TokenBuckets::default()
            },
        });
    }

    const DAY: i64 = 86_400_000;

    #[test]
    fn days_are_per_runtime_newest_first_in_the_zone() {
        let now = Utc
            .with_ymd_and_hms(2026, 11, 19, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let mut rows = UsageRows::default();
        call(&mut rows, now, Runtime::Codex, "gpt-5", 1_000);
        call(&mut rows, now - DAY, Runtime::Codex, "gpt-5", 500);
        call(&mut rows, now - DAY, Runtime::Codex, "mystery-model", 7);
        call(&mut rows, now - 40 * DAY, Runtime::Codex, "gpt-5", 9);
        // 01:00 UTC on the 19th is still the 18th three hours west.
        let early = Utc
            .with_ymd_and_hms(2026, 11, 19, 1, 0, 0)
            .unwrap()
            .timestamp_millis();
        call(&mut rows, early, Runtime::Claude, "claude-sonnet-4-5", 10);
        let west = FixedOffset::west_opt(3 * 3600).unwrap();

        let history = daily_history(&rows, &west, now, 30);

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].runtime, "claude");
        assert_eq!(history[0].days[0].date, "2026-11-18");
        let codex = &history[1];
        let dates: Vec<&str> = codex.days.iter().map(|day| day.date.as_str()).collect();
        assert_eq!(dates, ["2026-11-19", "2026-11-18"]);
        assert_eq!(codex.days[1].tokens, 507);
        assert_eq!(codex.days[1].models[0].model, "gpt-5");
        assert_eq!(codex.days[1].unknown, ["mystery-model"]);
        assert!(codex.days[0].cost > 0.0);
    }

    #[test]
    fn no_use_is_no_runtime() {
        assert!(daily_history(&UsageRows::default(), &Utc, 0, 30).is_empty());
    }
}
