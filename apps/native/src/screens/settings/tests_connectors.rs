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
