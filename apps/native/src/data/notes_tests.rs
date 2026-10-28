//! The notes store over a fresh database.

use super::*;
use gpui::{AppContext, TestAppContext};

pub(crate) fn input(project: &str, title: &str) -> CreateProjectNote {
    CreateProjectNote {
        project_id: project.into(),
        project_name: format!("Projeto {project}"),
        title: title.into(),
        content: String::new(),
        context_snapshot: String::new(),
        source_session_id: None,
        source_workspace_id: None,
        source_task_title: String::new(),
    }
}

#[gpui::test]
fn notes_save_after_the_pause_and_come_back(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = cx.update(|cx| test_store(dir.path(), cx));
    store.update(cx, |store, cx| store.load(cx));
    cx.run_until_parked();
    assert!(!store.read_with(cx, |store, _| store.loading));
    store.update(cx, |store, cx| store.create(input("p-1", ""), cx).detach());
    cx.run_until_parked();
    let note = store.read_with(cx, |store, _| store.notes[0].clone());
    store.update(cx, |store, cx| {
        store.edit(
            &note.id,
            NotePatch {
                title: Some("Cache".into()),
                ..Default::default()
            },
            cx,
        );
        store.edit(
            &note.id,
            NotePatch {
                content: Some("Redis?".into()),
                ..Default::default()
            },
            cx,
        );
    });
    // Shown at once, saving until the pause ends.
    assert!(store.read_with(cx, |store, _| store.saving.contains(&note.id)));
    let drafts = std::fs::read_to_string(dir.path().join("notes-pending.json")).unwrap();
    assert!(drafts.contains("Redis?"), "kept while owed");
    cx.executor().advance_clock(SAVE_DELAY);
    cx.run_until_parked();
    let (saving, revision) = store.read_with(cx, |store, _| {
        (store.saving.is_empty(), store.notes[0].revision)
    });
    assert!(saving);
    assert_eq!(revision, 2);
    // Read by another store, as the next run would.
    let again = cx.new(|_| {
        NotesStore::new(
            dir.path().join("database.sqlite"),
            dir.path().join("other.json"),
        )
    });
    again.update(cx, |store, cx| store.load(cx));
    cx.run_until_parked();
    let saved = again.read_with(cx, |store, _| store.notes[0].clone());
    assert_eq!(
        (saved.title.as_str(), saved.content.as_str()),
        ("Cache", "Redis?")
    );
    assert_eq!(note_prompt(&saved), "Cache\n\nRedis?");
    // Completed, then deleted.
    store.update(cx, |store, cx| {
        store.edit_now(
            &note.id,
            NotePatch {
                status: Some("completed".into()),
                ..Default::default()
            },
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(store.read_with(cx, |store, _| store.open_count()), 0);
    store.update(cx, |store, cx| {
        store.remove(vec![note.id.clone()], cx).detach()
    });
    cx.run_until_parked();
    assert!(store.read_with(cx, |store, _| store.notes.is_empty()));
}

#[gpui::test]
fn unsaved_text_comes_back_and_saves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = cx.update(|cx| test_store(dir.path(), cx));
    store.update(cx, |store, cx| {
        store.create(input("p-1", "Antes"), cx).detach()
    });
    cx.run_until_parked();
    let note = store.read_with(cx, |store, _| store.notes[0].clone());
    std::fs::write(
        dir.path().join("notes-pending.json"),
        serde_json::json!({ note.id.clone(): { "title": "Depois", "content": "texto" } })
            .to_string(),
    )
    .unwrap();
    let fresh = cx.new(|_| {
        NotesStore::new(
            dir.path().join("database.sqlite"),
            dir.path().join("notes-pending.json"),
        )
    });
    fresh.update(cx, |store, cx| store.load(cx));
    cx.run_until_parked();
    let shown = fresh.read_with(cx, |store, _| store.notes[0].clone());
    assert_eq!(
        (shown.title.as_str(), shown.content.as_str(), shown.revision),
        ("Depois", "texto", 2)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes-pending.json")).unwrap(),
        "{}"
    );
}

#[test]
fn a_prompt_carries_the_context() {
    let note = ProjectNote {
        id: "n".into(),
        project_id: "p".into(),
        project_name: "P".into(),
        title: "".into(),
        content: "Ver".into(),
        context_snapshot: "trecho".into(),
        source_session_id: None,
        source_workspace_id: None,
        source_task_title: "Login".into(),
        implementation_task_id: None,
        status: "open".into(),
        color: "amber".into(),
        pinned: false,
        revision: 1,
        created_at: String::new(),
        updated_at: String::new(),
        completed_at: None,
    };
    assert_eq!(
        note_prompt(&note),
        "Ver\n\n---\nLogin\n<context_snapshot>\ntrecho\n</context_snapshot>"
    );
}
