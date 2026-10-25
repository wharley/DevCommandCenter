//! The dock's bar driven headlessly: renaming a tab, closing one whose
//! program runs, the selection sent to the agent, the other scope, the
//! expanded dock, a local server found, restarting and interrupting.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance, px, size};
use serde_json::{Value, json};

use super::dock::{DockEvent, ScopeKind, TerminalDock};
use super::tabs::TabStore;
use super::tests::{place, text_of};
use crate::data::desktop_api::DesktopApi;
use crate::data::terminal_api::{ActivityStatus, TerminalActivity, TerminalEvent};
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

const OWNER: &str = "/api/v1/terminals/by-owner/terminal%3Aworktree%3Aw-1%3At1";

fn opened(pty: &str, chunks: &[&str]) -> String {
    json!({ "ptyId": pty, "existing": true, "session": { "status": "running" }, "chunks": chunks })
        .to_string()
}

fn ok() -> String {
    json!({ "ok": true }).to_string()
}

type Events = Rc<RefCell<Vec<String>>>;

#[allow(clippy::type_complexity)]
fn open<'a>(
    cx: &'a mut TestAppContext,
    routes: Vec<(&'static str, u16, String)>,
) -> (
    Entity<TerminalDock>,
    &'a mut VisualTestContext,
    Events,
    std::thread::JoinHandle<Vec<(String, String)>>,
) {
    open_with(
        cx,
        routes,
        TabStore::default().with_tab("worktree:w-1", "t1"),
    )
}

