//! The drawings from their numbers, and the dialog driven headlessly over
//! a folder of fixture logs: indexed on open, then the metric and the
//! period recomputed from the rows.

use std::path::Path;

use dcc_core::domain::usage_logs::Metric;
use dcc_infra::usage_logs::Sources;
use gpui::{Modifiers, TestAppContext, WindowAppearance};

use super::{Indexing, Usage, charts, heatmap};
use crate::theme::{Theme, ThemePreference};

#[test]
fn bars_stack_from_the_bottom_and_the_tallest_fills_the_chart() {
    let red = gpui::red();
    let columns = vec![vec![(red, 1.0), (red, 3.0)], vec![(red, 2.0)], vec![]];
    assert_eq!(charts::tallest(&columns), 4.0);
    let layout = charts::layout(&columns, 100.0);
    assert_eq!(layout[0], vec![(0.0, 25.0), (25.0, 75.0)]);
    assert_eq!(layout[1], vec![(0.0, 50.0)]);
    assert!(layout[2].is_empty());
    assert_eq!(heatmap::intensity(0.0, 10.0), 0.0);
    assert_eq!(heatmap::intensity(10.0, 10.0), 1.0);
    assert!(heatmap::intensity(1.0, 10.0) > 0.15);
}

/// A Codex rollout of today: one turn, two calls.
fn write_rollout(dir: &Path) {
    let at = |minutes: i64| {
        (chrono::Utc::now() - chrono::Duration::minutes(minutes))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    let tokens = |at: String, total: u64| {
        format!(
            r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"total_tokens":{total}}},"last_token_usage":{{"input_tokens":1000,"cached_input_tokens":600,"cache_write_input_tokens":0,"output_tokens":100,"reasoning_output_tokens":20,"total_tokens":1100}}}}}}}}"#
        )
    };
    let lines = [
        format!(
            r#"{{"timestamp":"{}","type":"session_meta","payload":{{"id":"cx-1","cwd":"/nowhere/repo","originator":"dcc","source":"vscode"}}}}"#,
            at(10)
        ),
        format!(
            r#"{{"timestamp":"{}","type":"turn_context","payload":{{"turn_id":"t1","model":"gpt-5.5"}}}}"#,
            at(9)
        ),
        tokens(at(8), 1100),
        tokens(at(7), 2200),
    ];
    let folder = dir.join("sessions").join("2026").join("11").join("12");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("rollout-test-cx-1.jsonl"),
        lines.join("\n") + "\n",
    )
    .unwrap();
}

#[gpui::test]
fn opening_indexes_the_logs_and_the_filters_recompute(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    write_rollout(dir.path());
    let db = dir.path().join("database.sqlite");
    let sources = Sources {
        claude: Vec::new(),
        codex: vec![dir.path().join("sessions")],
    };
    let (usage, cx) =
        cx.add_window_view(move |_, cx| Usage::with_sources(db.clone(), Some(sources.clone()), cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let usage = usage.read(cx);
        assert_eq!(usage.indexing, Indexing::Idle);
        let summary = usage.summary.as_ref().unwrap();
        assert_eq!(summary.calls, 2);
        assert_eq!(summary.turns, 1);
        assert_eq!(summary.tokens.total(), 2200);
        assert_eq!(summary.dcc_percent, 100.0);
        assert_eq!(usage.filter.period_days, Some(30));
    });
    for selector in [
        "usage-filters",
        "usage-stats",
        "usage-days",
        "usage-heatmap",
        "usage-bands",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    assert!(cx.debug_bounds("usage-via-dcc").is_some());

    // "Turnos" counts turns.
    let bounds = cx.debug_bounds("usage-metric-5").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let usage = usage.read(cx);
        assert_eq!(usage.metric, Metric::Turns);
        assert_eq!(usage.summary.as_ref().unwrap().metric_total, 1.0);
    });

    // A period reads the rows again from the index.
    let bounds = cx.debug_bounds("usage-period-7").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let usage = usage.read(cx);
        assert_eq!(usage.filter.period_days, Some(7));
        assert_eq!(usage.summary.as_ref().unwrap().period_days, 7);
        assert_eq!(usage.summary.as_ref().unwrap().calls, 2);
    });

    // Hiding the only way in leaves nothing to show.
    let bounds = cx.debug_bounds("usage-via-dcc").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(usage.read(cx).summary.as_ref().unwrap().calls, 0);
    });
    assert!(cx.debug_bounds("usage-days").is_none());
    assert!(cx.debug_bounds("usage-via-dcc").is_some());
}
