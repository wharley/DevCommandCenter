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
        offer_prompt: "Posso revisar essas mudanças?".into(),
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
            if let AgentPageEvent::OpenSession {
                task_id,
                session_id,
            } = event
            {
                seen.borrow_mut().push(format!("{task_id} {session_id}"));
            }
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

#[gpui::test]
fn the_editor_saves_what_the_person_owns(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let cleanup = dir.path().join("cleanup-alert.json");
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/agents/a-1", 200, r#"{"agent":{}}"#.into()),
        ("/api/v1/agents/a-3", 200, r#"{"agent":{}}"#.into()),
    ]);
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("a-1".into(), sidebar, composer, cx)
    });
    cx.run_until_parked();
    let reporter = AgentRecord {
        name: "Repórter".into(),
        ..agent("a-3", "chronicler", Vec::new())
    };
    let path = cleanup.clone();
    page.update(cx, |page, cx| {
        page.cleanup_path = path;
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("a-1", "reviewer", Vec::new()), reporter],
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
    cx.run_until_parked();
    page.update_in(cx, |page, window, cx| page.open_editor(window, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-editor").is_some());
    assert!(cx.debug_bounds("agent-editor-offer").is_some());
    assert!(cx.debug_bounds("agent-editor-cleanup").is_none());
    let editor = page.read_with(cx, |page, _| page.editor.as_ref().unwrap().0.clone());
    let bounds = cx.debug_bounds("agent-editor-color-pink").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        editor.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        editor
            .name
            .update(cx, |input, cx| input.set_text(" Crítico ", cx));
        editor.set_execution(Some("codex".into()), None, None, cx);
        editor.offer = false;
        editor.save(cx);
    });
    cx.run_until_parked();
    assert!(
        page.read_with(cx, |page, _| page.editor.is_none()),
        "closed after saving"
    );
    // The Reporter's cleanup alert is kept beside it.
    page.update(cx, |page, cx| page.show("a-3".into(), cx));
    page.update_in(cx, |page, window, cx| page.open_editor(window, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-editor-cleanup").is_some());
    assert!(cx.debug_bounds("agent-editor-offer").is_none());
    let editor = page.read_with(cx, |page, _| page.editor.as_ref().unwrap().0.clone());
    editor.update(cx, |editor, cx| {
        editor.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        editor
            .threshold
            .update(cx, |input, cx| input.set_text("12", cx));
        editor.save(cx);
    });
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    let body: serde_json::Value = serde_json::from_str(&seen[0].1).unwrap();
    assert_eq!(body["name"], "Crítico");
    assert_eq!(body["avatar"]["color"], "pink");
    assert_eq!(body["providerId"], "codex");
    assert_eq!(body["offerPrompt"], "");
    assert_eq!(body["role"], "Revise o diff.");
    assert_eq!(body["extraInstructions"], "Seja breve.");
    let alert = crate::data::cleanup_alert::CleanupAlert::load(&cleanup);
    assert_eq!(alert.threshold_gb, 12);
    assert!(alert.enabled);
}

#[gpui::test]
fn the_reporters_page_is_its_recap(cx: &mut TestAppContext) {
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
        AgentPage::new("a-3".into(), sidebar, composer, cx)
    });
    cx.run_until_parked();
    let reporter = AgentRecord {
        name: "Repórter".into(),
        ..agent(
            "a-3",
            "chronicler",
            vec![session("s-9", "w-1", AgentState::Done)],
        )
    };
    page.update(cx, |page, cx| {
        page.db_path = db;
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![reporter],
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("recap-card").is_some());
    // No conversations on its page, and nothing moved in an empty database.
    assert!(cx.debug_bounds("agent-session-0").is_none());
    assert!(cx.debug_bounds("recap-empty").is_some());
    assert!(!page.read_with(cx, |page, cx| page.sidebar.read(cx).recap_unread()));
}

#[test]
fn weekday_times_read_like_intl() {
    let value = chrono::Local::now().to_rfc3339();
    let pt = crate::data::time::weekday_time(&value, false);
    assert!(pt.contains(".,"), "{pt}");
    let en = crate::data::time::weekday_time(&value, true);
    assert!(en.ends_with("AM") || en.ends_with("PM"), "{en}");
}

#[gpui::test]
fn the_researcher_starts_an_idea_with_its_conversation(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (port, desktop) = fake_desktop_routes(vec![
        (
            "/api/v1/research/ideas",
            200,
            r#"{"rootPath":"/home/ana/dcc-ideias/ideia-1","createdAt":"x"}"#.into(),
        ),
        (
            "/api/v1/workspaces",
            200,
            r#"{"workspaceId":"w-idea","setupReport":{"status":"pending","steps":[],"message":null},"setupHints":[]}"#
                .into(),
        ),
        ("/api/v1/sessions/start", 200, r#"{"session":{"id":"s-idea"}}"#.into()),
        ("/api/v1/sessions/s-idea/agent", 200, r#"{"ok":true}"#.into()),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("a-2".into(), sidebar, composer, cx)
    });
    cx.run_until_parked();
    let researcher = AgentRecord {
        name: "Pesquisador".into(),
        provider_id: Some("codex".into()),
        ..agent("a-2", "researcher", Vec::new())
    };
    page.update(cx, |page, cx| {
        page.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        let mut providers = dcc_providers::offline_catalog().providers;
        for provider in &mut providers {
            provider.enabled = true;
        }
        page.composer.update(cx, |composer, _| {
            composer.catalog = Some(std::sync::Arc::new(providers))
        });
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![researcher],
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-new-idea").is_some());
    assert!(cx.debug_bounds("agent-session-0").is_none());
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    page.update(cx, |_, cx| {
        cx.subscribe_self(move |_, event: &AgentPageEvent, _| {
            if let AgentPageEvent::IdeaStarted {
                task_id,
                session_id,
            } = event
            {
                sink.borrow_mut().push(format!("{task_id} {session_id}"));
            }
        })
        .detach();
    });
    let bounds = cx.debug_bounds("agent-new-idea-start").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["w-idea s-idea"]);
    let body = |index: usize| serde_json::from_str::<serde_json::Value>(&seen[index].1).unwrap();
    let workspace = body(1);
    assert_eq!(workspace["workspaceRoot"], "/home/ana/dcc-ideias/ideia-1");
    assert_eq!(workspace["isolationMode"], "localDirect");
    assert_eq!(workspace["name"], serde_json::Value::Null);
    assert!(
        workspace["projectId"]
            .as_str()
            .unwrap()
            .starts_with("ideia-1-")
    );
    assert_eq!(body(2)["providerId"], "codex");
    assert_eq!(body(2)["title"], "Pesquisador");
    assert_eq!(body(3)["agentId"], "a-2");
    assert_eq!(seen.len(), 4, "no kickoff turn");
}

#[test]
fn an_idea_keeps_its_folder_name_with_the_time() {
    assert_eq!(
        super::idea::idea_project_id("/home/ana/dcc-ideias/Ideia X", 35),
        format!(
            "{}-z",
            dcc_core::application::linked_task::infer_project_id_from_workspace_root(
                "/home/ana/dcc-ideias/Ideia X"
            )
        )
    );
}
