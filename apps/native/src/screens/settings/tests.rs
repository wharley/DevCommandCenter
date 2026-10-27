//! Settings driven headlessly: it opens on a section the native app has,
//! the search narrows the navigation (and Esc clears it before closing),
//! the appearance's choices go out as events.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{SectionId, Settings, SettingsEvent, SettingsState};
use crate::i18n::Locale;
use crate::theme::{Density, Theme, ThemePreference};

type Events = Rc<RefCell<Vec<String>>>;

pub(super) fn open(
    section: Option<SectionId>,
    cx: &mut TestAppContext,
) -> (Entity<Settings>, &mut VisualTestContext, Events) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let state = SettingsState {
        theme: ThemePreference::System,
        locale: Locale::PtBr,
        density: Density::Comfortable,
        workspace: (Some("Checkout".into()), Some("/repo".into())),
        default_pick: None,
    };
    let (settings, cx) =
        cx.add_window_view(move |window, cx| Settings::new(state, section, window, cx));
    let events: Events = Rc::default();
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&settings, move |_, event: &SettingsEvent, _| {
            sink.borrow_mut().push(match event {
                SettingsEvent::Closed => "closed".into(),
                SettingsEvent::OpenShortcuts => "shortcuts".into(),
                SettingsEvent::DefaultPick(pick) => {
                    format!("pick {} {}", pick.provider_id, pick.model_id)
                }
                SettingsEvent::ProvidersChanged => "providers".into(),
                SettingsEvent::Theme(theme) => format!("theme {theme:?}"),
                SettingsEvent::Locale(locale) => format!("locale {locale:?}"),
                SettingsEvent::Density(density) => format!("density {density:?}"),
            })
        })
        .detach();
    });
    cx.run_until_parked();
    (settings, cx, events)
}

pub(super) fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn it_opens_on_a_section_it_has_and_the_search_narrows(cx: &mut TestAppContext) {
    // "Computer Use" is not here yet: the first section the native app has.
    let (settings, cx, events) = open(Some(SectionId::ComputerUse), cx);
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::General
    );
    assert!(cx.debug_bounds("settings-section-appearance").is_some());
    assert!(cx.debug_bounds("settings-section-experimental").is_some());
    assert!(cx.debug_bounds("settings-section-model").is_some());
    assert!(
        cx.debug_bounds("settings-section-decisionProvider")
            .is_some()
    );
    assert!(cx.debug_bounds("settings-section-aiMemory").is_some());
    assert!(cx.debug_bounds("settings-section-integrations").is_some());
    assert!(cx.debug_bounds("settings-section-computerUse").is_none());
    cx.simulate_input("experim");
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-section-appearance").is_none());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::Experimental
    );
    // Nothing matches: "Mostrar todas as seções".
    cx.simulate_input(" zzz");
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-show-all").is_some());
    // Esc clears the search first, then closes.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-section-appearance").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["closed"]);
}

#[gpui::test]
fn the_appearance_choices_go_out(cx: &mut TestAppContext) {
    let (settings, cx, events) = open(Some(SectionId::Appearance), cx);
    click(cx, "settings-theme-dark");
    click(cx, "settings-locale-1");
    click(cx, "settings-density-1");
    assert_eq!(
        events.borrow().as_slice(),
        ["theme Dark", "locale En", "density Compact"]
    );
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        assert_eq!(settings.theme, ThemePreference::Dark);
        assert_eq!(settings.density, Density::Compact);
    });
    click(cx, "settings-section-experimental");
    assert!(cx.debug_bounds("settings-reset-metrics").is_some());
}

