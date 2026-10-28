//! `ReviewBranchContext`: a branch → its base, names in full, as the review
//! surfaces show it (the changes list, the pull request's summary).

use gpui::{AnyElement, App, SharedString, div, prelude::*, px};

use crate::i18n::Translate;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Icon, surface_edge};

pub fn branch_context(head: String, base: Option<String>, cx: &App) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let name = |label: SharedString, value: String, grow: f32| {
        div()
            // `flex: 3 1 150px` for the branch, `1 1 45px` for the base.
            .flex_grow(grow)
            .flex_shrink(1.0)
            .flex_basis(px(if grow > 2.0 { 150.0 } else { 45.0 }))
            .min_w_0()
            .child(
                div()
                    .mb(px(3.0))
                    .text_size(px(9.0))
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .font_family(fonts::MONO)
                    .text_size(px(11.0))
                    .child(value),
            )
    };
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.0))
        .min_w_0()
        .px(px(13.0))
        .py(px(11.0))
        .border_1()
        .border_color(surface_edge(cx.theme()))
        .rounded(px(12.0))
        .bg(crate::theme::color::mix_srgb(
            colors.accent_calm,
            0.04,
            colors.background,
        ))
        .child(Icon::new("git-branch").size(13.0).color(colors.accent_calm))
        .child(name(cx.t("review.branch"), head, 3.0))
        .when_some(base, |row, base| {
            row.child(Icon::new("arrow-right").size(12.0)).child(name(
                cx.t("review.base"),
                base,
                1.0,
            ))
        })
        .into_any_element()
}
