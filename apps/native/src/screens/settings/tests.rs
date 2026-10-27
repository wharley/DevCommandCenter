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

fn open(
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

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn it_opens_on_a_section_it_has_and_the_search_narrows(cx: &mut TestAppContext) {
    // "Decision Provider" is not here yet: the first section the native app has.
    let (settings, cx, events) = open(Some(SectionId::DecisionProvider), cx);
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::General
    );
    assert!(cx.debug_bounds("settings-section-appearance").is_some());
    assert!(cx.debug_bounds("settings-section-experimental").is_some());
    assert!(cx.debug_bounds("settings-section-model").is_some());
    assert!(
        cx.debug_bounds("settings-section-decisionProvider")
            .is_none()
    );
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
fn providers_pick_the_default_switch_off_and_keep_the_launch_settings(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    use dcc_core::domain::provider::HealthStatus;
    let (settings, cx, events) = open(Some(SectionId::Model), cx);
    // No desktop: the built-in list, the default's provider (none: the first).
    assert!(cx.debug_bounds("settings-providers").is_some());
    assert!(cx.debug_bounds("settings-provider-codex").is_some());
    let mut providers = dcc_providers::offline_catalog().providers;
    let codex = providers.iter_mut().find(|p| p.id.0 == "codex").unwrap();
    codex.health = HealthStatus::Unhealthy {
        reason: "Codex is not signed in: run `codex login`".into(),
    };
    let model = codex.models[0].id.clone();
    let binary = providers
        .iter()
        .find(|p| p.capabilities.supports_runtime_binary)
        .map(|p| p.id.0.clone())
        .unwrap();
    let catalog = serde_json::json!({ "providers": providers }).to_string();
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/mobile/catalog", 200, catalog),
        (
            "/api/v1/providers/codex/availability",
            200,
            r#"{"availability":{}}"#.into(),
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
    // The provider needs setup: said in Portuguese, with "Verificar de novo".
    click(cx, "settings-provider-codex");
    assert!(cx.debug_bounds("settings-provider-problem").is_some());
    assert!(cx.debug_bounds("settings-provider-recheck").is_some());
    // A model becomes the default for new tasks.
    let selector: &'static str = Box::leak(format!("settings-model-{model}").into_boxed_str());
    click(cx, selector);
    assert_eq!(events.borrow().as_slice(), [format!("pick codex {model}")]);
    // Off through the desktop: the switch says so, and the composer rereads.
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.set_availability("codex".into(), false, cx)
        })
    });
    cx.run_until_parked();
    let seen = server.join().unwrap();
    assert_eq!(seen[1].0, "PUT /api/v1/providers/codex/availability");
    assert!(seen[1].1.contains("false"), "{}", seen[1].1);
    assert_eq!(events.borrow().last().unwrap(), "providers");
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        let codex = settings.providers.current().unwrap();
        assert!(!codex.enabled);
    });
    // Off: its problem is no longer shown, its models cannot be picked.
    assert!(cx.debug_bounds("settings-provider-problem").is_none());
    // Advanced: an executable that cannot run means setup, at once; kept in
    // the file the composer reads; "Redefinir" clears it.
    let id = binary.clone();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.choose_provider(id, cx);
            settings
                .providers
                .binary
                .update(cx, |input, cx| input.set_text("/nope/agent", cx));
            settings.runtime_typed(super::providers::Field::Binary, cx);
        })
    });
    cx.run_until_parked();
    let file = dir.path().join("provider-runtime.json");
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("/nope/agent"), "{saved}");
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        let provider = settings.providers.current().unwrap();
        assert_eq!(provider.id.0, binary);
        assert!(
            matches!(&provider.health, HealthStatus::Degraded { .. }),
            "{:?}",
            provider.health
        );
    });
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.clear_runtime(cx)));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        assert!(settings.providers.binary.read(cx).text().is_empty());
        assert!(!matches!(
            settings.providers.current().unwrap().health,
            HealthStatus::Degraded { .. }
        ));
    });
}

