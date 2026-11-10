//! What goes on now, headlessly: an agent at work in a task of the room's
//! project shows under the room's conversation, wherever it was called from.

use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::data::tasks::TaskSummary;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn revisor() -> AgentRecord {
    AgentRecord {
        id: "rev".into(),
        name: "Revisor".into(),
        preset: Some("reviewer".into()),
        avatar: dcc_core::domain::agent_proposal::mascot_for("Revisor"),
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
        // Called by the CI trigger into the task's own conversation.
        sessions: vec![AgentSession {
            session_id: "s-task".into(),
            workspace_id: "w-rastreio".into(),
            state: AgentState::Working,
            completed_at: None,
            project_id: "checkout".into(),
            title: None,
            updated_at: "2026-11-10T14:05:00Z".into(),
            room: None,
        }],
    }
}

#[gpui::test]
fn an_agent_at_work_in_the_projects_task_shows_in_its_room(cx: &mut TestAppContext) {
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
    let mut task = TaskSummary::test("w-rastreio");
    task.name = "Código de rastreio".into();
    task.project_id = "checkout".into();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![revisor()],
                    tasks: vec![task],
                    projects: vec![crate::data::tasks::Project {
                        id: "r-checkout".into(),
                        root_path: "/p/vendeagora-app".into(),
                        display_name: "Checkout".into(),
                        icon: None,
                        color: None,
                        pinned_at: None,
                        project_id: "checkout".into(),
                        name: "vendeagora-app".into(),
                        base_branch: "main".into(),
                        missing: false,
                    }],
                    ..SidebarData::default()
                },
                cx,
            )
        });
        // The Checkout has no room yet: its work shows in Geral.
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-at-work-0").is_some());
    // Once the Checkout has its room, it shows there and not in Geral.
    page.update(cx, |page, cx| {
        page.rooms = vec!["checkout".into()];
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-at-work-0").is_none());
    page.update(cx, |page, cx| {
        page.open_room_of(Some("checkout".into()), cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("room-at-work-0").is_some());
}
