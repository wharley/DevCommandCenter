//! The agents' Browser opens with a fake desktop: the card for a request,
//! "Abrir e permitir" answering it and asking the shell to open it, an
//! approval from a phone opening once, a preapproved request on screen
//! opening without a card, and the lifecycle confirmed.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, TestAppContext, VisualTestContext, WindowAppearance, px, size};

use super::approval::{ApprovalEvent, BrowserApprovals};
use crate::data::browser_opens::{OpenRequest, OpensEvent};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

fn request(id: &str, session: &str, preapproved: bool) -> OpenRequest {
    OpenRequest {
        request_id: id.into(),
        workspace_id: "w-1".into(),
        session_id: session.into(),
        provider_id: "claude".into(),
        url: "https://example.com/plans".into(),
        reason: "Conferir a página de planos".into(),
        expires_at_ms: u64::MAX / 2,
        preapproved,
    }
}

fn host(
    cx: &mut TestAppContext,
    port: u16,
) -> (
    Entity<BrowserApprovals>,
    &mut VisualTestContext,
    Rc<RefCell<Vec<String>>>,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (approvals, cx) = cx.add_window_view(|_, cx| {
        let mut approvals =
            BrowserApprovals::new(None, Rc::new(|_, _| Some("Ajustar planos".into())), cx);
        approvals.set_api(DesktopApi::new(port, "k"));
        approvals
    });
    cx.simulate_resize(size(px(900.0), px(700.0)));
    let opened: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&approvals, move |_, event: &ApprovalEvent, _| {
            let ApprovalEvent::Open(request) = event;
            sink.borrow_mut().push(request.request_id.clone());
        })
        .detach();
    });
    (approvals, cx, opened)
}

#[gpui::test]
fn the_card_answers_and_the_shell_opens_what_was_allowed(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_routes(vec![
        ("/api/v1/browser/bridge/opens/r-1/allow", 200, "{}".into()),
        ("/api/v1/browser/bridge/opens/r-3/allow", 200, "{}".into()),
        (
            "/api/v1/browser/bridge/opens/r-1/opened",
            200,
            r#"{"status":"approved"}"#.into(),
        ),
    ]);
    let (approvals, cx, opened) = host(cx, port);
    let apply = |cx: &mut VisualTestContext, event: OpensEvent| {
        cx.update(|_, cx| approvals.update(cx, |approvals, cx| approvals.apply(event, cx)));
        cx.run_until_parked();
    };
    apply(cx, OpensEvent::Waiting(vec![request("r-1", "s-1", false)]));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("browser-approval").is_some(), "the card");
    let allow = cx
        .debug_bounds("browser-approval-allow")
        .expect("Abrir e permitir");
    cx.simulate_click(allow.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*opened.borrow(), ["r-1"]);
    // The desktop no longer lists it: the card is gone.
    apply(cx, OpensEvent::Waiting(Vec::new()));
    assert!(cx.update(|_, cx| approvals.read(cx).busy.is_none()));
    // A phone allowed one: it opens once, however often it is heard.
    apply(cx, OpensEvent::Approved(request("r-2", "s-1", false)));
    apply(cx, OpensEvent::Approved(request("r-2", "s-1", false)));
    assert_eq!(*opened.borrow(), ["r-1", "r-2"]);
    // Preapproved on the conversation on screen: no card, it opens.
    cx.update(|_, cx| {
        approvals.update(cx, |approvals, cx| {
            approvals.set_visible_session(Some("s-9".into()), cx)
        })
    });
    apply(cx, OpensEvent::Waiting(vec![request("r-3", "s-9", true)]));
    assert_eq!(*opened.borrow(), ["r-1", "r-2", "r-3"]);
    // The Browser opened r-1 on lifecycle 41.
    cx.update(|_, cx| {
        approvals.update(cx, |approvals, cx| {
            approvals.confirm(request("r-1", "s-1", false), 41, cx)
        })
    });
    cx.run_until_parked();
    let requests = server.join().unwrap();
    let allow: serde_json::Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(allow["workspaceId"], "w-1");
    assert_eq!(allow["sessionId"], "s-1");
    let ack: serde_json::Value = serde_json::from_str(&requests[2].1).unwrap();
    assert_eq!(ack["lifecycleToken"], 41);
}
