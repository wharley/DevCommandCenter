//! The agent's screen over the conversation (`ComputerUsePreview` in
//! `ActiveThreadViewport`): it follows the conversation on screen and its
//! turn, hears the desktop's computer-use events through the approvals'
//! stream, and "Parar agente" stops the turn like the composer's stop.

use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::Shell;
use crate::screens::computer_use::approval::{ComputerApprovals, LiveEvent};
use crate::screens::computer_use::preview::{ComputerPreview, PreviewEvent};
use crate::screens::conversation::ConversationView;

pub(super) fn link(
    approvals: &Entity<ComputerApprovals>,
    conversation: &Entity<ConversationView>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> [Subscription; 3] {
    let api = if cfg!(test) {
        None
    } else {
        crate::data::desktop_api::DesktopApi::load()
    };
    let preview = cx.new(|_| ComputerPreview::new(api));
    conversation.update(cx, |view, cx| {
        view.computer_preview = Some(preview.clone());
        cx.notify();
    });
    let live = {
        let preview = preview.clone();
        cx.subscribe(approvals, move |_, _, LiveEvent(event), cx| {
            preview.update(cx, |preview, cx| preview.apply(event, cx));
        })
    };
    let stop = {
        let conversation = conversation.downgrade();
        cx.subscribe_in(&preview, window, move |_, _, event, _, cx| {
            let PreviewEvent::Stop = event;
            if let Some(conversation) = conversation.upgrade() {
                let composer = conversation.read(cx).composer.clone();
                composer.update(cx, |composer, cx| composer.stop(cx));
            }
        })
    };
    let shown = cx.observe(conversation, move |_, conversation, cx| {
        let view = conversation.read(cx);
        let session = view.shown_session();
        let running = view.data().is_some_and(|data| data.running);
        preview.update(cx, |preview, cx| preview.follow(session, running, cx));
    });
    [live, stop, shown]
}
