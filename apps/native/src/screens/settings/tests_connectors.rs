//! Settings → Conectores against a fake desktop: the connected ones and the
//! catalog, the search, testing and removing, and "Conectar" signing in
//! through the browser or with a token.

use gpui::TestAppContext;
use serde_json::{Value, json};

use super::SectionId;
use super::tests::{click, open};
use crate::data::connectors::{View, integration_for_url};

fn record(id: &str, name: &str, url: &str, enabled: bool, owners: &[&str], check: Value) -> Value {
    json!({
        "definition": { "id": id, "displayName": name, "enabled": enabled,
            "transport": { "type": "http", "url": url },
            "trust": { "currentFingerprint": "f",
                "decision": if enabled { json!({ "type": "trusted", "fingerprint": "f" }) }
                    else { json!({ "type": "untrusted" }) } } },
        "oauthProviderIds": owners, "connectionCheck": check,
    })
}

fn list(records: Vec<Value>) -> String {
    json!({ "integrations": records }).to_string()
}

fn attach(settings: &gpui::Entity<super::Settings>, port: u16, cx: &mut gpui::VisualTestContext) {
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
}

#[gpui::test]
fn connected_ones_and_the_catalog_are_tested_and_removed(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Integrations), cx);
    assert!(cx.debug_bounds("settings-connectors").is_some());
    let linear = record(
        "linear-1",
        "Linear",
        "https://mcp.linear.app/mcp",
        true,
        &["dcc"],
        json!({ "state": "connected", "tools": [{}, {}], "message": null }),
    );
    let custom = record(
        "acme-1",
        "Acme",
        "https://mcp.acme.dev/mcp",
        true,
        &["claude_code"],
        Value::Null,
    );
    let (port, server) = fake_desktop_routes(vec![
        (
            "/api/v1/mcp/integrations",
            200,
            list(vec![linear.clone(), custom.clone()]),
        ),
        (
            "/api/v1/mcp/test",
            200,
            r#"{"check":{"state":"connected","tools":[{}],"message":null}}"#.into(),
        ),
        (
            "/api/v1/mcp/integrations",
            200,
            list(vec![linear.clone(), custom.clone()]),
        ),
        (
            "/api/v1/mcp/disconnect",
            200,
            r#"{"disconnected":true}"#.into(),
        ),
        (
            "/api/v1/mcp/integrations/remove",
            200,
            r#"{"removed":true}"#.into(),
        ),
        ("/api/v1/mcp/integrations", 200, list(vec![custom.clone()])),
    ]);
    attach(&settings, port, cx);
    // Linear is connected (out of the catalog); Acme asks to reconnect.
    assert!(cx.debug_bounds("settings-connector-linear-1").is_some());
    assert!(
        cx.debug_bounds("settings-connector-reconnect-acme-1")
            .is_some()
    );
    assert!(
        cx.debug_bounds("settings-connector-reconnect-linear-1")
            .is_none()
    );
    assert!(
        cx.debug_bounds("settings-connector-preset-linear")
            .is_none()
    );
    assert!(
        cx.debug_bounds("settings-connector-preset-notion")
            .is_some()
    );
    // The search narrows both.
    click(cx, "settings-connectors");
    cx.update(|window, cx| {
        let search = settings.read(cx).connectors.search.clone();
        let focus = gpui::Focusable::focus_handle(search.read(cx), cx);
        window.focus(&focus, cx);
    });
    cx.simulate_input("slack");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("settings-connector-preset-notion")
            .is_none()
    );
    assert!(cx.debug_bounds("settings-connector-preset-slack").is_some());
    assert!(cx.debug_bounds("settings-connector-linear-1").is_none());
    let view = cx.update(|_, cx| {
        let integrations = settings.read(cx).connectors.list().to_vec();
        View::of(integration_for_url(&integrations, "https://mcp.linear.app/mcp").unwrap())
    });
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.test_connector(&view, cx)));
    cx.run_until_parked();
    // Removing a signed-in one signs it out first, after the confirmation.
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.connectors.confirm = Some(super::connectors_confirm_remove(view.clone()));
            cx.notify();
        })
    });
    cx.run_until_parked();
    click(cx, "settings-connector-confirm-ok");
    let seen = server.join().unwrap();
    cx.run_until_parked();
    let requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert_eq!(
        requests,
        [
            "GET /api/v1/mcp/integrations",
            "POST /api/v1/mcp/test",
            "GET /api/v1/mcp/integrations",
            "POST /api/v1/mcp/disconnect",
            "POST /api/v1/mcp/integrations/remove",
            "GET /api/v1/mcp/integrations",
        ]
    );
    assert!(
        seen[4].1.contains(r#""deleteCredentials":true"#),
        "{}",
        seen[4].1
    );
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).connectors.list().len()),
        1
    );
}

