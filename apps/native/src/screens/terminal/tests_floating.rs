//! The floating terminal driven headlessly against a fake desktop: its
//! shell opens in its own folder under its own key, says nothing of a task,
//! offers nothing to an agent, and ⌘⌥T opens, focuses and hides it.

use gpui::{TestAppContext, WindowAppearance};
use serde_json::{Value, json};

use super::dock::{ScopeKind, TerminalDock};
use super::floating::{FLOATING_SCOPE, floating_place};
use super::tabs::TabStore;
use super::tests::text_of;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

#[test]
fn its_one_scope_is_its_own_folder_under_its_own_key() {
    let place = floating_place("/Users/me".into());
    let scope = place.scope(ScopeKind::Worktree);
    assert_eq!(scope.kind, ScopeKind::Project);
    assert_eq!(scope.key, FLOATING_SCOPE);
    assert_eq!(scope.cwd.as_deref(), Some("/Users/me"));
    // No task's count reads it: it is no workspace's id.
    assert_eq!(place.scopes()[0].cwd, None);
}

#[gpui::test]
fn it_opens_in_its_folder_and_the_shortcut_toggles_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let owner = "/api/v1/terminals/by-owner/terminal%3Afloating%3At1";
    let (port, server) = fake_desktop_routes(vec![(
        owner,
        200,
        json!({ "ptyId": "pty-3", "existing": false, "chunks": [] }).to_string(),
    )]);
    let tabs = TabStore::default().with_tab(FLOATING_SCOPE, "t1");
    let api = DesktopApi::new(port, "k");
    let (dock, cx) = cx.add_window_view(move |_, _| {
        TerminalDock::floating_in("/Users/me".into(), tabs, Some(api))
    });
    cx.update(|window, cx| dock.update(cx, |dock, cx| dock.toggle_floating(window, cx)));
    cx.run_until_parked();
    let requests = server.join().unwrap();
    let opened: Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(opened["cwd"], "/Users/me");
    assert!(cx.debug_bounds("floating-terminal").is_some());
    assert!(cx.debug_bounds("terminal-dock").is_none());

    // Its banner tells the folder, not a workspace; nothing goes to an agent.
    let text = text_of(&dock, cx);
    assert!(text.contains("cwd: /Users/me"), "{text}");
    assert!(!text.contains("workspace:"), "{text}");
    let agent = cx.update(|_, cx| {
        let session = dock.read(cx).active_session(cx).unwrap();
        session.read(cx).agent
    });
    assert!(!agent);

    // Focused, the shortcut hides it; the shell stays.
    let focused = cx.update(|window, cx| {
        let session = dock.read(cx).active_session(cx).unwrap();
        session.read(cx).focus_handle().is_focused(window)
    });
    assert!(focused);
    cx.update(|window, cx| dock.update(cx, |dock, cx| dock.toggle_floating(window, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("floating-terminal").is_none());
    assert!(cx.update(|_, cx| dock.read(cx).active_session(cx).is_some()));

    // Again, it comes back focused.
    cx.update(|window, cx| dock.update(cx, |dock, cx| dock.toggle_floating(window, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("floating-terminal").is_some());
}
