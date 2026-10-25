//! The block on screen (`parseMergeConflictHunks`): its place among the
//! ones left, both sides (`SidePreview`), and "Aceitar atual / base /
//! ambos"; or that none is left.

use dcc_core::domain::merge_conflict::ConflictResolution;
use gpui::{
    AnyElement, Context, FontWeight, SharedString, div, linear_color_stop, linear_gradient,
    prelude::*, px,
};

use super::MergeResolver;
use super::view::dark;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tracked};

/// `SidePreview`: one side of the block, sky for the workspace, amber for the base.
fn side_preview(label: String, text: &str, current: bool, cx: &gpui::App) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let dark = dark(cx);
    let (border, head_border, head_bg, head_text, dot) = if current {
        (
            tw::SKY_500.alpha(0.2),
            tw::SKY_500.alpha(0.15),
            tw::SKY_500.alpha(0.06),
            if dark { tw::SKY_300 } else { tw::SKY_700 }.hsla(),
            tw::SKY_500.hsla(),
        )
    } else {
        (
            tw::AMBER_500.alpha(0.25),
            tw::AMBER_500.alpha(0.15),
            tw::AMBER_500.alpha(0.07),
            if dark { tw::AMBER_200 } else { tw::AMBER_800 }.hsla(),
            tw::AMBER_500.hsla(),
        )
    };
    let shown = if text.is_empty() { "∅" } else { text };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(12.0))
        .border_1()
        .border_color(border)
        .bg(colors.background.opacity(0.8))
        .shadow_sm()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(8.0))
                .border_b_1()
                .border_color(head_border)
                .bg(head_bg)
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(10.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(head_text)
                .child(div().size(px(6.0)).flex_shrink_0().rounded_full().bg(dot))
                .child(div().min_w_0().overflow_hidden().child(Tracked::new(
                    label.to_uppercase(),
                    10.0,
                    0.08,
                ))),
        )
        .child(
            div()
                .id(SharedString::from(format!("merge-side-{current}")))
                .min_h(px(96.0))
                .max_h(px(220.0))
                .overflow_y_scroll()
                .px(px(12.0))
                .py(px(10.0))
                .font_family(fonts::MONO)
                .text_size(px(11.0))
                .line_height(px(20.0))
                .child(shown.trim_end_matches('\n').to_string()),
        )
        .into_any_element()
}

/// The panel over the result.
pub(crate) fn blocks_panel(
    resolver: &MergeResolver,
    current: &str,
    incoming: &str,
    cx: &mut Context<MergeResolver>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let open = resolver.open.as_ref().expect("an open file");
    let busy_any = resolver.busy.is_some();
    let (current, incoming) = (current.to_string(), incoming.to_string());
    let hunks = open.hunks();
    let hunk = open.hunk();
    let at = open.active_hunk.min(hunks.len().saturating_sub(1));
    let blocks = match &hunk {
        Some(hunk) => div()
            .child(
                div()
                    .mb(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "merge-hunk-position".into())
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.border)
                            .px(px(8.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                cx.t_with(
                                    "mergeConflict.hunk.position",
                                    &Args::default()
                                        .with("current", (at + 1).to_string())
                                        .with("total", hunks.len().to_string()),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("mergeConflict.hunk.manualHint")),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .gap(px(4.0))
                            .child(
                                Button::new("merge-hunk-previous")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::IconXs)
                                    .disabled(at == 0)
                                    .on_click(cx.listener(|resolver, _, _, cx| {
                                        resolver.step_hunk(false, cx)
                                    }))
                                    .child(Icon::new("chevron-left").size(16.0)),
                            )
                            .child(
                                Button::new("merge-hunk-next")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::IconXs)
                                    .disabled(at + 1 >= hunks.len())
                                    .on_click(cx.listener(|resolver, _, _, cx| {
                                        resolver.step_hunk(true, cx)
                                    }))
                                    .child(Icon::new("chevron-right").size(16.0)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(side_preview(current.clone(), &hunk.current_text, true, cx))
                    .child(side_preview(
                        incoming.clone(),
                        &hunk.incoming_text,
                        false,
                        cx,
                    )),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(
                        Button::new("merge-accept-current")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .disabled(busy_any)
                            .on_click(cx.listener(|resolver, _, _, cx| {
                                resolver.apply_hunk(ConflictResolution::Current, cx)
                            }))
                            .child(div().max_w(px(200.0)).truncate().child(cx.t_with(
                                "mergeConflict.hunk.accept",
                                &Args::default().with("branch", current.clone()),
                            ))),
                    )
                    .child(
                        Button::new("merge-accept-incoming")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .disabled(busy_any)
                            .on_click(cx.listener(|resolver, _, _, cx| {
                                resolver.apply_hunk(ConflictResolution::Incoming, cx)
                            }))
                            .child(div().max_w(px(200.0)).truncate().child(cx.t_with(
                                "mergeConflict.hunk.accept",
                                &Args::default().with("branch", incoming.clone()),
                            ))),
                    )
                    .child(
                        Button::new("merge-accept-both")
                            .size(ButtonSize::Xs)
                            .disabled(busy_any)
                            .on_click(cx.listener(|resolver, _, _, cx| {
                                resolver.apply_hunk(ConflictResolution::Both, cx)
                            }))
                            .child(cx.t("mergeConflict.hunk.acceptBoth")),
                    ),
            )
            .into_any_element(),
        None => div()
            .debug_selector(|| "merge-hunks-done".into())
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(tw::EMERALD_600.hsla())
            .child(Icon::new("check").size(16.0))
            .child(cx.t("mergeConflict.hunk.nonePending"))
            .into_any_element(),
    };
    let panel = div()
        .flex_shrink_0()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(linear_gradient(
            180.0,
            linear_color_stop(colors.muted.opacity(0.3), 0.0),
            linear_color_stop(colors.muted.opacity(0.1), 1.0),
        ))
        .p(px(16.0))
        .child(blocks);
    panel.into_any_element()
}
