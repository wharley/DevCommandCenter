//! A task picked in the menu bar panel (`menu-bar-open-task`): its
//! conversation, on the session the panel chose.

use gpui::Context;

use super::{Shell, Surface};

impl Shell {
    pub fn open_from_menu_bar(
        &mut self,
        task_id: String,
        session_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.reveal_task(&task_id, cx));
        self.conversation.update(cx, |conversation, cx| {
            conversation.open(&task_id, session_id, cx)
        });
        self.show(Surface::Conversation, cx);
    }
}
