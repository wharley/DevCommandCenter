//! The chosen pull request (`dcc-pr-detail`): "Voltar" when narrow, the
//! Resumo | Código tabs, "Abrir tarefa" or "Trabalhar nisto", open on the forge, the
//! title and who opened it; then the summary — branch → base, reviewers,
//! comments and checks, the description, the checks one by one, the
//! activity in markdown — and the comment box at the foot.

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::PrHub;
use super::code::Tab;
use super::list::{author_avatar, check_icon};
use crate::data::pr_hub::relative_date;
use crate::i18n::{I18n, Locale, Translate};
use crate::markdown::{MarkdownStyle, parse, render_markdown};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_srgb;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, branch_context, card_shadow, spinner, surface_edge,
};

/// `dcc-pr-summary-card`, its title with the calm dot.
fn summary_card(title: SharedString, body: AnyElement, cx: &gpui::App) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    div()
        .p(px(18.0))
        .border_1()
        .border_color(surface_edge(theme))
        .rounded(px(18.0))
        .bg(mix_srgb(colors.card, 0.45, colors.background))
        .shadow(card_shadow())
        .child(
            div()
                .mb(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    div()
                        .size(px(5.0))
                        .rounded_full()
                        .bg(colors.accent_calm.opacity(0.65)),
                )
                .child(title),
        )
        .child(body)
        .into_any_element()
}

