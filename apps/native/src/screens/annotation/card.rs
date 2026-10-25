//! The card's state: the code it carries, the instruction field and what
//! the person chose. ⌘↵ sends, Enter is a new line, Esc, the × and a click
//! outside close it. The text field takes the focus as it opens.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, Pixels,
    Size, Subscription, Window, actions,
};

use super::AnnotationRequest;
use crate::i18n::Translate;
use crate::ui::{TextArea, TextAreaEvent};

actions!(annotation_card, [SendAnnotation, CloseAnnotation]);

pub(crate) const CONTEXT: &str = "AnnotationCard";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-enter", SendAnnotation, Some(CONTEXT)),
        KeyBinding::new("escape", CloseAnnotation, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnnotationCardEvent {
    /// "Enviar" (this conversation) or "Nova sessão".
    Submit {
        instruction: String,
        new_session: bool,
    },
    /// "Editar no composer": the snippet into the composer's draft.
    EditInComposer(String),
    /// "Adicionar como evidência", with the instruction as its note.
    AddToReview(String),
    Cancel,
}

pub struct AnnotationCard {
    pub(crate) request: AnnotationRequest,
    pub(crate) instruction: Entity<TextArea>,
    pub(crate) focus: FocusHandle,
    /// The card as last drawn: it is placed once measured (`useLayoutEffect`).
    pub(crate) size: Rc<Cell<Option<Size<Pixels>>>>,
    _events: Subscription,
}

impl EventEmitter<AnnotationCardEvent> for AnnotationCard {}

impl Focusable for AnnotationCard {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl AnnotationCard {
    pub fn new(request: AnnotationRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = cx.t("diffAnnotate.instructionPlaceholder");
        let instruction = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(placeholder)
                .with_max_height(240.0)
        });
        let events = cx.subscribe_in(
            &instruction,
            window,
            |card, area, event: &TextAreaEvent, _, cx| match event {
                TextAreaEvent::Changed => cx.notify(),
                // Enter is a new line; ⌘↵ sends.
                TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
                TextAreaEvent::Cancelled => card.cancel(cx),
                _ => {}
            },
        );
        instruction.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            request,
            instruction,
            focus: cx.focus_handle(),
            size: Rc::default(),
            _events: events,
        }
    }

    /// What was typed, trimmed.
    pub(crate) fn text(&self, cx: &App) -> String {
        self.instruction.read(cx).text().trim().to_string()
    }

    pub(crate) fn can_submit(&self, cx: &App) -> bool {
        !self.text(cx).is_empty()
    }

    pub(crate) fn submit(&mut self, new_session: bool, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        let instruction = self.text(cx);
        cx.emit(AnnotationCardEvent::Submit {
            instruction,
            new_session,
        });
    }

    pub(crate) fn edit_in_composer(&mut self, cx: &mut Context<Self>) {
        let instruction = self.text(cx);
        cx.emit(AnnotationCardEvent::EditInComposer(instruction));
    }

    pub(crate) fn add_to_review(&mut self, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        let note = self.text(cx);
        cx.emit(AnnotationCardEvent::AddToReview(note));
    }

    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(AnnotationCardEvent::Cancel);
    }
}
