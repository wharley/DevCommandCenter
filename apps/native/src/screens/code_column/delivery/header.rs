//! The delivery header over the review: the branch and the account, the
//! delivery rail (`rail.rs`), then where things stand with the next step's
//! button; the pull request's checks open under it. A moving line at its
//! bottom while git is read again or a step runs.

use std::time::Duration;

use dcc_core::domain::commit_mode::{CommitButtonStatus, CommitMode, commit_translation_key};
use gpui::{
    AnyElement, BoxShadow, Context, FontWeight, Hsla, SharedString, div, linear_color_stop,
    linear_gradient, point, prelude::*, px, relative,
};

use super::super::CodeColumn;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

/// The pull request's states in the named state colours (`StateTone`'s
/// fills, the same in both themes).
pub(crate) fn pr_open() -> Hsla {
    tw::EMERALD_600.hsla()
}
pub(crate) fn pr_merged() -> Hsla {
    tw::VIOLET_600.hsla()
}
pub(crate) fn pr_closed() -> Hsla {
    tw::RED_600.hsla()
}
pub(crate) fn pr_conflicts() -> Hsla {
    tw::AMBER_600.hsla()
}

/// `commitModeClassName`: a filled button on PR states, else the outline.
fn button_colors(mode: CommitMode, cx: &Context<CodeColumn>) -> (Hsla, Hsla, Hsla) {
    let colors = cx.theme().colors.clone();
    let white = gpui::white();
    match mode {
        CommitMode::Fix | CommitMode::Closed => (pr_closed(), white, pr_closed()),
        CommitMode::ResolveConflicts => (pr_conflicts(), white, pr_conflicts()),
        CommitMode::CompleteMerge => (tw::EMERALD_600.hsla(), white, tw::EMERALD_600.alpha(0.9)),
        CommitMode::Merge | CommitMode::OpenPr => (pr_open(), white, pr_open()),
        CommitMode::Merged => (pr_merged(), white, pr_merged()),
        _ => (
            colors.background,
            colors.foreground,
            colors.accent.opacity(0.6),
        ),
    }
}

