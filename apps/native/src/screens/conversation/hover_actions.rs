//! The actions under a prompt or a reply, shown while the pointer is over it
//! (`opacity-0 group-hover:opacity-100`, `absolute right-1 bottom-0`). Copy
//! works here; edit, fork and "save as note" need the composer (screen 3).

use gpui::{AnyElement, App, ClipboardItem, SharedString, div, prelude::*, px};

use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// The group name each message row sets, so its own actions show.
pub(super) const MESSAGE_GROUP: &str = "conversation-message";

pub(super) fn copy_action(id: SharedString, content: String, cx: &App) -> AnyElement {
    let muted = cx.theme().colors.muted_foreground;
    div()
        .absolute()
        .right(px(4.0))
        .bottom_0()
        .flex()
        .items_center()
        .justify_end()
        .opacity(0.0)
        .group_hover(MESSAGE_GROUP, |style| style.opacity(1.0))
        .child(
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .size(px(20.0))
                .rounded(px(4.0))
                .cursor_pointer()
                .text_color(muted.opacity(0.28))
                .hover(move |style| style.text_color(muted))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(content.clone()))
                })
                .child(Icon::new("copy").size(14.0)),
        )
        .into_any_element()
}
