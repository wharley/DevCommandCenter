//! The thread's own notes, centered under `max-w-[42rem]`: `SystemMessage`
//! (a labelled pill) and `DelegationHandBackRow` (the `[DCC]` turn DCC wrote
//! to the agent, one line with the text it sent one click away).

use dcc_core::domain::timeline::{DelegationHandBack, HandBackOutcome, TimelineMessage};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::{ConversationView, time_ago};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// `flex justify-center px-4` with the `max-w-[42rem]` column inside.
pub(super) fn centered(content: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .min_w_0()
        .justify_center()
        .px(px(16.0))
        .child(content)
}

pub(super) fn system_row(
    view: &ConversationView,
    message: &TimelineMessage,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mono = theme.fonts.mono.clone();
    let radius = theme.radius;
    let stamp = time_ago(view, message.created_at.as_deref(), cx);
    // The session action (opening another conversation) arrives with screen 2c.
    centered(
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .max_w(px(672.0))
            .min_w_0()
            .rounded(radius.md)
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.3))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(12.0))
            .line_height(px(24.0))
            .text_color(colors.muted_foreground)
            .child(
                div()
                    .flex_shrink_0()
                    .mt(px(3.0))
                    .rounded(radius.sm)
                    .bg(colors.background.opacity(0.8))
                    .px(px(6.0))
                    .py(px(2.0))
                    .font_family(mono)
                    .text_size(px(10.0))
                    .line_height(px(14.0))
                    .child(crate::ui::Tracked::new(
                        message.label.to_uppercase(),
                        10.0,
                        0.08,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(SharedString::from(message.content.clone())),
            )
            .children(stamp.map(|stamp| {
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground.opacity(0.6))
                    .child(stamp)
            })),
    )
    .into_any_element()
}

fn outcome_key(outcome: HandBackOutcome) -> &'static str {
    match outcome {
        HandBackOutcome::Finished => "delegation.handBack.finished",
        HandBackOutcome::Failed => "delegation.handBack.failed",
        HandBackOutcome::Applied => "delegation.handBack.applied",
        HandBackOutcome::Discarded => "delegation.handBack.discarded",
        HandBackOutcome::TaskTurn => "delegation.handBack.taskTurn",
    }
}

pub(super) fn hand_back_row(
    view: &ConversationView,
    message: &TimelineMessage,
    hand_back: &DelegationHandBack,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mono = theme.fonts.mono.clone();
    let radius = theme.radius;
    let key = SharedString::from(format!("hand-back-{}", message.id));
    let open = view.is_open(&key);
    let stamp = time_ago(view, message.created_at.as_deref(), cx);
    let toggle_key = key.clone();
    let body = open.then(|| {
        div()
            .id(SharedString::from(format!("{key}-body")))
            .mt(px(6.0))
            .max_h(px(288.0))
            .overflow_y_scroll()
            .rounded(radius.md)
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.3))
            .px(px(12.0))
            .py(px(8.0))
            .font_family(mono)
            .text_size(px(11.0))
            .line_height(px(17.0))
            .text_color(colors.muted_foreground)
            .child(SharedString::from(message.content.clone()))
    });
    centered(
        div()
            .w_full()
            .max_w(px(672.0))
            .text_size(px(11.5))
            .line_height(px(20.0))
            .text_color(colors.muted_foreground)
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        Icon::new("corner-down-right")
                            .size(14.0)
                            .color(colors.muted_foreground.opacity(0.7)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(cx.t(outcome_key(hand_back.outcome))),
                    )
                    .child(div().opacity(0.4).child("·"))
                    .child(
                        div()
                            .id(key.clone())
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(2.0))
                            .rounded(radius.sm)
                            .cursor_pointer()
                            .text_color(colors.muted_foreground.opacity(0.8))
                            .hover(|style| style.text_color(colors.foreground).underline())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.toggle(toggle_key.clone(), cx)
                            }))
                            .child(Icon::new("chevron-right").size(12.0).rotate(if open {
                                90.0
                            } else {
                                0.0
                            }))
                            .child(cx.t("delegation.handBack.showSent")),
                    )
                    .children(stamp.map(|stamp| {
                        div()
                            .ml_auto()
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground.opacity(0.6))
                            .child(stamp)
                    })),
            )
            .children(body),
    )
    .into_any_element()
}