#[gpui::test]
fn general_keeps_the_mac_awake_for_the_limit_through_the_desktop(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(None, cx);
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::General
    );
    // No desktop: no keep-awake card.
    assert!(cx.debug_bounds("settings-keep-awake").is_none());
    assert!(cx.debug_bounds("settings-copy-diagnostics").is_some());
    let (port, server) = fake_desktop_routes(vec![
        (
            "/api/v1/keep-awake",
            200,
            r#"{"supported":true,"mode":"off","active":null}"#.into(),
        ),
        (
            "/api/v1/keep-awake",
            200,
            r#"{"supported":true,"mode":"during_agents","active":{"reason":{"kind":"agentsWorking","count":1},"label":"x"}}"#.into(),
        ),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_path_buf();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_api(
                crate::data::desktop_api::DesktopApi::new(port, "k"),
                data.clone(),
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-keep-awake").is_some());
    // On by default for new tasks: the Mac may sleep, so the warning offers
    // the hold. (The cards sit below the fold of the test window: the
    // handlers are called as the clicks would.)
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.set_auto_resume(true, cx)));
    cx.run_until_parked();
    assert!(dcc_infra::usage_limit_settings::load_settings(&data).auto_resume);
    assert!(cx.debug_bounds("settings-limit-keep-awake").is_some());
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_keep_awake(crate::data::keep_awake::Mode::DuringAgents, cx)
        })
    });
    cx.run_until_parked();
    let seen = server.join().unwrap();
    assert_eq!(seen[1].0, "PUT /api/v1/keep-awake");
    assert!(seen[1].1.contains("during_agents"), "{}", seen[1].1);
    assert!(cx.debug_bounds("settings-limit-keep-awake").is_none());
    // Copying the diagnostics says so.
    click(cx, "settings-copy-diagnostics");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert!(copied.unwrap().contains("schemaVersion"));
}

gpui::actions!(settings_test, [Probe]);

#[gpui::test]
fn shortcuts_record_a_global_key_before_the_app_bindings(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let fired = Rc::new(RefCell::new(0));
    let count = fired.clone();
    cx.update(|cx| {
        cx.bind_keys([gpui::KeyBinding::new("cmd-k", Probe, None)]);
        cx.on_action(move |_: &Probe, _| *count.borrow_mut() += 1);
    });
    let (settings, cx, events) = open(Some(SectionId::Shortcuts), cx);
    // No desktop: only the keyboard shortcuts card.
    assert!(cx.debug_bounds("settings-quick-composer").is_none());
    assert!(cx.debug_bounds("settings-open-cheatsheet").is_some());
    let status = |shortcut: &str| {
        format!(r#"{{"supported":true,"shortcut":{shortcut},"shortcutError":false}}"#)
    };
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/quick-composer/status", 200, status("null")),
        (
            "/api/v1/appshots/status",
            200,
            r#"{"supported":false,"shortcut":null}"#.into(),
        ),
        (
            "/api/v1/quick-composer/shortcut",
            200,
            r#"{"ok":true}"#.into(),
        ),
        (
            "/api/v1/quick-composer/status",
            200,
            status(r#""Super+KeyK""#),
        ),
    ]);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_api(
                crate::data::desktop_api::DesktopApi::new(port, "k"),
                dir.path().to_path_buf(),
                cx,
            )
        })
    });
    cx.run_until_parked();
    // Appshots is not offered here: its card hides.
    assert!(cx.debug_bounds("settings-quick-composer").is_some());
    assert!(cx.debug_bounds("settings-appshots").is_none());
    assert!(cx.debug_bounds("settings-quick-composer-disable").is_none());
    // Esc cancels without closing Settings.
    click(cx, "settings-quick-composer-shortcut");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| settings.read(cx).shortcuts.recording.is_none()));
    assert!(events.borrow().is_empty(), "{:?}", events.borrow());
    // A key without a modifier is not one; ⌘K is recorded, the app's ⌘K
    // does not fire.
    click(cx, "settings-quick-composer-shortcut");
    cx.simulate_keystrokes("k cmd-k");
    cx.run_until_parked();
    assert_eq!(*fired.borrow(), 0);
    let seen = server.join().unwrap();
    assert_eq!(seen[2].0, "PUT /api/v1/quick-composer/shortcut");
    assert!(seen[2].1.contains("Super+KeyK"), "{}", seen[2].1);
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        assert!(settings.shortcuts.recording.is_none());
        assert_eq!(
            settings
                .shortcuts
                .quick
                .as_ref()
                .unwrap()
                .shortcut
                .as_deref(),
            Some("Super+KeyK")
        );
    });
    assert!(cx.debug_bounds("settings-quick-composer-disable").is_some());
    // Not recording: the app's ⌘K is back.
    cx.simulate_keystrokes("cmd-k");
    assert_eq!(*fired.borrow(), 1);
    click(cx, "settings-open-cheatsheet");
    assert_eq!(events.borrow().as_slice(), ["shortcuts"]);
}

