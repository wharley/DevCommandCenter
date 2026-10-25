//! "Resolver arquivo com agente" against a fake desktop and events handed
//! in by the test: the answer put in the editor for review, one left with
//! markers refused, a cancelled run, the editor read-only meanwhile, closing
//! held back, and "Ver sessão".

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dcc_core::domain::merge_conflict::WorkspaceGitConflictOperation;
use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, SessionId, TurnId};
use gpui::{TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use super::agent::AgentStatus;
use super::tests::{checks, click, source, state};
use super::{
    AgentPick, MergeResolver, MergeResolverEvent, MergeResolverRequest, Pending, ResultView,
};
use crate::data::conversation::ThreadStart;
use crate::data::desktop_api::DesktopApi;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

type Shared = Arc<Mutex<Vec<SessionEventRecord>>>;
type Seen = std::rc::Rc<std::cell::RefCell<Vec<MergeResolverEvent>>>;

struct Run<'a> {
    resolver: gpui::Entity<MergeResolver>,
    cx: &'a mut gpui::VisualTestContext,
    events: Shared,
    seen: Seen,
    server: std::thread::JoinHandle<Vec<(String, String)>>,
}

fn open<'a>(cx: &'a mut TestAppContext, extra: Vec<(&'static str, u16, String)>) -> Run<'a> {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::Toaster::init(cx);
        crate::screens::file_view::bind_keys(cx);
    });
    let merge = WorkspaceGitConflictOperation::Merge;
    let mut routes = vec![
        (
            "/api/v1/git/conflict-state",
            200,
            state(merge, vec![source()]),
        ),
        ("/api/v1/git/validation-config", 200, checks(&[])),
        (
            "/api/v1/sessions/start",
            200,
            json!({ "session": { "id": "s-1" } }).to_string(),
        ),
        (
            "/api/v1/sessions/s-1/turns",
            200,
            json!({ "turn": { "id": "t-1" } }).to_string(),
        ),
    ];
    routes.extend(extra);
    let (port, server) = fake_desktop_routes(routes);
    let request = MergeResolverRequest {
        root: "/tmp/checkout".into(),
        task_id: Some("w-1".into()),
        base_branch: Some("main".into()),
        forge_login: None,
        workspace_label: None,
        agent: Some(AgentPick {
            start: ThreadStart {
                workspace_id: "w-1".into(),
                project_id: "p".into(),
                additional_workspace_ids: Vec::new(),
                name: Some("Pagamentos".into()),
                auto_named: false,
            },
            provider_id: "claude_code".into(),
            model: Some("claude-sonnet-5-5".into()),
        }),
    };
    let events: Shared = Arc::default();
    let reader = events.clone();
    let (resolver, cx) = cx.add_window_view(move |_, cx| {
        let mut resolver =
            MergeResolver::new(request, cx).with_endpoint(DesktopApi::new(port, "k"), cx);
        resolver.read_events = Arc::new(move |_| reader.lock().unwrap().clone());
        resolver
    });
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    let seen: Seen = Default::default();
    let sink = seen.clone();
    cx.update(|_, cx| {
        cx.subscribe(&resolver, move |_, event: &MergeResolverEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });
    cx.run_until_parked();
    Run {
        resolver,
        cx,
        events,
        seen,
        server,
    }
}

fn record(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
    SessionEventRecord {
        event_id: format!("e{sequence}"),
        session_id: SessionId("s-1".into()),
        sequence,
        occurred_at: "2026-10-25T00:00:00Z".into(),
        kind,
    }
}

fn started() -> SessionEventRecord {
    record(
        1,
        serde_json::from_value(json!({ "type": "turn_started", "turnId": "t-1", "prompt": "x" }))
            .unwrap(),
    )
}

/// The run's turn answering `content`, then done.
fn answer(run: &mut Run, content: &str) {
    let token = run.resolver.read_with(run.cx, |resolver, _| {
        resolver.agent.as_ref().unwrap().token.clone()
    });
    let envelope = format!(
        "<DCC_MERGE_RESOLUTION token=\"{token}\">{}</DCC_MERGE_RESOLUTION token=\"{token}\">",
        json!({ "resolvedContent": content, "explanation": "Mantém os dois lados." })
    );
    *run.events.lock().unwrap() = vec![
        started(),
        record(
            2,
            SessionEventKind::TurnDelta {
                turn_id: TurnId("t-1".into()),
                content: envelope,
            },
        ),
        record(
            3,
            SessionEventKind::TurnCompleted {
                turn_id: TurnId("t-1".into()),
            },
        ),
    ];
    run.cx.executor().advance_clock(Duration::from_millis(300));
    run.cx.run_until_parked();
}

