//! The team's column, headlessly: a working agent's line names its task and
//! the task's project room carries the live dot, Geral none.

use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use super::team_state::{TeamFacts, Tone, state_line};
use crate::agent::Mood;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::tasks::{Project, TaskSummary};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

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
        sessions: vec![AgentSession {
            session_id: "s-task".into(),
            workspace_id: "t1".into(),
            state: AgentState::Working,
            completed_at: None,
            project_id: "checkout".into(),
            title: None,
            updated_at: "2026-11-10T10:00:00Z".into(),
            room: None,
        }],
    }
}

fn project() -> Project {
    Project {
        id: "r-checkout".into(),
        root_path: "/p/checkout".into(),
        display_name: "checkout".into(),
        icon: None,
        color: None,
        pinned_at: None,
        project_id: "checkout".into(),
        name: "checkout".into(),
        base_branch: "main".into(),
        missing: false,
    }
}

#[gpui::test]
fn a_working_agent_shows_its_task_and_its_room_the_dot(cx: &mut TestAppContext) {
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
        page.rooms = vec!["checkout".into()];
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![dev()],
                    projects: vec![project()],
                    tasks: vec![TaskSummary {
                        name: "Corrigir domínio principal".into(),
                        project_id: "checkout".into(),
                        ..TaskSummary::test("t1")
                    }],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        cx.notify();
    });
    cx.run_until_parked();
    let line = page.update(cx, |page, cx| {
        let sidebar = page.sidebar.read(cx);
        let facts = TeamFacts::read(sidebar);
        state_line(&sidebar.agents()[0], Mood::Working, &facts, cx)
    });
    assert_eq!(line.0.as_ref(), "Corrigir domínio principal");
    assert!(line.1 == Tone::Active);
    // The project's room has the dot (it is the first listed); Geral none.
    assert!(cx.debug_bounds("team-agent-0").is_some());
    assert!(cx.debug_bounds("team-room-0-running").is_some());
    assert!(cx.debug_bounds("team-room-row-running").is_none());
}
