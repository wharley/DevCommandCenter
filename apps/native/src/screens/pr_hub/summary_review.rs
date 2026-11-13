//! The summary's footer: the verdict, the text and a send button that says
//! what it sends, so a pull request can be approved without opening its
//! code. "Comentar" is a comment on the pull request, as before; "Aprovar"
//! and "Solicitar alterações" send the text as the review, with the line
//! comments waiting in "Código".

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::PrHub;
use crate::data::pr_hub::Item;
use crate::data::pr_review::{ReviewEvent, can_submit};
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, StateTone};
use crate::ui::{Button, ButtonSize, Icon, spinner};

impl PrHub {
    /// Whether the summary's send button has something to send.
    fn summary_ready(&self, cx: &gpui::App) -> bool {
        let text = self.comment.read(cx).text().to_string();
        match self.review.event {
            ReviewEvent::Comment => !text.trim().is_empty() && !self.sending,
            event => can_submit(
                event,
                self.review.drafts.len(),
                &text,
                self.review.submitting,
            ),
        }
    }

    /// The summary's send: a comment, or the review with this text.
    pub(super) fn send_from_summary(&mut self, cx: &mut Context<Self>) {
        if !self.summary_ready(cx) {
            return;
        }
        if self.review.event == ReviewEvent::Comment {
            return self.send_comment(cx);
        }
        // The review's own field carries it: a failed send keeps the text
        // there, in "Código".
        let text = self.comment.read(cx).text().to_string();
        let (body, comment) = (self.review.body.clone(), self.comment.clone());
        body.update(cx, |area, cx| area.set_text(&text, cx));
        comment.update(cx, |area, cx| area.set_text("", cx));
        self.submit_review(cx);
    }

    pub(super) fn summary_footer(
        &self,
        item: &Item,
        pad: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let capabilities = self
            .merge_detail(item)
            .and_then(|detail| detail.as_ref().ok())
            .and_then(|detail| detail.review_capabilities.as_ref());
        let events = self.review_event_buttons(item, capabilities, cx);
        let ready = self.summary_ready(cx);
        let busy = self.sending || self.review.submitting;
        let pending = self.review.drafts.len();
        let error = self
            .comment_error
            .clone()
            .or_else(|| self.review.submit_error.clone());
        div()
            .debug_selector(|| "pr-summary-footer".into())
            .flex_none()
            .border_t_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.background.opacity(0.95))
            .px(px(pad))
            .py(px(12.0))
            .child(
                div()
                    .mx_auto()
                    .max_w(px(896.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.muted.opacity(0.25))
                    .p(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "pr-comment".into())
                            .w_full()
                            .min_w_0()
                            .min_h(px(36.0))
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .child(self.comment.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .rounded(px(8.0))
                                    .bg(colors.muted.opacity(0.5))
                                    .p(px(4.0))
                                    .children(events),
                            )
                            // The line comments waiting in "Código" go with
                            // a review, not with a plain comment.
                            .when(pending > 0, |row| {
                                row.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(StateTone::NeedsYou.text(&theme))
                                        .child(cx.t_with(
                                            "pullRequests.code.pendingCount",
                                            &Args::count(pending as i64),
                                        )),
                                )
                            })
                            .child(
                                div().ml_auto().child(
                                    Button::new("pr-send-comment")
                                        .size(ButtonSize::Sm)
                                        .text_size(12.0)
                                        .disabled(!ready)
                                        .on_click(
                                            cx.listener(|hub, _, _, cx| hub.send_from_summary(cx)),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(6.0))
                                                .child(if busy {
                                                    spinner("pr-sending", 14.0).into_any_element()
                                                } else {
                                                    Icon::new("send").size(13.0).into_any_element()
                                                })
                                                .child(cx.t(&format!(
                                                    "pullRequests.code.sendEvents.{}",
                                                    self.review.event.key()
                                                ))),
                                        ),
                                ),
                            ),
                    ),
            )
            .children(error.map(|error| {
                div()
                    .mx_auto()
                    .mt(px(6.0))
                    .max_w(px(896.0))
                    .text_size(px(11.0))
                    .text_color(StateTone::Failed.text(&theme))
                    .child(if error.is_empty() {
                        cx.t("pullRequests.code.reviewError").to_string()
                    } else {
                        error
                    })
            }))
            .into_any_element()
    }
}
