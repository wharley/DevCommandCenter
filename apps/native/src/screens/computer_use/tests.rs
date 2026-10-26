//! The computer-use card with a fake desktop: it reads the Mac's state for
//! the request, "Permitir" waits for an app to be chosen, the answer goes
//! with the apps, and a permission opens its settings.

use std::rc::Rc;

use gpui::{TestAppContext, VisualTestContext, WindowAppearance, px, size};

use super::approval::{ComputerApprovals, format_remaining};
use crate::data::computer_use::{ComputerEvent, ControlRequest};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

const READY: &str = r#"{"supported":true,"runtimeAttached":true,"accessibility":{"granted":true,"canRequest":false},"screenRecording":{"granted":true,"canRequest":false},"targets":[{"bundleId":"com.apple.finder","name":"Finder"},{"bundleId":"com.apple.Safari","name":"Safari"}]}"#;

#[test]
fn the_time_left_reads_like_the_desktop() {
    assert_eq!(format_remaining(65_000), "1:05");
    assert_eq!(format_remaining(44_100), "45s");
}

#[gpui::test]
fn allowing_waits_for_an_app_and_sends_the_ones_chosen(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/computer/bridge/status", 200, READY.into()),
        (
            "/api/v1/computer/bridge/requests/0123456789abcdef0123456789abcdef/respond",
            200,
            r#"{"grant":{}}"#.into(),
        ),
    ]);
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (approvals, cx) = cx.add_window_view(|_, cx| {
        let mut approvals =
            ComputerApprovals::new(None, Rc::new(|_, _| Some("Ajustar planos".into())), cx);
        approvals.set_api(DesktopApi::new(port, "k"));
        approvals
    });
    cx.simulate_resize(size(px(900.0), px(800.0)));
    let apply = |cx: &mut VisualTestContext, requests: Vec<ControlRequest>| {
        cx.update(|_, cx| {
            approvals.update(cx, |approvals, cx| {
                approvals.apply(ComputerEvent::Requests(requests), cx)
            })
        });
        cx.run_until_parked();
    };
    apply(
        cx,
        vec![ControlRequest {
            request_id: "0123456789abcdef0123456789abcdef".into(),
            session_id: "s-1".into(),
            workspace_id: "w-1".into(),
            provider_id: "codex".into(),
            reason: "Abrir o Finder".into(),
            remaining_ms: 120_000,
        }],
    );
    // The Mac's state came: ready, but nothing chosen yet.
    let (ready, can) = cx.update(|_, cx| {
        let approvals = approvals.read(cx);
        (approvals.status.is_some(), approvals.can_allow())
    });
    assert!(ready);
    assert!(!can, "no app chosen by default");
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("computer-approval").is_some());
    let finder = cx.debug_bounds("computer-app-0").expect("Finder");
    cx.simulate_click(finder.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| approvals.read(cx).can_allow()));
    cx.update(|_, cx| approvals.update(cx, |approvals, cx| approvals.decide(true, cx)));
    cx.run_until_parked();
    apply(cx, Vec::new());
    assert!(cx.update(|_, cx| approvals.read(cx).current().is_none()));
    let requests = server.join().unwrap();
    let status: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(status["sessionId"], "s-1");
    let answer: serde_json::Value = serde_json::from_str(&requests[1].1).unwrap();
    assert_eq!(answer["allowed"], true);
    assert_eq!(
        answer["allowedBundleIds"],
        serde_json::json!(["com.apple.finder"])
    );
}

#[gpui::test]
fn the_preview_follows_the_agent_and_stops_it(cx: &mut TestAppContext) {
    use super::preview::{ComputerPreview, PreviewEvent, activity_label_key};
    use crate::data::computer_use::Activity;
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
    let frame = format!(
        r#"{{"frame":{{"sessionId":"s-1","target":{{"bundleId":"com.apple.finder","name":"Finder","width":800,"height":600}},"mimeType":"image/png","imageBase64":"{PNG}","capturedAtMs":1}}}}"#
    );
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/computer/bridge/preview", 200, frame),
        (
            "/api/v1/computer/bridge/status",
            200,
            r#"{"grant":{"armed":true,"remainingMs":600000}}"#.into(),
        ),
    ]);
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (preview, cx) = cx.add_window_view(|_, _| {
        let mut preview = ComputerPreview::new(None);
        preview.set_api(DesktopApi::new(port, "k"));
        preview
    });
    cx.simulate_resize(size(px(900.0), px(700.0)));
    let stops = Rc::new(std::cell::Cell::new(0));
    let counter = stops.clone();
    cx.update(|_, cx| {
        cx.subscribe(&preview, move |_, event: &PreviewEvent, _| {
            let PreviewEvent::Stop = event;
            counter.set(counter.get() + 1);
        })
        .detach();
        preview.update(cx, |preview, cx| {
            preview.follow(Some("s-1".into()), true, cx)
        });
    });
    cx.run_until_parked();
    let app = cx.update(|_, cx| {
        preview
            .read(cx)
            .frame
            .as_ref()
            .map(|frame| frame.app.clone())
    });
    assert_eq!(app.as_deref(), Some("Finder"));
    // The agent clicks: the step shows, and "Parar agente" stops the turn.
    cx.update(|_, cx| {
        preview.update(cx, |preview, cx| {
            preview.apply(
                &ComputerEvent::Activity(Activity {
                    session_id: "s-1".into(),
                    activity_id: "a1".into(),
                    tool: "dcc_computer_click".into(),
                    phase: "started".into(),
                }),
                cx,
            )
        })
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("computer-preview").is_some());
    let stop = cx
        .debug_bounds("computer-preview-stop")
        .expect("Parar agente");
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    assert_eq!(stops.get(), 1);
    // Hidden, a pill brings it back.
    let hide = cx.debug_bounds("computer-preview-hide").expect("Ocultar");
    cx.simulate_click(hide.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let reopen = cx
        .debug_bounds("computer-preview-reopen")
        .expect("Mostrar tela do agente");
    cx.simulate_click(reopen.center(), gpui::Modifiers::none());
    assert!(!cx.update(|_, cx| preview.read(cx).hidden));
    // Another conversation's events are not this one's; the grant ending is.
    cx.update(|_, cx| {
        preview.update(cx, |preview, cx| {
            preview.apply(&ComputerEvent::PreviewCleared("s-2".into()), cx);
            assert!(preview.frame.is_some());
            preview.apply(&ComputerEvent::PreviewCleared("s-1".into()), cx);
            assert!(preview.frame.is_none() && preview.activity.is_none());
        })
    });
    assert_eq!(activity_label_key("dcc_computer_type"), "typing");
    assert_eq!(activity_label_key("dcc_computer_other"), "working");
    let requests = server.join().unwrap();
    let asked: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(asked["sessionId"], "s-1");
}
