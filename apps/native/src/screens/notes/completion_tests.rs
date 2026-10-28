//! A note's task: the link a new task leaves, and the question when that
//! task is completed — complete the note, or keep it open.

use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance, px, size};

use super::NotesWorkspace;
use crate::data::notes::{CreateProjectNote, NotesStore};
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn a_completed_task_asks_about_its_notes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let db = crate::test_support::task_database(dir.path(), "w-1", dir.path(), None);
    drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
    let store = cx.new(|_| NotesStore::new(db, dir.path().join("pending.json")));
    let note = |title: &str| CreateProjectNote {
        project_id: "project".into(),
        project_name: "App".into(),
        title: title.into(),
        content: String::new(),
        context_snapshot: String::new(),
        source_session_id: None,
        source_workspace_id: None,
        source_task_title: String::new(),
    };
    store.update(cx, |store, cx| {
        store.create(note("Cache"), cx).detach();
        store.create(note("Logs"), cx).detach();
    });
    cx.run_until_parked();
    let ids: Vec<String> = store.read_with(cx, |store, _| {
        store.notes.iter().map(|note| note.id.clone()).collect()
    });
    let (notes, cx) = cx.add_window_view(move |window, cx| NotesWorkspace::new(store, window, cx));
    cx.simulate_resize(size(px(1200.0), px(900.0)));
    cx.run_until_parked();
    // Both made into the task w-1; a task that does not exist links nothing.
    notes.update(cx, |notes, cx| {
        notes.link_task(&ids[0], "w-1".into(), cx);
        notes.link_task(&ids[1], "w-gone".into(), cx);
    });
    cx.run_until_parked();
    let links: Vec<Option<String>> = notes.read_with(cx, |notes, cx| {
        ids.iter()
            .map(|id| {
                notes
                    .store
                    .read(cx)
                    .notes
                    .iter()
                    .find(|note| &note.id == id)
                    .unwrap()
                    .implementation_task_id
                    .clone()
            })
            .collect()
    });
    assert_eq!(links, [Some("w-1".to_string()), None]);
    // Another task completed: no question.
    notes.update(cx, |notes, cx| notes.task_completed(vec!["w-9".into()], cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-completion").is_none());
    // "Manter abertas" leaves it open.
    notes.update(cx, |notes, cx| notes.task_completed(vec!["w-1".into()], cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-completion").is_some());
    let bounds = cx.debug_bounds("notes-keep-open").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("notes-completion").is_none());
    // Asked again, "Marcar como concluída" completes it.
    notes.update(cx, |notes, cx| notes.task_completed(vec!["w-1".into()], cx));
    cx.run_until_parked();
    let bounds = cx.debug_bounds("notes-complete-linked").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let status = notes.read_with(cx, |notes, cx| {
        notes
            .store
            .read(cx)
            .notes
            .iter()
            .find(|note| note.id == ids[0])
            .unwrap()
            .status
            .clone()
    });
    assert_eq!(status, "completed");
    assert!(cx.debug_bounds("notes-completion").is_none());
}
