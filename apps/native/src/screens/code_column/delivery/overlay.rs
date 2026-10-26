//! The delivery dialogs over the window, wherever the step was asked: the
//! desktop keeps them in the workspace panel, outside the inspector, so a
//! commit or a pull request from the conversation's menu needs neither
//! the column open nor its "Mudanças" tab. The shell draws this always;
//! it repaints with the column.

use gpui::{Context, Entity, Render, Subscription, Window, div, prelude::*};

use super::super::CodeColumn;

pub struct DeliveryOverlay {
    column: Entity<CodeColumn>,
    _column: Subscription,
}

impl DeliveryOverlay {
    pub fn new(column: Entity<CodeColumn>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&column, |_, _, cx| cx.notify());
        Self {
            column,
            _column: observe,
        }
    }
}

impl Render for DeliveryOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Built in the column's context: their buttons act on it.
        let dialogs = self
            .column
            .update(cx, |column, cx| super::dialogs::dialogs(column, window, cx));
        div().children(dialogs)
    }
}
