//! The conversation's header driven headlessly against a fake desktop:
//! its title and buttons, the terminal count, resume, the history with a
//! delegated child under its parent, closing a conversation whose turn
//! runs (asked first, the turn stopped, then closed), restoring one.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowAppearance};
use serde_json::Value;

use super::header::TerminalCounts;
use super::{ConversationEvent, ConversationView};
use crate::data::conversation::Conversation;
use crate::data::desktop_api::DesktopApi;
use crate::data::session_list::SessionRow;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

fn row(id: &str, state: &str, running: bool, archived: bool, depth: u8) -> SessionRow {
    SessionRow {
        id: id.into(),
        title: format!("Conversa {id}"),
        provider_id: "codex".into(),
        model: None,
        state: state.into(),
        turn_count: 2,
        running,
        archived,
        depth,
    }
}

fn data() -> Conversation {
    Conversation {
        task_id: "task".into(),
        session_id: Some("a".into()),
        provider_id: Some("codex".into()),
        workspace_name: Some("Pagamentos".into()),
        sessions: Arc::new(vec![
            row("a", "completed", false, false, 0),
            row("child", "active", true, false, 1),
            row("b", "active", false, false, 0),
            row("old", "completed", false, true, 0),
        ]),
        ..Conversation::default()
    }
}

type Events = Rc<RefCell<Vec<&'static str>>>;

#[allow(clippy::type_complexity)]
fn open<'a>(
    cx: &'a mut TestAppContext,
    routes: Vec<(&'static str, u16, String)>,
) -> (
    gpui::Entity<ConversationView>,
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
    let (view, cx) = cx.add_window_view(ConversationView::new);
    let events: Events = Rc::default();
    let sink = events.clone();
    view.update(cx, |view, cx| {
        view.composer.update(cx, |composer, _| {
            composer.endpoint = Some(DesktopApi::new(port, "k"));
        });
        view.task_id = Some("task".into());
        view.loading = false;
        view.apply(Rc::new(data()), cx);
        cx.subscribe(&cx.entity(), move |_, _, event: &ConversationEvent, _| {
            sink.borrow_mut().push(match event {
                ConversationEvent::OpenTerminal => "terminal",
                ConversationEvent::ToggleBrowser => "browser",
                ConversationEvent::OpenSessionSearch => "session-search",
                ConversationEvent::ReviewDelegation { .. } => "delegation",
            });
        })
        .detach();
    });
    cx.run_until_parked();
    (view, cx, events, server)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} on screen"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_buttons_reach_their_surfaces_and_resume(cx: &mut TestAppContext) {
    let (view, cx, events, server) =
        open(cx, vec![("/api/v1/sessions/a/resume", 200, "{}".into())]);
    assert!(cx.debug_bounds("conversation-header").is_some());
    assert!(
        cx.debug_bounds("header-terminal-count").is_none(),
        "nothing running"
    );
    view.update(cx, |view, cx| {
        view.header.terminals = TerminalCounts {
            current: 1,
            total: 3,
        };
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("header-terminal-count").is_some());
    click(cx, "header-terminal");
    click(cx, "header-browser");
    assert_eq!(events.borrow().as_slice(), ["terminal", "browser"]);
    // The shown conversation completed: it can be resumed.
    click(cx, "header-resume");
    let requests = server.join().unwrap();
    assert_eq!(requests[0].0, "POST /api/v1/sessions/a/resume");
}

#[gpui::test]
fn a_running_conversation_is_asked_about_then_stopped_and_closed(cx: &mut TestAppContext) {
    let (view, cx, _, server) = open(
        cx,
        vec![
            ("/api/v1/sessions/child/abort", 200, "{}".into()),
            (
                "/api/v1/sessions/child/close",
                200,
                r#"{"sessionId":"child","deletedHistory":false,"archivedAt":"x"}"#.into(),
            ),
            ("/api/v1/sessions/old/restore", 200, "{}".into()),
        ],
    );
    click(cx, "header-history");
    // The child sits under its parent; the archived one apart.
    assert!(cx.debug_bounds("history-row-0").is_some());
    assert!(cx.debug_bounds("history-row-1").is_some());
    assert!(cx.debug_bounds("history-archived-3").is_some());
    click(cx, "history-close-1");
    let asked = cx.update(|_, cx| view.read(cx).header.pending_close.clone());
    let asked = asked.expect("asked first");
    assert!(asked.requires_abort && !asked.delete_history);
    click(cx, "close-session-confirm");
    assert!(cx.update(|_, cx| view.read(cx).header.pending_close.is_none()));
    click(cx, "header-history");
    click(cx, "history-archived-3");
    let requests = server.join().unwrap();
    let paths: Vec<&str> = requests.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "POST /api/v1/sessions/child/abort",
            "POST /api/v1/sessions/child/close",
            "POST /api/v1/sessions/old/restore",
        ]
    );
    let close: Value = serde_json::from_str(&requests[1].1).unwrap();
    assert_eq!(close["deleteHistory"], false, "it had turns: archived");
}

fn rows_with(texts: &[&str]) -> Arc<Vec<dcc_core::domain::timeline::TimelineMessage>> {
    use dcc_core::domain::session::SessionEventRecord;
    let mut kinds = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        kinds.push(serde_json::json!({ "type": "turn_started", "turnId": format!("t{index}"), "prompt": text }));
        kinds.push(serde_json::json!({ "type": "turn_completed", "turnId": format!("t{index}") }));
    }
    let records: Vec<SessionEventRecord> = kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            serde_json::from_value(serde_json::json!({
                "eventId": format!("evt-{index}"),
                "sessionId": "a",
                "sequence": index + 1,
                "occurredAt": format!("2026-05-01T12:00:{:02}Z", index),
                "kind": kind,
            }))
            .unwrap()
        })
        .collect();
    Arc::new(dcc_core::domain::timeline::project_session(&records, "a"))
}

#[gpui::test]
fn find_in_the_conversation_steps_through_its_hits(cx: &mut TestAppContext) {
    let (view, cx, _, _server) = open(cx, vec![("/never", 200, "{}".into())]);
    view.update(cx, |view, cx| {
        let mut next = data();
        next.rows = rows_with(&["corrija o checkout", "e o timeout", "checkout de novo"]);
        next.last_sequence = 6;
        view.apply(Rc::new(next), cx);
    });
    cx.run_until_parked();
    // The header's button opens the bar with its field focused (⌘F goes
    // through the shell).
    click(cx, "header-thread-find");
    assert!(cx.debug_bounds("thread-find-bar").is_some());
    cx.simulate_input("checkout");
    cx.run_until_parked();
    let focus = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        cx.update(|_, cx| view.read(cx).find_focus().map(str::to_string))
    };
    let first = focus(cx).expect("a hit");
    assert!(cx.debug_bounds("thread-find-focus").is_some(), "the ring");
    cx.simulate_keystrokes("enter");
    let second = focus(cx).expect("the next");
    assert_ne!(first, second);
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(focus(cx), Some(first.clone()));
    cx.simulate_keystrokes("enter enter");
    assert_eq!(focus(cx), Some(first), "it wraps");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("thread-find-bar").is_none());
}
