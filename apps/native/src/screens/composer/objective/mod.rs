//! "Objetivo" in the composer: the provider's own goal (`/goal`). The panel
//! asks for the intent and what "pronto" means and sends them as `/goal`;
//! while that goal lives it shows it (`native`).

mod native;
mod panel;

pub(super) use panel::objective_control;

use gpui::{AppContext, Context, Entity, Focusable, Subscription, Window};

use super::Composer;
use crate::i18n::Translate;
use crate::ui::text_area::{TextArea, TextAreaEvent};

/// The panel's fields while the composer lives.
pub(crate) struct ObjectiveState {
    pub open: bool,
    pub intent: Entity<TextArea>,
    pub done_when: Entity<TextArea>,
    _events: Vec<Subscription>,
}

impl ObjectiveState {
    pub fn new(window: &mut Window, cx: &mut Context<Composer>) -> Self {
        let intent_hint = cx.t("composer.objective.intentPlaceholder");
        let done_hint = cx.t("composer.objective.doneWhenPlaceholder");
        let intent = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(intent_hint)
                .with_max_height(180.0)
        });
        let done_when = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(done_hint)
                .with_max_height(160.0)
        });
        let area = |composer: &mut Composer,
                    area: &Entity<TextArea>,
                    event: &TextAreaEvent,
                    window: &mut Window,
                    cx: &mut Context<Composer>| match event {
            TextAreaEvent::Changed => cx.notify(),
            // Enter is a new line here; nothing in the form sends on Enter.
            TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
            TextAreaEvent::Cancelled => composer.set_objective_open(false, window, cx),
            _ => {}
        };
        let events = vec![
            cx.subscribe_in(&intent, window, area),
            cx.subscribe_in(&done_when, window, area),
        ];
        Self {
            open: false,
            intent,
            done_when,
            _events: events,
        }
    }

    pub(crate) fn clear(&self, cx: &mut Context<Composer>) {
        self.intent.update(cx, |field, cx| field.set_text("", cx));
        self.done_when
            .update(cx, |field, cx| field.set_text("", cx));
    }
}

impl Composer {
    /// Another conversation: the panel closes and starts empty.
    pub(super) fn sync_objective(&mut self, switched: bool, cx: &mut Context<Self>) {
        if switched {
            self.objective.open = false;
            self.objective.clear(cx);
        }
    }

    pub(crate) fn set_objective_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.objective.open == open {
            return;
        }
        self.objective.open = open;
        if open {
            let handle = self.objective.intent.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        } else {
            self.focus(window, cx);
        }
        cx.notify();
    }
}
