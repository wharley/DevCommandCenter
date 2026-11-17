//! "Pedir revisão" on the summary's review card: a search over who can
//! review the pull request (250ms after typing stops), suggested people
//! first, each with their picture; a pick asks them on the forge, as
//! "Mudanças" does for a task's own pull request. What the forge refuses
//! is said as it says it.

use std::time::Duration;

use gpui::{
    AnyElement, AppContext, Context, Focusable, FontWeight, SharedString, Task, Window, div,
    prelude::*, px,
};

use super::PrHub;
use super::list::actor_avatar;
use crate::data::forge::{ReviewerCandidate, group_candidates};
use crate::data::pr_hub::{Actor, Item};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::popover::{Popover, PopoverAlign, PopoverSide};
use crate::ui::{Icon, TextInput, TextInputEvent, Toaster, spinner};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

/// The open search, for one pull request.
pub(crate) struct ReviewerPicker {
    /// The pull request it was opened on (`Item::id`).
    pr_id: String,
    search: gpui::Entity<TextInput>,
    query: String,
    /// None while the first read is out; an error keeps nothing.
    candidates: Option<Result<Vec<ReviewerCandidate>, ()>>,
    _read: Option<Task<()>>,
}

/// Who is already on the review, so they are not offered again: the
/// author, who was asked and who reviewed.
pub(super) fn on_the_review(item: &Item, reviewed: &[String]) -> Vec<String> {
    item.author
        .iter()
        .map(|author| author.login.clone())
        .chain(item.reviewers.iter().map(|actor| actor.login.clone()))
        .chain(reviewed.iter().cloned())
        .collect()
}

