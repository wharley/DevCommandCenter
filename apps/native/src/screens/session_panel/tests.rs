//! The Session tab driven headlessly: the frame and its count, the
//! delegations with their actions, and the live events in summary and
//! diagnostics.

use std::sync::Arc;

use dcc_core::domain::delegation::DelegationStatus;
use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, SessionId, TurnId};
use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::{SessionPanel, SessionPanelEvent, SessionTab};
use crate::data::conversation::{Conversation, DelegationRecord};
use crate::data::desktop_api::DesktopApi;
use crate::screens::conversation::ConversationView;
use crate::theme::{Theme, ThemePreference};

fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
    SessionEventRecord {
        event_id: format!("e-{sequence}"),
        session_id: SessionId("s-1".into()),
        sequence,
        occurred_at: String::new(),
        kind,
    }
}

fn sample() -> Conversation {
    let turn = || TurnId("t-1".into());
    let review = DelegationRecord {
        id: "d-review".into(),
        status: DelegationStatus::ReviewPending,
        mode: "review".into(),
        prompt: "Revise o parser".into(),
        provider_id: "codex".into(),
        parent_session_id: "s-1".into(),
        updated_at: "2026-10-27T10:00:00Z".into(),
        ..DelegationRecord::default()
    };
    let implement = DelegationRecord {
        id: "d-impl".into(),
        status: DelegationStatus::ReviewPending,
        mode: "implement".into(),
        implement: true,
        prompt: "Implemente".into(),
        provider_id: "claude_code".into(),
        parent_session_id: "s-1".into(),
        child_session_id: Some("s-child".into()),
        worktree: Some("/worktrees/child".into()),
        touched_files: vec!["src/lib.rs".into()],
        updated_at: "2026-10-27T11:00:00Z".into(),
        ..DelegationRecord::default()
    };
    Conversation {
        task_id: "task".into(),
        session_id: Some("s-1".into()),
        workspace_name: Some("Parser".into()),
        branch: Some("feat/parser".into()),
        workspace_path: Some("/repo".into()),
        delegations: [review, implement]
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect(),
        activity: Arc::new(vec![
            event(
                1,
                SessionEventKind::TurnStarted {
                    turn_id: turn(),
                    prompt: "Corrija o parser".into(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: None,
                },
            ),
            event(
                2,
                SessionEventKind::TurnDelta {
                    turn_id: turn(),
                    content: "ok".into(),
                },
            ),
            event(3, SessionEventKind::TurnCompleted { turn_id: turn() }),
        ]),
        ..Conversation::default()
    }
}

#[gpui::test]
fn activity_shows_the_delegations_and_the_live_events(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (port, desktop) = fake_desktop_routes(vec![(
        "/api/v1/delegations/d-review/approve",
        200,
        r#"{"status":"completed"}"#.into(),
    )]);
    let (panel, cx) = cx.add_window_view(|window, cx| {
        let conversation = cx.new(|cx| ConversationView::new(window, cx));
        SessionPanel::new(conversation, SessionTab::Activity, cx)
    });
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    panel.update(cx, |panel, cx| {
        panel.api = Some(DesktopApi::new(port, "k"));
        let conversation = panel.conversation.clone();
        conversation.update(cx, |view, cx| view.show_for_tests(sample(), cx));
        cx.subscribe_self(move |_, event: &SessionPanelEvent, _| {
            seen.borrow_mut().push(match event {
                SessionPanelEvent::OpenSession(id) => format!("session {id}"),
                SessionPanelEvent::OpenDiff(request) => format!(
                    "diff {} {} {:?}",
                    request.root,
                    request.path,
                    request.scope == WorkspaceGitPreviewScope::Unstaged
                ),
            })
        })
        .detach();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("session-panel").is_some());
    assert!(cx.debug_bounds("session-delegation-d-review").is_some());
    assert!(cx.debug_bounds("session-delegation-d-impl").is_some());
    assert!(cx.debug_bounds("session-delegation-apply-d-impl").is_some());
    assert!(
        cx.debug_bounds("session-delegation-apply-d-review")
            .is_none()
    );
    assert!(
        cx.debug_bounds("session-delegation-approve-d-review")
            .is_some()
    );
    // The implementation's file opens from its worktree.
    let implement = panel.read_with(cx, |panel, cx| {
        panel.data(cx).unwrap().delegations["d-impl"].clone()
    });
    panel.update(cx, |panel, cx| {
        panel.review_file(&implement, "src/lib.rs", cx);
        if let Some(child) = implement.child_session_id.clone() {
            cx.emit(SessionPanelEvent::OpenSession(child));
        }
    });
    assert_eq!(
        events.borrow().as_slice(),
        ["diff /worktrees/child src/lib.rs true", "session s-child"]
    );
    // Two milestones in the summary; the delta only in the diagnostics.
    let rows = |panel: &gpui::Entity<SessionPanel>, cx: &mut gpui::VisualTestContext| {
        panel.read_with(cx, |panel, cx| {
            let data = panel.data(cx).unwrap();
            data.activity
                .iter()
                .filter(|event| {
                    !panel.summary || crate::data::session_activity::is_milestone(event)
                })
                .count()
        })
    };
    assert_eq!(rows(&panel, cx), 2);
    let bounds = cx.debug_bounds("session-feed-view-1").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(rows(&panel, cx), 3);
    let review = panel.read_with(cx, |panel, cx| {
        panel.data(cx).unwrap().delegations["d-review"].clone()
    });
    panel.update(cx, |panel, cx| panel.approve(&review, cx));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/delegations/d-review/approve");
    assert_eq!(panel.read_with(cx, |panel, _| panel.busy.clone()), None);
}

#[gpui::test]
fn the_stage_shows_the_session_tab_next_to_the_chat(cx: &mut TestAppContext) {
    use crate::screens::stage::{Stage, StageEvent};
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::screens::stage::bind_keys(cx);
    });
    let (stage, cx) = cx.add_window_view(|_, cx| Stage::new(cx));
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = closed.clone();
    stage.update_in(cx, |stage, window, cx| {
        let conversation = cx.new(|cx| ConversationView::new(window, cx));
        conversation.update(cx, |view, cx| view.show_for_tests(sample(), cx));
        stage.open_session(conversation, Some(SessionTab::Activity), window, cx);
        cx.subscribe_self(move |_, event: &StageEvent, _| {
            if matches!(event, StageEvent::Closed { to_changes: false }) {
                seen.set(true);
            }
        })
        .detach();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("stage-session-tab").is_some());
    assert!(cx.debug_bounds("session-panel").is_some());
    stage.update(cx, |stage, cx| stage.show_chat(cx));
    cx.run_until_parked();
    assert!(stage.read_with(cx, |stage, _| stage.chat_active()));
    stage.update(cx, |stage, cx| stage.close(cx));
    cx.run_until_parked();
    assert!(closed.get());
    assert!(!stage.read_with(cx, |stage, _| stage.session_open()));
}

#[gpui::test]
fn details_read_the_forge_and_switch_its_account(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let context = |login: &str| {
        serde_json::json!({ "provider": "github", "host": "github.com", "remoteName": "origin",
            "namespace": "acme", "repo": "app", "remoteState": "ok", "status": "ready",
            "effectiveLogin": login, "knownHosts": ["github.com"] })
        .to_string()
    };
    let accounts = serde_json::json!({ "status": "ready", "login": "ana", "selectedLogin": "ana",
        "accounts": [{ "login": "ana", "name": "Ana" }, { "login": "bob" }],
        "message": "", "loginCommand": "gh auth login" })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/forge/context", 200, context("ana")),
        ("/api/v1/forge/accounts", 200, accounts.clone()),
        ("/api/v1/forge/select-login", 200, r#"{"ok":true}"#.into()),
        ("/api/v1/forge/context", 200, context("bob")),
        ("/api/v1/forge/accounts", 200, accounts),
    ]);
    let (panel, cx) = cx.add_window_view(|window, cx| {
        let conversation = cx.new(|cx| ConversationView::new(window, cx));
        SessionPanel::new(conversation, SessionTab::Context, cx)
    });
    panel.update(cx, |panel, cx| {
        panel.api = Some(DesktopApi::new(port, "k"));
        panel.open.extend(["runtime", "forge", "providers"]);
        let conversation = panel.conversation.clone();
        conversation.update(cx, |view, cx| view.show_for_tests(sample(), cx));
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("session-details").is_some());
    assert!(cx.debug_bounds("session-tab-context").is_some());
    assert!(cx.debug_bounds("session-forge-connect").is_some());
    assert!(cx.debug_bounds("session-forge-account-1").is_some());
    panel.update(cx, |panel, cx| panel.select_login("bob".into(), cx));
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    let requests: Vec<&str> = seen.iter().map(|(request, _)| request.as_str()).collect();
    assert_eq!(
        requests,
        [
            "POST /api/v1/forge/context",
            "POST /api/v1/forge/accounts",
            "POST /api/v1/forge/select-login",
            "POST /api/v1/forge/context",
            "POST /api/v1/forge/accounts",
        ]
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&seen[2].1).unwrap(),
        serde_json::json!({ "provider": "github", "host": "github.com", "login": "bob" })
    );
    let login = panel.read_with(cx, |panel, _| {
        panel
            .forge
            .context
            .as_ref()
            .and_then(|context| context.effective_login.clone())
    });
    assert_eq!(login.as_deref(), Some("bob"));
}