#[gpui::test]
fn provider_accounts_sign_in_add_rename_and_remove(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Model), cx);
    let account = |id: &str, label: &str, at: &str| {
        serde_json::json!({
            "id": id, "providerId": "claude_code", "label": label,
            "configDir": format!("/accounts/{id}"), "managed": true, "createdAt": at,
        })
    };
    let list =
        |accounts: Vec<serde_json::Value>| serde_json::json!({ "accounts": accounts }).to_string();
    let zedy = account("claude_code-zedy", "Zedy", "2026-10-01");
    let pessoal = account("claude_code-pessoal", "Pessoal", "2026-10-02");
    let catalog =
        serde_json::json!({ "providers": dcc_providers::offline_catalog().providers }).to_string();
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/provider-accounts", 200, list(vec![zedy.clone()])),
        ("/api/v1/mobile/catalog", 200, catalog),
        (
            "/api/v1/provider-accounts/status",
            200,
            r#"{"status":{"loggedIn":true,"email":"ana@x.com","plan":"max"},"loginCommand":null}"#.into(),
        ),
        (
            "/api/v1/provider-accounts/status",
            200,
            r#"{"status":{"loggedIn":false},"loginCommand":"CLAUDE_CONFIG_DIR=/accounts/claude_code-zedy claude login"}"#.into(),
        ),
        (
            "/api/v1/provider-accounts",
            200,
            serde_json::json!({ "account": pessoal }).to_string(),
        ),
        ("/api/v1/provider-accounts", 200, list(vec![zedy.clone(), pessoal.clone()])),
        (
            "/api/v1/provider-accounts/status",
            200,
            r#"{"status":{"loggedIn":false},"loginCommand":null}"#.into(),
        ),
        (
            "/api/v1/provider-accounts/claude_code-zedy",
            200,
            r#"{"account":{}}"#.into(),
        ),
        ("/api/v1/provider-accounts", 200, list(vec![zedy.clone(), pessoal.clone()])),
        ("/api/v1/provider-accounts/claude_code-pessoal", 200, r#"{"ok":true}"#.into()),
        ("/api/v1/provider-accounts", 200, list(vec![zedy.clone()])),
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
    click(cx, "settings-provider-claude_code");
    assert!(cx.debug_bounds("settings-provider-accounts").is_some());
    // The system login says who; Zedy offers "Entrar" and the command.
    assert!(
        cx.debug_bounds("settings-account-sign-in-default")
            .is_none()
    );
    assert!(
        cx.debug_bounds("settings-account-sign-in-claude_code-zedy")
            .is_some()
    );
    click(cx, "settings-account-copy-claude_code-zedy");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert!(copied.unwrap().contains("claude login"));
    cx.update(|_, cx| {
        let settings = settings.read(cx);
        let status = provider_status(settings, None);
        assert_eq!(status.as_deref(), Some("ana@x.com · max"));
    });
    // Add "Pessoal".
    cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings
                .providers
                .accounts
                .new_label
                .update(cx, |input, cx| input.set_text("Pessoal", cx));
            let _ = window;
            settings.add_account(cx)
        })
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("settings-account-sign-in-claude_code-pessoal")
            .is_some()
    );
    // Rename Zedy: Enter saves.
    click(cx, "settings-account-rename-claude_code-zedy");
    cx.simulate_input("Trabalho");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    // Remove Pessoal after the confirmation.
    click(cx, "settings-account-remove-claude_code-pessoal");
    click(cx, "settings-account-remove-ok");
    let seen = server.join().unwrap();
    let requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert_eq!(
        requests,
        [
            "GET /api/v1/provider-accounts",
            "GET /api/v1/mobile/catalog",
            "POST /api/v1/provider-accounts/status",
            "POST /api/v1/provider-accounts/status",
            "POST /api/v1/provider-accounts",
            "GET /api/v1/provider-accounts",
            "POST /api/v1/provider-accounts/status",
            "PUT /api/v1/provider-accounts/claude_code-zedy",
            "GET /api/v1/provider-accounts",
            "DELETE /api/v1/provider-accounts/claude_code-pessoal",
            "GET /api/v1/provider-accounts",
        ]
    );
    assert!(seen[3].1.contains("claude_code-zedy"), "{}", seen[3].1);
    assert!(seen[4].1.contains("Pessoal"), "{}", seen[4].1);
    assert!(seen[7].1.contains("Trabalho"), "{}", seen[7].1);
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("settings-account-sign-in-claude_code-pessoal")
            .is_none()
    );
}

fn provider_status(settings: &Settings, account: Option<&str>) -> Option<String> {
    match settings
        .providers
        .accounts
        .statuses
        .get(&("claude_code".to_string(), account.map(str::to_string)))?
    {
        super::providers::AccountStatusView::Ready(status) => status.who(),
        _ => None,
    }
}