#[allow(clippy::type_complexity)]
fn open_with<'a>(
    cx: &'a mut TestAppContext,
    routes: Vec<(&'static str, u16, String)>,
    tabs: TabStore,
) -> (
    Entity<TerminalDock>,
    &'a mut VisualTestContext,
    Events,
    std::thread::JoinHandle<Vec<(String, String)>>,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (port, server) = fake_desktop_routes(routes);
    let api = DesktopApi::new(port, "k");
    let (dock, cx) = cx.add_window_view(move |_, _| TerminalDock::new(340.0, tabs, Some(api)));
    cx.simulate_resize(size(px(1200.0), px(900.0)));
    let events: Events = Rc::default();
    let sink = events.clone();
    cx.update(|window, cx| {
        cx.subscribe(&dock, move |_, event: &DockEvent, _| {
            sink.borrow_mut().push(match event {
                DockEvent::Hidden => "hidden".into(),
                DockEvent::Resized(height) => format!("resized {height}"),
                DockEvent::Expanded(expanded) => format!("expanded {expanded}"),
                DockEvent::SendToAgent(text) => format!("agent {text}"),
                DockEvent::OpenServer(url) => format!("server {url}"),
            })
        })
        .detach();
        dock.update(cx, |dock, cx| {
            dock.set_place(Some(place()), cx);
            dock.show(ScopeKind::Worktree, window, cx);
        })
    });
    cx.run_until_parked();
    (dock, cx, events, server)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} on screen"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn set_running(dock: &Entity<TerminalDock>, pty: &str, cx: &mut VisualTestContext) {
    let pty = pty.to_string();
    cx.update(|_, cx| {
        let session = dock.read(cx).active_session(cx).unwrap();
        session.update(cx, |session, cx| {
            session.set_activity(
                Some(TerminalActivity {
                    pty_id: pty,
                    status: ActivityStatus::Running,
                    process_label: Some("npm run dev".into()),
                    pty_owner_key: None,
                }),
                cx,
            )
        });
        dock.update(cx, |_, cx| cx.notify());
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_tab_is_renamed_without_touching_its_shell(cx: &mut TestAppContext) {
    let (dock, cx, _, server) = open(cx, vec![(OWNER, 200, opened("pty-1", &["$ "]))]);
    server.join().unwrap();
    click(cx, "terminal-tab-rename-0");
    assert!(cx.debug_bounds("terminal-rename-input").is_some());
    let focused = cx.update(|window, cx| {
        let input = dock.read(cx).renaming.as_ref().unwrap().input.clone();
        gpui::Focusable::focus_handle(input.read(cx), cx).is_focused(window)
    });
    assert!(focused, "the field has the focus");
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("api");
    click(cx, "terminal-rename-save");
    let title = cx.update(|_, cx| {
        dock.read(cx).tabs.scope("worktree:w-1").tabs[0]
            .title
            .clone()
    });
    assert_eq!(title, "api");
    assert!(cx.debug_bounds("terminal-rename-input").is_none());
}

#[gpui::test]
fn a_running_program_is_asked_about_before_its_tab_closes(cx: &mut TestAppContext) {
    let (dock, cx, events, server) = open(
        cx,
        vec![
            (OWNER, 200, opened("pty-2", &["$ "])),
            ("/api/v1/terminals/pty-2/kill", 200, ok()),
        ],
    );
    set_running(&dock, "pty-2", cx);
    click(cx, "terminal-tab-close-0");
    // "Apenas ocultar" keeps the program and hides the dock.
    click(cx, "terminal-close-hide");
    assert!(cx.debug_bounds("terminal-dock").is_none());
    assert_eq!(events.borrow().as_slice(), ["hidden"]);
    cx.update(|window, cx| dock.update(cx, |dock, cx| dock.show(ScopeKind::Worktree, window, cx)));
    set_running(&dock, "pty-2", cx);
    click(cx, "terminal-tab-close-0");
    click(cx, "terminal-close-confirm");
    let requests = server.join().unwrap();
    assert_eq!(requests[1].0, "POST /api/v1/terminals/pty-2/kill");
}

#[gpui::test]
fn the_selection_goes_to_the_agent_as_evidence(cx: &mut TestAppContext) {
    let (dock, cx, events, server) = open(
        cx,
        vec![(OWNER, 200, opened("pty-3", &["error: boom\r\n$ "]))],
    );
    server.join().unwrap();
    cx.update(|_, cx| {
        let session = dock.read(cx).active_session(cx).unwrap();
        session.update(cx, |session, cx| {
            session
                .emulator
                .start_selection(super::emulator::SelectBy::Words, 0, 8, false);
            cx.notify();
        });
    });
    click(cx, "terminal-send-selection");
    let events = events.borrow();
    let sent = events
        .iter()
        .find(|event| event.starts_with("agent "))
        .expect("sent");
    assert!(
        sent.contains("<terminal_output>\nproject: Pagamentos"),
        "{sent}"
    );
    assert!(sent.contains("scope: Tarefa isolada · feat/x"), "{sent}");
    assert!(
        sent.contains("source: o trecho selecionado\n---\nboom\n"),
        "{sent}"
    );
    drop(events);
    // Nothing selected now: the recent output goes.
    cx.update(|_, cx| dock.update(cx, |dock, cx| dock.send_to_agent(cx)));
    cx.run_until_parked();
    let events = events_of(&dock, cx);
    assert!(
        events.contains("source: a saída recente\n---\nerror: boom\n$"),
        "{events}"
    );
}

/// What the dock sent last, read again through a fresh subscription.
fn events_of(dock: &Entity<TerminalDock>, cx: &mut VisualTestContext) -> String {
    let sent: Events = Rc::default();
    let sink = sent.clone();
    cx.update(|_, cx| {
        cx.subscribe(dock, move |_, event: &DockEvent, _| {
            if let DockEvent::SendToAgent(text) = event {
                sink.borrow_mut().push(text.clone());
            }
        })
        .detach();
        dock.update(cx, |dock, cx| dock.send_to_agent(cx));
    });
    cx.run_until_parked();
    sent.borrow().join("\n")
}

#[gpui::test]
fn the_other_scope_has_tabs_of_its_own(cx: &mut TestAppContext) {
    let (dock, cx, _, server) = open_with(
        cx,
        vec![
            (OWNER, 200, opened("pty-4", &["$ "])),
            (
                "/api/v1/terminals/by-owner/terminal%3Ap-1%3At2",
                200,
                opened("pty-5", &[]),
            ),
        ],
        TabStore::default()
            .with_tab("worktree:w-1", "t1")
            .with_tab("p-1", "t2"),
    );
    cx.update(|window, cx| {
        dock.update(cx, |dock, cx| dock.set_kind(ScopeKind::Project, window, cx))
    });
    cx.run_until_parked();
    let requests = server.join().unwrap();
    let project: Value = serde_json::from_str(&requests[1].1).unwrap();
    assert_eq!(project["cwd"], "/repo", "the project's checkout");
    let (kind, tabs) = cx.update(|_, cx| {
        let dock = dock.read(cx);
        (
            dock.scope().unwrap().kind,
            dock.tabs.scope("p-1").tabs.len(),
        )
    });
    assert_eq!(kind, ScopeKind::Project);
    assert_eq!(tabs, 1);
}

#[gpui::test]
fn expanding_takes_the_room_and_esc_steps_back(cx: &mut TestAppContext) {
    let (dock, cx, events, server) = open(cx, vec![(OWNER, 200, opened("pty-6", &["$ "]))]);
    server.join().unwrap();
    click(cx, "terminal-expand");
    assert!(cx.update(|_, cx| dock.read(cx).expanded));
    cx.update(|_, cx| dock.update(cx, |dock, cx| dock.step_back(cx)));
    assert!(cx.update(|_, cx| !dock.read(cx).expanded && dock.read(cx).open));
    cx.update(|_, cx| dock.update(cx, |dock, cx| dock.step_back(cx)));
    assert!(cx.update(|_, cx| !dock.read(cx).open));
    assert_eq!(
        events.borrow().as_slice(),
        ["expanded true", "expanded false", "hidden"]
    );
}

#[gpui::test]
fn a_local_server_shows_on_the_bar(cx: &mut TestAppContext) {
    let (dock, cx, _, server) = open(
        cx,
        vec![(
            OWNER,
            200,
            opened("pty-7", &["  ➜  Local:   http://localhost:5173/\r\n"]),
        )],
    );
    server.join().unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("terminal-dev-server").is_some());
    let found = cx.update(|_, cx| dock.read(cx).detected_server(cx).unwrap().url);
    assert_eq!(found, "http://localhost:5173/");
    cx.update(|_, cx| {
        dock.update(cx, |dock, cx| {
            dock.dispatch(
                TerminalEvent::Output {
                    pty_id: "pty-7".into(),
                    data: "api on http://127.0.0.1:8000/docs\r\n".into(),
                },
                cx,
            )
        })
    });
    let found = cx.update(|_, cx| dock.read(cx).detected_server(cx).unwrap().url);
    assert_eq!(found, "http://127.0.0.1:8000/docs");
    // Clearing the output forgets them.
    cx.update(|_, cx| dock.update(cx, |dock, cx| dock.clear_active(cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("terminal-dev-server").is_none());
}

#[gpui::test]
fn restarting_ends_the_shell_before_opening_another(cx: &mut TestAppContext) {
    let (dock, cx, _, server) = open(
        cx,
        vec![
            (OWNER, 200, opened("pty-8", &["old\r\n"])),
            ("/api/v1/terminals/pty-8/write", 200, ok()),
            ("/api/v1/terminals/pty-8/kill", 200, ok()),
            (OWNER, 200, opened("pty-9", &["fresh\r\n"])),
        ],
    );
    cx.update(|_, cx| dock.update(cx, |dock, cx| dock.interrupt_active(cx)));
    cx.run_until_parked();
    cx.update(|window, cx| dock.update(cx, |dock, cx| dock.restart_active(window, cx)));
    cx.run_until_parked();
    let requests = server.join().unwrap();
    cx.run_until_parked();
    let paths: Vec<&str> = requests
        .iter()
        .map(|(request, _)| request.as_str())
        .collect();
    assert_eq!(
        paths[1..],
        [
            "POST /api/v1/terminals/pty-8/write",
            "POST /api/v1/terminals/pty-8/kill",
            &format!("POST {OWNER}"),
        ]
    );
    let interrupt: Value = serde_json::from_str(&requests[1].1).unwrap();
    assert_eq!(interrupt["data"], "\u{3}");
    // The old shell's end does not end the new one.
    cx.update(|_, cx| {
        dock.update(cx, |dock, cx| {
            dock.dispatch(
                TerminalEvent::Exit {
                    pty_id: "pty-8".into(),
                    code: Some(-1),
                },
                cx,
            )
        })
    });
    let text = text_of(&dock, cx);
    assert!(text.contains("fresh") && !text.contains("old"), "{text}");
}
