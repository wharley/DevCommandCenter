//! The conversation header's delivery wired to the code column: the
//! column's state shown in the header whenever it changes, the header's
//! picks run by the column, its dialogs drawn over the window.

use gpui::{AppContext, Context, Entity, Subscription, Window};

use super::Shell;
use crate::screens::code_column::delivery::DeliveryOverlay;
use crate::screens::code_column::{CodeColumn, CodeColumnTab};
use crate::screens::conversation::ConversationView;
use crate::screens::conversation::header::delivery::{DeliveryButtons, DeliveryButtonsEvent};

pub(super) struct DeliveryLink {
    pub overlay: Entity<DeliveryOverlay>,
    buttons: Entity<DeliveryButtons>,
    _sync: Subscription,
    _picks: Subscription,
}

pub(super) fn link(
    column: &Entity<CodeColumn>,
    conversation: &Entity<ConversationView>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> DeliveryLink {
    let overlay = cx.new(|cx| DeliveryOverlay::new(column.clone(), cx));
    let buttons = conversation.read(cx).delivery.clone();
    let sync = cx.observe(column, |shell, _, cx| shell.sync_delivery(cx));
    let picks = cx.subscribe_in(
        &buttons,
        window,
        |shell, _, event: &DeliveryButtonsEvent, window, cx| match event {
            DeliveryButtonsEvent::ReviewChanges => shell.review_changes(cx),
            DeliveryButtonsEvent::Run(action) => shell.run_delivery(*action, window, cx),
        },
    );
    DeliveryLink {
        overlay,
        buttons,
        _sync: sync,
        _picks: picks,
    }
}

impl Shell {
    /// The header's buttons follow the column (they redraw only when it
    /// changed what they show).
    fn sync_delivery(&mut self, cx: &mut Context<Self>) {
        let dock = self.code_column.read(cx).dock();
        self.delivery
            .buttons
            .update(cx, |buttons, cx| buttons.set(dock, cx));
    }

    /// "Revisar mudanças da tarefa", and the menu's fix or conflicts.
    pub(super) fn review_changes(&mut self, cx: &mut Context<Self>) {
        self.set_code_column(true, Some(CodeColumnTab::Changes), cx);
    }

    pub(super) fn run_delivery(
        &mut self,
        action: dcc_core::domain::execution_dock::DockAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.code_column
            .update(cx, |column, cx| column.run_dock_action(action, window, cx));
    }
}
