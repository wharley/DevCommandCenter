use gpui::{TestAppContext, WindowAppearance};

use super::super::{Composer, ComposerEvent, ComposerSession};
use crate::data::conversation::ThreadStart;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_answers;
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<Composer>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_area::bind_keys(cx);
    });
    cx.add_window_view(Composer::new)
}

fn session(running: bool) -> ComposerSession {
    ComposerSession {
        task_id: Some("ws-1".into()),
        session_id: Some("a".into()),
        provider_id: Some("claude_code".into()),
        model: Some("claude-sonnet-5-5".into()),
        running,
        new_thread: Some(ThreadStart {
            workspace_id: "ws-1".into(),
            project_id: "checkout".into(),
            additional_workspace_ids: Vec::new(),
            name: Some("Pagamentos".into()),
            auto_named: false,
        }),
        ..ComposerSession::default()
    }
}

fn endpoint(composer: &mut Composer, port: u16) {
    composer.endpoint = Some(DesktopApi::new(port, "k"));
    composer.catalog = Some(std::sync::Arc::new(super::super::tests::catalog()));
    composer.catalog_at = Some(std::time::Instant::now());
}

#[gpui::test]
fn a_snippet_goes_into_this_conversation_never_in_plan_mode(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_answers(vec![(200, "{}".into())]);
    let (composer, cx) = open(cx);
    composer.update(cx, |composer, cx| {
        endpoint(composer, port);
        composer.set_session(session(false), cx);
        assert!(composer.send_annotation("Renomeie\n\n```\nx\n```".into(), false, cx));
    });
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls[0].0, "POST /api/v1/sessions/a/turns");
    let body: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(body["prompt"], "Renomeie\n\n```\nx\n```");
    assert_eq!(body["planMode"], false);
    // The field is left as it was.
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "")
    });
}

#[gpui::test]
fn while_the_agent_works_the_snippet_waits_in_the_queue(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_answers(vec![(200, "{}".into())]);
    let (composer, cx) = open(cx);
    composer.update(cx, |composer, cx| {
        endpoint(composer, port);
        composer.set_session(session(true), cx);
        assert!(composer.send_annotation("Veja isto".into(), false, cx));
    });
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls[0].0, "POST /api/v1/sessions/a/queue");
}

#[gpui::test]
fn a_new_session_starts_on_the_pick_and_comes_forward(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop_answers(vec![
        (200, r#"{"session":{"id":"s-new"}}"#.into()),
        (200, "{}".into()),
    ]);
    let (composer, cx) = open(cx);
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = opened.clone();
    cx.update(|_, cx| {
        cx.subscribe(&composer, move |_, event: &ComposerEvent, _| {
            if let ComposerEvent::OpenSession(id) = event {
                seen.borrow_mut().push(id.clone());
            }
        })
        .detach();
    });
    composer.update(cx, |composer, cx| {
        endpoint(composer, port);
        composer.set_session(session(false), cx);
        assert!(composer.send_annotation("Corrija".into(), true, cx));
    });
    cx.run_until_parked();
    let calls = server.join().unwrap();
    assert_eq!(calls[0].0, "POST /api/v1/sessions/start");
    let start: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(start["workspaceId"], "ws-1");
    assert_eq!(start["providerId"], "claude_code");
    assert_eq!(start["model"], "claude-sonnet-5-5");
    assert_eq!(start["title"], "Pagamentos");
    assert_eq!(calls[1].0, "POST /api/v1/sessions/s-new/turns");
    assert_eq!(*opened.borrow(), vec!["s-new".to_string()]);
}

#[gpui::test]
fn nothing_goes_without_a_conversation_or_while_busy(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update(cx, |composer, cx| {
        assert!(!composer.send_annotation("x".into(), false, cx));
        composer.set_session(session(false), cx);
        composer.busy = Some(super::super::Busy::Sending);
        assert!(!composer.send_annotation("x".into(), false, cx));
    });
}
