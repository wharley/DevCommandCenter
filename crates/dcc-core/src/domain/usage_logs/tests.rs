use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone};

use super::*;

/// UTC−3, like São Paulo, so days and hours shift from UTC.
fn tz() -> FixedOffset {
    FixedOffset::west_opt(3 * 3600).unwrap()
}

fn at(y: i32, m: u32, d: u32, h: u32) -> i64 {
    tz().with_ymd_and_hms(y, m, d, h, 0, 0)
        .unwrap()
        .timestamp_millis()
}

fn call(at_ms: i64, runtime: Runtime, via: Via, subagent: bool, model: &str) -> UsageEvent {
    UsageEvent {
        key: format!("{at_ms}-{model}-{subagent}"),
        kind: EventKind::Call,
        at_ms,
        runtime,
        model: model.into(),
        via,
        project: "dcc".into(),
        agent: if subagent {
            "Explore".into()
        } else {
            String::new()
        },
        session: if subagent {
            "s1/agent".into()
        } else {
            "s1".into()
        },
        subagent,
        tokens: TokenBuckets {
            input: 100,
            cache_read: 800,
            cache_write: 50,
            output: 50,
            reasoning: 10,
        },
    }
}

fn turn(at_ms: i64, runtime: Runtime) -> UsageEvent {
    UsageEvent {
        key: format!("t{at_ms}"),
        kind: EventKind::Turn,
        at_ms,
        runtime,
        model: String::new(),
        via: Via::Cli,
        project: "dcc".into(),
        agent: String::new(),
        session: "s1".into(),
        subagent: false,
        tokens: TokenBuckets::default(),
    }
}

fn rows(events: &[UsageEvent]) -> UsageRows {
    let mut rows = UsageRows::default();
    for event in events {
        rows.push(event);
    }
    rows
}

#[test]
fn kpis_streak_peaks_heatmap_and_bands_in_local_time() {
    let now = at(2026, 11, 12, 23);
    let events = vec![
        // Nov 10, 01:00 local (04:00 UTC): a Tuesday's small hours.
        call(
            at(2026, 11, 10, 1),
            Runtime::Claude,
            Via::Cli,
            false,
            "claude-opus-5-5",
        ),
        turn(at(2026, 11, 10, 1), Runtime::Claude),
        // Nov 11, two calls at 14:00, one from a subagent through DCC.
        call(
            at(2026, 11, 11, 14),
            Runtime::Codex,
            Via::Dcc,
            false,
            "gpt-5.5",
        ),
        call(
            at(2026, 11, 11, 14),
            Runtime::Claude,
            Via::Cli,
            true,
            "claude-opus-5-5",
        ),
        // Nov 12, 22:00 local is Nov 13 01:00 UTC: still the 12th here.
        call(
            at(2026, 11, 12, 22),
            Runtime::Codex,
            Via::Tui,
            false,
            "mystery-1",
        ),
        // Nov 5: inside 30 days, outside 7.
        call(
            at(2026, 11, 5, 9),
            Runtime::Codex,
            Via::Exec,
            false,
            "gpt-5.5",
        ),
    ];
    let rows = rows(&events);
    let filter = UsageFilter {
        period_days: Some(7),
        ..Default::default()
    };
    let summary = aggregate(
        &rows,
        &filter,
        Metric::TotalTokens,
        GroupBy::Runtime,
        &tz(),
        now,
    );

    assert_eq!(summary.calls, 4);
    assert_eq!(summary.turns, 1);
    assert_eq!(summary.tokens.total(), 4 * 1000);
    assert_eq!(summary.tokens.new_tokens(), 4 * 200);
    assert_eq!(summary.sessions, 2);
    assert_eq!(summary.subagent_sessions, 1);
    assert_eq!(summary.metric_total, 4000.0);
    assert_eq!(summary.dcc_percent, 25.0);
    assert_eq!(summary.subagent_percent, 25.0);
    // Nov 6..=12, active on 10, 11 and 12.
    assert_eq!(summary.period_days, 7);
    assert_eq!(summary.days.len(), 7);
    assert_eq!(summary.active_days, 3);
    assert_eq!(summary.longest_streak, 3);
    let peak = summary.peak_day.unwrap();
    assert_eq!(peak.date, NaiveDate::from_ymd_opt(2026, 11, 11).unwrap());
    assert_eq!(peak.value, 2000.0);
    let hour = summary.peak_hour.unwrap();
    assert_eq!((hour.date.day0() + 1, hour.hour), (11, 14));
    // Tuesday (index 1) at 01:00; Wednesday (2) at 14:00.
    assert_eq!(summary.heatmap[1][1], 1000.0);
    assert_eq!(summary.heatmap[2][14], 2000.0);
    assert_eq!(summary.weekdays[3], 1000.0);
    assert_eq!(summary.hours[14], [1000.0, 1000.0]);
    assert_eq!(summary.bands[0].value, 1000.0);
    assert_eq!(summary.bands[2].percent, 50.0);
    assert_eq!(summary.bands[3].value, 1000.0);
    // Cost: the unknown model counts apart, never as zero silently.
    assert_eq!(summary.unknown_models, vec!["mystery-1".to_string()]);
    assert_eq!(summary.unpriced_calls, 1);
    assert!(summary.cost_by_runtime[0] > 0.0 && summary.cost_by_runtime[1] > 0.0);
    // Legend by runtime, the days' parts follow it.
    assert_eq!(summary.groups.len(), 2);
    let total: f64 = summary.days.iter().map(DayBar::total).sum();
    assert_eq!(total, 4000.0);
    // The exec call of Nov 5 is outside the 7 days.
    assert_eq!(summary.vias_present, vec![Via::Cli, Via::Tui, Via::Dcc]);
}

