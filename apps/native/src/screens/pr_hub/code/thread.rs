//! A conversation on a line (`dcc-code-inline-thread`): who wrote it and
//! whether it is resolved, the comment, then Responder, "Preparar com
//! agente" and Resolver/Reabrir, and the reply field when answering.

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::rows::thread_card;
use crate::data::pr_review::InlineComment;
use crate::i18n::Translate;
use crate::screens::pr_hub::PrHub;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

impl PrHub {
    /// A conversation's comment, with Responder and Resolver.
    pub(super) fn thread(
        &self,
        comment: &InlineComment,
        mark: super::since::ThreadMark,
        width: f32,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let green = tw::EMERALD_500.hsla();
        let id = comment.id.clone();
        let login = comment
            .author
            .as_ref()
            .map(|actor| actor.login.clone())
            .unwrap_or_else(|| cx.t("pullRequests.unknownAuthor").to_string());
        let replying = self.review.replying_to.as_deref() == Some(id.as_str());
        let actions = comment.thread_id.as_ref().map(|_| {
            let reply_to = id.clone();
            let toggled = comment.clone();
            div()
                .mt(px(8.0))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .child(
                    Button::new(gpui::SharedString::from(format!("pr-thread-reply-{id}")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .text_size(12.0)
                        .on_click(cx.listener(move |hub, _, window, cx| {
                            hub.start_reply(reply_to.clone(), window, cx)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(Icon::new("reply").size(12.0))
                                .child(cx.t("pullRequests.code.reply")),
                        ),
                )
                .child({
                    let preparing = self.agent_busy(cx).as_deref() == Some(&format!("reply:{id}"));
                    let prepared = comment.clone();
                    Button::new(gpui::SharedString::from(format!("pr-thread-prepare-{id}")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .text_size(12.0)
                        .disabled(preparing)
                        .on_click(
                            cx.listener(move |hub, _, _, cx| {
                                hub.prepare_reply(prepared.clone(), cx)
                            }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(if preparing {
                                    crate::ui::spinner("pr-thread-preparing", 12.0)
                                        .into_any_element()
                                } else {
                                    Icon::new("sparkles").size(12.0).into_any_element()
                                })
                                .child(cx.t("pullRequests.code.prepareWithAgent")),
                        )
                })
                .child(
                    Button::new(gpui::SharedString::from(format!("pr-thread-resolve-{id}")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .text_size(12.0)
                        .on_click(cx.listener(move |hub, _, _, cx| {
                            hub.toggle_resolved(toggled.clone(), cx)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(Icon::new("check").size(12.0))
                                .child(cx.t(if comment.resolved == Some(true) {
                                    "pullRequests.code.reopenThread"
                                } else {
                                    "pullRequests.code.resolveThread"
                                })),
                        ),
                )
        });
        let reply_box = replying.then(|| {
            let empty = self.review.reply.read(cx).text().trim().is_empty();
            let sent = comment.clone();
            div()
                .mt(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.border)
                .bg(colors.background.opacity(0.7))
                .p(px(8.0))
                .child(
                    div()
                        .debug_selector(|| "pr-thread-reply-field".into())
                        .min_h(px(64.0))
                        .text_size(px(11.0))
                        .child(self.review.reply.clone()),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap(px(8.0))
                        .child(
                            Button::new("pr-thread-reply-cancel")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Xs)
                                .text_size(12.0)
                                .on_click(cx.listener(|hub, _, _, cx| hub.cancel_reply(cx)))
                                .child(cx.t("pullRequests.code.cancel")),
                        )
                        .child(
                            Button::new("pr-thread-reply-send")
                                .size(ButtonSize::Xs)
                                .text_size(12.0)
                                .disabled(empty || self.review.replying)
                                .on_click(cx.listener(move |hub, _, _, cx| {
                                    hub.send_reply(sent.clone(), cx)
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .child(Icon::new("send").size(12.0))
                                        .child(cx.t("pullRequests.code.publishReply")),
                                ),
                        ),
                )
        });
        let selector = format!("pr-thread-{id}");
        thread_card(width, cx)
            .debug_selector(move || selector)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(format!("@{login}")),
                    )
                    .children(mark.badge(cx))
                    .when(comment.resolved == Some(true), |header| {
                        header.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .text_color(green)
                                .child(Icon::new("check").size(12.0))
                                .child(cx.t("pullRequests.code.resolved")),
                        )
                    }),
            )
            // The comment as the review drawer shows it: markdown, its
            // severity as a chip (`comment_view::comment_body`).
            .child(div().mt(px(6.0)).min_w_0().child(
                crate::screens::code_column::review::comment_view::comment_body(
                    &comment.body,
                    &format!("pr-thread-body-{id}"),
                    window,
                    cx,
                ),
            ))
            .children(actions)
            .children(reply_box)
            .into_any_element()
    }
}