#[gpui::test]
fn connect_signs_in_through_the_browser_or_with_a_token(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Integrations), cx);
    let notion_new = record(
        "notion-1",
        "Notion",
        "https://mcp.notion.com/mcp",
        false,
        &[],
        Value::Null,
    );
    let notion_on = record(
        "notion-1",
        "Notion",
        "https://mcp.notion.com/mcp",
        true,
        &[],
        Value::Null,
    );
    let github = record(
        "github-1",
        "GitHub",
        "https://api.githubcopilot.com/mcp/",
        false,
        &[],
        Value::Null,
    );
    let github_on = record(
        "github-1",
        "GitHub",
        "https://api.githubcopilot.com/mcp/",
        true,
        &[],
        Value::Null,
    );
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/mcp/integrations", 200, list(vec![])),
        // Notion: created, activated, the browser, signed in.
        ("/api/v1/mcp/integrations", 200, json!({ "integration": notion_new }).to_string()),
        ("/api/v1/mcp/integrations/activate", 200, json!({ "integration": notion_on }).to_string()),
        ("/api/v1/mcp/connect/start", 200,
            r#"{"kind":"authorize","loginId":"l-1","authorizationUrl":"https://notion.example/auth"}"#.into()),
        ("/api/v1/mcp/connect/wait", 200,
            r#"{"kind":"connected","check":{"state":"connected","tools":[{},{},{}],"message":null}}"#.into()),
        ("/api/v1/mcp/integrations", 200, list(vec![])),
        ("/api/v1/mcp/integrations", 200, list(vec![])),
        // GitHub: a personal token, checked before it is kept.
        ("/api/v1/mcp/integrations", 200, json!({ "integration": github }).to_string()),
        ("/api/v1/mcp/integrations/activate", 200, json!({ "integration": github_on }).to_string()),
        ("/api/v1/mcp/test", 200, r#"{"check":{"state":"connected","tools":[{}],"message":null}}"#.into()),
    ]);
    attach(&settings, port, cx);
    click(cx, "settings-connector-preset-notion");
    assert!(cx.debug_bounds("connector-connect").is_some());
    click(cx, "connector-continue");
    cx.run_until_parked();
    // Signed in: the dialog went and the list was read again.
    assert!(cx.debug_bounds("connector-connect").is_none());
    click(cx, "settings-connector-preset-github");
    // Its long texts wrap inside the dialog.
    let dialog = cx.debug_bounds("connector-connect").expect("the dialog");
    for text in ["connector-data-body", "connector-token-hint"] {
        let bounds = cx.debug_bounds(text).unwrap_or_else(|| panic!("{text}"));
        assert!(
            bounds.right() <= dialog.right() + gpui::px(0.5),
            "{text} runs past the dialog: {bounds:?} in {dialog:?}"
        );
    }
    // A token is needed before continuing.
    cx.update(|_, cx| {
        let (dialog, _) = settings.read(cx).connectors.connect.as_ref().unwrap();
        assert!(dialog.read(cx).primary_disabled(cx));
    });
    cx.update(|window, cx| {
        let (dialog, _) = settings.read(cx).connectors.connect.as_ref().unwrap();
        let token = dialog.read(cx).token.clone();
        let focus = gpui::Focusable::focus_handle(token.read(cx), cx);
        window.focus(&focus, cx);
    });
    cx.simulate_input("ghp_secret");
    click(cx, "connector-continue");
    let seen = server.join().unwrap();
    cx.run_until_parked();
    let created: Value = serde_json::from_str(&seen[1].1).unwrap();
    assert_eq!(created["transport"]["url"], "https://mcp.notion.com/mcp");
    assert_eq!(created["credentials"], json!([]));
    assert!(
        seen[3].1.contains(r#""definitionId":"notion-1""#),
        "{}",
        seen[3].1
    );
    assert!(seen[4].1.contains(r#""loginId":"l-1""#), "{}", seen[4].1);
    let with_token = seen
        .iter()
        .filter(|(request, _)| request == "POST /api/v1/mcp/integrations")
        .nth(1)
        .unwrap();
    let body: Value = serde_json::from_str(&with_token.1).unwrap();
    assert_eq!(body["credentials"][0]["secret"], "Bearer ghp_secret");
    assert!(
        seen.iter()
            .any(|(request, _)| request == "POST /api/v1/mcp/test")
    );
    assert!(cx.debug_bounds("connector-connect").is_none());
}

#[gpui::test]
fn a_custom_connector_gets_its_name_and_scopes_before_connecting(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Integrations), cx);
    let acme = record(
        "acme-1",
        "Acme",
        "https://mcp.acme.dev/mcp",
        false,
        &[],
        Value::Null,
    );
    let acme_on = record(
        "acme-1",
        "Acme",
        "https://mcp.acme.dev/mcp",
        true,
        &[],
        Value::Null,
    );
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/mcp/integrations", 200, list(vec![])),
        (
            "/api/v1/mcp/integrations",
            200,
            json!({ "integration": acme }).to_string(),
        ),
        (
            "/api/v1/mcp/scopes",
            200,
            r#"{"scopes":"read write","logo":null}"#.into(),
        ),
        (
            "/api/v1/mcp/integrations/activate",
            200,
            json!({ "integration": acme_on }).to_string(),
        ),
        (
            "/api/v1/mcp/connect/start",
            200,
            r#"{"kind":"noAuthRequired"}"#.into(),
        ),
        (
            "/api/v1/mcp/test",
            200,
            r#"{"check":{"state":"connected","tools":[],"message":null}}"#.into(),
        ),
    ]);
    attach(&settings, port, cx);
    click(cx, "settings-connectors-add");
    assert!(cx.debug_bounds("connector-custom").is_some());
    type Pick =
        fn(&super::connectors::ConnectorCustom) -> gpui::Entity<crate::ui::text_input::TextInput>;
    let field = |pick: Pick, cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            let (dialog, _) = settings
                .read(cx)
                .connectors
                .custom
                .as_ref()
                .expect("the custom dialog");
            pick(dialog.read(cx))
        })
    };
    let focus = |pick: Pick, cx: &mut gpui::VisualTestContext| {
        let input = field(pick, cx);
        cx.update(|window, cx| {
            let handle = gpui::Focusable::focus_handle(input.read(cx), cx);
            window.focus(&handle, cx);
        });
    };
    // An address that is not one says so.
    focus(|dialog| dialog.url.clone(), cx);
    cx.simulate_input("http://acme.dev");
    click(cx, "custom-connector-continue");
    assert!(cx.debug_bounds("connector-custom").is_some());
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("https://mcp.acme.dev/mcp");
    cx.run_until_parked();
    let name_input = field(|dialog| dialog.name.clone(), cx);
    let name = cx.update(|_, cx| name_input.read(cx).text().to_string());
    assert_eq!(name, "Acme");
    click(cx, "custom-connector-more");
    focus(|dialog| dialog.scopes.clone(), cx);
    cx.simulate_input("read  write");
    click(cx, "custom-connector-continue");
    assert!(cx.debug_bounds("connector-custom").is_none());
    assert!(cx.debug_bounds("connector-connect").is_some());
    click(cx, "connector-continue");
    let seen = server.join().unwrap();
    cx.run_until_parked();
    let created: Value = serde_json::from_str(&seen[1].1).unwrap();
    assert_eq!(created["displayName"], "Acme");
    assert!(
        seen[2].1.contains(r#""scopes":"read write""#),
        "{}",
        seen[2].1
    );
    assert_eq!(seen[5].0, "POST /api/v1/mcp/test");
    assert!(cx.debug_bounds("connector-connect").is_none());
}

