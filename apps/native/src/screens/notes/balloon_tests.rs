//! The balloons over a fresh database: the task's notes and the pinned ones
//! come back, typing saves, the color, folding and the place on disk,
//! dragging inside the window, closing for this visit, completing, and
//! "Adicionar à conversa atual" only on a task of the note's project.

use gpui::{Modifiers, MouseButton, TestAppContext, WindowAppearance, point, px, size};

use super::{NoteProject, NotesEvent, NotesScope, NotesWorkspace};
use crate::data::note_balloons::read_positions;
use crate::data::notes::{CreateProjectNote, NotePatch, SAVE_DELAY, test_store};
use crate::theme::{Theme, ThemePreference};

fn input(title: &str, task: Option<&str>) -> CreateProjectNote {
    CreateProjectNote {
        project_id: "p-1".into(),
        project_name: "App".into(),
        title: title.into(),
        content: String::new(),
        context_snapshot: String::new(),
        source_session_id: None,
        source_workspace_id: task.map(str::to_string),
        source_task_title: "Login".into(),
    }
}

/// `debug_bounds` takes a `'static` selector.
fn selector(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn scope(task: &str) -> NotesScope {
    NotesScope {
        project_id: "p-1".into(),
        project_name: "App".into(),
        workspace_id: Some(task.into()),
        session_id: None,
        task_title: "Login".into(),
    }
}

#[gpui::test]
fn balloons_come_back_edit_move_and_close(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let store = cx.update(|cx| test_store(dir.path(), cx));
    // The source task does not exist, so the note keeps no link: pin it.
    store.update(cx, |store, cx| {
        store.create(input("Fixada", None), cx).detach();
        store.create(input("Outra", None), cx).detach();
    });
    cx.run_until_parked();
    let pinned = store.read_with(cx, |store, _| {
        store
            .notes
            .iter()
            .find(|note| note.title == "Fixada")
            .unwrap()
            .id
            .clone()
    });
    store.update(cx, |store, cx| {
        store.edit_now(
            &pinned,
            NotePatch {
                pinned: Some(true),
                ..Default::default()
            },
            cx,
        )
    });
    cx.run_until_parked();
    let positions = dir.path().join("positions.json");
    let (notes, cx) = cx.add_window_view({
        let (store, positions) = (store.clone(), positions.clone());
        move |window, cx| NotesWorkspace::new(store, window, cx).with_positions(positions)
    });
    cx.simulate_resize(size(px(1200.0), px(900.0)));
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    notes.update(cx, |notes, cx| {
        cx.subscribe_self(move |_, event: &NotesEvent, _| {
            if let NotesEvent::Use(text) = event {
                sink.borrow_mut().push(text.clone());
            }
        })
        .detach();
        notes.set_context(
            Some(scope("w-1")),
            vec![NoteProject {
                id: "p-1".into(),
                name: "App".into(),
            }],
            cx,
        );
    });
    cx.run_until_parked();
    // Only the pinned one comes back.
    let open: Vec<String> = notes.read_with(cx, |notes, _| {
        notes.floating.iter().map(|(id, _)| id.clone()).collect()
    });
    assert_eq!(open, std::slice::from_ref(&pinned));
    assert!(
        cx.debug_bounds(selector(format!("note-balloon-{pinned}")))
            .is_some()
    );
    // Typing the title saves after the pause.
    let balloon = notes.read_with(cx, |notes, _| notes.balloons[&pinned].0.clone());
    balloon.update(cx, |balloon, cx| {
        let title = balloon.title.clone();
        title.update(cx, |input, cx| input.set_text("Fixada e editada", cx));
        balloon.edit(
            NotePatch {
                title: Some("Fixada e editada".into()),
                ..Default::default()
            },
            cx,
        );
    });
    cx.executor().advance_clock(SAVE_DELAY);
    cx.run_until_parked();
    let saved = store.read_with(cx, |store, _| {
        store
            .notes
            .iter()
            .find(|note| note.id == pinned)
            .unwrap()
            .clone()
    });
    assert_eq!(
        (saved.title.as_str(), saved.revision),
        ("Fixada e editada", 3)
    );
    // A color, then folded into a bubble; the place goes to disk.
    let bounds = cx.debug_bounds("note-color-mint").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let color = store.read_with(cx, |store, _| {
        store
            .notes
            .iter()
            .find(|note| note.id == pinned)
            .unwrap()
            .color
            .clone()
    });
    assert_eq!(color, "mint");
    let bounds = cx
        .debug_bounds(selector(format!("note-fold-{pinned}")))
        .unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    assert!(read_positions(&positions)[&pinned].minimized);
    // Dragged far left and up: it stops 8px in and 40px down.
    let handle = cx
        .debug_bounds(selector(format!("note-handle-{pinned}")))
        .unwrap()
        .center();
    cx.simulate_mouse_down(handle, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("note-drag-layer").is_some());
    cx.simulate_mouse_move(
        point(px(1.0), px(1.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(px(1.0), px(1.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let place = notes.read_with(cx, |notes, _| notes.floating[0].1);
    assert_eq!((place.x, place.y), (8.0, 40.0));
    assert!(cx.debug_bounds("note-drag-layer").is_none());
    // Off this task's project, "Adicionar" is off; on it, the text goes out.
    let reason = balloon.read_with(cx, |balloon, _| balloon.use_reason.is_some());
    assert!(!reason);
    balloon.update(cx, |balloon, cx| balloon.use_here(cx));
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), ["Fixada e editada"]);
    assert!(
        notes.read_with(cx, |notes, _| notes.floating.is_empty()),
        "used, it closes"
    );
    // Closed: gone for this visit, back on the next.
    balloon.update(cx, |balloon, cx| balloon.close(cx));
    cx.run_until_parked();
    assert!(notes.read_with(cx, |notes, _| notes.floating.is_empty()));
    notes.update(cx, |notes, cx| notes.restore_balloons(cx));
    assert!(notes.read_with(cx, |notes, _| notes.floating.is_empty()));
    notes.update(cx, |notes, cx| {
        let projects = notes.projects.clone();
        notes.set_context(Some(scope("w-2")), projects.clone(), cx);
        notes.set_context(Some(scope("w-1")), projects, cx);
    });
    cx.run_until_parked();
    let back = notes.read_with(cx, |notes, _| notes.floating.clone());
    assert_eq!(back.len(), 1);
    assert!(
        back[0].1.minimized && back[0].1.x == 8.0,
        "where it was left"
    );
    // Completed: saved and gone.
    let balloon = notes.read_with(cx, |notes, _| notes.balloons[&pinned].0.clone());
    balloon.update(cx, |balloon, cx| balloon.toggle_done(cx));
    cx.run_until_parked();
    let status = store.read_with(cx, |store, _| {
        store
            .notes
            .iter()
            .find(|note| note.id == pinned)
            .unwrap()
            .status
            .clone()
    });
    assert_eq!(status, "completed");
    assert!(notes.read_with(cx, |notes, _| notes.floating.is_empty()));
}

#[gpui::test]
fn using_a_note_needs_a_task_of_its_project(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let store = cx.update(|cx| test_store(dir.path(), cx));
    store.update(cx, |store, cx| {
        store.create(input("Ideia", None), cx).detach()
    });
    cx.run_until_parked();
    let id = store.read_with(cx, |store, _| store.notes[0].id.clone());
    let (notes, cx) = cx.add_window_view(move |window, cx| NotesWorkspace::new(store, window, cx));
    cx.simulate_resize(size(px(1200.0), px(900.0)));
    notes.update(cx, |notes, cx| notes.show(&id, cx));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds(selector(format!("note-balloon-{id}")))
            .is_some()
    );
    let balloon = notes.read_with(cx, |notes, _| notes.balloons[&id].0.clone());
    assert!(
        balloon.read_with(cx, |balloon, _| balloon.use_reason.is_some()),
        "no task on screen"
    );
    let place = notes.read_with(cx, |notes, _| notes.floating[0].1);
    assert_eq!((place.x, place.y), (830.0, 85.0), "the default, top right");
}
