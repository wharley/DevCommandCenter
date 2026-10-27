//! Settings → Conexões against a fake desktop: the paired phones and the
//! relay, revoking one, saving the relay's address, the QR, approving a
//! phone that scanned it, and the audit log.

use gpui::TestAppContext;
use serde_json::{Value, json};

use super::SectionId;
use super::tests::open;

fn status(url: &str) -> String {
    json!({ "relayUrl": url, "relayUrlFromEnv": false, "connected": true, "roomId": "r",
        "desktopFingerprint": "AB12-CD34", "activeConnections": 1 })
    .to_string()
}

#[gpui::test]
fn phones_are_revoked_paired_and_audited(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    let (settings, cx, _) = open(Some(SectionId::Connections), cx);
    let expires = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
    let pixel = json!({ "deviceId": "phone-1", "deviceName": "Pixel", "userAgent": "DCC/1.0",
        "createdAt": "2026-10-20 12:00:00", "lastUsedAt": null, "lastIp": null,
        "revoked": false, "kind": "relay" });
    let (port, desktop) = fake_desktop_routes(vec![
        (
            "/api/v1/remote/devices",
            200,
            json!({ "devices": [pixel] }).to_string(),
        ),
        (
            "/api/v1/remote/status",
            200,
            status("wss://relay.devcommandcenter.com"),
        ),
        (
            "/api/v1/remote/pairing/approvals",
            200,
            r#"{"approvals":[]}"#.into(),
        ),
        (
            "/api/v1/remote/devices/revoke",
            200,
            r#"{"revoked":true}"#.into(),
        ),
        ("/api/v1/remote/devices", 200, r#"{"devices":[]}"#.into()),
        ("/api/v1/remote/relay", 200, status("wss://mine.dev")),
        (
            "/api/v1/remote/pairing/offers",
            200,
            json!({ "url": "dcc://pair#secret", "expiresAt": expires,
                "desktopFingerprint": "AB12-CD34" })
            .to_string(),
        ),
        (
            "/api/v1/remote/pairing/approvals",
            200,
            json!({ "approvals": [{ "id": "ap-1", "deviceName": "iPhone", "platform": "ios",
                "fingerprint": "9F3A-77C1", "expiresAt": expires }] })
            .to_string(),
        ),
        (
            "/api/v1/remote/pairing/approvals/ap-1",
            200,
            r#"{"ok":true}"#.into(),
        ),
        ("/api/v1/remote/devices", 200, r#"{"devices":[]}"#.into()),
        (
            "/api/v1/remote/audit?limit=50",
            200,
            json!({ "entries": [{ "id": 2, "event": "revoke", "deviceId": "phone-1",
                "ip": null, "userAgent": null, "detailsJson": null,
                "createdAt": "2026-10-27 12:00:00" }] })
            .to_string(),
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
    assert!(cx.debug_bounds("settings-connections").is_some());
    assert!(cx.debug_bounds("settings-relay").is_some());
    assert!(cx.debug_bounds("settings-device-phone-1").is_some());
    let url = cx.update(|_, cx| {
        settings
            .read(cx)
            .connections
            .relay_input
            .read(cx)
            .text()
            .to_string()
    });
    assert_eq!(url, "wss://relay.devcommandcenter.com");

    let pixel = cx.update(|_, cx| settings.read(cx).connections.devices[0].clone());
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.revoke_device(pixel, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-device-phone-1").is_none());

    // A new address, saved.
    cx.update(|_, cx| {
        settings.update(cx, |settings, cx| {
            let input = settings.connections.relay_input.clone();
            input.update(cx, |input, cx| input.set_text(" wss://mine.dev ", cx));
            settings.save_relay(cx);
        })
    });
    cx.run_until_parked();

    // The QR, then the phone that scanned it.
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.open_pairing(cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-relay-pairing").is_some());
    assert!(cx.debug_bounds("settings-relay-qr").is_some());
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.load_approvals(cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-relay-approval").is_some());
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.answer_pairing(true, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-relay-approval").is_none());
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.close_pairing(cx)));
    cx.update(|_, cx| settings.update(cx, |settings, cx| settings.toggle_audit(cx)));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-connections-audit-open").is_some());
    let mut requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    // The three first reads go out together.
    requests[..3].sort_unstable();
    assert_eq!(
        requests,
        [
            "GET /api/v1/remote/devices",
            "GET /api/v1/remote/pairing/approvals",
            "GET /api/v1/remote/status",
            "POST /api/v1/remote/devices/revoke",
            "GET /api/v1/remote/devices",
            "POST /api/v1/remote/relay",
            "POST /api/v1/remote/pairing/offers",
            "GET /api/v1/remote/pairing/approvals",
            "POST /api/v1/remote/pairing/approvals/ap-1",
            "GET /api/v1/remote/devices",
            "GET /api/v1/remote/audit?limit=50",
        ]
    );
    let body = |request: &str| {
        let (_, body) = seen.iter().find(|(seen, _)| seen == request).unwrap();
        serde_json::from_str::<Value>(body).unwrap()
    };
    assert_eq!(
        body("POST /api/v1/remote/devices/revoke"),
        json!({ "deviceId": "phone-1" })
    );
    assert_eq!(
        body("POST /api/v1/remote/relay"),
        json!({ "relayUrl": "wss://mine.dev" })
    );
    assert_eq!(
        body("POST /api/v1/remote/pairing/approvals/ap-1"),
        json!({ "approve": true })
    );
    let relay = cx.update(|_, cx| settings.read(cx).connections.relay.clone().unwrap());
    assert_eq!(relay.relay_url.as_deref(), Some("wss://mine.dev"));
}
