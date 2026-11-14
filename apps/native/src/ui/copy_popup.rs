//! "Copiar", where a right click on selected text landed: the one-item
//! menu the conversation and the team room open over their own selection.

use gpui::{
    AnyElement, App, ClickEvent, MouseButton, Pixels, Point, Window, anchored, deferred, div,
    prelude::*, px,
};

use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::Icon;

pub fn copy_popup(
    id: &'static str,
    at: Point<Pixels>,
    on_copy: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let (hover, foreground) = (colors.accent, colors.accent_foreground);
    let item = div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(radius.md)
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(13.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover).text_color(foreground))
        // The press is the menu's, not a new selection under it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(on_copy)
        .child(Icon::new("copy").size(14.0))
        .child(cx.t("native.copySelection"));
    let panel = div()
        .min_w(px(140.0))
        .p(px(4.0))
        .rounded(radius.lg)
        .border_1()
        .border_color(colors.border)
        .bg(colors.popover)
        .text_color(colors.popover_foreground)
        .shadow_md()
        .child(item);
    deferred(
        anchored()
            .position(at)
            .snap_to_window_with_margin(px(8.0))
            .child(panel),
    )
    .with_priority(2)
    .into_any_element()
}
