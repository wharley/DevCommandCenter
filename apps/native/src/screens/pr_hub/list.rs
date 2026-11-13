//! The hub's frame and its list column (`dcc-pr-list`): the title with
//! "Atualizar", the three filters with how many each holds, the search,
//! then a row per pull request — its state in one icon, the title, and
//! "repo #n · author · age" — or loading, the error, or "nenhum PR"; a
//! warning when some projects did not answer.

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, canvas, div, prelude::*, px};

use super::PrHub;
use crate::data::pr_hub::{Filter, Item, filtered, relative_date};
use crate::data::pr_row_state::row_state;
use crate::data::pr_sections::Section;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::color::mix_srgb;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, StateTone};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

/// `CheckState`: a check, an alert, a clock or a dot.
pub(super) fn check_icon(state: &str) -> AnyElement {
    match state {
        "success" => Icon::new("check")
            .size(14.0)
            .stroke(2.2)
            .color(tw::EMERALD_500.hsla()),
        "failure" => Icon::new("circle-alert")
            .size(14.0)
            .stroke(2.0)
            .color(tw::RED_500.hsla()),
        "pending" => Icon::new("clock-3")
            .size(14.0)
            .stroke(2.0)
            .color(tw::AMBER_500.hsla()),
        _ => Icon::new("circle-dot").size(14.0).stroke(1.8),
    }
    .into_any_element()
}

