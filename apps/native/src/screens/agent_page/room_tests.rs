//! The team room driven headlessly: the agents' conversations together, a
//! line opening its conversation, who would answer as the person writes,
//! and a message sent to the agent's own conversation.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::{AgentPage, AgentPageEvent};
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::team_room::{HomeFeed, Receipt, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn agent(id: &str, name: &str, preset: &str, sessions: Vec<AgentSession>) -> AgentRecord {
    AgentRecord {
        id: id.into(),
        name: name.into(),
        preset: Some(preset.into()),
        avatar: dcc_core::domain::agent::AgentAvatar {
            color: "blue".into(),
            arms: 3,
            eyes: "round".into(),
            shape: "octopus".into(),
            accessory: "none".into(),
        },
        provider_id: Some("claude_code".into()),
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: "r".into(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        projects: Vec::new(),
        sessions,
    }
}

fn line(key: &str, agent: &str, speaker: Speaker, text: &str, at: &str) -> RoomLine {
    RoomLine {
        key: key.into(),
        agent_id: agent.into(),
        session_id: "s-home".into(),
        speaker,
        text: text.into(),
        at: at.into(),
        receipt: (speaker == Speaker::Agent).then_some(Receipt {
            seconds: Some(42),
            tokens: Some(1200),
        }),
    }
}

#[gpui::test]
fn the_room_shows_the_team_together_and_sends_to_who_answers(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (port, desktop) = fake_desktop_routes(vec![(
        "/api/v1/sessions/s-home/turns",
        200,
        r#"{"turn":{"id":"t-2"}}"#.into(),
    )]);
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        composer.update(cx, |composer, _| {
            let providers: Vec<dcc_core::domain::provider::ProviderDescriptor> =
                serde_json::from_value(serde_json::json!([{
                    "id": "claude_code", "label": "Claude Code", "description": "",
                    "models": [{ "id": "claude-sonnet-5-5", "label": "Claude Sonnet 5.5",
                                 "description": "", "recommended": true, "effortLevels": [] }],
                    "capabilities": {
                        "streaming": true, "mcpSupport": "unsupported", "tools": true,
                        "vision": true, "resumable": true, "experimental": false,
                        "canBeDelegationTarget": true, "canRequestDelegation": true,
                        "supportsReadOnlyDelegation": true, "supportsEditDelegation": true,
                        "approvalPolicies": ["read_only", "ask", "auto", "full_access"],
                        "planModeSupport": "native"
                    },
                    "health": "Healthy", "stable": true
                }]))
                .unwrap();
            composer.catalog = Some(std::sync::Arc::new(providers));
        });
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let home = AgentSession {
        session_id: "s-home".into(),
        workspace_id: "w-home".into(),
        state: AgentState::Idle,
        completed_at: None,
        project_id: "dcc-agent-rev".into(),
        title: None,
        updated_at: "2026-11-07T10:01:00Z".into(),
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![
                        agent("rev", "Revisor", "reviewer", vec![home]),
                        agent("res", "Pesquisador", "researcher", Vec::new()),
                        agent("rep", "Repórter", "chronicler", Vec::new()),
                    ],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("team-room").is_some());
    assert!(cx.debug_bounds("room-empty").is_some());
    // The Reporter runs no model: no name to address.
    assert!(cx.debug_bounds("room-address-1").is_some());
    assert!(cx.debug_bounds("room-address-2").is_none());

    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    let room = page.update(cx, |page, cx| {
        cx.subscribe_self(move |_, event: &AgentPageEvent, _| {
            if let AgentPageEvent::OpenSession {
                task_id,
                session_id,
            } = event
            {
                sink.borrow_mut().push(format!("{task_id} {session_id}"));
            }
        })
        .detach();
        page.room.as_ref().unwrap().0.clone()
    });
    room.update(cx, |room, cx| {
        let mut feed = HomeFeed::new("rev", "s-home");
        feed.lines = vec![
            line(
                "a",
                "rev",
                Speaker::Person,
                "revisa o PR",
                "2026-11-07T10:00:00Z",
            ),
            line(
                "b",
                "rev",
                Speaker::Agent,
                "Tudo certo.",
                "2026-11-07T10:00:42Z",
            ),
        ];
        room.take(vec![feed]);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-said-0").is_some());
    assert!(cx.debug_bounds("room-answer-1").is_some());
    let open = cx.debug_bounds("room-open-1").unwrap();
    cx.simulate_click(open.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["w-home s-home"]);

    // Writing shows who would answer; "@Pesquisador" addresses the Researcher.
    let address = cx.debug_bounds("room-address-1").unwrap();
    cx.simulate_click(address.center(), Modifiers::none());
    cx.run_until_parked();
    room.update(cx, |room, cx| {
        assert_eq!(room.input.read(cx).text(), "@Pesquisador ");
        let (agent, reason) = room.pick("@Pesquisador compara filas", cx).unwrap();
        assert_eq!(agent.id, "res");
        assert_eq!(reason, dcc_core::domain::team_room::RouteReason::Mention);
        // Without a sign, the subject stays with the Reviewer only while fresh.
        let (agent, _) = room.pick("e o resto?", cx).unwrap();
        assert_eq!(agent.id, "rev");
    });

    // A message for the Reviewer goes to its own conversation.
    room.update(cx, |room, cx| {
        room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        room.input
            .update(cx, |area, cx| area.set_text("o CI falhou no checkout", cx));
        room.send(cx);
        assert_eq!(room.pending.len(), 1);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-pending-0").is_some());
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/sessions/s-home/turns");
    let body: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    assert_eq!(body["prompt"], "o CI falhou no checkout");
    assert_eq!(body["providerId"], "claude_code");
    // Once its conversation has it, the message leaves the pending ones.
    room.update(cx, |room, _| {
        let mut feed = HomeFeed::new("rev", "s-home");
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        feed.lines = vec![line(
            "c",
            "rev",
            Speaker::Person,
            "o CI falhou no checkout",
            &at,
        )];
        room.take(vec![feed]);
        assert!(room.pending.is_empty());
    });
}
