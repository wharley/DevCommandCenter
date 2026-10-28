//! The panel against a fake desktop: the counts, the tasks needing the
//! person first and the recent ones apart, the keep-awake hold, CPU and
//! memory, the last snapshot kept (marked stale) when a read fails, and an
//! action failing without the main window.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use super::MenuBarPanel;
use crate::data::desktop_api::DesktopApi;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn the_panel_reads_the_desktop_and_keeps_the_last_snapshot(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let snapshot = json!({
        "tasks": [
            { "workspaceId": "w1", "sessionId": "s1", "title": "Login", "project": "app",
              "status": "permission", "updatedAt": "2026-10-28T10:00:00Z" },
            { "workspaceId": "w2", "sessionId": "s2", "title": "Cache", "project": "app",
              "status": "running", "updatedAt": "2026-10-28T09:59:00Z" },
            { "workspaceId": "w3", "sessionId": "s3", "title": "Docs", "project": "site",
              "status": "completed", "updatedAt": "2026-10-28T09:00:00Z" }
        ],
        "metrics": { "cpuPercent": 12.34, "memoryBytes": 536870912u64, "processCount": 5 },
        "keepAwake": { "reason": { "kind": "agentsWorking", "count": 2 }, "label": "" }
    })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/menu-bar", 200, snapshot),
        ("/api/v1/menu-bar", 500, r#"{"error":"down"}"#.into()),
    ]);
    let (panel, cx) = cx.add_window_view(MenuBarPanel::new);
    cx.simulate_resize(size(px(368.0), px(480.0)));
    // No desktop: unavailable, no tasks. Then the desktop's snapshot.
    cx.run_until_parked();
    assert!(panel.read_with(cx, |panel, _| panel.error && panel.snapshot.is_none()));
    panel.update(cx, |panel, cx| {
        panel.api = Some(DesktopApi::new(port, "k"));
        panel.refresh(cx);
    });
    cx.run_until_parked();
    let (running, waiting) = panel.read_with(cx, |panel, _| (panel.running(), panel.waiting()));
    assert_eq!((running, waiting), (1, 1));
    for row in ["menu-bar-task-0", "menu-bar-task-1", "menu-bar-task-2"] {
        assert!(cx.debug_bounds(row).is_some(), "{row}");
    }
    assert!(cx.debug_bounds("menu-bar-keep-awake").is_some());
    assert!(cx.debug_bounds("menu-bar-empty").is_none());
    // The next read fails: the snapshot stays, marked stale.
    cx.executor().advance_clock(super::POLL);
    cx.run_until_parked();
    let _ = desktop.join().unwrap();
    let (error, kept) = panel.read_with(cx, |panel, _| (panel.error, panel.snapshot.is_some()));
    assert!(error && kept);
    // Without the main window, "Abrir DCC" says it could not.
    let bounds = cx.debug_bounds("menu-bar-open").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(panel.read_with(cx, |panel, _| panel.action_error));
}

#[gpui::test]
fn nothing_running_says_so(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::En, cx);
    });
    let (port, desktop) = fake_desktop_routes(vec![(
        "/api/v1/menu-bar",
        200,
        json!({ "tasks": [], "metrics": null }).to_string(),
    )]);
    let (panel, cx) = cx.add_window_view(MenuBarPanel::new);
    cx.simulate_resize(size(px(368.0), px(480.0)));
    panel.update(cx, |panel, cx| {
        panel.api = Some(DesktopApi::new(port, "k"));
        panel.refresh(cx);
    });
    cx.run_until_parked();
    let _ = desktop.join().unwrap();
    assert!(cx.debug_bounds("menu-bar-empty").is_some());
    assert!(cx.debug_bounds("menu-bar-task-0").is_none());
    assert!(cx.debug_bounds("menu-bar-keep-awake").is_none());
}