fn commit_button(
    column: &CodeColumn,
    mode: CommitMode,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let status = column.delivery.button();
    let label = column.delivery.request_label();
    let local_only = column
        .status
        .as_ref()
        .is_some_and(|status| !status.has_remote);
    let text = if local_only && mode == CommitMode::CommitAndPush {
        cx.t("composer.executionDock.actions.commit")
    } else {
        cx.t_with(
            &commit_translation_key(mode, status),
            &Args::default().with("requestLabel", label),
        )
    };
    // Mergeable, but the checks have not finished: a plain button saying so.
    let waiting = mode == CommitMode::Merge && column.checks_running();
    let text = if waiting {
        cx.t("commit.checksRunning")
    } else {
        text
    };
    let locked = matches!(mode, CommitMode::Merged | CommitMode::Closed);
    let disabled = locked || status == CommitButtonStatus::Busy || column.delivery.busy;
    let (background, foreground, hover) = if waiting {
        button_colors(CommitMode::Push, cx)
    } else {
        button_colors(mode, cx)
    };
    let dark = cx.theme().appearance == Appearance::Dark;
    let border = if dark { colors.input } else { colors.border };
    div()
        .id("delivery-commit")
        .debug_selector(|| "delivery-commit".into())
        .flex()
        .h(px(28.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .px(px(10.0))
        .bg(background)
        .text_color(foreground)
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        // `shadow-xs`
        .shadow(vec![BoxShadow {
            color: Hsla::black().opacity(0.05),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(2.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .map(|button| {
            if disabled {
                button.opacity(0.5)
            } else {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(|column, _, window, cx| column.press_commit(window, cx)))
            }
        })
        .when(status == CommitButtonStatus::Busy, |button| {
            button.child(spinner("delivery-commit-busy", 14.0))
        })
        .when(status == CommitButtonStatus::Done, |button| {
            button.child(Icon::new("check").size(14.0))
        })
        .child(text)
        .into_any_element()
}

/// The outline icon button that opens the resolver on a ready merge.
fn review_resolution(cx: &mut Context<CodeColumn>) -> AnyElement {
    let label = cx.t("inspector.gitConfirmation.reviewResolution");
    crate::ui::Tooltip::new(
        "delivery-review-resolution-tip",
        Button::new("delivery-review-resolution")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::IconSm)
            .on_click(cx.listener(|column, _, _, cx| column.open_merge_resolver(cx)))
            .child(Icon::new("file-diff").size(14.0)),
        move |_, _| div().child(label.clone()).into_any_element(),
    )
    .side(crate::ui::tooltip::Side::Bottom)
    .into_any_element()
}

/// The moving line while git is read again (`shine 2s infinite linear`).
fn shimmer(cx: &Context<CodeColumn>) -> AnyElement {
    let primary = cx.theme().colors.primary.opacity(0.5);
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(1.0))
        .overflow_hidden()
        .child(crate::ui::animated(
            div().absolute().top_0().h_full().w(relative(1.0)),
            "delivery-shimmer",
            crate::ui::looping(Duration::from_secs(2)),
            cx,
            move |line, delta| {
                line.left(relative(delta * 2.0 - 1.0)).bg(linear_gradient(
                    90.0,
                    linear_color_stop(primary.opacity(0.0), 0.0),
                    linear_color_stop(primary, 0.5),
                ))
            },
        ))
        .into_any_element()
}

pub(crate) fn header(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let mode = column.commit_mode();
    let title: SharedString = column
        .current_branch()
        .or_else(|| column.task_branch())
        .unwrap_or_else(|| "Git".to_string())
        .into();
    let refreshing = column.delivery.busy || column.delivery.reading;
    // Nothing on the branch yet: no "Criar PR" (`suppressEmptyCreatePr`).
    let empty_request =
        mode == CommitMode::CreatePr && column.changes.branch_count().unwrap_or(0) == 0;
    let show_button =
        mode != CommitMode::Merged && !empty_request && column.delivery.task.is_some();
    let rail = column.delivery_rail(mode);
    let (standing, approvals) = super::rail::standing_text(column, &rail, cx);
    let branch = div()
        .flex()
        .h(px(32.0))
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .px(px(12.0))
        .child(
            Icon::new("git-branch")
                .size(14.0)
                .stroke(1.8)
                .color(colors.muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .children(crate::screens::code_column::review::identity_chip(
            column, cx,
        ));
    let next = div()
        .flex()
        .min_h(px(28.0))
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .pt(px(8.0))
        .pb(px(10.0))
        .child(
            div()
                .debug_selector(|| "delivery-standing".into())
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(standing),
                )
                .children(approvals.map(|approvals| {
                    div()
                        .truncate()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(approvals)
                })),
        )
        // "Revisar resolução" (`onReviewConflictResolution`).
        .when(mode == CommitMode::CompleteMerge, |row| {
            row.child(review_resolution(cx))
        })
        .when(show_button, |row| {
            row.child(commit_button(column, mode, cx))
        });
    div()
        .id("delivery-header")
        .debug_selector(|| "delivery-header".into())
        .relative()
        .flex()
        .flex_col()
        .min_w_0()
        .flex_shrink_0()
        .overflow_hidden()
        .bg(colors.muted.opacity(0.25))
        // `shadow-[inset_0_-1px_0_color-mix(in_oklch,var(--border)_60%,transparent)]`
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .when(refreshing, |header| header.child(shimmer(cx)))
        .child(branch)
        .child(
            div()
                .px(px(8.0))
                .pt(px(2.0))
                .child(super::rail::rail_strip(column, &rail, cx)),
        )
        .child(next)
        .children(crate::screens::code_column::review::checks_list(column, cx))
        .into_any_element()
}
