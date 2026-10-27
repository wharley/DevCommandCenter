//! The agent's page driven headlessly: its header and how to call it, its
//! conversations (a read result rests, a pick opens the task), the extra
//! instructions, the folded role, the Researcher's ideas only, and a page
//! for an agent that is gone.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::{AgentPage, AgentPageEvent};
use crate::data::agents::{AgentRecord, AgentSession, AgentState};
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn session(id: &str, task: &str, state: AgentState) -> AgentSession {
    AgentSession {
        session_id: id.into(),
        workspace_id: task.into(),
        state,
        completed_at: None,
        project_id: "p-1".into(),
        title: Some(format!("Conversa {id}")),
        updated_at: "2026-10-27T10:05:00Z".into(),
    }
}

fn agent(id: &str, preset: &str, sessions: Vec<AgentSession>) -> AgentRecord {
    AgentRecord {
        id: id.into(),
        name: "Revisor".into(),
        preset: Some(preset.into()),
        avatar: dcc_core::domain::agent::AgentAvatar {
            color: "violet".into(),
            arms: 3,
            eyes: "round".into(),
        },
        provider_id: None,
        model: None,
        effort: None,
        kickoff_prompt: String::new(),
        role: "Revise o diff.".into(),
        extra_instructions: "Seja breve.".into(),
        sessions,
    }
}

#[gpui::test]
fn the_page_shows_the_agent_and_opens_its_conversations(cx: &mut TestAppContext) {
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
            session("s-1", "w-1", AgentState::NeedsYou),
            session("s-2", "w-gone", AgentState::Idle),
        ],
    );
    let researcher = AgentRecord {
        id: "a-2".into(),
        name: "Pesquisador".into(),
        extra_instructions: String::new(),
        ..agent(
            "a-2",
            "researcher",
            vec![
                session("s-3", "w-idea", AgentState::Idle),
                session("s-4", "w-1", AgentState::Idle),
            ],
        )
    };
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("a-1".into(), sidebar, composer, cx)
    });
    // After the sidebar's own first read.
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![reviewer, researcher],
                    idea_tasks: [("w-idea".to_string(), "Ideia de cache".to_string())].into(),
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    page.update(cx, |_, cx| {
        cx.subscribe_self(move |_, event: &AgentPageEvent, _| {
            let AgentPageEvent::OpenSession {
                task_id,
                session_id,
            } = event;
            seen.borrow_mut().push(format!("{task_id} {session_id}"));
        })
        .detach();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-page").is_some());
    assert!(cx.debug_bounds("agent-session-0").is_some());
    assert!(cx.debug_bounds("agent-session-1").is_some());
    assert!(cx.debug_bounds("agent-role").is_none());
    let bounds = cx.debug_bounds("agent-session-0").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["w-1 s-1"]);
    let bounds = cx.debug_bounds("agent-role-toggle").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-role").is_some());
    // The Researcher lists only its ideas in progress.
    page.update(cx, |page, cx| page.show("a-2".into(), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-session-0").is_some());
    assert!(cx.debug_bounds("agent-session-1").is_none());
    assert!(
        cx.debug_bounds("agent-role").is_none(),
        "the role folds again"
    );
    page.update(cx, |page, cx| page.show("gone".into(), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-page-missing").is_some());
}

#[test]
fn dates_read_like_intl_medium() {
    let at = chrono::Local::now().with_timezone(&chrono::Local);
    let value = at.to_rfc3339();
    let pt = crate::data::time::medium_date_time(&value, false);
    assert!(pt.contains(" de "), "{pt}");
    let en = crate::data::time::medium_date_time(&value, true);
    assert!(en.ends_with("AM") || en.ends_with("PM"), "{en}");
    assert_eq!(crate::data::time::medium_date_time("nope", false), "nope");
}