#[test]
fn filters_and_metrics() {
    let now = at(2026, 11, 12, 15);
    let events = vec![
        call(
            at(2026, 11, 11, 10),
            Runtime::Claude,
            Via::Desktop,
            false,
            "claude-opus-5-5",
        ),
        call(
            at(2026, 11, 11, 10),
            Runtime::Claude,
            Via::Desktop,
            true,
            "claude-opus-5-5",
        ),
        call(
            at(2026, 11, 11, 11),
            Runtime::Codex,
            Via::Dcc,
            false,
            "gpt-5.5",
        ),
        turn(at(2026, 11, 11, 10), Runtime::Claude),
        turn(at(2026, 11, 11, 11), Runtime::Codex),
    ];
    let rows = rows(&events);

    let codex_only = UsageFilter {
        runtimes: [false, true],
        ..Default::default()
    };
    let summary = aggregate(&rows, &codex_only, Metric::Calls, GroupBy::Via, &tz(), now);
    assert_eq!(summary.metric_total, 1.0);
    assert_eq!(summary.dcc_percent, 100.0);
    assert_eq!(summary.runtimes_present, [true, true]);

    let subagents = UsageFilter {
        scope: Scope::Subagents,
        ..Default::default()
    };
    let summary = aggregate(
        &rows,
        &subagents,
        Metric::Output,
        GroupBy::Agent,
        &tz(),
        now,
    );
    assert_eq!(summary.metric_total, 50.0);
    assert_eq!(summary.groups, vec![("Explore".to_string(), 50.0)]);

    let no_desktop = UsageFilter {
        hidden_vias: vec![Via::Desktop],
        ..Default::default()
    };
    let summary = aggregate(
        &rows,
        &no_desktop,
        Metric::Turns,
        GroupBy::Runtime,
        &tz(),
        now,
    );
    // The Claude turn came through the CLI.
    assert_eq!(summary.metric_total, 2.0);
    assert_eq!(summary.vias_present.len(), 3);

    let all = UsageFilter::default();
    let summary = aggregate(&rows, &all, Metric::Cost, GroupBy::Model, &tz(), now);
    // Opus 5.5: 100×4 + 800×0.2 + 50×5 + 50×20 per million; GPT-5.5: 100×5 + 800×0.5 + 50×5 + 50×30.
    let opus = (400.0 + 160.0 + 250.0 + 1000.0) / 1e6;
    let gpt = (500.0 + 400.0 + 250.0 + 1500.0) / 1e6;
    assert!((summary.cost_usd - (2.0 * opus + gpt)).abs() < 1e-12);
    assert!((summary.metric_total - summary.cost_usd).abs() < 1e-12);
    assert_eq!(summary.period_days, 30);
    assert!(summary.unknown_models.is_empty());
}

#[test]
fn everything_starts_at_the_first_day_and_extra_groups_fold_into_others() {
    let now = at(2026, 11, 12, 15);
    let models = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let events: Vec<UsageEvent> = models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            call(
                at(2026, 11, 3 + index as u32, 10),
                Runtime::Codex,
                Via::Cli,
                false,
                model,
            )
        })
        .collect();
    let rows = rows(&events);
    let all = UsageFilter {
        period_days: None,
        ..Default::default()
    };
    let summary = aggregate(&rows, &all, Metric::TotalTokens, GroupBy::Model, &tz(), now);
    assert_eq!(summary.days.first().unwrap().date.day(), 3);
    assert_eq!(summary.period_days, 10);
    assert_eq!(summary.active_days, 8);
    assert_eq!(summary.longest_streak, 8);
    assert_eq!(summary.groups.len(), 7);
    assert_eq!(summary.groups[6].0, OTHERS_GROUP);
    assert_eq!(summary.groups[6].1, 2000.0);
    assert_eq!(summary.unknown_models.len(), 8);
}
