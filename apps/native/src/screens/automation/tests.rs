//! Driven headlessly against a fake desktop: the config becomes the draft,
//! a task is added and typed, runs wait for a save, the save goes with the
//! hash it was read with, a fix asks first, and the report shows.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Modifiers, TestAppContext, WindowAppearance};

use super::{AutomationEvent, ProjectAutomation};
use crate::data::automation::Kind;
use crate::data::desktop_api::DesktopApi;
use crate::i18n::Locale;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

fn config(tasks: serde_json::Value, hash: &str) -> String {
    serde_json::json!({
        "setupCommand": "yarn install", "tasks": tasks, "beforeMerge": ["lint"], "beforePush": [],
        "deliveryPolicy": { "minimumApprovals": 1, "requirePipeline": true,
            "requireResolvedDiscussions": false, "requireCurrentBase": false,
            "requireBeforeMergeChecks": false },
        "sourcePath": "/repo/.dcc.toml", "configHash": hash, "trackedInGit": true,
    })
    .to_string()
}

#[gpui::test]
fn the_dcc_toml_is_edited_saved_and_run(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let lint = serde_json::json!({ "id": "lint", "label": "Lint", "command": "yarn lint",
        "kind": "check", "cwd": null, "timeoutSeconds": 600 });
    let fix = serde_json::json!({ "id": "check_2", "label": null, "command": "cargo fmt",
        "kind": "fix", "cwd": null, "timeoutSeconds": 600 });
    let report = serde_json::json!({
        "report": { "status": "failed", "sourcePath": null,
            "steps": [{ "command": "cargo fmt", "success": false, "exitCode": 1,
                "output": "boom", "timedOut": false, "durationMs": 5, "truncated": false }] },
        "changedFiles": true,
    })
    .to_string();
    let (port, server) = fake_desktop_routes(vec![
        (
            "/api/v1/workspaces/automation",
            200,
            config(serde_json::json!([lint]), "h1"),
        ),
        (
            "/api/v1/workspaces/automation",
            200,
            config(serde_json::json!([lint, fix]), "h2"),
        ),
        ("/api/v1/workspaces/automation/run", 200, report),
        (
            "/api/v1/workspaces/automation",
            200,
            config(serde_json::json!([lint, fix]), "h3"),
        ),
    ]);
    let (automation, cx) = cx.add_window_view(move |window, cx| {
        ProjectAutomation::new("/repo".into(), Some(DesktopApi::new(port, "k")), window, cx)
    });
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&automation, move |_, event: &AutomationEvent, _| {
            sink.borrow_mut().push(match event {
                AutomationEvent::Closed => "closed",
                AutomationEvent::WorkspaceChanged => "changed",
            })
        })
        .detach();
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let automation = automation.read(cx);
        assert_eq!(automation.fields.len(), 1);
        assert_eq!(automation.draft.setup_command, "yarn install");
        assert!(automation.draft.policy.require_pipeline);
        assert!(!automation.dirty());
    });
    assert!(cx.debug_bounds("automation-task-0").is_some());
    // A new task: the draft is dirty, so runs wait for a save.
    let bounds = cx.debug_bounds("automation-add-task").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(automation.read(cx).dirty()));
    // (The new card sits below the fold of the test window.)
    assert!(cx.debug_bounds("automation-command-1").is_some());
    cx.update(|window, cx| {
        let field = automation.read(cx).fields[1].command.clone();
        let focus = gpui::Focusable::focus_handle(field.read(cx), cx);
        window.focus(&focus, cx);
    });
    cx.simulate_input("cargo fmt");
    cx.update(|_, cx| {
        automation.update(cx, |automation, cx| automation.set_kind(1, Kind::Fix, cx))
    });
    cx.update(|window, cx| automation.update(cx, |automation, cx| automation.save(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!automation.read(cx).dirty()));
    // A fix asks first; then the report, failed steps open.
    cx.update(|window, cx| {
        automation.update(cx, |automation, cx| {
            automation.run(vec!["check_2".into()], window, cx)
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("automation-confirm-run").is_some());
    let bounds = cx.debug_bounds("automation-confirm-run").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let seen = server.join().unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("automation-report").is_some());
    assert_eq!(events.borrow().as_slice(), ["changed"]);
    let saved: serde_json::Value = serde_json::from_str(&seen[1].1).unwrap();
    assert_eq!(seen[1].0, "PUT /api/v1/workspaces/automation");
    assert_eq!(saved["expectedConfigHash"], "h1");
    assert_eq!(saved["tasks"][1]["command"], "cargo fmt");
    assert_eq!(saved["tasks"][1]["kind"], "fix");
    assert_eq!(saved["beforeMerge"], serde_json::json!(["lint"]));
    let ran: serde_json::Value = serde_json::from_str(&seen[2].1).unwrap();
    assert_eq!(ran["taskIds"], serde_json::json!(["check_2"]));
    assert_eq!(ran["expectedConfigHash"], "h2");
    // After the files changed: read again.
    cx.update(|_, cx| {
        let hash = automation
            .read(cx)
            .config()
            .and_then(|config| config.config_hash.clone());
        assert_eq!(hash.as_deref(), Some("h3"));
    });
    let bounds = cx.debug_bounds("automation-close").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    assert_eq!(events.borrow().last(), Some(&"closed"));
}
