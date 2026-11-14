//! "Origem" under the task's header (`ui::origin_strip`): when an agent
//! opened the task, who asked for it and where it started, read once from
//! the database as the task opens. A task the person made shows nothing.

use gpui::{AnyElement, Context, Task, div, prelude::*, px};

use super::{ConversationEvent, ConversationView};
use crate::data::task_origin::OriginRead;
use crate::theme::ActiveTheme;
use crate::ui::origin_strip::origin_strip;

#[derive(Default)]
pub(crate) struct TaskOriginState {
    pub(crate) read: Option<OriginRead>,
    /// The chain shows.
    pub(crate) open: bool,
    _reading: Option<Task<()>>,
}

impl ConversationView {
    /// Another task: its origin is read (a test sets its own).
    pub(super) fn read_origin(&mut self, task_id: &str, cx: &mut Context<Self>) {
        self.origin = TaskOriginState::default();
        if cfg!(test) {
            return;
        }
        let task = task_id.to_string();
        self.origin._reading = Some(cx.spawn(async move |view, cx| {
            let wanted = task.clone();
            let read = cx
                .background_executor()
                .spawn(async move { crate::data::task_origin::read(&wanted) })
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.task_id.as_deref() == Some(task.as_str()) && read.is_some() {
                    view.origin.read = read;
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn origin_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let read = self.origin.read.as_ref()?;
        let colors = cx.theme().colors.clone();
        let strip = origin_strip(
            "task-origin",
            read,
            self.origin.open,
            Vec::new(),
            |view: &mut Self, cx| {
                view.origin.open = !view.origin.open;
                cx.notify();
            },
            |_, agent_id, room, cx| cx.emit(ConversationEvent::OpenAgentRoom { agent_id, room }),
            cx,
        )?;
        Some(
            div()
                .flex_shrink_0()
                .border_b_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.background.opacity(0.95))
                .px(px(16.0))
                .py(px(6.0))
                .child(strip)
                .into_any_element(),
        )
    }
}