#[gpui::test]
fn manage_sets_tool_policies_and_projects_and_the_import_offer_is_dismissed(
    cx: &mut TestAppContext,
) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Integrations), cx);
    let mut acme = record(
        "acme-1",
        "Acme",
        "https://mcp.acme.dev/mcp",
        true,
        &["dcc"],
        json!({
            "state": "connected", "message": null, "checkedAt": "2026-10-27T12:00:00Z",
            "tools": [{ "name": "search", "annotations": { "readOnlyHint": true } }],
        }),
    );
    acme["bindings"] = json!([{ "scope": { "type": "global" }, "enabled": true }]);
    let natives = json!({ "servers": [
        { "app": "claudeCode", "name": "linear", "url": "https://mcp.linear.app/mcp", "hasHeaders": false },
    ] })
    .to_string();
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/mcp/integrations", 200, list(vec![acme.clone()])),
        ("/api/v1/mcp/native-servers", 200, natives),
        ("/api/v1/mcp/tool-policy", 200, r#"{"policy":{}}"#.into()),
        ("/api/v1/mcp/integrations", 200, list(vec![acme.clone()])),
        (
            "/api/v1/mcp/project",
            200,
            r#"{"excludedProjectIds":["p-1"]}"#.into(),
        ),
        ("/api/v1/mcp/integrations", 200, list(vec![acme.clone()])),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let offered = dir.path().join("offered.json");
    cx.update(|_, cx| {
        settings.update(cx, |settings, _| {
            settings.connectors.import.path = Some(offered.clone())
        })
    });
    attach(&settings, port, cx);
    // "Encontramos Linear em Claude Code."
    assert!(cx.debug_bounds("settings-connectors-import").is_some());
    click(cx, "settings-connectors-import-later");
    assert!(cx.debug_bounds("settings-connectors-import").is_none());
    assert_eq!(
        crate::data::connector_custom::read_offered(&offered),
        ["https://mcp.linear.app/mcp"]
    );
    let view = cx.update(|_, cx| View::of(&settings.read(cx).connectors.list()[0].clone()));
    cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings.open_manage(&view, window, cx);
            settings.set_manage_projects(vec![("p-1".into(), "Zedy".into())]);
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("connector-manage").is_some());
    click(cx, "manage-policy-search-allow");
    cx.run_until_parked();
    click(cx, "manage-project-p-1");
    let seen = server.join().unwrap();
    cx.run_until_parked();
    assert!(seen[2].1.contains(r#""decision":"allow""#), "{}", seen[2].1);
    assert!(seen[4].1.contains(r#""enabled":false"#), "{}", seen[4].1);
}

/// The "…" of a connected one opens over the settings dialog, and its
/// items take the click (a menu inside a dialog once painted under it).
#[gpui::test]
fn the_more_button_opens_its_menu_over_the_dialog(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Integrations), cx);
    let linear = record(
        "linear-1",
        "Linear",
        "https://mcp.linear.app/mcp",
        true,
        &["dcc"],
        json!({ "state": "connected", "tools": [{}, {}], "message": null }),
    );
    let (port, _server) =
        fake_desktop_routes(vec![("/api/v1/mcp/integrations", 200, list(vec![linear]))]);
    attach(&settings, port, cx);
    click(cx, "settings-connector-menu-linear-1");
    assert!(cx.debug_bounds("menu-panel").is_some());
    click(cx, "menu-item-0");
    let managed = cx.update(|_, cx| {
        settings
            .read(cx)
            .connectors
            .managed
            .as_ref()
            .map(|managed| managed.id.clone())
    });
    assert_eq!(
        managed.as_deref(),
        Some("linear-1"),
        "\"Gerenciar\" took the click"
    );
}
