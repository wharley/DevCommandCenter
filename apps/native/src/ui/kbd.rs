//! `InlineShortcutDisplay`: a shortcut as small key caps.

use gpui::{AnyElement, App, BoxShadow, Hsla, SharedString, div, point, prelude::*, px};

use crate::theme::ActiveTheme;

/// The modifier keys as the desktop names them on this platform.
pub fn command_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

pub fn shortcut_keys(keys: &[&str], cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let caps = keys.iter().map(|key| {
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(20.0))
            .min_w(px(20.0))
            .px(px(6.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.background)
            .text_size(px(11.0))
            .line_height(px(11.0))
            .text_color(colors.muted_foreground)
            .shadow(vec![BoxShadow {
                color: Hsla::black().opacity(0.08),
                offset: point(px(0.0), px(-1.0)),
                blur_radius: px(0.0),
                spread_radius: px(0.0),
                inset: true,
            }])
            .child(SharedString::from(key.to_string()))
    });
    div()
        .flex()
        .items_center()
        .gap(px(1.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .children(caps)
        .into_any_element()
}
