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
    /// "Comentar | Aprovar | Solicitar alterações": the verdict's segments,
    /// a blocked one dimmed with its reason on hover.
    pub(in crate::screens::pr_hub) fn review_event_buttons(
        &self,
        item: &Item,
        capabilities: Option<&crate::data::pr_review::ReviewCapabilities>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let review = &self.review;
        ReviewEvent::ALL
            .into_iter()
            .map(|event| {
                let blocked = event_blocked(event, item, capabilities);
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
                    .text_size(px(11.0))
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
            .collect()
    }

    pub(super) fn review_footer(
        &self,
        item: &Item,
        detail: &Detail,
        narrow: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let dark = theme.appearance == Appearance::Dark;
        let review = &self.review;
        let body = review.body.read(cx).text().to_string();
        let ready = can_submit(review.event, review.drafts.len(), &body, review.submitting);
        // Nothing to preview: back to writing.
        let preview = review.preview && !body.trim().is_empty();
        // The pull request's files as chips that open them above.
        let mut preview_blocks = Vec::new();
        if preview {
            preview_blocks = crate::markdown::parse(&body);
            let paths: Vec<String> = detail.files.iter().map(|file| file.path.clone()).collect();
            crate::markdown::files::mark(&mut preview_blocks, &paths);
        }
        let mode = |id: &'static str, key: &str, on: bool, to: bool, cx: &mut Context<Self>| {
            let fg = colors.foreground;
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .rounded(px(6.0))
                .px(px(8.0))
                .py(px(3.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .map(|button| {
                    if on {
                        button.bg(colors.muted.opacity(0.7)).text_color(fg)
                    } else {
                        button
                            .text_color(colors.muted_foreground)
                            .hover(move |button| button.text_color(fg))
                    }
                })
                .on_click(cx.listener(move |hub, _, _, cx| {
                    hub.review.preview = to;
                    cx.notify();
                }))
                .child(cx.t(key))
        };
        let modes = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(mode(
                "pr-review-write",
                "pullRequests.code.write",
                !preview,
                false,
                cx,
            ))
            .child(mode(
                "pr-review-preview-tab",
                "pullRequests.code.preview",
                preview,
                true,
                cx,
            ));
        let events = self.review_event_buttons(item, detail.review_capabilities.as_ref(), cx);
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
                    // The verdict first: it names the send button below.
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
                    .children(review.verdict.map(|verdict| {
                        crate::ui::verdict_chip::verdict_chip("pr-review-verdict", verdict, cx)
                    }))
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
                    .child(div().ml_auto().child(self.agent_button(cx))),
            )
            .child(
                div()
                    .mt(px(8.0))
                    // The text has the field's whole width; the button sits
                    // under it, so a labelled button never narrows the text.
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.muted.opacity(0.25))
                    .p(px(8.0))
                    .child(if preview {
                        div()
                            .id("pr-review-preview")
                            .debug_selector(|| "pr-review-preview".into())
                            .w_full()
                            .min_w_0()
                            .min_h(px(36.0))
                            .max_h(px(220.0))
                            .overflow_y_scroll()
                            .text_size(px(12.0))
                            .child(crate::markdown::render_markdown(
                                &preview_blocks,
                                crate::markdown::MarkdownStyle::card(),
                                "pr-review-preview-text",
                                window,
                                cx,
                            ))
                            .into_any_element()
                    } else {
                        div()
                            .debug_selector(|| "pr-review-body".into())
                            .w_full()
                            .min_w_0()
                            // Shorter while empty: the button's row is new.
                            .min_h(px(36.0))
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .child(review.body.clone())
                            .into_any_element()
                    })
                    // "Escrever | Prévia", then the button that says what
                    // it sends.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(modes)
                            .child(
                                Button::new("pr-review-send")
                                    .size(ButtonSize::Sm)
                                    .text_size(12.0)
                                    .disabled(!ready)
                                    .on_click(cx.listener(|hub, _, _, cx| hub.submit_review(cx)))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(if review.submitting {
                                                spinner("pr-review-sending", 14.0)
                                                    .into_any_element()
                                            } else {
                                                Icon::new("send").size(13.0).into_any_element()
                                            })
                                            .child(cx.t(&format!(
                                                "pullRequests.code.sendEvents.{}",
                                                review.event.key()
                                            ))),
                                    ),
                            ),
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
                    .agent_error
                    .clone()
                    .or_else(|| review.thread_error.clone())
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
