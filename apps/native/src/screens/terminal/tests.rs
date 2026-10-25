//! The dock driven headlessly against a fake desktop: the worktree's shell
//! opened with what it printed, fitted to the room, typed into (text and a
//! control key), fed from the stream, ended, and its tab closed.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::{Value, json};

use super::dock::{ScopeKind, TerminalDock, TerminalPlace};
use super::session::{SessionStatus, ShellContext};
use super::tabs::TabStore;
use crate::data::desktop_api::DesktopApi;
use crate::data::terminal_api::TerminalEvent;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

pub(super) fn place() -> TerminalPlace {
    TerminalPlace {
        workspace_id: "w-1".into(),
        project_id: Some("p-1".into()),
        root_path: Some("/repo".into()),
        worktree_path: Some("/repo/.dcc/w-1".into()),
        project_label: "Pagamentos".into(),
        branch: Some("feat/x".into()),
        base_branch: Some("main".into()),
        context: ShellContext {
            workspace_name: "feat".into(),
            branch: "feat/x".into(),
            provider_label: Some("Claude".into()),
            session_state: "active".into(),
        },
    }
}

pub(super) fn text_of(
    dock: &gpui::Entity<TerminalDock>,
    cx: &mut gpui::VisualTestContext,
) -> String {
    cx.update(|_, cx| {
        let session = dock.read(cx).active_session(cx).expect("a session");
        let session = session.read(cx);
        let palette =
            super::palette::TerminalPalette::new(false, &crate::theme::palette::Palette::light());
        session
            .emulator
            .screen(&palette)
            .rows
            .iter()
            .map(|row| row.text.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

#[gpui::test]
fn a_shell_opens_takes_keys_and_closes_with_its_tab(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let owner = "/api/v1/terminals/by-owner/terminal%3Aworktree%3Aw-1%3At1";
    let (port, server) = fake_desktop_routes(vec![
        (
            owner,
            200,
            json!({
                "ptyId": "pty-7",
                "existing": false,
                "session": { "status": "running" },
                "chunks": ["welcome\r\n$ "],
            })
            .to_string(),
        ),
        (
            "/api/v1/terminals/pty-7/resize",
            200,
            json!({ "ok": true }).to_string(),
        ),
        (
            "/api/v1/terminals/pty-7/write",
            200,
            json!({ "ok": true }).to_string(),
        ),
        (
            "/api/v1/terminals/pty-7/write",
            200,
            json!({ "ok": true }).to_string(),
        ),
        (
            "/api/v1/terminals/pty-7/write",
            200,
            json!({ "ok": true }).to_string(),
        ),
        (
            "/api/v1/terminals/pty-7/kill",
            200,
            json!({ "ok": true }).to_string(),
        ),
    ]);
    let tabs = TabStore::default().with_tab("worktree:w-1", "t1");
    let api = DesktopApi::new(port, "k");
    let (dock, cx) = cx.add_window_view(move |_, _| TerminalDock::new(340.0, tabs, Some(api)));
    cx.simulate_resize(size(px(1200.0), px(900.0)));
    cx.update(|window, cx| {
        dock.update(cx, |dock, cx| {
            dock.set_place(Some(place()), cx);
            dock.show(ScopeKind::Worktree, window, cx);
        })
    });
    cx.run_until_parked();
    // A second frame: the fit measured on the first one applies.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("terminal-dock").is_some());
    assert!(cx.debug_bounds("terminal-tab-0").is_some());
    let status = cx.update(|_, cx| {
        let session = dock.read(cx).active_session(cx).unwrap();
        session.read(cx).status.clone()
    });
    assert_eq!(status, SessionStatus::Running);
    assert!(
        text_of(&dock, cx).starts_with("welcome\n$"),
        "{}",
        text_of(&dock, cx)
    );

    // Text through the platform's input, then a control key.
    cx.simulate_input("ls");
    cx.simulate_keystrokes("ctrl-c");
    cx.run_until_parked();

    // What the shell prints, and its end.
    cx.update(|_, cx| {
        dock.update(cx, |dock, cx| {
            dock.dispatch(
                TerminalEvent::Output {
                    pty_id: "pty-7".into(),
                    data: "file.txt\r\n".into(),
                },
                cx,
            );
        })
    });
    assert!(text_of(&dock, cx).contains("file.txt"));

    // Closing the tab ends its shell; the dock opens a new one.
    let close = cx
        .debug_bounds("terminal-tab-close-0")
        .expect("close button");
    cx.simulate_click(close.center(), Modifiers::none());
    cx.run_until_parked();
    let requests = server.join().unwrap();
    let paths: Vec<&str> = requests
        .iter()
        .map(|(request, _)| request.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            format!("POST {owner}").as_str(),
            "POST /api/v1/terminals/pty-7/resize",
            "POST /api/v1/terminals/pty-7/write",
            "POST /api/v1/terminals/pty-7/write",
            "POST /api/v1/terminals/pty-7/write",
            "POST /api/v1/terminals/pty-7/kill",
        ]
    );
    let opened: Value = serde_json::from_str(&requests[0].1).unwrap();
    assert_eq!(opened["cwd"], "/repo/.dcc/w-1");
    let resized: Value = serde_json::from_str(&requests[1].1).unwrap();
    assert_ne!(resized["cols"], 120, "fitted to the dock");
    // One write a key, in order.
    let typed: Vec<Value> = requests[2..5]
        .iter()
        .map(|(_, body)| serde_json::from_str::<Value>(body).unwrap()["data"].clone())
        .collect();
    assert_eq!(typed, [json!("l"), json!("s"), json!("\u{3}")]);
    let tabs = cx.update(|_, cx| dock.read(cx).tabs.scope("worktree:w-1").tabs.len());
    assert_eq!(tabs, 1, "an open dock keeps a tab");
}

#[gpui::test]
fn an_ended_shell_says_so(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Dark, WindowAppearance::Dark));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let owner = "/api/v1/terminals/by-owner/terminal%3Aworktree%3Aw-1%3At1";
    let (port, server) = fake_desktop_routes(vec![(
        owner,
        200,
        json!({ "ptyId": "pty-9", "existing": true, "chunks": ["$ "] }).to_string(),
    )]);
    let tabs = TabStore::default().with_tab("worktree:w-1", "t1");
    let api = DesktopApi::new(port, "k");
    let (dock, cx) = cx.add_window_view(move |_, _| TerminalDock::new(340.0, tabs, Some(api)));
    cx.update(|window, cx| {
        dock.update(cx, |dock, cx| {
            dock.set_place(Some(place()), cx);
            dock.show(ScopeKind::Worktree, window, cx);
        })
    });
    cx.run_until_parked();
    server.join().unwrap();
    cx.update(|_, cx| {
        dock.update(cx, |dock, cx| {
            dock.dispatch(
                TerminalEvent::Exit {
                    pty_id: "pty-9".into(),
                    code: Some(130),
                },
                cx,
            )
        })
    });
    assert!(text_of(&dock, cx).contains("[Terminal exited with code 130]"));
    let status = cx.update(|_, cx| {
        let session = dock.read(cx).active_session(cx).unwrap();
        session.read(cx).status.clone()
    });
    assert_eq!(status, SessionStatus::Exited(Some(130)));
    // Hiding keeps the shell; the dock draws nothing.
    cx.update(|_, cx| dock.update(cx, |dock, cx| dock.hide(cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("terminal-dock").is_none());
}
