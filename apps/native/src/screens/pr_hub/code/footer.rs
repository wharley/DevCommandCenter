//! The review at the foot of the code tab (`dcc-pr-review-footer`): the
//! drafts waiting, who sends it, the event (Comentar, Aprovar, Solicitar
//! alterações, off with the reason), the text and send, and what came back.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px, relative};

use crate::data::pr_hub::{Detail, Item};
use crate::data::pr_review::{ReviewEvent, can_submit, event_blocked};
use crate::i18n::{Args, Translate};
use crate::screens::pr_hub::PrHub;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, Icon, Tooltip, spinner};

impl PrHub {
    pub(super) fn review_footer(
        &self,
        item: &Item,
        detail: &Detail,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let dark = theme.appearance == Appearance::Dark;
        let review = &self.review;
        let body = review.body.read(cx).text().to_string();
        let ready = can_submit(review.event, review.drafts.len(), &body, review.submitting);
        let events: Vec<AnyElement> = ReviewEvent::ALL
            .into_iter()
            .map(|event| {
                let blocked = event_blocked(event, item, detail.review_capabilities.as_ref());
                let chosen = review.event == event;
                let selector = format!("pr-review-event-{}", event.key());
                let fg = colors.foreground;
                let button = div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector)
                    .rounded(px(6.0))
                    .border_1()
                    .px(px(9.0))
                    .py(px(3.0))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .map(|button| {
                        if chosen {
                            button
                                .bg(colors.background)
                                .shadow_sm()
                                .border_color(colors.border.opacity(0.7))
                                .text_color(colors.foreground)
                        } else {
                            button
                                .border_color(gpui::transparent_black())
                                .text_color(colors.muted_foreground)
                        }
                    })
                    .map(|button| match blocked {
                        Some(_) => button.opacity(0.35),
                        None => button
                            .cursor_pointer()
                            .hover(move |button| button.text_color(fg))
                            .on_click(cx.listener(move |hub, _, _, cx| hub.set_event(event, cx))),
                    })
                    .child(cx.t(&format!("pullRequests.code.reviewEvents.{}", event.key())));
                match blocked {
                    Some(reason) => {
                        let reason = cx.t(reason);
                        Tooltip::new(
                            SharedString::from(format!("pr-review-event-tip-{}", event.key())),
                            button,
                            move |_, _| div().child(reason.clone()).into_any_element(),
                        )
                        .into_any_element()
                    }
                    None => button.into_any_element(),
                }
            })
            .collect();
        let line = |color: gpui::Hsla, icon: &'static str, text: String| {
            div()
                .mt(px(8.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(10.0))
                .text_color(color)
                .child(Icon::new(icon).size(12.0))
                .child(text)
        };
        let red = tw::RED_500.hsla();
        div()
            .debug_selector(|| "pr-review-footer".into())
            .id("pr-review-footer")
            .flex_none()
            .border_t_1()
            .border_color(colors.border)
            .bg(colors.background)
            .px(px(16.0))
            .py(px(12.0))
            .when(narrow, |footer| {
                footer.max_h(relative(0.42)).overflow_y_scroll()
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Icon::new("message-square-plus")
                            .size(16.0)
                            .color(colors.muted_foreground),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t("pullRequests.code.submitReview")),
                    )
                    .when(!review.drafts.is_empty(), |row| {
                        row.child(
                            div()
                                .debug_selector(|| "pr-review-pending".into())
                                .rounded_full()
                                .bg(tw::AMBER_500.hsla().opacity(0.1))
                                .px(px(8.0))
                                .py(px(2.0))
                                .text_size(px(9.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if dark {
                                    tw::AMBER_400.hsla()
                                } else {
                                    tw::AMBER_600.hsla()
                                })
                                .child(cx.t_with(
                                    "pullRequests.code.pendingCount",
                                    &Args::count(review.drafts.len() as i64),
                                )),
                        )
                    })
                    .children(item.forge_login.as_ref().map(|login| {
                        div()
                            .text_size(px(9.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t_with(
                                "pullRequests.code.submittingAs",
                                &Args::default().with("login", login.clone()),
                            ))
                    }))
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .rounded(px(8.0))
                            .bg(colors.muted.opacity(0.5))
                            .p(px(4.0))
                            .children(events),
                    ),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .items_end()
                    .gap(px(8.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.muted.opacity(0.25))
                    .p(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "pr-review-body".into())
                            .flex_1()
                            .min_w_0()
                            .min_h(px(56.0))
                            .text_size(px(11.0))
                            .child(review.body.clone()),
                    )
                    .child(
                        Button::new("pr-review-send")
                            .size(ButtonSize::IconSm)
                            .disabled(!ready)
                            .on_click(cx.listener(|hub, _, _, cx| hub.submit_review(cx)))
                            .child(if review.submitting {
                                spinner("pr-review-sending", 14.0).into_any_element()
                            } else {
                                Icon::new("send").size(14.0).into_any_element()
                            }),
                    ),
            )
            .children(review.submit_error.clone().map(|error| {
                line(
                    red,
                    "circle-alert",
                    if error.is_empty() {
                        cx.t("pullRequests.code.reviewError").to_string()
                    } else {
                        error
                    },
                )
            }))
            .children(
                review
                    .thread_error
                    .clone()
                    .map(|error| line(red, "circle-alert", error)),
            )
            .children(
                review
                    .warning
                    .clone()
                    .map(|warning| line(tw::AMBER_500.hsla(), "circle-alert", warning)),
            )
            .when(review.submitted, |footer| {
                footer.child(line(
                    tw::EMERALD_500.hsla(),
                    "check",
                    cx.t("pullRequests.code.reviewSent").to_string(),
                ))
            })
            .into_any_element()
    }
}
