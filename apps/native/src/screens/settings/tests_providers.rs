//! Settings → Providers driven headlessly against a fake desktop: the
//! default model, the switch, the launch settings, the accounts, the plan
//! and Antigravity.

use gpui::TestAppContext;

use super::tests::{click, open};
use super::{SectionId, Settings};

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

#[gpui::test]
fn plan_usage_spends_a_reset_and_antigravity_installs(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, events) = open(Some(SectionId::Model), cx);
    // Codex with usage and resets, without accounts; then Antigravity.
    let mut providers: Vec<_> = dcc_providers::offline_catalog()
        .providers
        .into_iter()
        .filter(|p| p.id.0 == "codex" || p.id.0 == "antigravity")
        .collect();
    providers.sort_by_key(|p| p.id.0 != "codex");
    let codex = &mut providers[0];
    codex.capabilities.supports_accounts = false;
    codex.capabilities.supports_account_usage = true;
    codex.capabilities.supports_account_resets = true;
    let catalog = serde_json::json!({ "providers": providers }).to_string();
    let usage = serde_json::json!({ "usage": {
        "providerId": "codex", "state": "available", "updatedAt": "2026-10-27T12:00:00Z",
        "isCached": false,
        "windows": [{ "id": "five_hour", "usedPercent": 60.0, "remainingPercent": 40.0,
            "isExhausted": false, "resetsAt": "2026-10-27T15:00:00Z" }],
        "resetCredits": { "availableCount": 1, "credits": [] },
    }})
    .to_string();
    let status = |installed: bool| {
        serde_json::json!({ "managedRuntimeInstalled": installed, "runtimeVersion": installed.then_some("1.2.3"),
            "signedIn": false, "cachedModelCount": 0 })
        .to_string()
    };
    let (port, server) = fake_desktop_routes(vec![
        (
            "/api/v1/provider-accounts",
            200,
            r#"{"accounts":[]}"#.into(),
        ),
        ("/api/v1/mobile/catalog", 200, catalog.clone()),
        ("/api/v1/provider-accounts/usage", 200, usage.clone()),
        (
            "/api/v1/provider-accounts/reset",
            200,
            r#"{"outcome":"reset"}"#.into(),
        ),
        ("/api/v1/provider-accounts/usage", 200, usage),
        ("/api/v1/antigravity/status", 200, status(false)),
        (
            "/api/v1/antigravity/install",
            200,
            r#"{"binaryPath":"/x/agy","version":"1.2.3"}"#.into(),
        ),
        (
            "/api/v1/provider-accounts",
            200,
            r#"{"accounts":[]}"#.into(),
        ),
        ("/api/v1/mobile/catalog", 200, catalog),
        ("/api/v1/antigravity/status", 200, status(true)),
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
    // "Uso da conta": the window and the saved reset.
    assert!(cx.debug_bounds("settings-usage-default").is_some());
    click(cx, "settings-usage-reset-0");
    assert!(cx.update(|_, cx| settings.read(cx).providers.usage.confirm.is_some()));
    click(cx, "settings-usage-reset-ok");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| settings.read(cx).providers.usage.confirm.is_none()));
    // Antigravity: install the official runtime.
    click(cx, "settings-provider-antigravity");
    assert!(cx.debug_bounds("settings-antigravity").is_some());
    click(cx, "settings-antigravity-install");
    let seen = server.join().unwrap();
    cx.run_until_parked();
    let requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert_eq!(
        &requests[..7],
        [
            "GET /api/v1/provider-accounts",
            "GET /api/v1/mobile/catalog",
            "POST /api/v1/provider-accounts/usage",
            "POST /api/v1/provider-accounts/reset",
            "POST /api/v1/provider-accounts/usage",
            "POST /api/v1/antigravity/status",
            "POST /api/v1/antigravity/install",
        ]
    );
    assert!(
        seen[2].1.contains(r#""providerId":"codex""#),
        "{}",
        seen[2].1
    );
    assert!(events.borrow().iter().any(|event| event == "providers"));
    cx.update(|_, cx| {
        let status = settings
            .read(cx)
            .providers
            .antigravity
            .status
            .clone()
            .unwrap();
        assert!(status.managed_runtime_installed);
    });
}
