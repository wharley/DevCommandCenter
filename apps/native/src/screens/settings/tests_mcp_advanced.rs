//! Settings → Conectores → Avançado against a fake desktop: a server's
//! status in the open session, a local command created, reviewed and
//! turned on, then off, then removed with its credentials.

use gpui::TestAppContext;
use serde_json::{Value, json};

use super::SectionId;
use super::tests::open;
use crate::data::mcp_advanced::{SessionContext, TransportDraft};

fn server(id: &str, enabled: bool) -> Value {
    json!({
        "definition": { "id": id, "displayName": "Acme", "enabled": enabled,
            "updatedAt": "2026-10-27T09:00:00Z",
            "transport": { "type": "stdio", "executable": "/bin/acme", "args": ["--stdio"] },
            "secretRefs": [{ "target": { "type": "environmentVariable", "name": "API_KEY" } }],
            "trust": { "currentFingerprint": "sha256:f",
                "decision": if enabled { json!({ "type": "trusted", "fingerprint": "sha256:f" }) }
                    else { json!({ "type": "untrusted" }) } } },
        "bindings": [{ "scope": { "type": "global" }, "enabled": true,
            "updatedAt": "2026-10-27T09:00:00Z" }],
        "toolPolicies": [{ "toolName": "write", "decision": "deny",
            "updatedAt": "2026-10-27T09:00:00Z" }],
        "credentialCount": 1,
    })
}

fn list(records: Vec<Value>) -> String {
    json!({ "integrations": records }).to_string()
}

#[gpui::test]
fn a_local_server_is_reviewed_turned_on_off_and_removed(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Integrations), cx);
    let statuses = json!({ "statuses": [{
        "definitionId": "acme-1", "providerId": "codex", "providerVersion": "0.50",
        "sessionId": "s-1", "state": "connected", "checkedAt": "2026-10-27T11:00:00Z",
        "tools": [{ "name": "search", "annotations": { "readOnlyHint": true } }],
    }] });
    let (port, desktop) = fake_desktop_routes(vec![
        (
            "/api/v1/mcp/integrations",
            200,
            list(vec![server("acme-1", true)]),
        ),
        ("/api/v1/mcp/runtime", 200, statuses.to_string()),
        (
            "/api/v1/mcp/integrations",
            200,
            json!({ "integration": server("new-1", false) }).to_string(),
        ),
        (
            "/api/v1/mcp/integrations",
            200,
            list(vec![server("acme-1", true), server("new-1", false)]),
        ),
        (
            "/api/v1/mcp/integrations/activate",
            200,
            json!({ "integration": server("new-1", true) }).to_string(),
        ),
        (
            "/api/v1/mcp/integrations",
            200,
            list(vec![server("acme-1", true), server("new-1", true)]),
        ),
        (
            "/api/v1/mcp/integrations/disable",
            200,
            r#"{"ok":true}"#.into(),
        ),
        (
            "/api/v1/mcp/integrations",
            200,
            list(vec![server("acme-1", true), server("new-1", false)]),
        ),
        (
            "/api/v1/mcp/integrations/remove",
            200,
            r#"{"removed":true}"#.into(),
        ),
        (
            "/api/v1/mcp/integrations",
            200,
            list(vec![server("acme-1", true)]),
        ),
    ]);
    cx.update(|_, cx| {
        settings.update(cx, |settings, _| {
            settings.session = SessionContext {
                project_id: Some("p-1".into()),
                session_id: Some("s-1".into()),
                provider_id: Some("codex".into()),
                created_at: Some("2026-10-27T10:00:00Z".into()),
            };
        })
    });
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
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.toggle_advanced(cx)));
    cx.run_until_parked();
    // Reported in the session: the tools it lists plus the one with a policy.
    assert!(cx.debug_bounds("settings-mcp-advanced").is_some());
    assert!(cx.debug_bounds("settings-mcp-acme-1").is_some());
    assert!(cx.debug_bounds("settings-mcp-tools-acme-1").is_some());

    // A local command, with its key in the environment.
    cx.update(|window, cx| {
        settings.update(cx, |settings, cx| {
            settings.open_advanced_create(window, cx);
            settings.set_advanced_transport(TransportDraft::Stdio, cx);
            settings.add_advanced_credential(window, cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-mcp-create").is_some());
    let fill = |cx: &mut gpui::VisualTestContext,
                pick: fn(&super::Settings) -> gpui::Entity<crate::ui::TextInput>,
                text: &str| {
        cx.update(|window, cx| {
            let input = pick(settings.read(cx));
            window.focus(&gpui::Focusable::focus_handle(input.read(cx), cx), cx);
        });
        cx.simulate_input(text);
        cx.run_until_parked();
    };
    fn form(settings: &super::Settings) -> &super::connectors::Form {
        settings.connectors.advanced.form.as_ref().unwrap()
    }
    fill(cx, |settings| form(settings).name.clone(), "Acme local");
    fill(
        cx,
        |settings| form(settings).executable.clone(),
        "/bin/acme",
    );
    fill(
        cx,
        |settings| form(settings).credentials[0].name.clone(),
        "API_KEY",
    );
    fill(
        cx,
        |settings| form(settings).credentials[0].secret.clone(),
        "s3cret",
    );
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.create_advanced(cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-mcp-review").is_some());
    assert!(cx.debug_bounds("settings-mcp-review-preview").is_some());

    // "Confirmar e ativar" for the fingerprint reviewed.
    let reviewed = cx.update(|_, cx| match &settings.read(cx).connectors.advanced.panel {
        super::connectors::Panel::Review(integration) => (**integration).clone(),
        _ => panic!("not reviewing"),
    });
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.activate_advanced(reviewed, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-mcp-review").is_none());
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.disable_advanced("new-1".into(), cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.connectors.advanced.remove = Some("new-1".into());
            settings.connectors.advanced.delete_credentials = true;
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-mcp-remove-confirm").is_some());
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.remove_advanced(cx)));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    let requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert_eq!(
        requests,
        [
            "GET /api/v1/mcp/integrations",
            "POST /api/v1/mcp/runtime",
            "POST /api/v1/mcp/integrations",
            "GET /api/v1/mcp/integrations",
            "POST /api/v1/mcp/integrations/activate",
            "GET /api/v1/mcp/integrations",
            "POST /api/v1/mcp/integrations/disable",
            "GET /api/v1/mcp/integrations",
            "POST /api/v1/mcp/integrations/remove",
            "GET /api/v1/mcp/integrations",
        ]
    );
    let body = |index: usize| serde_json::from_str::<Value>(&seen[index].1).unwrap();
    assert_eq!(body(1), json!({ "sessionId": "s-1" }));
    assert_eq!(
        body(2),
        json!({
            "displayName": "Acme local",
            "transport": { "type": "stdio", "executable": "/bin/acme", "args": [], "cwd": null },
            "scope": { "type": "project", "projectId": "p-1" },
            "credentials": [{ "target": { "type": "environmentVariable", "name": "API_KEY" },
                "secret": "s3cret" }],
        })
    );
    assert_eq!(body(4)["expectedFingerprint"], "sha256:f");
    assert_eq!(body(6), json!({ "definitionId": "new-1" }));
    assert_eq!(
        body(8),
        json!({ "definitionId": "new-1", "deleteCredentials": true })
    );
    let advanced = cx.update(|_, cx| {
        let advanced = &settings.read(cx).connectors.advanced;
        (advanced.remove.clone(), advanced.busy.clone())
    });
    assert_eq!(advanced, (None, None));
}
