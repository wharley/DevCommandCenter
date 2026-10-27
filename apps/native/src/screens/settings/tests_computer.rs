//! Settings → Computer Use against a fake desktop: the Mac's state for the
//! open conversation, a permission's settings opened, and the armed grant's
//! countdown with "Desativar agora".

use gpui::TestAppContext;
use serde_json::{Value, json};

use super::SectionId;
use super::computer::remaining;
use super::tests::open;
use crate::data::computer_use::AccessKind;
use crate::data::mcp_advanced::SessionContext;

fn status(armed: bool) -> String {
    json!({
        "platform": "macos", "supported": true, "unsupportedReason": null,
        "minimumMacosVersion": null, "runtimeAttached": true,
        "accessibility": { "granted": true, "canRequest": false },
        "screenRecording": { "granted": false, "canRequest": true },
        "grant": { "sessionId": "s-1", "armed": armed, "remainingMs": if armed { 125_000 } else { 0 },
            "allowedBundleIds": if armed { json!(["com.apple.Safari"]) } else { json!([]) } },
        "targets": [{ "bundleId": "com.apple.Safari", "name": "Safari" },
            { "bundleId": "com.apple.Notes", "name": "Notes" }],
    })
    .to_string()
}

#[test]
fn the_countdown_reads_like_the_desktop() {
    assert_eq!(remaining(125_000), "2:05");
    assert_eq!(remaining(1), "0:01");
    assert_eq!(remaining(0), "0:00");
}

#[gpui::test]
fn the_mac_s_state_a_permission_and_the_grant_ended(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::ComputerUse), cx);
    assert_eq!(
        cx.update(|_, cx| settings.read(cx).active),
        SectionId::ComputerUse
    );
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/computer/bridge/status", 200, status(true)),
        ("/api/v1/computer/bridge/access", 200, status(true)),
        ("/api/v1/computer/bridge/status", 200, status(true)),
        (
            "/api/v1/computer/bridge/disarm",
            200,
            r#"{"sessionId":"s-1","armed":false,"remainingMs":0,"allowedBundleIds":[]}"#.into(),
        ),
        ("/api/v1/computer/bridge/status", 200, status(false)),
    ]);
    cx.update(|_, cx| {
        settings.update(cx, |settings, _| {
            settings.session = SessionContext {
                session_id: Some("s-1".into()),
                provider_id: Some("codex".into()),
                ..SessionContext::default()
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
    assert!(cx.debug_bounds("settings-computer").is_some());
    assert!(cx.debug_bounds("settings-computer-matrix").is_some());
    // Screen recording asks; accessibility is already there.
    assert!(
        cx.debug_bounds("settings-computer-request-screenRecording")
            .is_some()
    );
    assert!(
        cx.debug_bounds("settings-computer-request-accessibility")
            .is_none()
    );
    assert!(cx.debug_bounds("settings-computer-grant").is_some());
    let names = cx.update(|_, cx| {
        let (status, _) = settings.read(cx).computer.status.clone().unwrap();
        status.granted_app_names()
    });
    assert_eq!(names, ["Safari"]);
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            settings.request_computer_access(AccessKind::ScreenRecording, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.disarm_computer(cx)));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-computer-grant").is_none());
    let requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert_eq!(
        requests,
        [
            "POST /api/v1/computer/bridge/status",
            "POST /api/v1/computer/bridge/access",
            "POST /api/v1/computer/bridge/status",
            "POST /api/v1/computer/bridge/disarm",
            "POST /api/v1/computer/bridge/status",
        ]
    );
    let body = |index: usize| serde_json::from_str::<Value>(&seen[index].1).unwrap();
    assert_eq!(
        body(0),
        json!({ "sessionId": "s-1", "includeTargets": true })
    );
    assert_eq!(body(1), json!({ "kind": "screenRecording" }));
    assert_eq!(body(3), json!({ "sessionId": "s-1" }));
}
