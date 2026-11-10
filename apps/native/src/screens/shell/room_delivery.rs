//! The room's change card in the window: "Ver mudanças" opens the task on
//! "Mudanças"; its delivery step does the same and asks the code column to
//! open the dock's own dialog for that task once it has read it
//! (`CodeColumn::request_step`), so the person reviews before anything runs.

use gpui::Context;

use super::{Shell, Surface};
use crate::data::task_delivery::DeliveryStep;

impl Shell {
    pub(super) fn open_task_changes(&mut self, task_id: &str, cx: &mut Context<Self>) {
        self.conversation
            .update(cx, |conversation, cx| conversation.open(task_id, None, cx));
        self.show(Surface::Conversation, cx);
        self.review_changes(cx);
    }

    pub(super) fn deliver_task(
        &mut self,
        task_id: &str,
        step: DeliveryStep,
        cx: &mut Context<Self>,
    ) {
        self.open_task_changes(task_id, cx);
        self.code_column
            .update(cx, |column, cx| column.request_step(task_id, step, cx));
    }
}
