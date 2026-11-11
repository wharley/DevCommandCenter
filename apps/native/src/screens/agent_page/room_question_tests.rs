//! An agent's question answered in the team room: the card docks on the
//! field, steps through each decision ("1 de 2", an option, "Outra coisa"
//! with words), reviews the answers and sends them to the desktop, then
//! leaves, in place of "precisa de você na conversa dele".

use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::team_room::HomeFeed;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};
use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, SessionId};

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
fn a_question_is_stepped_through_and_sent_from_the_room(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (port, desktop) = fake_desktop_routes(vec![(
        "/api/v1/sessions/s-sup/respond-user-input",
        200,
        "{}".into(),
    )]);
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
        "type": "turn_started", "turnId": "t", "prompt": "publica a versão",
        "planMode": false, "model": null, "evidence": null, "retryOfTurnId": null,
        "approvalPolicy": "ask"
    }))
    .unwrap();
    let asked = serde_json::from_value::<SessionEventKind>(serde_json::json!({
        "type": "turn_user_input_requested", "turnId": "t", "requestId": "q-1",
        "questions": [
            { "id": "env", "header": "Destino", "question": "Qual ambiente?",
              "options": [{ "label": "staging", "description": "Seguro" },
                          { "label": "prod", "description": "" }] },
            { "id": "when", "header": "Quando", "question": "Quando publico?", "options": [] }
        ]
    }))
    .unwrap();
    room.update(cx, |room, cx| {
        room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        room.feeds = vec![HomeFeed::with_events(
            "sup",
            "s-sup",
            vec![event(1, started), event(2, asked)],
        )];
        cx.notify();
    });
    cx.run_until_parked();
    // Docked right above the field, in place of "precisa de você".
    let (card, field) = (
        cx.debug_bounds("room-question").unwrap(),
        cx.debug_bounds("room-input").unwrap(),
    );
    assert!(card.bottom() <= field.top() + gpui::px(1.0));
    assert!(cx.debug_bounds("room-dock").is_some());
    assert!(cx.debug_bounds("room-busy-sup").is_none());
    assert!(cx.debug_bounds("room-question-step").is_some());
    let click = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
    };
    // Nothing picked: "Próxima" does nothing.
    click(cx, "room-question-main");
    assert!(cx.debug_bounds("room-question-option-1").is_some());
    click(cx, "room-question-option-0");
    click(cx, "room-question-main");
    // The second decision has no options: only "Outra coisa".
    assert!(cx.debug_bounds("room-question-option-0").is_none());
    click(cx, "room-question-other");
    assert!(cx.debug_bounds("room-question-field").is_some());
    room.update(cx, |room, cx| {
        let input = room.questions.other_field("q-1", 1).unwrap();
        input.update(cx, |input, cx| input.set_text("depois das 18h", cx));
    });
    cx.run_until_parked();
    click(cx, "room-question-main");
    assert!(cx.debug_bounds("room-question-review").is_some());
    // Back and forth keeps what was picked.
    click(cx, "room-question-back");
    assert!(cx.debug_bounds("room-question-field").is_some());
    click(cx, "room-question-main");
    click(cx, "room-question-main");
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/sessions/s-sup/respond-user-input");
    let sent: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    assert_eq!(sent["requestId"], "q-1");
    assert_eq!(
        sent["answers"],
        serde_json::json!([
            { "question": "Qual ambiente?", "answer": "staging" },
            { "question": "Quando publico?", "answer": "depois das 18h" }
        ])
    );
    cx.run_until_parked();
    // Sent: the card leaves before the history says so.
    assert!(cx.debug_bounds("room-question").is_none());
}
