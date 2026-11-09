//! An agent's page in tabs, driven headlessly: picking the agent opens its
//! "Conversa" (the room on it alone), "Conversa completa" opens its latest
//! own conversation, and "Configuração" is the page, whose rows are its
//! task conversations, not its own.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::{AgentPage, AgentPageEvent};
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn session(id: &str, task: &str, project: &str, updated_at: &str) -> AgentSession {
    AgentSession {
        session_id: id.into(),
        workspace_id: task.into(),
        state: AgentState::Idle,
        completed_at: None,
        project_id: project.into(),
        title: None,
        updated_at: updated_at.into(),
    }
}

fn agent(id: &str, preset: &str, sessions: Vec<AgentSession>) -> AgentRecord {
    AgentRecord {
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
        id: id.into(),
        name: "Revisor".into(),
        preset: Some(preset.into()),
        avatar: dcc_core::domain::agent::AgentAvatar {
            shape: "octopus".to_string(),
            accessory: "none".to_string(),
            color: "violet".into(),
            arms: 3,
            eyes: "round".into(),
        },
        provider_id: None,
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: "Revise o diff.".into(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        sessions,
    }
}

#[gpui::test]
fn picking_an_agent_opens_its_conversation_tab(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let reviewer = agent(
        "a-1",
        "reviewer",
        vec![
            session("s-task", "w-1", "p-1", "2026-11-06T10:00:00Z"),
            session("s-old", "w-home", "dcc-agent-a-1", "2026-11-05T10:00:00Z"),
            session("s-home", "w-home", "dcc-agent-a-1", "2026-11-06T11:00:00Z"),
        ],
    );
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("a-1".into(), sidebar, composer, cx)
    });
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![reviewer, agent("a-3", "", Vec::new())],
                    agent_homes: [("a-1".to_string(), "w-home".to_string())].into(),
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
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
    cx.run_until_parked();
    // Built on its own, the page shows its settings: only the task
    // conversation is a row, the home ones are the "Conversa" tab.
    assert!(cx.debug_bounds("agent-session-0").is_some());
    assert!(cx.debug_bounds("agent-session-1").is_none());
    page.update(cx, |page, cx| {
        assert_eq!(
            page.sidebar
                .read(cx)
                .agent_page("a-1")
                .unwrap()
                .home_task
                .as_deref(),
            Some("w-home")
        );
    });

    // Picking it in the team column opens its conversation.
    let row = cx.debug_bounds("team-agent-0").unwrap();
    cx.simulate_click(row.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("team-room").is_some());
    assert!(cx.debug_bounds("agent-session-0").is_none());
    let whole = cx.debug_bounds("agent-tab-whole").unwrap();
    cx.simulate_click(whole.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["w-home s-home"]);

    // "Configuração" is the page again.
    let settings = cx.debug_bounds("agent-tabs-1").unwrap();
    cx.simulate_click(settings.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-session-0").is_some());
    assert!(cx.debug_bounds("team-room").is_none());

    // A person's agent with no conversation yet: the field waits for it.
    page.update(cx, |page, cx| page.show_conversation("a-3".into(), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-empty").is_some());
    assert!(cx.debug_bounds("agent-tab-whole").is_none());
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.read(cx);
        assert_eq!(room.focus.as_deref(), Some("a-3"));
    });
}
