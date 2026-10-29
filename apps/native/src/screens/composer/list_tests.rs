//! Markdown lists in the composer: Enter continues a list instead of
//! sending, an empty item leaves it, Tab nests, the hint shows while the
//! caret is on an item, and ⌘Enter sends the Markdown as typed.

use gpui::TestAppContext;

use super::ComposerSession;
use super::tests::{open, session, with_catalog};
use crate::test_support::fake_desktop;

#[gpui::test]
fn a_list_grows_with_enter_and_command_enter_sends_it(cx: &mut TestAppContext) {
    let (port, server) = fake_desktop("{}".into());
    let (composer, cx) = open(cx);
    composer.update_in(cx, |composer, window, cx| {
        with_catalog(composer, port);
        composer.set_session(
            ComposerSession {
                running: true,
                ..session("a")
            },
            cx,
        );
        composer.focus(window, cx);
    });
    let text = |cx: &mut gpui::VisualTestContext| {
        composer.read_with(cx, |composer, cx| {
            composer.input.read(cx).text().to_string()
        })
    };
    cx.simulate_input("Passos:");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("- ler");
    assert!(cx.debug_bounds("composer-list-hint").is_some());
    cx.simulate_keystrokes("enter");
    cx.simulate_input("testar");
    assert_eq!(text(cx), "Passos:\n- ler\n- testar");
    cx.simulate_keystrokes("tab");
    assert_eq!(text(cx), "Passos:\n- ler\n  - testar");
    cx.simulate_keystrokes("shift-tab enter enter");
    assert_eq!(
        text(cx),
        "Passos:\n- ler\n- testar\n",
        "the empty item left the list"
    );
    assert!(cx.debug_bounds("composer-list-hint").is_none());
    cx.simulate_input("Fim ");
    cx.simulate_keystrokes("cmd-b");
    cx.simulate_input("já");
    assert_eq!(text(cx), "Passos:\n- ler\n- testar\nFim **já**");
    cx.simulate_keystrokes("cmd-down shift-enter");
    cx.simulate_input("- ");
    cx.simulate_keystrokes("cmd-enter");
    cx.run_until_parked();
    let (request, body) = server.join().unwrap();
    assert_eq!(request, "POST /api/v1/sessions/a/queue");
    assert!(
        body.contains(r#""prompt":"Passos:\n- ler\n- testar\nFim **já**\n-"#),
        "{body}"
    );
}
