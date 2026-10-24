//! `WorkspaceReviewStateSection`: the review's state badge (folded), and
//! open, approvals / mergeability / base / discussions, then the reviewers
//! with their state, the ask to withdraw or repeat on hover, and "Pedir
//! revisão".

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, img, prelude::*, px};

use super::super::CodeColumn;
use super::{ReviewLoad, picker, state_label};
use crate::data::avatar::avatar;
use crate::data::forge::{ReviewState, Reviewer, ReviewerAction, reviewer_row_action};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Icon, Tracked, spinner};

/// `reviewStateClass`: (border, background, text).
fn state_colors(state: &str, cx: &Context<CodeColumn>) -> (Hsla, Hsla, Hsla) {
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    match state {
        "approved" | "not_required" => (
            tw::EMERALD_500.alpha(0.2),
            tw::EMERALD_500.alpha(0.1),
            if dark {
                tw::EMERALD_400
            } else {
                tw::EMERALD_700
            }
            .hsla(),
        ),
        "changes_requested" => (
            colors.destructive.opacity(0.2),
            colors.destructive.opacity(0.1),
            colors.destructive,
        ),
        "pending" => (
            tw::AMBER_500.alpha(0.2),
            tw::AMBER_500.alpha(0.1),
            if dark { tw::AMBER_400 } else { tw::AMBER_700 }.hsla(),
        ),
        _ => (
            colors.border.opacity(0.6),
            colors.muted.opacity(0.5),
            colors.muted_foreground,
        ),
    }
}

fn frame(blocking: bool, cx: &Context<CodeColumn>) -> gpui::Div {
    let colors = cx.theme().colors.clone();
    div()
        .debug_selector(|| "review-state".into())
        .flex_shrink_0()
        .overflow_hidden()
        .rounded(px(6.0))
        .border_1()
        .border_color(if blocking {
            tw::AMBER_500.alpha(0.25)
        } else {
            colors.border.opacity(0.6)
        })
        .bg(colors.muted.opacity(0.15))
}

fn cell(icon: AnyElement, text: SharedString, cx: &Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .bg(colors.background.opacity(0.6))
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(9.5))
        .text_color(colors.muted_foreground)
        .child(icon)
        .child(div().min_w_0().truncate().child(text))
        .into_any_element()
}

