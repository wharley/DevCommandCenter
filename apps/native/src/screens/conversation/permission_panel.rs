//! `PendingPermissionPanel`: the permissions the agent waits on, lifted out
//! of the thread to the bottom of the conversation (above the composer, when
//! it arrives), each with Permitir / Negar.

use dcc_core::domain::timeline::{Annotation, pending_permission_requests};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::ConversationView;
use super::cards::{ApprovalView, approval_card};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::Icon;

pub(super) fn permission_panel(
    view: &ConversationView,
    cx: &mut Context<ConversationView>,
) -> Option<AnyElement> {
    let data = view.data.clone()?;
    let pending = pending_permission_requests(&data.rows);
    if pending.is_empty() {
        return None;
    }
    let theme = cx.theme();
    let dark = theme.appearance == Appearance::Dark;
    let colors = theme.colors.clone();
    let mut cards: Vec<AnyElement> = Vec::with_capacity(pending.len());
    for annotation in &pending {
        if let Annotation::Approval {
            id,
            tool_name,
            title,
            description,
            command,
            file,
            behavior,
            ..
        } = annotation
        {
            cards.push(approval_card(
                view,
                ApprovalView {
                    request_id: id,
                    tool_name,
                    title: title.as_deref(),
                    description: description.as_deref(),
                    command: command.as_deref(),
                    file: file.as_deref(),
                    behavior: behavior.as_deref(),
                    live: true,
                },
                cx,
            ));
        }
    }
    let count = pending.len();
    Some(
        div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(tw::AMBER_500.alpha(0.3))
            .bg(tw::AMBER_500.alpha(0.06))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .mx_auto()
                    .mb(px(8.0))
                    .flex()
                    .items_start()
                    .gap(px(10.0))
                    .max_w(px(768.0))
                    .text_color(if dark {
                        tw::AMBER_100.hsla()
                    } else {
                        tw::AMBER_900.hsla()
                    })
                    .child(
                        div()
                            .mt(px(2.0))
                            .child(Icon::new("shield-alert").size(16.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(cx.t("conversation.permission.required")),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(12.0))
                                    .text_color(if dark {
                                        tw::AMBER_200.alpha(0.75)
                                    } else {
                                        tw::AMBER_800.alpha(0.8)
                                    })
                                    .child(cx.t("conversation.permission.waiting")),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .rounded_full()
                            .border_1()
                            .border_color(tw::AMBER_500.alpha(0.25))
                            .bg(colors.background.opacity(0.7))
                            .px(px(8.0))
                            .py(px(2.0))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t_with(
                                "conversation.permission.pendingCount",
                                &Args::count(count as i64),
                            )),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from("pending-permissions"))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .max_w(px(768.0))
                    .max_h(px(384.0))
                    .overflow_y_scroll()
                    .pr(px(4.0))
                    .children(cards),
            )
            .into_any_element(),
    )
}
