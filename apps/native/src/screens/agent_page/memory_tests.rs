//! The agent page's "Memória" driven headlessly: the notes with the learned
//! mark, the filter, editing a note in place, the history with "Voltar para
//! esta" on every version but the current one, and the Reporter without a
//! memory.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

use super::AgentPage;
use crate::data::agent_memory::{AgentMemory, MemoryNote, MemoryVersion};
use crate::data::agents::AgentRecord;
use crate::data::loader::SidebarData;
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

fn agent(id: &str, preset: &str) -> AgentRecord {
    AgentRecord {
        projects: Vec::new(),
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
        sessions: Vec::new(),
    }
}

fn note(id: &str, body: &str, source: &str) -> MemoryNote {
    MemoryNote {
        id: id.into(),
        body: body.into(),
        source: source.into(),
        project_id: None,
        updated_at: "2026-11-06T10:00:00Z".into(),
    }
}

#[gpui::test]
fn the_memory_lists_filters_edits_and_shows_its_history(cx: &mut TestAppContext) {
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
        AgentPage::new("a-1".into(), sidebar, composer, cx)
    });
    // Tall enough for the whole page, the memory below the triggers.
    cx.simulate_resize(gpui::size(gpui::px(1100.0), gpui::px(2000.0)));
    cx.run_until_parked();
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![agent("a-1", "reviewer"), agent("a-3", "chronicler")],
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
    cx.run_until_parked();
    // No answer yet: the card shows how the agent learns.
    assert!(cx.debug_bounds("agent-memory").is_some());
    assert!(cx.debug_bounds("agent-memory-note-0").is_none());
    assert!(cx.debug_bounds("agent-memory-history").is_none());

    let learned = note("n-1", "Responder em pt-BR", "learned");
    let mine = note("n-2", "Nunca fazer merge", "person");
    let memory = AgentMemory {
        usage: 1,
        project_usage: Default::default(),
        notes: vec![learned.clone(), mine.clone()],
        versions: vec![
            MemoryVersion {
                id: 2,
                created_at: "2026-11-06T10:01:00Z".into(),
                change: "added".into(),
                summary: mine.body.clone(),
                notes: vec![learned.clone(), mine.clone()],
            },
            MemoryVersion {
                id: 1,
                created_at: "2026-11-06T10:00:00Z".into(),
                change: "learned".into(),
                summary: learned.body.clone(),
                notes: vec![learned.clone()],
            },
        ],
    };
    page.update(cx, |page, cx| {
        page.memory.clone().unwrap().update(cx, |card, cx| {
            card.memory = Some(memory);
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-memory-note-0").is_some());
    assert!(cx.debug_bounds("agent-memory-note-1").is_some());
    // Few notes: no search yet.
    assert!(cx.debug_bounds("agent-memory-search").is_none());

    // "Suas" leaves the person's note only.
    let mine_filter = cx.debug_bounds("agent-memory-filter-2").unwrap();
    cx.simulate_click(mine_filter.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-memory-note-0").is_some());
    assert!(cx.debug_bounds("agent-memory-note-1").is_none());
    let all = cx.debug_bounds("agent-memory-filter-0").unwrap();
    cx.simulate_click(all.center(), Modifiers::none());
    cx.run_until_parked();

    // Editing turns the note into a field, filled with its text.
    let edit = cx.debug_bounds("agent-memory-edit-0").unwrap();
    cx.simulate_click(edit.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-memory-edit").is_some());
    page.update(cx, |page, cx| {
        let card = page.memory.clone().unwrap();
        let card = card.read(cx);
        let (id, input, _) = card.editing.as_ref().unwrap();
        assert_eq!(id, "n-1");
        assert_eq!(input.read(cx).text(), "Responder em pt-BR");
    });

    // The history: the current version first, without "Voltar para esta".
    let history = cx.debug_bounds("agent-memory-history").unwrap();
    cx.simulate_click(history.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-memory-version-0").is_some());
    assert!(cx.debug_bounds("agent-memory-version-1").is_some());
    assert!(cx.debug_bounds("agent-memory-restore-0").is_none());
    assert!(cx.debug_bounds("agent-memory-restore-1").is_some());

    // The Reporter runs no model: no memory on its page.
    page.update(cx, |page, cx| {
        page.memory.clone().unwrap().update(cx, |card, cx| {
            card.history_open = false;
            cx.notify();
        });
        page.show("a-3".into(), cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-memory").is_none());
}

pub(super) fn project(id: &str, name: &str) -> crate::data::tasks::Project {
    crate::data::tasks::Project {
        id: format!("/w/{id}"),
        root_path: format!("/w/{id}"),
        display_name: name.into(),
        icon: None,
        color: None,
        pinned_at: None,
        project_id: id.into(),
        name: name.into(),
        base_branch: "main".into(),
        missing: false,
    }
}

#[gpui::test]
fn each_project_keeps_its_own_layer(cx: &mut TestAppContext) {
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
        AgentPage::new("a-1".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1100.0), gpui::px(2000.0)));
    cx.run_until_parked();
    // It works in the Checkout only: the Site is not one of its layers.
    let mut reviewer = agent("a-1", "reviewer");
    reviewer.projects = vec!["p-loja".into()];
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![reviewer],
                    projects: vec![project("p-loja", "Checkout"), project("p-site", "Site")],
                    ..SidebarData::default()
                },
                cx,
            )
        })
    });
    cx.run_until_parked();
    let pnpm = MemoryNote {
        project_id: Some("p-loja".into()),
        ..note("n-2", "Usar pnpm", "learned")
    };
    page.update(cx, |page, cx| {
        page.memory.clone().unwrap().update(cx, |card, cx| {
            card.memory = Some(AgentMemory {
                usage: 1,
                project_usage: [("p-loja".to_string(), 1)].into_iter().collect(),
                notes: vec![note("n-1", "Responder em pt-BR", "learned"), pnpm.clone()],
                versions: Vec::new(),
            });
            cx.notify();
        })
    });
    cx.run_until_parked();
    // The person's layer first: one note, and the layer menu.
    assert!(cx.debug_bounds("agent-memory-layer").is_some());
    assert!(cx.debug_bounds("agent-memory-note-0").is_some());
    assert!(cx.debug_bounds("agent-memory-note-1").is_none());
    page.update(cx, |page, cx| {
        let card = page.memory.clone().unwrap();
        let card = card.read(cx);
        let layers: Vec<String> = card.layer_choices().into_iter().map(|(id, _)| id).collect();
        assert_eq!(layers, ["p-loja"]);
    });
    // The Checkout's layer lists its note, and a new note goes there.
    page.update(cx, |page, cx| {
        page.memory
            .clone()
            .unwrap()
            .update(cx, |card, cx| card.pick_layer(Some("p-loja".into()), cx))
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("agent-memory-note-0").is_some());
    assert!(cx.debug_bounds("agent-memory-note-1").is_none());
    page.update(cx, |page, cx| {
        let card = page.memory.clone().unwrap();
        let card = card.read(cx);
        let memory = card.memory.as_ref().unwrap();
        let shown = crate::data::agent_memory::visible(
            &memory.notes,
            card.layer.as_deref(),
            crate::data::agent_memory::NoteFilter::All,
            "",
        );
        assert_eq!(shown[0].body, "Usar pnpm");
    });
}
