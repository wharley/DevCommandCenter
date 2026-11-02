//! The actions under a prompt or a reply, shown while the pointer is over it
//! (`opacity-0 group-hover:opacity-100`, `absolute right-1 bottom-0`): copy
//! on both, and "Editar a partir daqui" on a prompt.

use gpui::{AnyElement, App, ClickEvent, ClipboardItem, SharedString, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// The group name each message row sets, so its own actions show.
pub(super) const MESSAGE_GROUP: &str = "conversation-message";

pub(super) fn actions(children: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    div()
        .absolute()
        .right(px(4.0))
        .bottom_0()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(2.0))
        .opacity(0.0)
        .group_hover(MESSAGE_GROUP, |style| style.opacity(1.0))
        .children(children)
        .into_any_element()
}

pub(super) fn action(
    id: SharedString,
    icon: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().colors.muted_foreground;
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
        .on_click(on_click)
        .child(Icon::new(icon).size(14.0))
        .into_any_element()
}

pub(super) fn copy(id: SharedString, content: String, cx: &App) -> AnyElement {
    action(
        id,
        "copy",
        move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(content.clone())),
        cx,
    )
}

pub(super) fn copy_action(id: SharedString, content: String, cx: &App) -> AnyElement {
    actions([copy(id, content, cx)])
}
