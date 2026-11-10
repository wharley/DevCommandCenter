//! An agent's approval answered in the team room: the card shows with its
//! answers, in place of "precisa de você na conversa dele".

use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::team_room::HomeFeed;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};
use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, SessionId, TurnId};

fn suporte() -> AgentRecord {
    AgentRecord {
        id: "sup".into(),
        name: "Suporte".into(),
        preset: None,
        avatar: dcc_core::domain::agent_proposal::mascot_for("Suporte"),
        provider_id: Some("codex".into()),
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: "r".into(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
        sessions: vec![AgentSession {
            session_id: "s-sup".into(),
            workspace_id: "w-home".into(),
            state: AgentState::NeedsYou,
            completed_at: None,
            project_id: "dcc-agent-sup".into(),
            title: None,
            updated_at: "2026-11-10T13:34:00Z".into(),
            room: None,
        }],
    }
}

fn event(sequence: u64, kind: SessionEventKind) -> SessionEventRecord {
    SessionEventRecord {
        event_id: format!("e-{sequence}"),
        session_id: SessionId("s-sup".into()),
        sequence,
        occurred_at: format!("2026-11-10T13:3{sequence}:00Z"),
        kind,
    }
}

#[gpui::test]
fn an_approval_is_answered_in_the_room(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![suporte()],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let room = page.update(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    let started = serde_json::from_value::<SessionEventKind>(serde_json::json!({
        "type": "turn_started", "turnId": "t", "prompt": "avisa o Daniel",
        "planMode": false, "model": null, "evidence": null, "retryOfTurnId": null,
        "approvalPolicy": "ask"
    }))
    .unwrap();
    room.update(cx, |room, cx| {
        let feed = HomeFeed::with_events(
            "sup",
            "s-sup",
            vec![
                event(1, started),
                event(
                    2,
                    SessionEventKind::TurnPermissionRequested {
                        turn_id: TurnId("t".into()),
                        request_id: "r-1".into(),
                        tool_name: "slack_send_message".into(),
                        title: Some("Approve MCP tool call".into()),
                        description: Some("Mensagem para o Daniel".into()),
                        command: None,
                        file: None,
                    },
                ),
            ],
        );
        room.feeds = vec![feed];
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-approval-0").is_some());
    // Docked: right above the field, as wide as it, not in the list.
    let (dock, field) = (
        cx.debug_bounds("room-dock").unwrap(),
        cx.debug_bounds("room-input").unwrap(),
    );
    assert!(dock.bottom() <= field.top() + gpui::px(1.0));
    assert!(field.top() - dock.bottom() < gpui::px(4.0));
    assert_eq!((dock.left(), dock.right()), (field.left(), field.right()));
    // Its answer is here, not "precisa de você na conversa dele".
    assert!(cx.debug_bounds("room-busy-sup").is_none());
}

#[gpui::test]
fn the_dock_stacks_three_and_counts_the_rest(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("sup".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![suporte()],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let room = page.update(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    room.update(cx, |room, cx| {
        room.feeds = (0..5)
            .map(|n| {
                let session = format!("s-{n}");
                let started = serde_json::from_value::<SessionEventKind>(serde_json::json!({
                    "type": "turn_started", "turnId": "t", "prompt": "x",
                    "planMode": false, "model": null, "evidence": null,
                    "retryOfTurnId": null, "approvalPolicy": "ask"
                }))
                .unwrap();
                let mut asked = event(
                    2,
                    SessionEventKind::TurnPermissionRequested {
                        turn_id: TurnId("t".into()),
                        request_id: format!("r-{n}"),
                        tool_name: "slack_send_message".into(),
                        title: None,
                        description: Some("Mensagem".into()),
                        command: None,
                        file: None,
                    },
                );
                asked.session_id = SessionId(session.clone());
                let mut first = event(1, started);
                first.session_id = SessionId(session.clone());
                HomeFeed::with_events("sup", &session, vec![first, asked])
            })
            .collect();
        cx.notify();
    });
    cx.run_until_parked();
    for shown in ["room-approval-0", "room-approval-1", "room-approval-2"] {
        assert!(cx.debug_bounds(shown).is_some());
    }
    assert!(cx.debug_bounds("room-approval-3").is_none());
    // "Recusar com motivo" opens its field inside the dock.
    let why = cx.debug_bounds("room-approval-why-0").unwrap();
    cx.simulate_click(why.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let (dock, approval) = (
        cx.debug_bounds("room-dock").unwrap(),
        cx.debug_bounds("room-approval-send-deny-0").unwrap(),
    );
    assert!(dock.contains(&approval.center()));
}
