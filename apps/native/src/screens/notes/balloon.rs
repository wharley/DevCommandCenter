//! A note's balloon (`FloatingNote`): its title and text written straight
//! into the note, its color, pin, fold into a bubble, close, complete or
//! reopen, delete; "Criar tarefa" and "Adicionar à conversa atual" go to
//! the shell. Where it sits is the workspace's, which drags it.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Pixels, Point, SharedString,
    Subscription, Window,
};

use crate::data::note_balloons::FloatingState;
use crate::data::notes::{NotePatch, NotesStore, ProjectNote};
use crate::i18n::Translate;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{TextArea, TextAreaEvent};

/// `maxLength`
const TITLE_LIMIT: usize = 160;
const CONTENT_LIMIT: usize = 20_000;

pub enum BalloonEvent {
    /// Dragging starts at this point of the window.
    Grab(Point<Pixels>),
    /// The handle's arrows (12px, 40 with Shift).
    Nudge(f32, f32),
    /// Folded or opened.
    Placed(FloatingState),
    Close,
    Delete,
    Use,
    CreateTask,
}

pub struct NoteBalloon {
    pub note_id: String,
    pub store: Entity<NotesStore>,
    pub state: FloatingState,
    pub title: Entity<TextInput>,
    pub content: Entity<TextArea>,
    pub context_open: bool,
    pub busy: bool,
    /// The drag handle, which also takes the arrows.
    pub handle: FocusHandle,
    /// Why "Adicionar à conversa atual" is off, if it is.
    pub use_reason: Option<SharedString>,
    /// Its size as painted, for keeping it in the window.
    pub size: Rc<Cell<Option<(f32, f32)>>>,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<BalloonEvent> for NoteBalloon {}

impl NoteBalloon {
    pub fn new(
        note: &ProjectNote,
        store: Entity<NotesStore>,
        state: FloatingState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title_placeholder = cx.t("notes.titlePlaceholder");
        let content_placeholder = cx.t("notes.contentPlaceholder");
        let title = cx.new(|cx| {
            let mut input = TextInput::new(window, cx)
                .with_placeholder(title_placeholder)
                .with_max_len(TITLE_LIMIT);
            input.set_text(note.title.clone(), cx);
            input
        });
        let content = cx.new(|cx| {
            let mut area = TextArea::new(window, cx).with_placeholder(content_placeholder);
            area.set_text(&note.content, cx);
            area
        });
        let inputs = vec![
            cx.subscribe(&title, |balloon, input, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    let text = input.read(cx).text().to_string();
                    balloon.edit(
                        NotePatch {
                            title: Some(text),
                            ..NotePatch::default()
                        },
                        cx,
                    );
                }
            }),
            cx.subscribe(&content, |balloon, area, event, cx| match event {
                TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
                TextAreaEvent::Changed => {
                    let mut text = area.read(cx).text().to_string();
                    if text.chars().count() > CONTENT_LIMIT {
                        text = text.chars().take(CONTENT_LIMIT).collect();
                        let cut = text.clone();
                        area.update(cx, |area, cx| area.set_text(&cut, cx));
                    }
                    balloon.edit(
                        NotePatch {
                            content: Some(text),
                            ..NotePatch::default()
                        },
                        cx,
                    );
                }
                // `onBlur`: saved now.
                TextAreaEvent::Blurred => {
                    let id = balloon.note_id.clone();
                    balloon.store.update(cx, |store, cx| store.flush(&id, cx));
                }
                _ => {}
            }),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        Self {
            note_id: note.id.clone(),
            store,
            state,
            title,
            content,
            context_open: false,
            busy: false,
            handle: cx.focus_handle(),
            use_reason: None,
            size: Rc::new(Cell::new(None)),
            _inputs: inputs,
        }
    }

    pub fn note(&self, cx: &gpui::App) -> Option<ProjectNote> {
        self.store
            .read(cx)
            .notes
            .iter()
            .find(|note| note.id == self.note_id)
            .cloned()
    }

    pub fn edit(&mut self, patch: NotePatch, cx: &mut Context<Self>) {
        let id = self.note_id.clone();
        self.store
            .update(cx, |store, cx| store.edit(&id, patch, cx));
    }

    /// `perform`: what is owed is saved first.
    fn perform(&mut self, event: BalloonEvent, cx: &mut Context<Self>) {
        let id = self.note_id.clone();
        self.store.update(cx, |store, cx| store.flush(&id, cx));
        cx.emit(event);
    }

    pub fn toggle_pin(&mut self, cx: &mut Context<Self>) {
        let pinned = self.note(cx).is_some_and(|note| note.pinned);
        self.edit(
            NotePatch {
                pinned: Some(!pinned),
                ..NotePatch::default()
            },
            cx,
        );
    }

    pub fn set_color(&mut self, color: &str, cx: &mut Context<Self>) {
        self.edit(
            NotePatch {
                color: Some(color.to_string()),
                ..NotePatch::default()
            },
            cx,
        );
    }

    pub fn toggle_minimized(&mut self, cx: &mut Context<Self>) {
        self.state.minimized = !self.state.minimized;
        cx.emit(BalloonEvent::Placed(self.state));
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.perform(BalloonEvent::Close, cx);
    }

    pub fn use_here(&mut self, cx: &mut Context<Self>) {
        if self.use_reason.is_none() {
            self.perform(BalloonEvent::Use, cx);
        }
    }

    pub fn create_task(&mut self, cx: &mut Context<Self>) {
        self.perform(BalloonEvent::CreateTask, cx);
    }

    /// Completed (or opened again), saved, and the balloon goes.
    pub fn toggle_done(&mut self, cx: &mut Context<Self>) {
        let done = self.note(cx).is_some_and(|note| note.status == "completed");
        let id = self.note_id.clone();
        self.store.update(cx, |store, cx| {
            store.edit_now(
                &id,
                NotePatch {
                    status: Some(if done { "open" } else { "completed" }.into()),
                    ..NotePatch::default()
                },
                cx,
            )
        });
        cx.emit(BalloonEvent::Close);
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        let id = self.note_id.clone();
        self.store.update(cx, |store, cx| store.retry(&id, cx));
    }
}