/// `ReviewerAvatar` (24px) / `CandidateAvatar` (20px): the picture or two
/// letters.
pub(super) fn person(
    url: Option<&str>,
    label: &str,
    side: f32,
    text_size: f32,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let picture = avatar(url, cx);
    let letters: String = label.chars().take(2).collect::<String>().to_uppercase();
    div()
        .flex()
        .size(px(side))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded_full()
        .bg(colors.muted)
        .border_1()
        .border_color(colors.border.opacity(0.7))
        .text_size(px(text_size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(colors.muted_foreground)
        .map(|frame| match picture {
            Some(picture) => frame.child(img(picture).size_full().rounded_full()),
            None => frame.child(letters),
        })
        .into_any_element()
}

/// `approvalsLabel`
fn approvals(review: &ReviewState, state: &str, cx: &Context<CodeColumn>) -> SharedString {
    if !review.approvals_available {
        return cx.t("inspector.reviewState.approvalsUnavailable");
    }
    if review.approvals_required.is_some() || review.approvals_left.is_some() {
        let required = review
            .approvals_required
            .unwrap_or(review.approvals_received + review.approvals_left.unwrap_or(0));
        return cx.t_with(
            "inspector.reviewState.approvals",
            &Args::default()
                .with("received", review.approvals_received.to_string())
                .with("required", required.to_string()),
        );
    }
    if state == "not_required" {
        cx.t("inspector.reviewState.noApprovalRequired")
    } else if review.approvals_received > 0 {
        cx.t_with(
            "inspector.reviewState.approvalsRecorded",
            &Args::count(i64::from(review.approvals_received)),
        )
    } else {
        cx.t("inspector.reviewState.approvalRequirementUnknown")
    }
}

fn reviewer_row(
    column: &CodeColumn,
    reviewer: &Reviewer,
    index: usize,
    provider: Option<&str>,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let label = reviewer
        .login
        .clone()
        .or_else(|| reviewer.name.clone())
        .unwrap_or_else(|| "?".to_string());
    let name: SharedString = match (&reviewer.login, &reviewer.name) {
        (Some(login), _) => format!("@{login}").into(),
        (None, Some(name)) => name.clone().into(),
        (None, None) => cx.t("inspector.reviewState.unknownReviewer"),
    };
    let action = reviewer_row_action(reviewer, provider);
    let changing = match (&column.review.changing, &reviewer.login) {
        (Some((login, _)), Some(this)) => login.eq_ignore_ascii_case(this),
        _ => false,
    };
    let busy = column.review.changing.is_some();
    let group = SharedString::from(format!("reviewer-{index}"));
    let button = action.zip(reviewer.login.clone()).map(|(action, login)| {
        let icon = if action == ReviewerAction::Remove {
            "x"
        } else {
            "rotate-cw"
        };
        let hover = colors.accent;
        div()
            .id(("reviewer-action", index))
            .debug_selector(move || format!("reviewer-action-{index}"))
            .flex()
            .size(px(20.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_color(colors.muted_foreground)
            .when(!changing, |button| {
                button
                    .opacity(0.0)
                    .group_hover(group.clone(), |style| style.opacity(1.0))
            })
            .map(|button| {
                if busy {
                    button.opacity(if changing { 1.0 } else { 0.0 })
                } else {
                    button
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(move |column, _, _, cx| {
                            column.change_reviewer(login.clone(), action, cx)
                        }))
                }
            })
            .child(if changing {
                spinner(("reviewer-changing", index), 12.0).into_any_element()
            } else {
                Icon::new(icon).size(12.0).into_any_element()
            })
    });
    div()
        .group(group.clone())
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(6.0))
        .px(px(4.0))
        .py(px(4.0))
        .child(person(
            reviewer.avatar_url.as_deref(),
            &label,
            24.0,
            8.5,
            cx,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(10.0))
                .child(name),
        )
        .child(
            div()
                .text_size(px(9.5))
                .text_color(colors.muted_foreground)
                .child(state_label(
                    "inspector.reviewState.reviewerState",
                    &reviewer.state,
                    cx,
                )),
        )
        .children(button)
        .into_any_element()
}

fn body(
    column: &CodeColumn,
    review: &ReviewState,
    state: &str,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let muted_icon = |name: &'static str| Icon::new(name).size(14.0).into_any_element();
    let mergeable = if review.has_conflicts == Some(true) {
        cell(
            Icon::new("circle-x")
                .size(14.0)
                .color(colors.destructive)
                .into_any_element(),
            cx.t("inspector.reviewState.conflicts"),
            cx,
        )
    } else if review.mergeable.as_deref() == Some("mergeable") {
        cell(
            Icon::new("circle-check")
                .size(14.0)
                .color(tw::EMERALD_500.hsla())
                .into_any_element(),
            cx.t("inspector.reviewState.mergeable"),
            cx,
        )
    } else {
        cell(
            muted_icon("git-merge"),
            cx.t("inspector.reviewState.mergeabilityUnknown"),
            cx,
        )
    };
    let base = match review.behind_by {
        None => cx.t("inspector.reviewState.baseUnknown"),
        Some(0) => cx.t("inspector.reviewState.baseCurrent"),
        Some(count) => cx.t_with(
            "inspector.reviewState.behindBase",
            &Args::count(i64::from(count)),
        ),
    };
    let discussions = match review.discussions_resolved {
        None => cx.t("inspector.reviewState.discussionsUnknown"),
        Some(true) => cx.t("inspector.reviewState.discussionsResolved"),
        Some(false) => cx.t("inspector.reviewState.discussionsOpen"),
    };
    let grid = div()
        .grid()
        .grid_cols(2)
        .gap(px(6.0))
        .child(cell(
            muted_icon("user-round-check"),
            approvals(review, state, cx),
            cx,
        ))
        .child(mergeable)
        .child(cell(muted_icon("git-merge"), base, cx))
        .child(cell(muted_icon("circle-dot"), discussions, cx));
    let reviewers = review.reviewers_available.then(|| {
        let rows: Vec<AnyElement> = review
            .reviewers
            .iter()
            .enumerate()
            .map(|(index, reviewer)| {
                reviewer_row(column, reviewer, index, review.provider.as_deref(), cx)
            })
            .collect();
        let excluded: Vec<String> = review
            .reviewers
            .iter()
            .filter_map(|r| r.login.clone())
            .collect();
        div()
            .mt(px(8.0))
            .child(
                div()
                    .mb(px(4.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_color(colors.muted_foreground)
                            .font_weight(FontWeight::MEDIUM)
                            .child(Tracked::new(
                                cx.t("inspector.reviewState.reviewers").to_uppercase(),
                                9.0,
                                0.08,
                            )),
                    )
                    .child(picker::trigger(column, excluded, cx)),
            )
            .child(if rows.is_empty() {
                div()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("inspector.reviewState.noReviewers"))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(rows)
                    .into_any_element()
            })
    });
    div()
        .border_t_1()
        .border_color(colors.border.opacity(0.4))
        .px(px(10.0))
        .py(px(8.0))
        .child(grid)
        .children(reviewers)
        .into_any_element()
}

pub(crate) fn review_section(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let pr_number = column.delivery.pr.as_ref().and_then(|pr| pr.number);
    if !column.review.review_enabled(pr_number) {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    match column.review.state.as_ref()? {
        ReviewLoad::Pending => Some(
            div()
                .debug_selector(|| "review-state".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .bg(colors.muted.opacity(0.2))
                .px(px(10.0))
                .py(px(8.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(spinner("review-state-loading", 14.0))
                .child(cx.t("inspector.reviewState.loading"))
                .into_any_element(),
        ),
        ReviewLoad::Failed => {
            let text = if dark { tw::AMBER_300 } else { tw::AMBER_700 }.hsla();
            Some(
                div()
                    .debug_selector(|| "review-state".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(tw::AMBER_500.alpha(0.2))
                    .bg(tw::AMBER_500.alpha(0.05))
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(11.0))
                    .text_color(text)
                    .child(Icon::new("circle-alert").size(14.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(cx.t("inspector.reviewState.loadFailed")),
                    )
                    .child(
                        div()
                            .id("review-state-retry")
                            .flex()
                            .h(px(24.0))
                            .items_center()
                            .rounded(px(6.0))
                            .px(px(6.0))
                            .text_size(px(10.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(tw::AMBER_500.alpha(0.1)))
                            .on_click(cx.listener(|column, _, _, cx| column.retry_review(cx)))
                            .child(cx.t("inspector.reviewState.tryAgain")),
                    )
                    .into_any_element(),
            )
        }
        ReviewLoad::Ready(review) => {
            review.number?;
            let review = review.as_ref().clone();
            let state = review
                .review_state
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let pending = review
                .reviewers
                .iter()
                .filter(|r| r.state == "pending")
                .count();
            let blocking = review.has_conflicts == Some(true)
                || review.review_state.as_deref() == Some("changes_requested")
                || review.discussions_resolved == Some(false);
            let open = column.review.state_open;
            let (border, background, text) = state_colors(&state, cx);
            let icon = match state.as_str() {
                "approved" | "not_required" => Icon::new("shield-check")
                    .size(14.0)
                    .color(tw::EMERALD_500.hsla()),
                "changes_requested" => Icon::new("message-square-warning")
                    .size(14.0)
                    .color(colors.destructive),
                _ => Icon::new("users").size(14.0).color(tw::AMBER_500.hsla()),
            };
            let link = review.url.clone().map(|url| {
                let hover = colors.accent;
                div()
                    .id("review-state-open")
                    .flex()
                    .size(px(24.0))
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .text_color(colors.muted_foreground)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .child(Icon::new("external-link").size(14.0))
            });
            let header = div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(8.0))
                .child(
                    div()
                        .id("review-state-toggle")
                        .debug_selector(|| "review-state-toggle".into())
                        .flex()
                        .h(px(24.0))
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(8.0))
                        .ml(px(-4.0))
                        .px(px(4.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|column, _, _, cx| column.toggle_review_state(cx)))
                        .child(Icon::new("chevron-right").size(14.0).rotate(if open {
                            90.0
                        } else {
                            0.0
                        }))
                        .child(icon)
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child(cx.t("inspector.reviewState.title")),
                        )
                        .when(pending > 0, |row| {
                            row.child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(9.5))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t_with(
                                        "inspector.reviewState.pendingReviewers",
                                        &Args::count(pending as i64),
                                    )),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .h(px(20.0))
                        .flex_shrink_0()
                        .items_center()
                        .rounded_full()
                        .border_1()
                        .border_color(border)
                        .bg(background)
                        .px(px(6.0))
                        .text_size(px(9.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(text)
                        .child(state_label("inspector.reviewState.state", &state, cx)),
                )
                .children(link);
            let body = open.then(|| body(column, &review, &state, cx));
            Some(
                frame(blocking, cx)
                    .child(header)
                    .children(body)
                    .into_any_element(),
            )
        }
    }
}
