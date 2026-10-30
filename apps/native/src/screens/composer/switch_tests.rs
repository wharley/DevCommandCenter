//! Sending and opening another task before the desktop answers: the sent
//! prompt leaves its conversation at once, the other task keeps its own
//! draft, and a failed send puts the prompt back only where it came from.

use gpui::TestAppContext;

use super::tests::{open, session, with_catalog};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop;

fn text(composer: &gpui::Entity<super::Composer>, cx: &mut gpui::VisualTestContext) -> String {
    composer.read_with(cx, |composer, cx| {
        composer.input.read(cx).text().to_string()
    })
}

#[gpui::test]
fn a_sent_prompt_does_not_come_back_after_switching_tasks(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(session("b"), cx);
        composer.focus(window, cx);
    });
    cx.simulate_input("rascunho de b");
    composer.update_in(cx, |composer, window, cx| {
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    cx.simulate_input("rode os testes");
    // Enter, then another task at once: the desktop has not answered yet.
    composer.update_in(cx, |composer, window, cx| {
        composer.send(window, cx);
        composer.set_session(session("b"), cx);
    });
    assert_eq!(text(&composer, cx), "rascunho de b");
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert!(request.contains("/sessions/a/"), "{request}");
    assert!(body.contains("rode os testes"), "{body}");
    assert_eq!(text(&composer, cx), "rascunho de b", "b keeps its draft");
    composer.read_with(cx, |composer, _| {
        assert!(!composer.session.running, "a's turn does not run on b");
    });
    composer.update(cx, |composer, cx| composer.set_session(session("a"), cx));
    assert_eq!(text(&composer, cx), "", "the sent prompt is gone from a");
}

#[gpui::test]
fn a_failed_send_goes_back_to_its_own_task(cx: &mut TestAppContext) {
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.endpoint = Some(DesktopApi::new(free, "k"));
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    cx.simulate_input("olá");
    composer.update_in(cx, |composer, window, cx| {
        composer.send(window, cx);
        assert_eq!(composer.input.read(cx).text(), "", "empties at once");
        composer.set_session(session("b"), cx);
    });
    cx.run_until_parked();
    assert_eq!(text(&composer, cx), "", "never into the other task");
    composer.update(cx, |composer, cx| composer.set_session(session("a"), cx));
    assert_eq!(text(&composer, cx), "olá", "back in its own draft");
}