#[gpui::test]
fn decision_provider_saves_the_form_and_shows_the_history(cx: &mut TestAppContext) {
    use crate::data::decision_provider::Point;
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::DecisionProvider), cx);
    assert!(cx.debug_bounds("settings-decision-configuration").is_some());
    let saved = |mode: &str| {
        serde_json::json!({
            "provider": "typesafe", "mode": mode, "modelRouting": "manual",
            "memoryFilterEnabled": true, "skillRouterEnabled": true, "modelRouterEnabled": true,
            "completionReviewEnabled": true, "toolGuardEnabled": true,
            "baseUrl": "https://api.typesafe.ai", "model": "jev-latest", "memoryThreshold": 0.65,
            "skillConfidenceThreshold": 0.65, "modelConfidenceThreshold": 0.8,
            "completionThreshold": 0.8, "toolRiskThreshold": 0.65,
            "apiKeyConfigured": true, "restartRequired": false,
        })
        .to_string()
    };
    let history = serde_json::json!([{
        "evaluation": { "version": "1", "contextTruncated": true,
            "scores": [{ "key": "validation", "probability": 0.42 }] },
        "id": 7, "sessionId": "s", "turnId": null, "decisionPoint": "completion_review",
        "provider": "typesafe", "mode": "observe", "status": "completed", "model": "jev-latest",
        "candidateCount": 4, "selectedCount": 1, "selectedIndices": [], "selectedLabels": ["needs_review:validation"],
        "threshold": 0.8, "durationMs": 120, "error": null, "createdAt": "2026-10-27T12:00:00Z",
    }])
    .to_string();
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/decision-provider/settings", 200, saved("observe")),
        ("/api/v1/decision-provider/settings", 200, saved("enforce")),
        ("/api/v1/decision-provider/history?limit=100", 200, history),
    ]);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_api(
                crate::data::desktop_api::DesktopApi::new(port, "k"),
                dir.path().to_path_buf(),
                cx,
            )
        })
    });
    cx.run_until_parked();
    // The saved key is never shown: "Remover chave salva" is offered.
    assert!(cx.debug_bounds("settings-decision-clear-key").is_some());
    // A new key typed shows as dots and cannot be copied.
    click(cx, "settings-decision-key");
    cx.simulate_input("sk-1");
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        assert_eq!(settings.decision.draft.api_key.as_deref(), Some("sk-1"));
        assert_eq!(settings.decision.api_key.read(cx).display_text(), "••••");
        assert!(cx.read_from_clipboard().is_none());
    });
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.update_decision(
                |draft| {
                    draft.mode = "enforce".into();
                    draft.set_threshold(Point::ToolGuard, "0.4");
                },
                cx,
            )
        })
    });
    // (The button sits below the fold of the test window.)
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.save_decision(cx)));
    cx.run_until_parked();
    // Saved: the form shows what the desktop kept, the key field empty.
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        assert_eq!(settings.decision.draft.mode, "enforce");
        assert!(settings.decision.api_key.read(cx).text().is_empty());
    });
    // The history, with the criteria on demand.
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.select_decision_tab(1, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-decision-history").is_some());
    click(cx, "settings-decision-details-7");
    assert!(cx.update(|_, cx| settings.read(cx).decision.open_details.contains(&7)));
    let seen = server.join().unwrap();
    assert_eq!(seen[1].0, "PUT /api/v1/decision-provider/settings");
    let body: serde_json::Value = serde_json::from_str(&seen[1].1).unwrap();
    assert_eq!(body["apiKey"], "sk-1");
    assert_eq!(body["mode"], "enforce");
    assert_eq!(body["toolRiskThreshold"], 0.4);
    assert_eq!(body["clearApiKey"], false);
}

