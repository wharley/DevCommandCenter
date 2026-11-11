//! The approvals waiting in the room, docked to the field (Compozy's
//! permission dock, Monocode's ledge): one compact row each — the agent's
//! face, "Suporte quer usar slack_send_message", its detail on one line and
//! the answers — stacked on top of the field, as wide as it, so the question
//! stays where the person types and never scrolls away with the list. At
//! most three show, then "+N". Drawn by the room, not by the list's rows.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_approvals::{Answer, RoomApproval};
use crate::agent::AgentAvatarView;
use crate::i18n::{Args, Translate};
use crate::screens::conversation::approval_reason::{may_always_allow, short_tool};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant};

/// How many approvals stack above the field.
const MAX_DOCKED: usize = 3;

/// The approvals that show and how many wait beyond them.
pub(crate) fn split_docked<T>(all: Vec<T>) -> (Vec<T>, usize) {
    let more = all.len().saturating_sub(MAX_DOCKED);
    (all.into_iter().take(MAX_DOCKED).collect(), more)
}

impl TeamRoom {
    /// The dock above the field, when something waits for the person: the
    /// question being answered (`room_question_card`) on top, then the
    /// approvals.
    pub(super) fn approval_dock(
        &self,
        approvals: Vec<RoomApproval>,
        question: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if approvals.is_empty() && question.is_none() {
            return None;
        }
        let asked = question.is_some();
        let colors = cx.theme().colors.clone();
        let amber = crate::theme::tailwind::AMBER_500.hsla();
        let (shown, more) = split_docked(approvals);
        let rows: Vec<AnyElement> = shown
            .into_iter()
            .enumerate()
            .map(|(index, approval)| self.dock_row(index, approval, index > 0 || asked, cx))
            .collect();
        Some(
            div()
                .debug_selector(|| "room-dock".into())
                .flex()
                .flex_col()
                .overflow_hidden()
                // Its bottom edge is the field's top one.
                .rounded_t(px(super::room_composer::CORNER))
                .border_1()
                .border_b_0()
                .border_color(amber.opacity(0.6))
                .bg(amber.opacity(0.06))
                .children(question)
                .children(rows)
                .when(more > 0, |dock| {
                    dock.child(
                        div()
                            .px(px(12.0))
                            .pb(px(6.0))
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                cx.t_with("agents.room.approvalMore", &Args::count(more as i64)),
                            ),
                    )
                })
                .into_any_element(),
        )
    }

    fn dock_row(
        &self,
        index: usize,
        approval: RoomApproval,
        divided: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let amber = crate::theme::tailwind::AMBER_500.hsla();
        let tool = short_tool(&approval.request.tool_name).to_string();
        let busy = self.answering.contains(&approval.request.request_id);
        let reason_open = self
            .approval_reason
            .as_ref()
            .filter(|(id, _)| *id == approval.request.request_id)
            .map(|(_, input)| input.clone());
        let detail = approval.request.detail();
        let button = |id: &str, label: SharedString, variant: ButtonVariant| {
            Button::new(SharedString::from(format!("room-approval-{id}-{index}")))
                .variant(variant)
                .size(ButtonSize::Sm)
                .disabled(busy)
                .child(label)
        };
        let (allow, always, deny, why) = (
            approval.clone(),
            approval.clone(),
            approval.clone(),
            approval.request.request_id.clone(),
        );
        let mut actions = div().flex_none().flex().items_center().gap(px(4.0));
        if reason_open.is_none() {
            actions = actions
                .child(
                    button(
                        "why",
                        cx.t("conversation.permission.denyWithReason"),
                        ButtonVariant::Ghost,
                    )
                    .on_click(cx.listener(move |room, _, window, cx| {
                        room.open_reason(why.clone(), window, cx)
                    })),
                )
                .child(
                    button(
                        "deny",
                        cx.t("conversation.permission.deny"),
                        ButtonVariant::Outline,
                    )
                    .on_click(
                        cx.listener(move |room, _, _, cx| {
                            room.answer(&deny, Answer::Deny, None, cx)
                        }),
                    ),
                )
                .when(may_always_allow(&approval.request.tool_name), |row| {
                    row.child(
                        button(
                            "always",
                            cx.t("conversation.permission.always"),
                            ButtonVariant::Outline,
                        )
                        .on_click(cx.listener(move |room, _, _, cx| {
                            room.answer(&always, Answer::Always, None, cx)
                        })),
                    )
                })
                .child(
                    button(
                        "allow",
                        cx.t("conversation.permission.allow"),
                        ButtonVariant::Default,
                    )
                    .on_click(cx.listener(move |room, _, _, cx| {
                        room.answer(&allow, Answer::Allow, None, cx)
                    })),
                );
        }
        // "Recusar com motivo" opens its field inside the dock.
        let reason_row = reason_open.map(|input| {
            let sent = approval.clone();
            let field = input.clone();
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px(px(10.0))
                        .py(px(5.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.input)
                        .bg(colors.background)
                        .child(input),
                )
                .child(
                    button(
                        "send-deny",
                        cx.t("conversation.permission.denySend"),
                        ButtonVariant::Default,
                    )
                    .on_click(cx.listener(move |room, _, _, cx| {
                        let reason = field.read(cx).text().trim().to_string();
                        if !reason.is_empty() {
                            room.answer(&sent, Answer::Deny, Some(reason), cx);
                        }
                    })),
                )
        });
        div()
            .debug_selector(move || format!("room-approval-{index}"))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .py(px(8.0))
            .when(divided, |row| {
                row.border_t_1().border_color(amber.opacity(0.25))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(div().flex_none().child(AgentAvatarView::new(
                        SharedString::from(format!("room-approval-face-{index}")),
                        &approval.agent.avatar,
                        None,
                        22.0,
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(120.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(
                                        cx.t_with(
                                            "agents.room.approval",
                                            &Args::default()
                                                .with("agent", approval.agent.name.clone())
                                                .with("tool", tool),
                                        ),
                                    ),
                            )
                            .children(detail.map(|detail| {
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(colors.muted_foreground)
                                    .child(detail)
                            })),
                    )
                    .child(actions),
            )
            .children(reason_row)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_approvals_show_and_the_rest_is_counted() {
        assert_eq!(split_docked(vec![1, 2]), (vec![1, 2], 0));
        assert_eq!(split_docked(vec![1, 2, 3]), (vec![1, 2, 3], 0));
        assert_eq!(split_docked(vec![1, 2, 3, 4, 5]), (vec![1, 2, 3], 2));
    }
}
