//! Code picked in a diff or a file opens the card over the window; what it
//! sends goes through the conversation's composer (`handleSubmitAnnotation`,
//! `handleEditAnnotationInComposer`). Sending or editing from the stage
//! brings the Chat forward (`revealConversation`); "Adicionar como
//! evidência" puts the snippet in the composer's draft — the evidence tray
//! stays out of the native app, which is the desktop's own fallback — and
//! leaves the stage as it is, to go on reviewing.

use dcc_core::domain::code_annotation::content;
use gpui::{AppContext, Context, Entity, Window};

use super::Shell;
use crate::i18n::Translate;
use crate::screens::annotation::AnnotationRequest;
use crate::screens::annotation::card::{AnnotationCard, AnnotationCardEvent};
use crate::ui::Toaster;

impl Shell {
    pub(super) fn open_annotation(
        &mut self,
        request: AnnotationRequest,
        from_stage: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let card = cx.new(|cx| AnnotationCard::new(request, window, cx));
        let events = cx.subscribe_in(&card, window, move |shell, card, event, window, cx| {
            shell.on_annotation(card, event, from_stage, window, cx)
        });
        self.annotation = Some((card, events));
        cx.notify();
    }

    fn on_annotation(
        &mut self,
        card: &Entity<AnnotationCard>,
        event: &AnnotationCardEvent,
        from_stage: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let annotation = card.read(cx).request.annotation.clone();
        let composer = self.conversation.read(cx).composer.clone();
        match event {
            AnnotationCardEvent::Submit {
                instruction,
                new_session,
            } => {
                let prompt = content(&annotation, instruction);
                let new_session = *new_session;
                let sent = composer.update(cx, |composer, cx| {
                    composer.send_annotation(prompt, new_session, cx)
                });
                if !sent {
                    Toaster::error(cx.t("native.sendFailed"), cx);
                }
                if from_stage {
                    self.stage.update(cx, |stage, cx| stage.show_chat(cx));
                }
            }
            AnnotationCardEvent::EditInComposer(instruction) => {
                let text = content(&annotation, instruction);
                if from_stage {
                    self.stage.update(cx, |stage, cx| stage.show_chat(cx));
                }
                composer.update(cx, |composer, cx| composer.append_text(&text, window, cx));
            }
            AnnotationCardEvent::AddToReview(note) => {
                let text = content(&annotation, note);
                composer.update(cx, |composer, cx| composer.append_text(&text, window, cx));
            }
            AnnotationCardEvent::Cancel => {}
        }
        self.annotation = None;
        cx.notify();
    }
}
