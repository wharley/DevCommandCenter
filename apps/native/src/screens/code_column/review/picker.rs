//! `RequestReviewPicker`: "Pedir revisão" opens a search over who can
//! review (250ms after typing stops), suggested people first; a pick asks
//! them.

use std::time::Duration;

use gpui::{
    AnyElement, AppContext, Context, Focusable, FontWeight, SharedString, Window, div, prelude::*,
    px,
};

use super::super::CodeColumn;
use super::Picker;
use super::state::person;
use crate::data::forge::{ReviewerAction, ReviewerCandidate, group_candidates};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::popover::Popover;
use crate::ui::popover::{PopoverAlign, PopoverSide};
use crate::ui::{Icon, TextInput, TextInputEvent, spinner};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

impl CodeColumn {
    pub(crate) fn set_picker_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !open {
            self.review.picker = None;
            cx.notify();
            return;
        }
        if self.review.picker.is_some() {
            return;
        }
        let placeholder = cx.t("inspector.reviewState.requestReviewSearch");
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        cx.subscribe(&search, |column, input, event: &TextInputEvent, cx| {
            if matches!(event, TextInputEvent::Changed) {
                let query = input.read(cx).text().trim().to_string();
                column.search_reviewers(query, true, cx);
            }
        })
        .detach();
        let focus = search.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.review.picker = Some(Picker {
            search,
            query: String::new(),
            candidates: None,
            _read: None,
        });
        self.search_reviewers(String::new(), false, cx);
        cx.notify();
    }

    /// The candidates for `query`, after the debounce while typing.
    fn search_reviewers(&mut self, query: String, debounce: bool, cx: &mut Context<Self>) {
        let (Some(root), Some(number)) = (self.root.clone(), self.reviewed_number()) else {
            return;
        };
        let api = self.review_api();
        let login = self.review.login();
        let Some(picker) = self.review.picker.as_mut() else {
            return;
        };
        picker.query = query.clone();
        picker._read = Some(cx.spawn(async move |this, cx| {
            if debounce {
                cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            }
            let search = query.clone();
            let found = cx
                .background_executor()
                .spawn(async move {
                    let query = Some(search.as_str()).filter(|query| !query.is_empty());
                    api?.reviewer_candidates(&root, number, query, login.as_deref())
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                let Some(picker) = column.review.picker.as_mut() else {
                    return;
                };
                if picker.query != query {
                    return;
                }
                // `keepPreviousData`: a failed search keeps nothing new.
                picker.candidates = Some(found.map(|found| found.candidates).map_err(|_| ()));
                cx.notify();
            });
        }));
    }

    fn pick_reviewer(&mut self, login: String, cx: &mut Context<Self>) {
        self.review.picker = None;
        self.change_reviewer(login, ReviewerAction::Request, cx);
    }
}

fn heading(label: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.muted_foreground)
        .child(label)
        .into_any_element()
}

fn item(candidate: &ReviewerCandidate, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let login = candidate.login.clone();
    let hover = colors.muted;
    div()
        .id(SharedString::from(format!("reviewer-candidate-{login}")))
        .debug_selector({
            let login = login.clone();
            move || format!("reviewer-candidate-{login}")
        })
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(cx.theme().radius.sm)
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(11.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(cx.listener(move |column, _, _, cx| column.pick_reviewer(login.clone(), cx)))
        .child(person(
            candidate.avatar_url.as_deref(),
            &candidate.login,
            20.0,
            8.0,
            cx,
        ))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(format!("@{}", candidate.login)),
        )
        .when_some(candidate.name.clone(), |row, name| {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(name),
            )
        })
        .into_any_element()
}

fn list(column: &CodeColumn, excluded: &[String], cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let message = |content: AnyElement| {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(12.0))
            .text_size(px(11.0))
            .child(content)
            .into_any_element()
    };
    let Some(picker) = column.review.picker.as_ref() else {
        return div().into_any_element();
    };
    match &picker.candidates {
        None => message(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(colors.muted_foreground)
                .child(spinner("reviewer-candidates-loading", 14.0))
                .child(cx.t("inspector.reviewState.requestReviewLoading"))
                .into_any_element(),
        ),
        Some(Err(())) => message(
            div()
                .text_color(crate::theme::tailwind::AMBER_700.hsla())
                .child(cx.t("inspector.reviewState.requestReviewLoadFailed"))
                .into_any_element(),
        ),
        Some(Ok(candidates)) => {
            let candidates = candidates.clone();
            let (suggested, others) = group_candidates(&candidates, excluded);
            if suggested.is_empty() && others.is_empty() {
                return div()
                    .py(px(16.0))
                    .text_center()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("inspector.reviewState.requestReviewEmpty"))
                    .into_any_element();
            }
            let mut rows = Vec::new();
            if !suggested.is_empty() {
                rows.push(heading(
                    cx.t("inspector.reviewState.requestReviewSuggested"),
                    cx,
                ));
                rows.extend(suggested.into_iter().map(|candidate| item(candidate, cx)));
            }
            if !others.is_empty() {
                rows.push(heading(
                    cx.t("inspector.reviewState.requestReviewPeople"),
                    cx,
                ));
                rows.extend(others.into_iter().map(|candidate| item(candidate, cx)));
            }
            div()
                .id("reviewer-candidates")
                .max_h(px(256.0))
                .overflow_y_scroll()
                .p(px(4.0))
                .children(rows)
                .into_any_element()
        }
    }
}

/// The "Pedir revisão" button and its popover.
pub(super) fn trigger(
    column: &CodeColumn,
    excluded: Vec<String>,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let open = column.review.picker.is_some();
    let busy = matches!(column.review.changing, Some((_, ReviewerAction::Request)));
    let foreground = colors.foreground;
    let label = cx.t("inspector.reviewState.requestReview");
    let trigger = div()
        .id("request-review-trigger")
        .debug_selector(|| "request-review-trigger".into())
        .flex()
        .h(px(20.0))
        .items_center()
        .gap(px(4.0))
        .rounded(px(6.0))
        .px(px(6.0))
        .text_size(px(9.5))
        .text_color(colors.muted_foreground)
        .when(!busy, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.text_color(foreground))
        })
        .when(busy, |button| button.opacity(0.5))
        .child(if busy {
            spinner("request-review-busy", 12.0).into_any_element()
        } else {
            Icon::new("user-plus").size(12.0).into_any_element()
        })
        .child(label);
    let entity = cx.entity().downgrade();
    let content = cx.entity().downgrade();
    Popover::new(
        "request-review-picker",
        open && !busy,
        trigger,
        move |_, cx| {
            content
                .update(cx, |column, cx| {
                    let Some(picker) = column.review.picker.as_ref() else {
                        return div().into_any_element();
                    };
                    let colors = cx.theme().colors.clone();
                    let search = div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(36.0))
                        .px(px(12.0))
                        .border_b_1()
                        .border_color(colors.border)
                        .child(div().opacity(0.5).child(Icon::new("search").size(16.0)))
                        .child(div().flex_1().min_w_0().child(picker.search.clone()));
                    div()
                        .child(search)
                        .child(list(column, &excluded, cx))
                        .into_any_element()
                })
                .unwrap_or_else(|_| div().into_any_element())
        },
        move |open, window, cx| {
            let _ = entity.update(cx, |column, cx| column.set_picker_open(open, window, cx));
        },
    )
    .side(PopoverSide::Bottom)
    .align(PopoverAlign::End)
    .side_offset(6.0)
    .width(256.0)
    .padding(0.0)
    .into_any_element()
}