impl PrHub {
    pub(crate) fn set_reviewer_picker_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.selected_item(cx).filter(|_| open) else {
            self.reviewer_picker = None;
            cx.notify();
            return;
        };
        if self
            .reviewer_picker
            .as_ref()
            .is_some_and(|picker| picker.pr_id == item.id)
        {
            return;
        }
        let placeholder = cx.t("inspector.reviewState.requestReviewSearch");
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        cx.subscribe(&search, |hub, input, event: &TextInputEvent, cx| {
            if matches!(event, TextInputEvent::Changed) {
                let query = input.read(cx).text().trim().to_string();
                hub.search_reviewers(query, true, cx);
            }
        })
        .detach();
        let focus = search.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.reviewer_picker = Some(ReviewerPicker {
            pr_id: item.id.clone(),
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
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        let Some(picker) = self.reviewer_picker.as_mut() else {
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
                    api.reviewer_candidates(
                        &item.repository_root,
                        item.number,
                        query,
                        item.forge_login.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |hub, cx| {
                let Some(picker) = hub.reviewer_picker.as_mut() else {
                    return;
                };
                if picker.query != query {
                    return;
                }
                picker.candidates = Some(found.map(|found| found.candidates).map_err(|_| ()));
                cx.notify();
            });
        }));
    }

    /// A pick: asked on the forge; the card reads again when it answers.
    pub(crate) fn request_reviewer(&mut self, login: String, cx: &mut Context<Self>) {
        self.reviewer_picker = None;
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        if self.requesting_review {
            return;
        }
        self.requesting_review = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let asked = cx
                .background_executor()
                .spawn(async move {
                    api.request_reviewers(
                        &item.repository_root,
                        item.number,
                        &[login],
                        &[],
                        item.forge_login.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.requesting_review = false;
                match asked {
                    Ok(()) => {
                        hub.detail = None;
                        hub.ensure_detail(cx);
                        hub.refresh(cx);
                    }
                    Err(error) => Toaster::error(super::message(error, cx), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn candidate_row(candidate: &ReviewerCandidate, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let login = candidate.login.clone();
        let actor = Actor {
            login: candidate.login.clone(),
            name: candidate.name.clone(),
            avatar_url: candidate.avatar_url.clone(),
        };
        div()
            .id(SharedString::from(format!("pr-reviewer-candidate-{login}")))
            .debug_selector({
                let login = login.clone();
                move || format!("pr-reviewer-candidate-{login}")
            })
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(cx.theme().radius.sm)
            .px(px(8.0))
            .py(px(6.0))
            .text_size(px(11.5))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.muted))
            .on_click(cx.listener(move |hub, _, _, cx| hub.request_reviewer(login.clone(), cx)))
            .child(actor_avatar(Some(&actor), cx))
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
                        .text_size(px(10.5))
                        .text_color(colors.muted_foreground)
                        .child(name),
                )
            })
            .into_any_element()
    }

    fn candidate_list(&self, excluded: &[String], cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let line = |content: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(12.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(content)
                .into_any_element()
        };
        let Some(picker) = self.reviewer_picker.as_ref() else {
            return div().into_any_element();
        };
        let candidates = match &picker.candidates {
            None => {
                return line(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(spinner("pr-reviewer-candidates-loading", 14.0))
                        .child(cx.t("inspector.reviewState.requestReviewLoading"))
                        .into_any_element(),
                );
            }
            Some(Err(())) => {
                return line(
                    div()
                        .text_color(colors.warning)
                        .child(cx.t("inspector.reviewState.requestReviewLoadFailed"))
                        .into_any_element(),
                );
            }
            Some(Ok(candidates)) => candidates.clone(),
        };
        let (suggested, others) = group_candidates(&candidates, excluded);
        if suggested.is_empty() && others.is_empty() {
            return line(
                div()
                    .child(cx.t("inspector.reviewState.requestReviewEmpty"))
                    .into_any_element(),
            );
        }
        let heading = |label: SharedString| {
            div()
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(label)
                .into_any_element()
        };
        let mut rows = Vec::new();
        if !suggested.is_empty() {
            rows.push(heading(
                cx.t("inspector.reviewState.requestReviewSuggested"),
            ));
            rows.extend(
                suggested
                    .into_iter()
                    .map(|candidate| Self::candidate_row(candidate, cx)),
            );
        }
        if !others.is_empty() {
            rows.push(heading(cx.t("inspector.reviewState.requestReviewPeople")));
            rows.extend(
                others
                    .into_iter()
                    .map(|candidate| Self::candidate_row(candidate, cx)),
            );
        }
        div()
            .id("pr-reviewer-candidates")
            .max_h(px(256.0))
            .overflow_y_scroll()
            .p(px(4.0))
            .children(rows)
            .into_any_element()
    }

    /// The "Pedir revisão" button under the reviewers, and its search.
    pub(super) fn request_review_trigger(
        &self,
        item: &Item,
        excluded: Vec<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let open = self
            .reviewer_picker
            .as_ref()
            .is_some_and(|picker| picker.pr_id == item.id);
        let busy = self.requesting_review;
        let foreground = colors.foreground;
        let trigger = div()
            .id("pr-request-review")
            .debug_selector(|| "pr-request-review".into())
            .flex()
            .h(px(24.0))
            .items_center()
            .gap(px(5.0))
            .rounded(px(6.0))
            .px(px(6.0))
            .text_size(px(11.5))
            .text_color(colors.muted_foreground)
            .when(!busy, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.text_color(foreground))
            })
            .when(busy, |button| button.opacity(0.5))
            .child(if busy {
                spinner("pr-request-review-busy", 12.0).into_any_element()
            } else {
                Icon::new("user-plus").size(13.0).into_any_element()
            })
            .child(cx.t("inspector.reviewState.requestReview"));
        let (entity, content) = (cx.entity().downgrade(), cx.entity().downgrade());
        let picker = Popover::new(
            "pr-request-review-picker",
            open && !busy,
            trigger,
            move |_, cx| {
                content
                    .update(cx, |hub, cx| {
                        let Some(picker) = hub.reviewer_picker.as_ref() else {
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
                            .child(hub.candidate_list(&excluded, cx))
                            .into_any_element()
                    })
                    .unwrap_or_else(|_| div().into_any_element())
            },
            move |open, window, cx| {
                let _ = entity.update(cx, |hub, cx| hub.set_reviewer_picker_open(open, window, cx));
            },
        )
        .side(PopoverSide::Bottom)
        .align(PopoverAlign::Start)
        .side_offset(6.0)
        .width(280.0)
        .padding(0.0);
        div().flex().child(picker).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_is_on_the_review_is_not_offered_again() {
        let actor = |login: &str| Actor {
            login: login.into(),
            ..Actor::default()
        };
        let item = Item {
            author: Some(actor("ana")),
            reviewers: vec![actor("bia")],
            ..Item::default()
        };
        let on = on_the_review(&item, &["caio".to_string()]);
        assert_eq!(on, ["ana", "bia", "caio"]);
        let candidates = vec![
            ReviewerCandidate {
                login: "Bia".into(),
                ..Default::default()
            },
            ReviewerCandidate {
                login: "duda".into(),
                suggested: true,
                ..Default::default()
            },
            ReviewerCandidate {
                login: "edu".into(),
                ..Default::default()
            },
        ];
        let (suggested, others) = group_candidates(&candidates, &on);
        assert_eq!(suggested[0].login, "duda");
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].login, "edu");
    }
}