#[gpui::test]
fn ai_memory_saves_restarts_and_retries_the_queue(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::AiMemory), cx);
    let status = serde_json::json!({
        "mode": "managed", "running": true, "url": "http://127.0.0.1:49374",
        "dataDir": "/data/ai-memory", "binaryPath": null, "version": "2.6.1",
        "logPath": "/data/ai-memory.log", "message": null,
    })
    .to_string();
    let saved = |mode: &str, restart: bool| {
        serde_json::json!({ "mode": mode, "baseUrl": null, "workspace": "dcc-workspace",
            "project": "dcc-project", "dataDir": null, "tokenConfigured": true,
            "restartRequired": restart })
        .to_string()
    };
    let outbox = serde_json::json!([{ "sessionId": "session-123456789", "attempts": 2,
        "nextAttemptAt": "2026-10-27T13:00:00Z", "lastError": "timeout", "eventCount": 9 }])
    .to_string();
    let exports = serde_json::json!([{ "id": 1, "sessionId": "session-123456789",
        "status": "retrying", "attempts": 2, "eventCount": 9, "acceptedCount": 0,
        "nextAttemptAt": null, "error": "timeout", "startedAt": "2026-10-27T12:00:00Z",
        "finishedAt": "2026-10-27T12:00:01Z" }])
    .to_string();
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/ai-memory/sidecar", 200, status.clone()),
        ("/api/v1/ai-memory/settings", 200, saved("managed", false)),
        ("/api/v1/ai-memory/settings", 200, saved("remote", true)),
        ("/api/v1/ai-memory/sidecar", 200, status),
        ("/api/v1/app/restart", 200, r#"{"ok":true}"#.into()),
        ("/api/v1/ai-memory/outbox?limit=50", 200, outbox),
        (
            "/api/v1/ai-memory/export-history?limit=100",
            200,
            exports.clone(),
        ),
        (
            "/api/v1/ai-memory/outbox/session-123456789/retry",
            200,
            "null".into(),
        ),
        ("/api/v1/ai-memory/outbox?limit=50", 200, "[]".into()),
        ("/api/v1/ai-memory/export-history?limit=100", 200, exports),
    ]);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_api(
                crate::data::desktop_api::DesktopApi::new(port, "k"),
                dir.path().to_path_buf(),
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-memory-status").is_some());
    // Remote: the server field takes typing; saving says DCC must restart.
    click(cx, "settings-memory-mode-remote");
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.save_memory(cx)));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(settings.read(cx).memory.restart_required));
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.restart_desktop(cx)));
    cx.run_until_parked();
    // The queue, "Reprocessar", then the queue again.
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.select_memory_tab(1, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-memory-history").is_some());
    click(cx, "settings-memory-retry-session-123456789");
    let seen = server.join().unwrap();
    cx.run_until_parked();
    let put = seen
        .iter()
        .find(|(request, _)| request == "PUT /api/v1/ai-memory/settings")
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(&put.1).unwrap();
    assert_eq!(body["mode"], "remote");
    assert_eq!(body["workspace"], "dcc-workspace");
    assert!(
        seen.iter()
            .any(|(request, _)| request == "POST /api/v1/app/restart")
    );
    assert!(
        seen.iter()
            .any(|(request, _)| request == "POST /api/v1/ai-memory/outbox/session-123456789/retry")
    );
    cx.update(|_, cx| assert_eq!(settings.read(cx).memory.outbox.as_deref(), Some(&[][..])));
}

#[gpui::test]
fn git_opens_the_project_automation_over_settings(cx: &mut TestAppContext) {
    let (settings, cx, events) = open(Some(SectionId::Git), cx);
    // The task's name, and "Configurar automação" (its checkout is known).
    assert!(cx.debug_bounds("settings-git-workspace").is_some());
    click(cx, "settings-git-automation");
    assert!(cx.debug_bounds("project-automation").is_some());
    cx.update(|_, cx| assert!(settings.read(cx).automation.is_some()));
    click(cx, "automation-close");
    assert!(cx.debug_bounds("project-automation").is_none());
    // Settings stays open.
    assert!(events.borrow().is_empty(), "{:?}", events.borrow());
}
