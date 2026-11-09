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
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
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
        tasks: Vec::new(),
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
                        agent("res", "Pesquisa", "", Vec::new()),
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
    assert!(
        cx.debug_bounds("room-empty").is_none(),
        "the day's recap is there"
    );
    // The day's recap opens in place and reads what moved.
    let db = dir.path().join("database.sqlite");
    drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, _| room.db_path = db);
    });
    let recap = cx.debug_bounds("room-recap").unwrap();
    cx.simulate_click(recap.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("recap-card").is_some());
    assert!(cx.debug_bounds("recap-empty").is_some());
    let recap = cx.debug_bounds("room-recap").unwrap();
    cx.simulate_click(recap.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("recap-card").is_none());

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

    // "@" opens the team above the field; a pick becomes "@Pesquisa ".
    room.update(cx, |room, cx| {
        room.input.update(cx, |area, cx| area.set_text("@", cx));
        room.update_mention(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-mentions").is_some());
    assert!(cx.debug_bounds("room-mention-1").is_some());
    room.update(cx, |room, cx| {
        room.input.update(cx, |area, cx| area.set_text("@pe", cx));
        room.update_mention(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-mention-0").is_some());
    assert!(cx.debug_bounds("room-mention-1").is_none());
    room.update(cx, |room, cx| room.accept_mention(None, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-mentions").is_none());
    room.update(cx, |room, cx| {
        assert_eq!(room.input.read(cx).text(), "@Pesquisa ");
        let (agent, reason) = room.pick("@Pesquisa compara filas", cx).unwrap();
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

#[gpui::test]
fn what_an_agent_finished_in_a_task_is_a_card_until_it_is_read(cx: &mut TestAppContext) {
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
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    // Finished now: after the reading markers' baseline, so unread.
    let done = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let task = AgentSession {
        session_id: "s-task".into(),
        workspace_id: "w-1".into(),
        state: AgentState::Done,
        completed_at: Some(done.clone()),
        project_id: "p-1".into(),
        title: Some("Checkout".into()),
        updated_at: done,
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("rev", "Revisor", "reviewer", vec![task])],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    page.update(cx, |_, cx| {
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
    });
    assert!(cx.debug_bounds("room-card-0").is_some());
    assert!(cx.debug_bounds("room-unread").is_some());
    let card = cx.debug_bounds("room-card-0").unwrap();
    cx.simulate_click(card.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["w-1 s-task"]);
    // Read now: the card stays, the count goes.
    assert!(cx.debug_bounds("room-card-0").is_some());
    assert!(cx.debug_bounds("room-unread").is_none());
}

#[gpui::test]
fn an_agent_suggests_a_teammate_and_only_the_persons_click_passes_it(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (port, desktop) = fake_desktop_routes(vec![(
        "/api/v1/sessions/s-res/turns",
        200,
        r#"{"turn":{"id":"t-9"}}"#.into(),
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
    let home = |id: &str, agent: &str| AgentSession {
        session_id: id.into(),
        workspace_id: format!("w-{agent}"),
        state: AgentState::Idle,
        completed_at: None,
        project_id: format!("dcc-agent-{agent}"),
        title: None,
        updated_at: "2026-11-07T10:01:00Z".into(),
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![
                        agent("rev", "Revisor", "reviewer", vec![home("s-home", "rev")]),
                        agent("res", "Pesquisador", "", vec![home("s-res", "res")]),
                    ],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let room = page.read_with(cx, |page, _| page.room.as_ref().unwrap().0.clone());
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
                "s-home:t-1",
                "rev",
                Speaker::Agent,
                "O PR está ok.\n\n@Pesquisador pode comparar as duas filas?",
                "2026-11-07T10:00:42Z",
            ),
        ];
        room.take(vec![feed]);
        room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        cx.notify();
    });
    cx.run_until_parked();
    // Nothing went yet: the room asks.
    assert!(cx.debug_bounds("room-handoff-1").is_some());
    let pass = cx.debug_bounds("room-handoff-pass-1").unwrap();
    cx.simulate_click(pass.center(), Modifiers::none());
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    assert_eq!(seen[0].0, "POST /api/v1/sessions/s-res/turns");
    let body: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    let prompt = body["prompt"].as_str().unwrap();
    assert!(
        prompt.starts_with("↪ Revisor → Pesquisador: @Pesquisador pode comparar"),
        "{prompt}"
    );
    assert!(prompt.contains("O PR está ok."));
    room.read_with(cx, |room, _| {
        assert_eq!(
            room.handoffs.get("s-home:t-1"),
            Some(crate::data::room_handoffs::Handled::Passed)
        );
    });
    // Passed, the bar says so and asks no more.
    assert!(cx.debug_bounds("room-handoff-1").is_some());
    assert!(cx.debug_bounds("room-handoff-pass-1").is_none());
    // An ignored suggestion leaves.
    room.update(cx, |room, cx| room.ignore_handoff("s-home:t-1", cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-handoff-1").is_none());
}

#[gpui::test]
fn the_check_in_is_a_card_and_the_room_pauses_every_trigger(cx: &mut TestAppContext) {
    use crate::data::agent_triggers::AgentTriggers;
    use crate::data::check_ins::{CheckInKind, CheckIns, Running, stalled};
    let dir = tempfile::tempdir().unwrap();
    let triggers = dir.path().join("agent-triggers.json");
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        cx.set_global(AgentTriggers::load(triggers));
        // A stall found after the first read of its kind.
        let mut checks = CheckIns::load(None);
        let now = chrono::Utc::now();
        checks.take(CheckInKind::TaskStalled, Vec::new(), now);
        let turn = Running {
            workspace_id: "w-1".into(),
            session_id: "s-1".into(),
            title: "Checkout".into(),
            project: "Loja".into(),
            project_id: "p".into(),
            started_at: "t".into(),
            last_event_at: Some((now - chrono::Duration::minutes(30)).to_rfc3339()),
        };
        checks.take(CheckInKind::TaskStalled, stalled(&[turn], now), now);
        cx.set_global(checks);
    });
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("rev", "Revisor", "reviewer", Vec::new())],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-check-in-0").is_some());
    assert!(cx.debug_bounds("room-unread").is_some());
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    page.update(cx, |_, cx| {
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
    });
    let card = cx.debug_bounds("room-check-in-0").unwrap();
    cx.simulate_click(card.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["w-1 s-1"]);
    assert_eq!(cx.update(|_, cx| cx.global::<CheckIns>().unread()), 0);

    // "Pausar agentes" holds every trigger until "Retomar".
    let pause = cx.debug_bounds("room-pause").unwrap();
    cx.simulate_click(pause.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| cx.global::<AgentTriggers>().paused()));
}

#[gpui::test]
fn an_agent_past_its_limit_is_a_card_that_opens_its_page(cx: &mut TestAppContext) {
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
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let qa = AgentRecord {
        monthly_token_limit: Some(2_000_000),
        ..agent("qa", "QA", "", Vec::new())
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("rev", "Revisor", "reviewer", Vec::new()), qa],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            room.limits = vec!["qa".into()];
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-limit-0").is_some());
    let adjust = cx.debug_bounds("room-limit-adjust-0").unwrap();
    cx.simulate_click(adjust.center(), Modifiers::none());
    cx.run_until_parked();
    page.read_with(cx, |page, _| {
        assert_eq!(page.agent_id, "qa");
        assert!(!page.room_open);
    });
}

#[gpui::test]
fn the_weekly_bulletin_opens_in_place_and_reads_the_week(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let db = dir.path().join("database.sqlite");
    drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("rev", "Revisor", "reviewer", Vec::new())],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, _| room.db_path = db);
    });
    assert!(cx.debug_bounds("room-bulletin-body").is_none());
    let card = cx.debug_bounds("room-bulletin").unwrap();
    cx.simulate_click(card.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-bulletin-body").is_some());
    // Nobody worked in an empty database.
    assert!(cx.debug_bounds("room-bulletin-empty").is_some());
}

#[gpui::test]
fn routines_show_as_lines_proposals_and_a_brake(cx: &mut TestAppContext) {
    use crate::data::agent_brake::UNATTENDED_LIMIT;
    use crate::data::agent_triggers::{AgentTriggers, Trigger};
    use crate::data::routines::Routines;
    use dcc_core::domain::agent_routine::{QUIET_ANSWER, Routine, routine_prompt};
    let dir = tempfile::tempdir().unwrap();
    let routine = Routine {
        id: "r-1".into(),
        agent_id: "qa".into(),
        days: vec![0],
        hour: 9,
        minute: 0,
        prompt: "O que travou no Checkout?".into(),
        enabled: false,
        proposed: true,
        last_run_at: None,
        created_at: "t".into(),
    };
    let asked = routine_prompt(&routine);
    let triggers = dir.path().join("agent-triggers.json");
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        cx.set_global(Routines {
            list: vec![routine.clone()],
        });
        let mut triggers = AgentTriggers::load(triggers);
        let now = chrono::Utc::now();
        for index in 0..UNATTENDED_LIMIT {
            triggers.record("qa", Trigger::Routine, &format!("r{index}"), now);
        }
        cx.set_global(triggers);
    });
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("rev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1600.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![
                        agent("rev", "Revisor", "reviewer", Vec::new()),
                        agent("qa", "QA", "", Vec::new()),
                    ],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            let mut feed = HomeFeed::new("qa", "s-home");
            feed.lines = vec![
                line("a", "qa", Speaker::Person, &asked, "2026-11-09T09:00:00Z"),
                line(
                    "b",
                    "qa",
                    Speaker::Agent,
                    QUIET_ANSWER,
                    "2026-11-09T09:00:20Z",
                ),
            ];
            room.take(vec![feed]);
            cx.notify();
        });
    });
    cx.run_until_parked();
    // The routine's turn is one line, its quiet answer is not shown.
    assert!(cx.debug_bounds("room-routine-0").is_some());
    assert!(cx.debug_bounds("room-answer-1").is_none());
    assert!(cx.debug_bounds("room-routine-proposal-0").is_some());
    assert!(cx.debug_bounds("room-brake-0").is_some());
    let resume = cx.debug_bounds("room-brake-resume-0").unwrap();
    cx.simulate_click(resume.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-brake-0").is_none());
}
