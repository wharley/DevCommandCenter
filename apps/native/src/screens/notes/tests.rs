//! The notes library over a fresh database: it opens on the task's project,
//! filters by status, project and search, opens a note as a balloon, makes
//! a new one in the filter's project, and deletes the completed ones.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};

use super::{NoteProject, NotesScope, NotesWorkspace, StatusFilter, conversations, filtered};
use crate::data::notes::{CreateProjectNote, NotePatch, test_store};
use crate::theme::{Theme, ThemePreference};

fn input(project: &str, title: &str, conversation: &str) -> CreateProjectNote {
    CreateProjectNote {
        project_id: project.into(),
        project_name: format!("Projeto {project}"),
        title: title.into(),
        content: "texto".into(),
        context_snapshot: String::new(),
        source_session_id: None,
        source_workspace_id: None,
        source_task_title: conversation.into(),
    }
}

#[gpui::test]
fn the_library_filters_opens_creates_and_deletes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let store = cx.update(|cx| test_store(dir.path(), cx));
    store.update(cx, |store, cx| {
        store.create(input("p-1", "Cache", "Login"), cx).detach();
        store.create(input("p-1", "Logs", "Deploy"), cx).detach();
        store.create(input("p-2", "Outro", ""), cx).detach();
    });
    cx.run_until_parked();
    let done = store.read_with(cx, |store, _| {
        store
            .notes
            .iter()
            .find(|note| note.title == "Logs")
            .unwrap()
            .id
            .clone()
    });
    store.update(cx, |store, cx| {
        store.edit_now(
            &done,
            NotePatch {
                status: Some("completed".into()),
                ..Default::default()
            },
            cx,
        )
    });
    cx.run_until_parked();
    let (notes, cx) =
        cx.add_window_view(|window, cx| NotesWorkspace::new(store.clone(), window, cx));
    cx.simulate_resize(size(px(1200.0), px(900.0)));
    cx.run_until_parked();
    notes.update(cx, |notes, cx| {
        notes.set_context(
            Some(NotesScope {
                project_id: "p-1".into(),
                project_name: "Projeto p-1".into(),
                workspace_id: Some("w-1".into()),
                session_id: None,
                task_title: "Login".into(),
            }),
            vec![
                NoteProject {
                    id: "p-1".into(),
                    name: "App".into(),
                },
                NoteProject {
                    id: "p-2".into(),
                    name: "Site".into(),
                },
            ],
            cx,
        );
        notes.open_library(cx);
    });
    cx.run_until_parked();
    // The task's project, open notes: only "Cache".
    assert!(cx.debug_bounds("notes-library").is_some());
    assert!(cx.debug_bounds("note-card-0").is_some());
    assert!(cx.debug_bounds("note-card-1").is_none());
    let bounds = cx.debug_bounds("notes-status-completed").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("note-card-0").is_some(), "Logs, completed");
    assert!(cx.debug_bounds("notes-clear-completed").is_some());
    // Every project, then a search with nothing.
    notes.update(cx, |notes, cx| {
        notes.set_status(StatusFilter::Open, cx);
        notes.set_project_filter(None, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("note-card-1").is_some(), "Cache and Outro");
    notes.update(cx, |notes, cx| {
        notes
            .search
            .update(cx, |input, cx| input.set_text("nada", cx))
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-empty").is_some());
    notes.update(cx, |notes, cx| {
        notes.search.update(cx, |input, cx| input.set_text("", cx))
    });
    cx.run_until_parked();
    // A card opens its balloon and the library closes.
    let bounds = cx.debug_bounds("note-card-0").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let (open, floating) = notes.read_with(cx, |notes, _| (notes.open, notes.floating.clone()));
    assert!(!open);
    assert_eq!(floating.len(), 1);
    // A new one, in the filter's project with no context from another.
    notes.update(cx, |notes, cx| {
        notes.open_library(cx);
        notes.set_project_filter(Some("p-2".into()), cx);
        notes.create("trecho".into(), cx);
    });
    cx.run_until_parked();
    let created = store.read_with(cx, |store, _| store.notes[0].clone());
    assert_eq!(
        (created.project_id.as_str(), created.project_name.as_str()),
        ("p-2", "Site")
    );
    assert_eq!(
        created.context_snapshot, "",
        "the task is in another project"
    );
    let floating = notes.read_with(cx, |notes, _| notes.floating.clone());
    assert_eq!(floating.last(), Some(&created.id));
    // Deleting the completed ones, after the confirmation.
    notes.update(cx, |notes, cx| {
        notes.open_library(cx);
        notes.set_project_filter(None, cx);
        notes.set_status(StatusFilter::Completed, cx);
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("notes-clear-completed").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-delete").is_some());
    let bounds = cx.debug_bounds("notes-delete-confirm").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-delete").is_none());
    let left: Vec<String> = store.read_with(cx, |store, _| {
        store.notes.iter().map(|note| note.title.clone()).collect()
    });
    assert_eq!(left.len(), 3);
    assert!(!left.contains(&"Logs".to_string()));
}

#[gpui::test]
fn without_a_project_it_asks_for_one(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let store = cx.update(|cx| test_store(dir.path(), cx));
    let (notes, cx) =
        cx.add_window_view(|window, cx| NotesWorkspace::new(store.clone(), window, cx));
    cx.run_until_parked();
    notes.update(cx, |notes, cx| notes.create(String::new(), cx));
    cx.run_until_parked();
    assert!(
        notes.read_with(cx, |notes, _| notes.open),
        "the library opens"
    );
    assert!(store.read_with(cx, |store, _| store.notes.is_empty()));
}

#[test]
fn filters_read_like_the_desktop() {
    let note = |project: &str, status: &str, title: &str, conversation: &str| {
        let mut note: crate::data::notes::ProjectNote = serde_json::from_value(serde_json::json!({
            "id": title, "projectId": project, "projectName": project, "title": title,
            "content": "", "contextSnapshot": "", "sourceSessionId": null, "sourceWorkspaceId": null,
            "sourceTaskTitle": conversation, "implementationTaskId": null, "status": status,
            "color": "amber", "pinned": false, "revision": 1, "createdAt": "", "updatedAt": "",
            "completedAt": null
        }))
        .unwrap();
        note.content = "Algo Redis".into();
        note
    };
    let notes = vec![
        note("p-1", "open", "A", "Login"),
        note("p-1", "open", "B", "Login"),
        note("p-1", "completed", "C", "Deploy"),
        note("p-2", "open", "D", ""),
    ];
    let titles = |found: Vec<&crate::data::notes::ProjectNote>| {
        found
            .iter()
            .map(|note| note.title.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        titles(filtered(&notes, StatusFilter::Open, Some("p-1"), None, "")),
        ["A", "B"]
    );
    assert_eq!(
        titles(filtered(&notes, StatusFilter::Open, None, None, "redis")),
        ["A", "B", "D"]
    );
    assert_eq!(
        titles(filtered(
            &notes,
            StatusFilter::Completed,
            None,
            Some("Deploy"),
            ""
        )),
        ["C"]
    );
    assert_eq!(conversations(&notes, Some("p-1")), ["Login", "Deploy"]);
    assert!(conversations(&notes, Some("p-2")).is_empty());
}