#[gpui::test]
fn the_agents_answer_goes_to_the_editor_for_review(cx: &mut TestAppContext) {
    let mut run = open(cx, Vec::new());
    click(run.cx, "merge-resolve-with-agent");
    run.resolver.read_with(run.cx, |resolver, cx| {
        let agent = resolver.agent.as_ref().expect("running");
        assert_eq!(agent.status, AgentStatus::Analyzing);
        assert_eq!(agent.session_id.as_deref(), Some("s-1"));
        let open = resolver.open.as_ref().unwrap();
        assert!(open.editor.read(cx).read_only, "the editor waits");
    });
    assert!(run.cx.debug_bounds("merge-agent-run").is_some());
    // Closing waits for the agent.
    run.resolver
        .update(run.cx, |resolver, cx| resolver.request_close(cx));
    assert!(run.seen.borrow().is_empty());
    answer(&mut run, "before\nmine\ntheirs\nafter\n");
    run.resolver.read_with(run.cx, |resolver, cx| {
        let open = resolver.open.as_ref().unwrap();
        assert_eq!(open.buffer, "before\nmine\ntheirs\nafter\n");
        assert_eq!(open.view, ResultView::CompareCurrent);
        assert!(!open.editor.read(cx).read_only);
        assert_eq!(
            resolver.agent.as_ref().unwrap().status,
            AgentStatus::Completed
        );
        assert_eq!(
            resolver.suggestion.as_ref().unwrap().explanation,
            "Mantém os dois lados."
        );
        assert_eq!(resolver.busy, None);
    });
    assert!(run.cx.debug_bounds("merge-agent-suggestion").is_some());
    // "Ver sessão" over the unsaved answer asks, then opens it.
    click(run.cx, "merge-agent-session");
    run.resolver.read_with(run.cx, |resolver, _| {
        assert_eq!(
            resolver.pending,
            Some(Pending::OpenAgentSession("s-1".into()))
        )
    });
    click(run.cx, "merge-confirm-ok");
    assert_eq!(
        run.seen.borrow().as_slice(),
        &[MergeResolverEvent::OpenSession("s-1".into())]
    );
    let calls = run.server.join().unwrap();
    let body = |path: &str| -> serde_json::Value {
        serde_json::from_str(
            &calls
                .iter()
                .find(|(line, _)| line.contains(path))
                .unwrap()
                .1,
        )
        .unwrap()
    };
    let start = body("/api/v1/sessions/start");
    assert_eq!(start["workspaceId"], "w-1");
    assert_eq!(start["providerId"], "claude_code");
    assert_eq!(start["title"], "Resolver conflito: src/a.rs");
    let turn = body("/api/v1/sessions/s-1/turns");
    assert_eq!(turn["effort"], "medium");
    assert_eq!(turn["approvalPolicy"], "ask");
    assert_eq!(turn["planMode"], false);
    assert!(
        turn["toolInstructions"]
            .as_str()
            .unwrap()
            .contains("read-only Git conflict")
    );
    assert!(turn["prompt"].as_str().unwrap().contains("EDITABLE RESULT"));
}

#[gpui::test]
fn an_answer_with_markers_is_not_applied(cx: &mut TestAppContext) {
    let mut run = open(cx, Vec::new());
    click(run.cx, "merge-resolve-with-agent");
    answer(
        &mut run,
        "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> main\n",
    );
    run.resolver.read_with(run.cx, |resolver, _| {
        let agent = resolver.agent.as_ref().unwrap();
        assert_eq!(agent.status, AgentStatus::Failed);
        assert!(
            agent
                .error
                .as_deref()
                .unwrap()
                .contains("marcadores de conflito")
        );
        assert_eq!(
            resolver.open.as_ref().unwrap().buffer,
            super::tests::CONFLICTED
        );
        assert!(resolver.suggestion.is_none());
    });
    assert!(run.cx.debug_bounds("merge-agent-error").is_some());
}

#[gpui::test]
fn a_cancelled_run_aborts_the_turn_and_says_so(cx: &mut TestAppContext) {
    let run = open(cx, vec![("/api/v1/sessions/s-1/abort", 200, "{}".into())]);
    click(run.cx, "merge-resolve-with-agent");
    click(run.cx, "merge-agent-cancel");
    run.resolver.read_with(run.cx, |resolver, _| {
        assert_eq!(
            resolver.agent.as_ref().unwrap().status,
            AgentStatus::Cancelling
        )
    });
    run.cx.executor().advance_clock(Duration::from_millis(300));
    run.cx.run_until_parked();
    *run.events.lock().unwrap() = vec![
        started(),
        record(
            2,
            SessionEventKind::TurnAborted {
                turn_id: TurnId("t-1".into()),
                reason: Some("cancelado".into()),
            },
        ),
    ];
    run.cx.executor().advance_clock(Duration::from_millis(300));
    run.cx.run_until_parked();
    run.resolver.read_with(run.cx, |resolver, _| {
        let agent = resolver.agent.as_ref().unwrap();
        assert_eq!(agent.status, AgentStatus::Failed);
        assert_eq!(
            agent.error.as_deref(),
            Some("O agente interrompeu a sugestão de resolução: cancelado")
        );
    });
    let calls = run.server.join().unwrap();
    assert!(
        calls
            .iter()
            .any(|(line, _)| line.contains("/api/v1/sessions/s-1/abort"))
    );
}
