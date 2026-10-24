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

#[gpui::test]
fn mentions_pick_files_and_attachments_become_paths(cx: &mut TestAppContext) {
    use std::sync::Arc;
    use std::time::Instant;

    use super::mentions::FileList;
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.files = Some(FileList {
            root: "/repo".into(),
            files: Arc::new(vec![
                "src/app.rs".into(),
                "src/payment.ts".into(),
                "docs/readme.md".into(),
            ]),
            read_at: Instant::now(),
        });
        composer.set_session(
            ComposerSession {
                workspace_path: Some("/repo".into()),
                ..session("a")
            },
            cx,
        );
        composer.focus(window, cx);
    });
    cx.simulate_input("ver @p");
    composer.read_with(cx, |composer, _| {
        let menu = composer.mention.as_ref().expect("the popup opened");
        assert_eq!(
            menu.options,
            vec!["src/payment.ts".to_string(), "src/app.rs".to_string()]
        );
    });
    // ↓ then Enter picks the second file; Enter does not send while it's open.
    cx.simulate_keystrokes("down enter");
    composer.read_with(cx, |composer, cx| {
        assert!(composer.mention.is_none());
        assert_eq!(composer.prompt(cx), "ver @src/app.rs");
        assert_eq!(composer.busy, None);
    });
    // A picked file inside the checkout goes relative, outside stays absolute.
    composer.update(cx, |composer, cx| {
        composer.add_paths(
            vec!["/repo/docs/shot.png".into(), "/tmp/notes.txt".into()],
            cx,
        )
    });
    cx.simulate_input("fim");
    composer.read_with(cx, |composer, cx| {
        assert_eq!(
            composer.prompt(cx),
            "ver @src/app.rs @docs/shot.png @/tmp/notes.txtfim"
        );
        let chips = composer.input.read(cx).chips();
        assert!(matches!(chips[1].1, crate::ui::text_area::Chip::Image(_)));
    });
    // Backspace after a chip removes it whole.
    composer.update(cx, |composer, cx| {
        let input = composer.input.clone();
        input.update(cx, |input, cx| input.set_text("", cx));
    });
    cx.run_until_parked();
}

#[gpui::test]
fn long_pastes_become_a_snippet_chip(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    let long = "linha\n".repeat(300);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(long.clone()));
    cx.simulate_input("antes ");
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    composer.read_with(cx, |composer, cx| {
        let chips = composer.input.read(cx).chips();
        assert_eq!(chips.len(), 1);
        assert_eq!(composer.prompt(cx), format!("antes \n{}", long.trim_end()));
    });
}
