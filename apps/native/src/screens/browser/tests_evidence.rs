//! The agent's menu with a fake desktop: evidence captured with the
//! control on and collected into the composer's block, a capture lost with
//! the grant, and the Browser's activity newest first.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{TestAppContext, VisualTestContext};

use super::BrowserEvent;
use super::bridge::SurfaceBridge;
use super::page::PageEvent;
use super::tests::{Pages, open, send};
use crate::data::browser_bridge::ControlStatus;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;

#[gpui::test]
fn evidence_goes_to_the_agent_and_the_activity_reads_newest_first(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_routes(vec![
        (
            "/api/v1/browser/bridge/evidence/start",
            200,
            r#"{"captureId":"c-1","remainingMs":30000,"url":"https://example.com/docs","title":"Docs","text":"Guia <checkout>","selectionOnly":false,"truncated":false}"#.into(),
        ),
        (
            "/api/v1/browser/bridge/evidence/read",
            200,
            r#"{"events":[{"kind":"consoleError","sequence":1,"message":"boom","line":3,"column":9}],"truncated":false,"untrusted":true}"#.into(),
        ),
        (
            "/api/v1/browser/bridge/audit",
            200,
            r#"{"records":[{"origin":"ui","providerId":null,"tool":"dcc_browser_reload","grantState":"notApplicable","outcome":"executed","timestampMs":3},{"origin":"mcp","providerId":"claude","tool":"dcc_browser_click","grantState":"armed","outcome":"stale","timestampMs":9}]}"#.into(),
        ),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    let pages = Rc::new(Pages::default());
    let (surface, cx) = open(cx, &db, Some("https://example.com/docs"), &pages);
    let sent: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = sent.clone();
    cx.update(|_, cx| {
        cx.subscribe(&surface, move |_, event: &BrowserEvent, _| {
            if let BrowserEvent::SendEvidence(text) = event {
                sink.borrow_mut().push(text.clone());
            }
        })
        .detach();
        surface.update(cx, |surface, _| {
            surface.bridge = Some(SurfaceBridge::for_test(41).0);
            surface.bridge_api = Some(DesktopApi::new(port, "k"));
        });
    });
    send(
        &pages,
        PageEvent::Loaded("https://example.com/docs".into()),
        cx,
    );
    // Without the agent's control there is nothing to capture.
    assert!(!cx.update(|_, cx| surface.read(cx).can_start_evidence()));
    let arm = |cx: &mut VisualTestContext, armed: bool| {
        cx.update(|_, cx| {
            surface.update(cx, |surface, cx| {
                surface.set_control(
                    ControlStatus {
                        armed,
                        remaining_ms: if armed { 60_000 } else { 0 },
                    },
                    cx,
                )
            })
        });
        cx.run_until_parked();
    };
    arm(cx, true);
    cx.update(|_, cx| surface.update(cx, |surface, cx| surface.start_evidence(cx)));
    cx.run_until_parked();
    let seconds = cx.update(|_, cx| surface.read(cx).evidence.as_ref().map(|c| c.seconds_left()));
    assert_eq!(seconds, Some(30));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let collect = cx
        .debug_bounds("browser-evidence-collect")
        .expect("Coletar in the strip");
    cx.simulate_click(collect.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| surface.read(cx).evidence.is_none()),
        "spent"
    );
    let sent = sent.borrow().clone();
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].starts_with("<browser_evidence>\nurl: https://example.com/docs\ntitle: Docs\n")
    );
    assert!(sent[0].contains("texto da página ao iniciar (texto visível da página):\nGuia &lt;checkout>"));
    assert!(sent[0].contains("1. [consoleError] boom | at=3:9"));
    // The activity: newest first, with who did it and how it ended.
    cx.update(|_, cx| surface.update(cx, |surface, cx| surface.set_audit_open(true, cx)));
    cx.run_until_parked();
    let tools: Vec<String> = cx.update(|_, cx| {
        surface
            .read(cx)
            .audit
            .records
            .iter()
            .flatten()
            .map(|r| r.tool.clone())
            .collect()
    });
    assert_eq!(tools, ["dcc_browser_click", "dcc_browser_reload"]);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("browser-audit-entry-1").is_some());
    let requests = server.join().unwrap();
    let start: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(start["lifecycleToken"], 41);
    assert_eq!(start["sessionId"], "s-1");
    let read: serde_json::Value = serde_json::from_str(&requests[1].1).unwrap();
    assert_eq!(read["captureId"], "c-1");
    let audit: serde_json::Value = serde_json::from_str(&requests[2].1).unwrap();
    assert_eq!(audit["limit"], 50);
    // A capture ends with the grant that started it.
    cx.update(|_, cx| {
        surface.update(cx, |surface, _| {
            let now = std::time::Instant::now();
            surface.evidence = Some(super::evidence::Capture {
                capture_id: "c-2".into(),
                token: 41,
                page: Default::default(),
                started_at: chrono::Utc::now(),
                started: now,
                expires: now + std::time::Duration::from_secs(30),
            });
        })
    });
    arm(cx, false);
    let (evidence, error) = cx.update(|_, cx| {
        let surface = surface.read(cx);
        (surface.evidence.is_some(), surface.error.clone())
    });
    assert!(!evidence);
    assert_eq!(
        error.as_deref(),
        Some("A captura de evidência expirou antes de ser coletada.")
    );
}
