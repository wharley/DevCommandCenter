//! The composer driven headlessly: typing, Enter sending, the draft kept per
//! conversation, and a send that finds no desktop.

use gpui::{TestAppContext, WindowAppearance};

use super::{Composer, ComposerSession, continuation_prompt};
use crate::data::desktop_api::DesktopApi;
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<Composer>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_area::bind_keys(cx);
    });
    cx.add_window_view(Composer::new)
}

fn session(id: &str) -> ComposerSession {
    ComposerSession {
        session_id: Some(id.into()),
        provider_id: Some("claude_code".into()),
        model: Some("claude-sonnet-5-5".into()),
        ..ComposerSession::default()
    }
}

#[gpui::test]
fn types_keeps_drafts_and_sends_on_enter(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    composer.update_in(cx, |composer, window, cx| {
        composer.endpoint = Some(DesktopApi::new(free, "k"));
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_input("olá");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("mundo");
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "olá\nmundo");
        assert!(composer.can_send(cx));
    });
    // Another conversation starts empty; coming back restores the draft.
    composer.update(cx, |composer, cx| composer.set_session(session("b"), cx));
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "")
    });
    composer.update(cx, |composer, cx| composer.set_session(session("a"), cx));
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.input.read(cx).text(), "olá\nmundo")
    });
    // Enter sends; with no desktop the prompt stays for another try.
    composer.update_in(cx, |composer, window, cx| composer.focus(window, cx));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    composer.read_with(cx, |composer, cx| {
        assert_eq!(composer.busy, None);
        assert_eq!(composer.input.read(cx).text(), "olá\nmundo");
    });
}

#[gpui::test]
fn running_turns_show_stop_and_keep_enter_from_sending(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.set_session(
            ComposerSession {
                running: true,
                ..session("a")
            },
            cx,
        );
        composer.focus(window, cx);
    });
    cx.simulate_input("depois");
    cx.simulate_keystrokes("enter");
    composer.read_with(cx, |composer, cx| {
        assert!(!composer.can_send(cx));
        assert_eq!(composer.busy, None);
    });
}

#[test]
fn continuation_quotes_the_original_request() {
    assert_eq!(
        continuation_prompt(Some(" faça X "), "Continue.", "Pedido original"),
        "Continue.\n\nPedido original:\nfaça X"
    );
    assert_eq!(continuation_prompt(None, " Continue. ", "x"), "Continue.");
}
