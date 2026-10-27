//! "Meus feedbacks": the issue just created, the hint with ⟳, the kept page
//! when GitHub could not answer, loading, the empty state, each issue (its
//! title, number, kind, date and status) and the pager.

use dcc_infra::feedback::FeedbackIssue;
use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::view::notice;
use super::{Feedback, Tab};
use crate::data::feedback::status;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::color::hex;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

impl Feedback {
    fn issue_row(&self, index: usize, issue: &FeedbackIssue, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let english = cx.global::<I18n>().locale() == Locale::En;
        let state = status(issue);
        // `.dcc-feedback-status[data-state]`
        let (chip_text, chip_background) = match state {
            "open" => (
                Some(hex(if dark { 0x86efac } else { 0x15803d })),
                hex(0x22c55e).opacity(0x15 as f32 / 255.0),
            ),
            "completed" => (
                Some(hex(if dark { 0xc4b5fd } else { 0x7c3aed })),
                hex(0x8b5cf6).opacity(0x15 as f32 / 255.0),
            ),
            _ => (None, colors.foreground.opacity(0.06)),
        };
        let number = issue.number;
        div()
            .id(("feedback-issue", index))
            .debug_selector(move || format!("feedback-issue-{index}"))
            .w_full()
            .p(px(14.0))
            .border_1()
            .border_color(colors.border)
            .rounded(px(12.0))
            .cursor_pointer()
            .hover(|row| row.bg(colors.accent))
            .on_click(cx.listener(move |feedback, _, _, cx| feedback.open_issue(Some(number), cx)))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .child(issue.title.clone()),
                    )
                    .child(
                        div().mt(px(2.0)).flex_none().child(
                            Icon::new("external-link")
                                .size(14.0)
                                .color(colors.muted_foreground),
                        ),
                    ),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(format!("#{number}"))
                    .child("·")
                    .child(cx.t(&format!("feedback.categories.{}", issue.category)))
                    .child(crate::data::time::local_date(&issue.created_at, english))
                    .child(
                        div()
                            .ml_auto()
                            .px(px(7.0))
                            .py(px(2.0))
                            .rounded(px(6.0))
                            .bg(chip_background)
                            .text_size(px(10.0))
                            .when_some(chip_text, |chip, color| chip.text_color(color))
                            .child(cx.t(&format!("feedback.status.{state}"))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn history_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let login = self.login();
        let sent = self.submitted.clone().map(|issue| {
            let number = issue.number;
            notice(Some(tw::EMERALD_600.hsla()), cx)
                .debug_selector(|| "feedback-sent".into())
                .child(Icon::new("check").size(16.0))
                .child(cx.t_with(
                    "feedback.sent",
                    &Args::default().with("number", number.to_string()),
                ))
                .child(
                    Button::new("feedback-sent-github")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(move |feedback, _, _, cx| {
                            feedback.open_issue(Some(number), cx)
                        }))
                        .child(cx.t("feedback.github"))
                        .child(Icon::new("external-link").size(13.0)),
                )
        });
        let data = self.history.data.clone();
        let failure = self.history.failed.then(|| {
            notice(None, cx).child(cx.t(if data.is_some() {
                "feedback.cached"
            } else {
                "feedback.errors.failed"
            }))
        });
        let loading = (self.history.fetching && data.is_none()).then(|| {
            notice(None, cx)
                .child(spinner("feedback-history-loading", 14.0))
                .child(cx.t("feedback.loading"))
        });
        let empty = data
            .as_ref()
            .filter(|page| page.issues.is_empty())
            .map(|_| {
                div()
                    .debug_selector(|| "feedback-empty".into())
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(42.0))
                    .text_center()
                    .text_color(colors.muted_foreground)
                    .child(Icon::new("message-square-more").size(30.0).stroke(1.4))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t("feedback.empty")),
                    )
                    .child(
                        div()
                            .max_w(px(320.0))
                            .text_size(px(12.0))
                            .line_height(px(19.2))
                            .child(cx.t("feedback.emptyHint")),
                    )
                    .child(
                        Button::new("feedback-empty-new")
                            .variant(ButtonVariant::Outline)
                            .on_click(
                                cx.listener(|feedback, _, _, cx| feedback.set_tab(Tab::New, cx)),
                            )
                            .child(cx.t("feedback.new")),
                    )
            });
        let rows: Vec<AnyElement> = data
            .as_ref()
            .map(|page| {
                page.issues
                    .iter()
                    .enumerate()
                    .map(|(index, issue)| self.issue_row(index, issue, cx))
                    .collect()
            })
            .unwrap_or_default();
        let has_next = data.as_ref().is_some_and(|page| page.has_next);
        let page = self.page;
        let pager = login.is_some().then(|| {
            div()
                .mt(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    Button::new("feedback-history-github")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|feedback, _, _, cx| feedback.open_issue(None, cx)))
                        .child(cx.t("feedback.github"))
                        .child(Icon::new("external-link").size(13.0)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(12.0))
                        .child(
                            Button::new("feedback-previous")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .disabled(page == 1 || self.history.fetching)
                                .on_click(cx.listener(move |feedback, _, _, cx| {
                                    feedback.set_page(page - 1, cx)
                                }))
                                .child(cx.t("feedback.previous")),
                        )
                        .child(page.to_string())
                        .child(
                            Button::new("feedback-next")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .disabled(!has_next || self.history.fetching)
                                .on_click(cx.listener(move |feedback, _, _, cx| {
                                    feedback.set_page(page + 1, cx)
                                }))
                                .child(cx.t("feedback.next")),
                        ),
                )
        });
        div()
            .children(sent)
            .child(
                div()
                    .mb(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("feedback.historyHint")),
                    )
                    .child(
                        Button::new("feedback-refresh")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconSm)
                            .disabled(login.is_none() || self.history.fetching)
                            .on_click(cx.listener(|feedback, _, _, cx| feedback.load_history(cx)))
                            .child(Icon::new("refresh-cw").size(14.0)),
                    ),
            )
            .children(failure)
            .children(loading)
            .children(empty)
            .child(div().flex().flex_col().gap(px(8.0)).children(rows))
            .children(pager)
            .into_any_element()
    }
}
