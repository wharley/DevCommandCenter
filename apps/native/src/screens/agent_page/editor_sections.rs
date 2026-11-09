//! The agent's editor in four parts — who it is, how it works, what it may
//! do, and the rest — each opened by a small heading, so the settings that
//! matter are found where one looks for them.

use gpui::{AnyElement, App, FontWeight, SharedString, div, prelude::*, px};

use crate::theme::ActiveTheme;
use crate::ui::Tracked;

/// A part's heading, with room above it.
pub(super) fn section(title: SharedString, first: bool, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .when(!first, |heading| {
            heading
                .mt(px(12.0))
                .pt(px(20.0))
                .border_t_1()
                .border_color(colors.border.opacity(0.6))
        })
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.muted_foreground)
        .child(Tracked::new(title.to_uppercase(), 11.0, 0.08))
        .into_any_element()
}
