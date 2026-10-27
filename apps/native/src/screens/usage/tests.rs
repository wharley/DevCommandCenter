//! The dashboard's drawings from its numbers, and the dialog driven
//! headlessly: empty on a fresh database, the dashboard once there is
//! usage, a period button reading again.

use dcc_core::domain::usage::{
    DailyUsageSummary, ModelUsageSummary, ProviderUsageSummary, UsageDashboard, UsageTotals,
};
use gpui::{Modifiers, TestAppContext, WindowAppearance};

use super::{Usage, charts};
use crate::theme::{Theme, ThemePreference};

fn provider(id: &str, turns: u64, tokens: u64) -> ProviderUsageSummary {
    ProviderUsageSummary {
        provider_id: id.into(),
        sessions: 1,
        turns,
        measured_turns: turns,
        total_tokens: tokens,
        ..Default::default()
    }
}

fn day(date: &str, provider: &str, tokens: u64) -> DailyUsageSummary {
    DailyUsageSummary {
        date: date.into(),
        provider_id: provider.into(),
        turns: 1,
        measured_turns: 1,
        total_tokens: tokens,
    }
}

fn dashboard() -> UsageDashboard {
    UsageDashboard {
        totals: UsageTotals {
            sessions: 2,
            turns: 4,
            measured_turns: 3,
            total_tokens: 40_000,
            input_tokens: 30_000,
            output_tokens: 10_000,
            cost_usd: Some(1.5),
            ..Default::default()
        },
        providers: vec![
            provider("codex", 3, 30_000),
            provider("claude_code", 1, 10_000),
        ],
        models: vec![ModelUsageSummary {
            provider_id: "codex".into(),
            model: "gpt-5".into(),
            measured_turns: 3,
            total_tokens: 30_000,
            ..Default::default()
        }],
        daily: vec![
            day("2026-10-02", "codex", 10_000),
            day("2026-10-01", "codex", 20_000),
            day("2026-10-02", "claude_code", 10_000),
        ],
        ..Default::default()
    }
}

#[test]
fn the_ring_and_the_lines_follow_the_numbers() {
    let board = dashboard();
    let stops = charts::ring_stops(&board.providers, true);
    assert_eq!(stops.len(), 2);
    assert_eq!((stops[0].1, stops[0].2), (0.0, 75.0));
    assert_eq!((stops[1].1, stops[1].2), (75.0, 100.0));
    // Turns when no tokens were measured.
    let stops = charts::ring_stops(&board.providers, false);
    assert_eq!(stops[0].2, 75.0);
    let (dates, lines) = charts::timeline_lines(&board.daily, &board.providers, true);
    assert_eq!(dates, ["2026-10-01", "2026-10-02"]);
    // Codex peaks on the first day (at the top), Claude starts at zero.
    assert_eq!(lines[0].1[0], (14.0, 14.0));
    assert_eq!(lines[1].1[0], (14.0, 166.0));
    assert_eq!(lines[1].1[1].0, 746.0);
}

#[gpui::test]
fn empty_then_the_dashboard_and_a_period_reads_again(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
    let (usage, cx) = cx.add_window_view(move |_, cx| Usage::new(db.clone(), Some("p".into()), cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let usage = usage.read(cx);
        assert!(usage.dashboard.as_ref().unwrap().providers.is_empty());
        assert_eq!(usage.period, Some(30));
    });
    assert!(cx.debug_bounds("usage-project").is_some());
    assert!(cx.debug_bounds("usage-ring").is_none());
    cx.update(|_, cx| {
        usage.update(cx, |usage, cx| {
            usage.dashboard = Some(dashboard());
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("usage-ring").is_some());
    assert!(cx.debug_bounds("usage-timeline").is_some());
    let bounds = cx.debug_bounds("usage-period-7").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let usage = usage.read(cx);
        assert_eq!(usage.period, Some(7));
        // Read again from the (empty) database.
        assert!(usage.dashboard.as_ref().unwrap().providers.is_empty());
    });
}