/// `ActorAvatar`: the picture, else the first two letters.
pub(super) fn author_avatar(item: &Item, cx: &mut Context<PrHub>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let author = item.author.as_ref();
    let label = author
        .and_then(|actor| actor.name.clone().filter(|name| !name.is_empty()))
        .or_else(|| author.map(|actor| actor.login.clone()))
        .unwrap_or_else(|| "?".into());
    let picture =
        crate::data::avatar::avatar(author.and_then(|actor| actor.avatar_url.as_deref()), cx);
    div()
        .flex()
        .flex_none()
        .size(px(20.0))
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded_full()
        .border_1()
        .border_color(colors.border)
        .bg(colors.muted)
        .text_size(px(9.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.muted_foreground)
        .map(|frame| match picture {
            Some(image) => frame.child(gpui::img(image).size_full()),
            None => frame.child(label.chars().take(2).collect::<String>().to_uppercase()),
        })
        .into_any_element()
}

impl PrHub {
    pub(super) fn render_hub(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let narrow = self.narrow();
        let selected = self.selected_item(cx).is_some();
        let show_list = !(narrow && self.detail_open && selected);
        let show_detail = !narrow || (self.detail_open && selected);
        let width = self.width.clone();
        let this = cx.entity().downgrade();
        let list_width = (width.get() * 0.3).clamp(285.0, 350.0);
        div()
            .debug_selector(|| "pr-hub".into())
            .relative()
            .flex()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .bg(colors.background)
            // The container query: the width as it paints.
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let measured = f32::from(bounds.size.width);
                        let before = width.get();
                        width.set(measured);
                        if (before < super::NARROW) != (measured < super::NARROW)
                            || (before * 0.3).clamp(285.0, 350.0)
                                != (measured * 0.3).clamp(285.0, 350.0)
                        {
                            let _ = this.update(cx, |_, cx| cx.notify());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(show_list, |hub| {
                hub.child(self.list_column(
                    if narrow { None } else { Some(list_width) },
                    window,
                    cx,
                ))
            })
            .when(show_detail, |hub| hub.child(self.detail_column(window, cx)))
            .into_any_element()
    }

    fn filters(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let edge = colors.border.opacity(0.7);
        let buttons: Vec<AnyElement> = Filter::ALL
            .into_iter()
            .map(|filter| {
                let on = self.filter == filter;
                // How many it holds, whatever is being searched.
                let count = filtered(self.items(), filter, "", |item| {
                    crate::data::pr_news::reviewed(item, cx)
                })
                .len();
                div()
                    .id(SharedString::from(format!("pr-filter-{}", filter.key())))
                    .debug_selector(move || format!("pr-filter-{}", filter.key()))
                    .flex()
                    .flex_grow(match filter {
                        Filter::All => 0.8,
                        Filter::Reviewing => 1.0,
                        Filter::Mine => 1.45,
                    })
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .justify_center()
                    .rounded(px(6.0))
                    .px(px(6.0))
                    .py(px(6.0))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .map(|button| {
                        if on {
                            button
                                .bg(colors.background)
                                .text_color(colors.foreground)
                                .shadow_sm()
                                .border_1()
                                .border_color(edge)
                        } else {
                            button
                                .text_color(colors.muted_foreground)
                                .hover(|button| button.text_color(colors.foreground))
                        }
                    })
                    .on_click(cx.listener(move |hub, _, _, cx| hub.set_filter(filter, cx)))
                    .gap(px(5.0))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(cx.t(&format!("pullRequests.filters.{}", filter.key()))),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(colors.muted_foreground)
                            .child(count.to_string()),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .mb(px(12.0))
            .flex()
            .gap(px(4.0))
            .rounded(px(8.0))
            .bg(colors.muted.opacity(0.55))
            .p(px(4.0))
            .children(buttons)
            .into_any_element()
    }

    /// A pull request's row: its state in one icon, the title, then
    /// "repo #n · author · age"; a third line only for news since the
    /// person's review.
    fn card(
        &self,
        item: &Item,
        section: Option<Section>,
        english: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = &theme.colors;
        let selected = self.selected.as_deref() == Some(item.id.as_str());
        let now = crate::data::time::now_ms();
        let id = item.id.clone();
        let mine = crate::data::pr_news::my_review(item, cx);
        let state = row_state(item, mine.as_deref());
        let tone = state.tone().text(&theme);
        let author = item
            .author
            .as_ref()
            .map(|actor| actor.login.clone())
            .unwrap_or_else(|| cx.t("pullRequests.unknownAuthor").to_string());
        let age = relative_date(item.updated_at.as_deref(), now, english);
        let icon = Tooltip::text(
            SharedString::from(format!("pr-state-tip-{}", item.id)),
            div()
                .id(SharedString::from(format!("pr-state-{}", item.id)))
                .debug_selector({
                    let id = item.id.clone();
                    move || format!("pr-state-{id}")
                })
                .mt(px(1.0))
                .flex_none()
                .child(Icon::new(state.icon()).size(15.0).stroke(1.9).color(tone)),
            cx.t(&format!("pullRequests.rowState.{}", state.key())),
        );
        div()
            .id(SharedString::from(format!("pr-card-{}", item.id)))
            .debug_selector({
                let id = item.id.clone();
                move || format!("pr-card-{id}")
            })
            .w_full()
            .mb(px(2.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .rounded(px(8.0))
            .px(px(10.0))
            .py(px(8.0))
            .cursor_pointer()
            .map(|card| {
                if selected {
                    card.bg(mix_srgb(colors.accent_calm, 0.1, colors.background))
                } else {
                    card.hover(|card| card.bg(colors.accent.opacity(0.4)))
                }
            })
            .on_click(cx.listener(move |hub, _, _, cx| hub.select(id.clone(), cx)))
            .child(icon)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .line_clamp(2)
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(item.title.clone()),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .child(format!("{} #{}", item.repository_name, item.number)),
                            )
                            .child(div().flex_none().child("·"))
                            .child(div().min_w_0().truncate().child(author))
                            .child(div().flex_none().child("·"))
                            .child(div().flex_none().child(age)),
                    )
                    .children(review_row(item, section, mine.as_deref(), cx)),
            )
            .into_any_element()
    }

    fn list_column(
        &mut self,
        width: Option<f32>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let english = cx.global::<I18n>().locale() == Locale::En;
        let visible = self.visible(cx);
        let body: AnyElement = match &self.list {
            None => div()
                .flex()
                .h(px(192.0))
                .items_center()
                .justify_center()
                .text_color(colors.muted_foreground)
                .child(spinner("pr-list-loading", 20.0))
                .into_any_element(),
            Some(Err(error)) => div()
                .m(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(tw::RED_500.hsla().opacity(0.2))
                .bg(tw::RED_500.hsla().opacity(0.05))
                .p(px(12.0))
                .text_size(px(12.0))
                .text_color(if dark { tw::RED_400 } else { tw::RED_600 }.hsla())
                .child(if error.is_empty() {
                    cx.t("pullRequests.loadError").to_string()
                } else {
                    error.clone()
                })
                .into_any_element(),
            Some(Ok(_)) if visible.is_empty() => div()
                .debug_selector(|| "pr-list-empty".into())
                .flex()
                .flex_col()
                .h(px(192.0))
                .items_center()
                .justify_center()
                .px(px(24.0))
                .text_center()
                .child(
                    Icon::new("git-pull-request")
                        .size(28.0)
                        .color(colors.muted_foreground.opacity(0.6)),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("pullRequests.empty")),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(11.0))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("pullRequests.emptyHint")),
                )
                .into_any_element(),
            Some(Ok(_)) => {
                // Each item's section once; a section's count from them.
                let sections: Vec<_> = visible
                    .iter()
                    .map(|item| self.section_of(item, cx))
                    .collect();
                let mut rows: Vec<AnyElement> = Vec::new();
                let mut current = None;
                for (item, &section) in visible.iter().zip(&sections) {
                    if section != current {
                        current = section;
                        if let Some(section) = section {
                            let count = sections
                                .iter()
                                .filter(|other| **other == Some(section))
                                .count();
                            rows.push(self.section_header(section, count, cx));
                        }
                    }
                    if section.is_some_and(|section| self.section_folded(section)) {
                        continue;
                    }
                    rows.push(self.card(item, section, english, cx));
                }
                div().flex().flex_col().children(rows).into_any_element()
            }
        };
        let warnings = match &self.list {
            Some(Ok(list)) if !list.warnings.is_empty() => Some(list.warnings.len()),
            _ => None,
        };
        let fetching = self.fetching;
        div()
            .debug_selector(|| "pr-list".into())
            .flex()
            .flex_col()
            .flex_none()
            .h_full()
            .min_w_0()
            .map(|column| match width {
                Some(width) => column
                    .w(px(width))
                    .border_r_1()
                    .border_color(colors.border.opacity(0.7)),
                None => column.w_full(),
            })
            .bg(mix_srgb(colors.sidebar, 0.6, colors.background))
            .child(
                div()
                    .flex_none()
                    .border_b_1()
                    .border_color(colors.border.opacity(0.7))
                    .px(px(16.0))
                    .py(px(14.0))
                    .child(
                        div()
                            .mb(px(12.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .child(
                                        div()
                                            .text_size(px(17.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(cx.t("pullRequests.title")),
                                    )
                                    .child(
                                        div()
                                            .mt(px(2.0))
                                            .text_size(px(11.0))
                                            .text_color(colors.muted_foreground)
                                            .child(cx.t("pullRequests.subtitle")),
                                    ),
                            )
                            .child(Tooltip::text(
                                "pr-refresh-tip",
                                Button::new("pr-refresh")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::IconSm)
                                    .disabled(fetching)
                                    .on_click(cx.listener(|hub, _, _, cx| hub.refresh(cx)))
                                    .child(if fetching {
                                        spinner("pr-refreshing", 14.0).into_any_element()
                                    } else {
                                        Icon::new("refresh-cw").size(14.0).into_any_element()
                                    }),
                                cx.t("pullRequests.refresh"),
                            )),
                    )
                    .child(self.filters(cx))
                    .child(
                        div()
                            .debug_selector(|| "pr-search".into())
                            .flex()
                            .h(px(32.0))
                            .items_center()
                            .gap(px(8.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.border.opacity(0.8))
                            .bg(colors.background.opacity(0.8))
                            .px(px(10.0))
                            .text_size(px(12.0))
                            .child(
                                Icon::new("search")
                                    .size(14.0)
                                    .color(colors.muted_foreground),
                            )
                            .child(div().flex_1().min_w_0().child(self.search.clone())),
                    ),
            )
            .child(
                div()
                    .id("pr-list-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(8.0))
                    .child(body),
            )
            .children(warnings.map(|count| {
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(colors.border.opacity(0.7))
                    .px(px(12.0))
                    .py(px(8.0))
                    .text_size(px(10.0))
                    .text_color(if dark { tw::AMBER_400 } else { tw::AMBER_600 }.hsla())
                    .child(cx.t_with("pullRequests.partialWarning", &Args::count(count as i64)))
            }))
            .into_any_element()
    }
}

/// Under a pull request the person reviewed: what came since ("2 commits
/// novos · @daniel respondeu no PR") until they open it again, in amber,
/// after their own review ("Você aprovou") where the section does not
/// already say it.
fn review_row(
    item: &Item,
    section: Option<Section>,
    mine: Option<&str>,
    cx: &mut Context<PrHub>,
) -> Option<AnyElement> {
    let state = mine?;
    let theme = cx.theme().clone();
    let said_by_section = matches!(section, Some(Section::Approved | Section::WaitingOnAuthor));
    let mine = (!said_by_section).then(|| {
        let key = match state {
            "APPROVED" => "approved",
            "CHANGES_REQUESTED" => "changesRequested",
            "DISMISSED" => "dismissed",
            _ => "commented",
        };
        let id = item.id.clone();
        div()
            .debug_selector(move || format!("pr-my-review-{id}"))
            .flex_none()
            .text_color(theme.colors.muted_foreground)
            .child(cx.t(&format!("pullRequests.myReview.{key}")))
    });
    let news = crate::data::pr_news::is_new(item, cx)
        .then(|| crate::data::pr_news::item_news_text(item, cx))
        .flatten()
        .map(|text| {
            let id = item.id.clone();
            div()
                .debug_selector(move || format!("pr-news-{id}"))
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(StateTone::NeedsYou.text(&theme))
                .child(text)
        });
    if mine.is_none() && news.is_none() {
        return None;
    }
    Some(
        div()
            .mt(px(2.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.0))
            .line_height(px(15.0))
            .children(mine)
            .children(news)
            .into_any_element(),
    )
}
