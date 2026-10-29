//! `/` commands in the composer: the menu lists what the provider runs,
//! `/clear` empties the draft, `/goal` goes in for the person to finish and
//! is drawn as recognized, `/compact` goes as a turn of its own.

use std::sync::Arc;

use gpui::TestAppContext;

use super::mentions::MenuItem;
use super::slash::Command;
use super::tests::{catalog, open, session, with_catalog};
use crate::test_support::fake_desktop;

fn with_commands(composer: &mut super::Composer, port: u16) {
    with_catalog(composer, port);
    let mut providers = catalog();
    for provider in &mut providers {
        provider.capabilities.supports_compaction_command = true;
        provider.capabilities.supports_native_goal = true;
    }
    composer.catalog = Some(Arc::new(providers));
}

#[gpui::test]
fn slash_commands_clear_insert_and_compact(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_commands(composer, port);
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    let text = |cx: &mut gpui::VisualTestContext| {
        composer.read_with(cx, |composer, cx| {
            composer.input.read(cx).text().to_string()
        })
    };
    cx.simulate_input("/");
    composer.read_with(cx, |composer, _| {
        let menu = composer.mention.as_ref().expect("the menu opened");
        assert_eq!(
            menu.options,
            vec![
                MenuItem::Command(Command::Clear),
                MenuItem::Command(Command::Compact),
                MenuItem::Command(Command::Goal)
            ]
        );
    });
    // `/go` + Enter puts `/goal ` in, recognized, for the person to finish.
    cx.simulate_input("go");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "/goal ");
    cx.simulate_input("testes passando");
    composer.read_with(cx, |composer, cx| {
        assert!(composer.mention.is_none());
        assert_eq!(composer.input.read(cx).command_range(), Some(0..5));
    });
    // `/clear` empties the whole draft.
    cx.simulate_input(" /cl");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "");
    // `/compact` empties it too and sends the command as its own turn.
    cx.simulate_input("rascunho /comp");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(text(cx), "");
    let (request, body) = server.join().unwrap();
    assert!(request.starts_with("POST /api/v1/sessions/a"), "{request}");
    assert!(body.contains(r#""prompt":"/compact""#), "{body}");
}

#[gpui::test]
fn a_provider_without_them_offers_only_clear(cx: &mut TestAppContext) {
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, 1);
        composer.set_session(session("a"), cx);
        composer.focus(window, cx);
    });
    cx.simulate_input("/");
    composer.read_with(cx, |composer, cx| {
        let menu = composer.mention.as_ref().expect("the menu opened");
        assert_eq!(menu.options, vec![MenuItem::Command(Command::Clear)]);
        // Not offered, so not drawn as recognized either.
        assert_eq!(composer.input.read(cx).command_range(), None);
    });
}
