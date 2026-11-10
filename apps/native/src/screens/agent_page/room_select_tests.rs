//! Rooms, headlessly: an agent's Geral conversation and its conversation in
//! a project's room are shown each in its own room, and the column lists
//! the project's room once it is made.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn session(id: &str, room: Option<&str>) -> AgentSession {
    AgentSession {
        session_id: id.into(),
        workspace_id: "w-home".into(),
        state: AgentState::Idle,
        completed_at: None,
        project_id: "dcc-agent-dev".into(),
        title: None,
        updated_at: "2026-11-10T10:00:00Z".into(),
        room: room.map(str::to_string),
    }
}

fn dev() -> AgentRecord {
    AgentRecord {
        id: "dev".into(),
        name: "Dev".into(),
        preset: None,
        avatar: dcc_core::domain::agent::AgentAvatar {
            color: "purple".into(),
            arms: 2,
            eyes: "round".into(),
            shape: "robot".into(),
            accessory: "none".into(),
        },
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
        sessions: vec![
            session("s-geral", None),
            session("s-checkout", Some("checkout")),
        ],
    }
}

#[gpui::test]
fn each_room_shows_its_own_conversations(cx: &mut TestAppContext) {
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
        AgentPage::new("dev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![dev()],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let room = page.update(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    let shown = |cx: &mut gpui::VisualTestContext| {
        room.update(cx, |room, _| {
            room.feeds
                .iter()
                .map(|feed| feed.session_id.clone())
                .collect::<Vec<_>>()
        })
    };
    // The Geral room: only the Geral conversation.
    assert_eq!(shown(cx), ["s-geral"]);
    assert!(cx.debug_bounds("team-room-row").is_some());
    // The project the agent talks in is listed once its project exists;
    // here the room opens by its id.
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert_eq!(shown(cx), ["s-checkout"]);
    room.update(cx, |room, cx| {
        assert_eq!(room.room.as_deref(), Some("checkout"));
        assert!(room.placeholder_text(cx).contains("checkout"));
    });
    // Back on Geral.
    let geral = cx.debug_bounds("team-room-row").unwrap();
    cx.simulate_click(geral.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(shown(cx), ["s-geral"]);
    // "Nova sala" opens the projects' list.
    let new_room = cx.debug_bounds("team-new-room").unwrap();
    cx.simulate_click(new_room.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("team-room-picker").is_some());
}

#[gpui::test]
fn the_agents_conversation_tab_switches_rooms(cx: &mut TestAppContext) {
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
        AgentPage::new("dev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1400.0)));
    let mut dev = dev();
    // It talked in the Checkout's room last.
    dev.sessions[1].updated_at = "2026-11-10T12:00:00Z".into();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![dev],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        page.show_conversation("dev".into(), cx);
    });
    cx.run_until_parked();
    let room = page.update(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    let shown = |cx: &mut gpui::VisualTestContext| {
        room.update(cx, |room, _| {
            room.feeds
                .iter()
                .map(|feed| feed.session_id.clone())
                .collect::<Vec<_>>()
        })
    };
    assert!(cx.debug_bounds("agent-room-chips").is_some());
    assert_eq!(shown(cx), ["s-checkout"]);
    let geral = cx.debug_bounds("agent-room-0").unwrap();
    cx.simulate_click(geral.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(shown(cx), ["s-geral"]);
}
