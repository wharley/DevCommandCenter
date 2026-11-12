//! The dashboard's numbers over the period's rows, for one filter and one
//! metric, in a given time zone (the person's local one in the app, a
//! fixed one in tests): the KPIs, the active days and streak, the peak day
//! and hour, the days' stacked bars, the weekday × hour heatmap and the
//! four bands of the day.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate, TimeZone, Timelike};

use super::{EventKind, GroupBy, Metric, Scope, TokenBuckets, UsageRow, UsageRows, Via};
use crate::domain::pricing::{call_cost, model_price, ModelPrice};

/// The legend entry that gathers every group past the top ones.
pub const OTHERS_GROUP: &str = "\u{0}others";

/// Named groups in the days' chart; the rest go to `OTHERS_GROUP`.
const TOP_GROUPS: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub struct UsageFilter {
    /// 7, 30, 90, or `None` for everything kept.
    pub period_days: Option<u32>,
    /// `Runtime::index()` → shown.
    pub runtimes: [bool; 2],
    pub scope: Scope,
    /// Vias toggled off.
    pub hidden_vias: Vec<Via>,
}

impl Default for UsageFilter {
    fn default() -> Self {
        Self {
            period_days: Some(30),
            runtimes: [true, true],
            scope: Scope::All,
            hidden_vias: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DayBar {
    pub date: NaiveDate,
    /// One value per legend group, in `UsageSummary::groups` order.
    pub parts: Vec<f64>,
}

impl DayBar {
    pub fn total(&self) -> f64 {
        self.parts.iter().sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakDay {
    pub date: NaiveDate,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakHour {
    pub date: NaiveDate,
    pub hour: u32,
    pub value: f64,
}

/// A band of the day: 00–06, 06–12, 12–18, 18–24.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Band {
    pub value: f64,
    pub percent: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageSummary {
    pub tokens: TokenBuckets,
    /// Estimated, over the priced calls only.
    pub cost_usd: f64,
    pub cost_by_runtime: [f64; 2],
    /// Calls with tokens whose model has no price.
    pub unpriced_calls: u64,
    pub unknown_models: Vec<String>,
    pub calls: u64,
    pub turns: u64,
    pub sessions: u64,
    pub subagent_sessions: u64,
    /// The selected metric over the filtered rows.
    pub metric_total: f64,
    /// Percent of the metric that came through DCC.
    pub dcc_percent: f64,
    /// Percent of the metric from subagents.
    pub subagent_percent: f64,
    pub active_days: u32,
    pub period_days: u32,
    pub longest_streak: u32,
    pub peak_day: Option<PeakDay>,
    pub peak_hour: Option<PeakHour>,
    /// Legend: group key (runtime/via id, model, project, agent — empty is
    /// the main agent — or `OTHERS_GROUP`) and its total, largest first.
    pub groups: Vec<(String, f64)>,
    pub days: Vec<DayBar>,
    /// Monday first, local hours.
    pub heatmap: [[f64; 24]; 7],
    pub weekdays: [f64; 7],
    /// Per hour, per `Runtime::index()`.
    pub hours: [[f64; 2]; 24],
    pub bands: [Band; 4],
    /// Vias and runtimes found in the period, before those filters.
    pub vias_present: Vec<Via>,
    pub runtimes_present: [bool; 2],
}

fn value(metric: Metric, row: &UsageRow, cost: f64) -> f64 {
    match (metric, row.kind) {
        (Metric::Turns, EventKind::Turn) => 1.0,
        (_, EventKind::Turn) => 0.0,
        (Metric::TotalTokens, _) => row.tokens.total() as f64,
        (Metric::NewTokens, _) => row.tokens.new_tokens() as f64,
        (Metric::Output, _) => row.tokens.output as f64,
        (Metric::Cost, _) => cost,
        (Metric::Calls, _) => 1.0,
        (Metric::Turns, _) => 0.0,
    }
}

fn percent(part: f64, total: f64) -> f64 {
    if total <= 0.0 {
        0.0
    } else {
        (part / total * 100.0).clamp(0.0, 100.0)
    }
}

/// The dashboard over `rows` for `filter` and `metric`, days and hours in
/// `tz`, "today" being `now_ms`.
pub fn aggregate<Tz: TimeZone>(
    rows: &UsageRows,
    filter: &UsageFilter,
    metric: Metric,
    group_by: GroupBy,
    tz: &Tz,
    now_ms: i64,
) -> UsageSummary {
    let local = |ms: i64| {
        tz.timestamp_millis_opt(ms)
            .single()
            .map(|at| at.naive_local())
    };
    let today = local(now_ms).map_or_else(|| NaiveDate::MIN, |at| at.date());
    let start = filter
        .period_days
        .map(|days| today - Duration::days(i64::from(days.max(1)) - 1));

    let mut prices: HashMap<u32, Option<ModelPrice>> = HashMap::new();
    let mut unknown: HashSet<u32> = HashSet::new();
    let mut summary = UsageSummary::default();
    let mut vias_present: HashSet<Via> = HashSet::new();
    let mut sessions: HashSet<u32> = HashSet::new();
    let mut subagent_sessions: HashSet<u32> = HashSet::new();
    let mut per_day: HashMap<NaiveDate, HashMap<String, f64>> = HashMap::new();
    let mut active: HashSet<NaiveDate> = HashSet::new();
    let mut per_hour: HashMap<(NaiveDate, u32), f64> = HashMap::new();
    let mut group_totals: HashMap<String, f64> = HashMap::new();
    let mut dcc = 0.0;
    let mut subagent = 0.0;
    let mut first_day: Option<NaiveDate> = None;

    for row in &rows.rows {
        let Some(at) = local(row.at_ms) else {
            continue;
        };
        let date = at.date();
        if start.is_some_and(|start| date < start) || date > today {
            continue;
        }
        summary.runtimes_present[row.runtime.index()] = true;
        vias_present.insert(row.via);
        if !filter.runtimes[row.runtime.index()]
            || filter.hidden_vias.contains(&row.via)
            || match filter.scope {
                Scope::All => false,
                Scope::Main => row.subagent,
                Scope::Subagents => !row.subagent,
            }
        {
            continue;
        }
        let cost = match row.kind {
            EventKind::Turn => 0.0,
            EventKind::Call => {
                let price = *prices
                    .entry(row.model)
                    .or_insert_with(|| model_price(rows.string(row.model)));
                match price {
                    Some(price) => call_cost(
                        &price,
                        row.tokens.input,
                        row.tokens.cache_read,
                        row.tokens.cache_write,
                        row.tokens.output,
                    ),
                    None => {
                        if row.tokens.total() > 0 {
                            summary.unpriced_calls += 1;
                            unknown.insert(row.model);
                        }
                        0.0
                    }
                }
            }
        };
        match row.kind {
            EventKind::Call => {
                summary.calls += 1;
                summary.tokens.add(&row.tokens);
                summary.cost_usd += cost;
                summary.cost_by_runtime[row.runtime.index()] += cost;
            }
            EventKind::Turn => summary.turns += 1,
        }
        sessions.insert(row.session);
        if row.subagent {
            subagent_sessions.insert(row.session);
        }
        active.insert(date);
        first_day = Some(first_day.map_or(date, |first| first.min(date)));

        let amount = value(metric, row, cost);
        if amount == 0.0 {
            continue;
        }
        summary.metric_total += amount;
        if row.via == Via::Dcc {
            dcc += amount;
        }
        if row.subagent {
            subagent += amount;
        }
        let key = match group_by {
            GroupBy::Runtime => row.runtime.as_str().to_string(),
            GroupBy::Via => row.via.as_str().to_string(),
            GroupBy::Model => rows.string(row.model).to_string(),
            GroupBy::Project => rows.string(row.project).to_string(),
            GroupBy::Agent => rows.string(row.agent).to_string(),
        };
        *group_totals.entry(key.clone()).or_default() += amount;
        *per_day.entry(date).or_default().entry(key).or_default() += amount;
        let hour = at.hour();
        *per_hour.entry((date, hour)).or_default() += amount;
        let weekday = at.weekday().num_days_from_monday() as usize;
        summary.heatmap[weekday][hour as usize] += amount;
        summary.weekdays[weekday] += amount;
        summary.hours[hour as usize][row.runtime.index()] += amount;
    }

    summary.sessions = sessions.len() as u64;
    summary.subagent_sessions = subagent_sessions.len() as u64;
    summary.dcc_percent = percent(dcc, summary.metric_total);
    summary.subagent_percent = percent(subagent, summary.metric_total);
    let mut unknown: Vec<String> = unknown
        .into_iter()
        .map(|id| rows.string(id).to_string())
        .collect();
    unknown.sort();
    summary.unknown_models = unknown;
    summary.vias_present = Via::ALL
        .into_iter()
        .filter(|via| vias_present.contains(via))
        .collect();

    // The legend: the largest groups, then the rest together.
    let mut groups: Vec<(String, f64)> = group_totals.into_iter().collect();
    groups.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if groups.len() > TOP_GROUPS + 1 {
        let rest: f64 = groups[TOP_GROUPS..].iter().map(|(_, value)| value).sum();
        groups.truncate(TOP_GROUPS);
        groups.push((OTHERS_GROUP.to_string(), rest));
    }
    let slot = |key: &str| {
        groups
            .iter()
            .position(|(group, _)| group == key)
            .unwrap_or(groups.len().saturating_sub(1))
    };

    // Every day of the period, empty ones too.
    let first = start.or(first_day).unwrap_or(today);
    let mut date = first;
    let mut streak = 0;
    while date <= today {
        let mut parts = vec![0.0; groups.len()];
        if let Some(day) = per_day.get(&date) {
            for (key, amount) in day {
                parts[slot(key)] += amount;
            }
        }
        let bar = DayBar { date, parts };
        let total = bar.total();
        if total > 0.0 && summary.peak_day.is_none_or(|peak| total > peak.value) {
            summary.peak_day = Some(PeakDay { date, value: total });
        }
        if active.contains(&date) {
            streak += 1;
            summary.longest_streak = summary.longest_streak.max(streak);
        } else {
            streak = 0;
        }
        summary.days.push(bar);
        date += Duration::days(1);
    }
    summary.period_days = summary.days.len() as u32;
    summary.active_days = active.len() as u32;
    summary.groups = groups;

    let mut hours: Vec<((NaiveDate, u32), f64)> = per_hour.into_iter().collect();
    hours.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    summary.peak_hour = hours.first().map(|((date, hour), value)| PeakHour {
        date: *date,
        hour: *hour,
        value: *value,
    });

    for (band, range) in [(0, 0..6), (1, 6..12), (2, 12..18), (3, 18..24)] {
        let value: f64 = summary.hours[range]
            .iter()
            .map(|hour| hour[0] + hour[1])
            .sum();
        summary.bands[band] = Band {
            value,
            percent: percent(value, summary.metric_total),
        };
    }
    summary
}
