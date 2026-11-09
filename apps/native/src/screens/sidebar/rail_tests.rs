//! The collapsed rail: each task shows its project's face, and a dot when it
//! needs the person.

use gpui::{TestAppContext, WindowAppearance};

use super::Sidebar;
use crate::data::loader::SidebarData;
use crate::data::tasks::{TaskStatus, TaskSummary};
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn the_rail_marks_only_the_tasks_that_need_the_person(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (sidebar, cx) = cx.add_window_view(|_, cx| Sidebar::new(true, false, false, seen, cx));
    // A worktree still being prepared waits on the person; a ready one does not.
    let mut waiting = TaskSummary::test("w-setup");
    waiting.status = TaskStatus::SetupPending;
    waiting.worktree_path = Some("/wt".into());
    let data = SidebarData {
        tasks: vec![waiting, TaskSummary::test("w-ready")],
        ..SidebarData::default()
    };
    sidebar.update(cx, |sidebar, cx| sidebar.apply_for_tests(data, cx));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();

    let tile = cx.debug_bounds("rail-task-dot-w-setup").expect("a dot");
    assert!(tile.size.width > gpui::px(0.0));
    assert!(cx.debug_bounds("rail-task-dot-w-ready").is_none());
}

/// Reviewed pull requests with news: a count beside "Pull Requests", a dot
/// on the rail's icon; none without news.
#[gpui::test]
fn pull_requests_with_news_are_counted_open_and_dotted_collapsed(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    for collapsed in [false, true] {
        let seen = dir.path().join(format!("seen-{collapsed}.json"));
        let (sidebar, cx) =
            cx.add_window_view(|_, cx| Sidebar::new(collapsed, false, false, seen, cx));
        let mark = if collapsed {
            "rail-pull-requests-news"
        } else {
            "nav-pull-requests-count"
        };
        cx.run_until_parked();
        assert!(cx.debug_bounds(mark).is_none(), "{mark}: no news");
        sidebar.update(cx, |sidebar, cx| sidebar.set_pr_news(2, cx));
        cx.run_until_parked();
        assert!(cx.debug_bounds(mark).is_some(), "{mark}");
    }
}

/// "Agentes" counts the agents that need the person (amber) or left a
/// result; the rail shows a dot instead. The old row of mascots is gone.
#[gpui::test]
fn agents_waiting_are_counted_beside_agentes(cx: &mut TestAppContext) {
    use crate::data::agents::{AgentRecord, AgentSession, AgentState};
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let agent = |id: &str, state: Option<AgentState>| AgentRecord {
        id: id.into(),
        name: id.into(),
        preset: None,
        avatar: dcc_core::domain::agent::AgentAvatar {
            color: "blue".into(),
            arms: 3,
            eyes: "round".into(),
            shape: "octopus".into(),
            accessory: "none".into(),
        },
        provider_id: None,
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
        sessions: state
            .map(|state| AgentSession {
                session_id: format!("s-{id}"),
                workspace_id: "w".into(),
                state,
                completed_at: None,
                project_id: "p".into(),
                title: None,
                updated_at: String::new(),
            })
            .into_iter()
            .collect(),
    };
    for collapsed in [false, true] {
        let seen = dir.path().join(format!("agents-{collapsed}.json"));
        let (sidebar, cx) =
            cx.add_window_view(|_, cx| Sidebar::new(collapsed, false, false, seen, cx));
        let data = SidebarData {
            agents: vec![
                agent("calm", None),
                agent("asks", Some(AgentState::NeedsYou)),
                agent("works", Some(AgentState::Working)),
            ],
            ..SidebarData::default()
        };
        sidebar.update(cx, |sidebar, cx| sidebar.apply_for_tests(data, cx));
        cx.run_until_parked();
        assert_eq!(
            sidebar.read_with(cx, |sidebar, cx| sidebar.agents_waiting(cx)),
            (1, true)
        );
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.lead_agent()),
            Some("asks".to_string())
        );
        if collapsed {
            assert!(cx.debug_bounds("rail-agents-news").is_some());
        } else {
            assert!(cx.debug_bounds("nav-agents-count").is_some());
        }
    }
}

/// The rail drawn by Metal, saved to look at:
/// `DCC_RAIL_PICTURE=/tmp/x.png cargo test rail_picture -- --ignored`.
#[test]
#[ignore = "renders with the GPU"]
fn rail_picture() {
    use std::sync::Arc;

    use gpui::{AppContext, px, size};

    use crate::data::tasks::Project;

    let out = std::env::var("DCC_RAIL_PICTURE").expect("DCC_RAIL_PICTURE");
    let mut cx = gpui::HeadlessAppContext::with_platform(
        Arc::new(gpui_apple::AppleTextSystem::new()),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Dark, WindowAppearance::Dark));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let project = |root: &str| Project {
        id: root.into(),
        root_path: root.into(),
        display_name: root.into(),
        icon: None,
        color: None,
        pinned_at: None,
        project_id: root.into(),
        name: root.into(),
        base_branch: "main".into(),
        missing: false,
    };
    let task = |id: &str, root: &str| {
        let mut task = TaskSummary::test(id);
        task.root_path = root.into();
        task
    };
    let mut waiting = task("w-setup", "/omnifast");
    waiting.status = TaskStatus::SetupPending;
    waiting.worktree_path = Some("/wt".into());
    let data = SidebarData {
        projects: vec![project("/omnifast"), project("/duo"), project("/dcc")],
        tasks: vec![
            task("w-one", "/omnifast"),
            waiting,
            task("w-duo", "/duo"),
            task("w-dcc", "/dcc"),
            task("w-loose", "/elsewhere"),
        ],
        ..SidebarData::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let window = cx
        .open_window(size(px(76.0), px(760.0)), |_, cx| {
            cx.new(|cx| Sidebar::new(true, false, false, seen, cx))
        })
        .unwrap();
    // After the first read, which finds no database here.
    cx.run_until_parked();
    cx.update(|cx| {
        window.update(cx, |sidebar, _, cx| {
            sidebar.apply_for_tests(data, cx);
            sidebar.state.selected_task_id = Some("w-one".into());
        })
    })
    .unwrap();
    cx.update_window(window.into(), |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
    let image = cx.capture_screenshot(window.into()).unwrap();
    image.save(&out).unwrap();
}

/// A task an agent opened shows the agent's mascot on its row; the others
/// show only their project.
#[gpui::test]
fn a_task_shows_the_mascot_of_the_agent_it_belongs_to(cx: &mut TestAppContext) {
    use crate::data::agents::AgentRecord;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (sidebar, cx) = cx.add_window_view(|_, cx| Sidebar::new(false, false, false, seen, cx));
    let suporte = AgentRecord {
        id: "suporte".into(),
        name: "Suporte".into(),
        preset: None,
        avatar: dcc_core::domain::agent_proposal::mascot_for("Suporte"),
        provider_id: None,
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: String::new(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        projects: Vec::new(),
        monthly_token_limit: None,
        tool_access: Default::default(),
        auto_start_tasks: false,
        may_call: Vec::new(),
        account_id: None,
        sessions: Vec::new(),
    };
    let data = SidebarData {
        tasks: vec![TaskSummary::test("w-campo"), TaskSummary::test("w-mine")],
        agents: vec![suporte],
        task_agents: [("w-campo".to_string(), "suporte".to_string())].into(),
        ..SidebarData::default()
    };
    sidebar.update(cx, |sidebar, cx| sidebar.apply_for_tests(data, cx));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("task-agent-w-campo").is_some());
    assert!(cx.debug_bounds("task-agent-w-mine").is_none());
}