impl PrHub {
    /// A segment of "Resumo | Código".
    fn tab_button(&self, tab: Tab, label: &str, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let chosen = self.tab == tab;
        let selector = format!(
            "pr-tab-{}",
            if tab == Tab::Code { "code" } else { "summary" }
        );
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector)
            .rounded(px(6.0))
            .border_1()
            .px(px(11.0))
            .py(px(5.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .map(|button| {
                if chosen {
                    button
                        .bg(colors.background)
                        .shadow_sm()
                        .border_color(colors.border.opacity(0.7))
                } else {
                    button
                        .border_color(gpui::transparent_black())
                        .text_color(colors.muted_foreground)
                }
            })
            .on_click(cx.listener(move |hub, _, _, cx| hub.set_tab(tab, cx)))
            .child(cx.t(label))
            .into_any_element()
    }

    pub(super) fn detail_column(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let Some(item) = self.selected_item(cx) else {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("pullRequests.selectPrompt"))
                .into_any_element();
        };
        let english = cx.global::<I18n>().locale() == Locale::En;
        let now = crate::data::time::now_ms();
        let narrow_detail = self.width.get() < 620.0 + 350.0;
        let pad = if narrow_detail { 16.0 } else { 28.0 };
        let detail = self
            .detail
            .as_ref()
            .filter(|(id, _, _)| *id == item.id)
            .and_then(|(_, _, result)| result.clone());
        let loading = detail.is_none();
        let working = self.working_on.as_deref() == Some(item.id.as_str());
        let url = item.url.clone();
        let task = item.linked_workspace_id.clone();
        let header = div()
            .debug_selector(|| "pr-detail-header".into())
            .flex_none()
            .border_b_1()
            .border_color(colors.border.opacity(0.7))
            .bg(gpui::linear_gradient(
                120.0,
                gpui::linear_color_stop(mix_srgb(colors.accent_calm, 0.03, colors.background), 0.0),
                gpui::linear_color_stop(colors.background, 0.7),
            ))
            .px(px(pad))
            .pb(px(16.0))
            .pt(px(12.0))
            .when(self.narrow(), |header| {
                header.child(
                    div()
                        .id("pr-back")
                        .debug_selector(|| "pr-back".into())
                        .mb(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .py(px(4.0))
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .cursor_pointer()
                        .on_click(cx.listener(|hub, _, _, cx| {
                            hub.detail_open = false;
                            cx.notify();
                        }))
                        .child(Icon::new("arrow-left").size(14.0))
                        .child(cx.t("review.backToList")),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .rounded(px(8.0))
                            .bg(colors.muted.opacity(0.5))
                            .p(px(4.0))
                            .children([
                                self.tab_button(Tab::Summary, "pullRequests.tabs.summary", cx),
                                self.tab_button(Tab::Code, "pullRequests.tabs.code", cx),
                            ]),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(match task {
                                Some(task) => Button::new("pr-open-task")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .text_size(12.0)
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.emit(super::PrHubEvent::OpenTask(task.clone()))
                                    }))
                                    .child(cx.t("pullRequests.openTask"))
                                    .into_any_element(),
                                None => Button::new("pr-work-on")
                                    .size(ButtonSize::Sm)
                                    .text_size(12.0)
                                    .disabled(self.working_on.is_some())
                                    .on_click(cx.listener(|hub, _, _, cx| hub.work_on(cx)))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(if working {
                                                spinner("pr-working", 14.0).into_any_element()
                                            } else {
                                                Icon::new("code-xml").size(14.0).into_any_element()
                                            })
                                            .child(cx.t("pullRequests.workOn")),
                                    )
                                    .into_any_element(),
                            })
                            .children(self.merge_button(&item, cx))
                            .child(
                                Button::new("pr-open-forge")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::IconSm)
                                    .on_click(move |_, _, cx| cx.open_url(&url))
                                    .child(Icon::new("arrow-up-right").size(14.0)),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(if narrow_detail { 12.0 } else { 20.0 }))
                    .max_w(px(896.0))
                    .text_size(px(if narrow_detail { 17.0 } else { 20.0 }))
                    .line_height(px(28.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(item.title.clone()),
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
                    .child(author_avatar(&item, cx))
                    .child(
                        item.author
                            .as_ref()
                            .map(|actor| {
                                actor
                                    .name
                                    .clone()
                                    .filter(|name| !name.is_empty())
                                    .unwrap_or(actor.login.clone())
                            })
                            .unwrap_or_default(),
                    )
                    .child("·")
                    .child(format!("{} #{}", item.repository_name, item.number))
                    .child("·")
                    .child(relative_date(item.updated_at.as_deref(), now, english)),
            );
        let reviewers = if item.reviewers.is_empty() {
            cx.t("pullRequests.noReviewers").to_string()
        } else {
            item.reviewers
                .iter()
                .map(|reviewer| format!("@{}", reviewer.login))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let comments = detail
            .as_ref()
            .and_then(|detail| detail.as_ref().ok())
            .map_or(item.comment_count as usize, |detail| detail.comments.len());
        let check_label = |state: &str, cx: &gpui::App| {
            let key = format!("pullRequests.checkStates.{state}");
            let text = cx.t(&key);
            if text.as_ref() == key {
                state.to_string()
            } else {
                text.to_string()
            }
        };
        let fact = |icon: AnyElement, label: SharedString, value: String| {
            [
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_color(colors.muted_foreground)
                    .child(icon)
                    .child(label)
                    .into_any_element(),
                div().min_w_0().child(value).into_any_element(),
            ]
        };
        let facts = div()
            .debug_selector(|| "pr-facts".into())
            .px(px(4.0))
            .grid()
            .grid_cols(2)
            .gap_x(px(16.0))
            .gap_y(px(12.0))
            .text_size(px(12.0))
            .children(fact(
                Icon::new("user-round-check").size(14.0).into_any_element(),
                cx.t("pullRequests.reviewers"),
                reviewers,
            ))
            .children(fact(
                Icon::new("message-square").size(14.0).into_any_element(),
                cx.t("pullRequests.comments"),
                comments.to_string(),
            ))
            .children(fact(
                check_icon(&item.checks_state),
                cx.t("pullRequests.checks"),
                check_label(&item.checks_state, cx),
            ));
        let description = detail
            .as_ref()
            .and_then(|detail| detail.as_ref().ok())
            .and_then(|detail| detail.body.clone())
            .filter(|body| !body.is_empty())
            .or_else(|| item.body.clone().filter(|body| !body.is_empty()))
            .unwrap_or_else(|| cx.t("pullRequests.noDescription").to_string());
        let checks: AnyElement = match &detail {
            None => spinner("pr-checks-loading", 16.0).into_any_element(),
            Some(Ok(detail)) if !detail.checks.is_empty() => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(detail.checks.iter().map(|check| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.6))
                        .px(px(12.0))
                        .py(px(8.0))
                        .text_size(px(12.0))
                        .child(check_icon(&check.state))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(check.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(colors.muted_foreground)
                                .child(check_label(&check.state, cx)),
                        )
                }))
                .into_any_element(),
            _ => div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("pullRequests.noChecks"))
                .into_any_element(),
        };
        let activity: AnyElement = match &detail {
            Some(Err(error)) => div()
                .text_size(px(11.0))
                .text_color(tw::RED_500.hsla())
                .child(if error.is_empty() {
                    cx.t("pullRequests.detailError").to_string()
                } else {
                    error.clone()
                })
                .into_any_element(),
            Some(Ok(detail)) if !detail.comments.is_empty() => div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .children(
                    detail
                        .comments
                        .iter()
                        .map(|entry| {
                            let blocks = parse(&entry.body);
                            div()
                                .rounded(px(12.0))
                                .border_1()
                                .border_color(colors.border.opacity(0.7))
                                .bg(colors.card.opacity(0.35))
                                .p(px(12.0))
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
                                                .child(format!(
                                                    "@{}",
                                                    entry.author.as_ref().map_or_else(
                                                        || cx
                                                            .t("pullRequests.unknownAuthor")
                                                            .to_string(),
                                                        |actor| actor.login.clone()
                                                    )
                                                )),
                                        )
                                        .child("·")
                                        .child(relative_date(
                                            entry.created_at.as_deref(),
                                            now,
                                            english,
                                        )),
                                )
                                .child(div().mt(px(8.0)).min_w_0().overflow_hidden().child(
                                    render_markdown(
                                        &blocks,
                                        MarkdownStyle::reply(),
                                        &format!("pr-comment-{}", entry.id),
                                        window,
                                        cx,
                                    ),
                                ))
                                .into_any_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_any_element(),
            _ => div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("pullRequests.noActivity"))
                .into_any_element(),
        };
        let summary = div()
            .id("pr-summary")
            .debug_selector(|| "pr-summary".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(pad))
            .py(px(20.0))
            .child(
                div()
                    .mx_auto()
                    .max_w(px(896.0))
                    .flex()
                    .flex_col()
                    .gap(px(24.0))
                    .child(branch_context(
                        if item.head_branch.is_empty() {
                            "—".into()
                        } else {
                            item.head_branch.clone()
                        },
                        Some(item.base_branch.clone()).filter(|base| !base.is_empty()),
                        cx,
                    ))
                    .child(facts)
                    .child(summary_card(
                        cx.t("pullRequests.description"),
                        div()
                            .text_size(px(12.0))
                            .line_height(px(20.0))
                            .text_color(colors.foreground.opacity(0.85))
                            .child(description)
                            .into_any_element(),
                        cx,
                    ))
                    .child(summary_card(cx.t("pullRequests.checks"), checks, cx))
                    .child(summary_card(cx.t("pullRequests.activity"), activity, cx)),
            );
        let empty = self.comment.read(cx).text().trim().is_empty();
        let footer = div()
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
                    .items_end()
                    .gap(px(8.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.muted.opacity(0.25))
                    .p(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "pr-comment".into())
                            .flex_1()
                            .min_w_0()
                            .min_h(px(48.0))
                            .text_size(px(12.0))
                            .child(self.comment.clone()),
                    )
                    .child(
                        Button::new("pr-send-comment")
                            .size(ButtonSize::IconSm)
                            .disabled(empty || self.sending)
                            .on_click(cx.listener(|hub, _, _, cx| hub.send_comment(cx)))
                            .child(if self.sending {
                                spinner("pr-sending", 14.0).into_any_element()
                            } else {
                                Icon::new("send").size(14.0).into_any_element()
                            }),
                    ),
            )
            .children(self.comment_error.clone().map(|error| {
                div()
                    .mx_auto()
                    .mt(px(6.0))
                    .max_w(px(896.0))
                    .text_size(px(10.0))
                    .text_color(tw::RED_500.hsla())
                    .child(error)
            }));
        let _ = loading;
        div()
            .debug_selector(|| "pr-detail".into())
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(header)
            .map(|column| match self.tab {
                Tab::Summary => column.child(summary).child(footer),
                Tab::Code => column.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .child(self.code_tab(&item, window, cx)),
                ),
            })
            .children(self.offer_element())
            .children(self.merge_dialog(cx))
            .into_any_element()
    }
}
