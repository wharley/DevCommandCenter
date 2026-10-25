//! `TurnReviewSurface` under the scope chips: the turn's state, outcome
//! and files with Guarded Undo's shield, the notices, a card per file with
//! the turn's patch, and the floating "Desfazer".

use dcc_core::domain::turn_review::{
    TurnReviewSummary, can_prepare_guarded_undo, has_turn_review_line_stats, plan_turn_undo,
    resolve_guarded_undo_capture, resolve_guarded_undo_failure_reason, resolve_turn_review_outcome,
};
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, img, prelude::*, px};

use super::super::CodeColumn;
use super::super::changes::card_diff_height;
use super::TurnDiff;
use crate::i18n::{Args, Translate};
use crate::project::file_icon::file_icon;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::tooltip::Side;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

fn dark(cx: &Context<CodeColumn>) -> bool {
    cx.theme().appearance == Appearance::Dark
}

/// `text-amber-600`, in both themes.
fn amber(_: &Context<CodeColumn>) -> Hsla {
    tw::AMBER_600.hsla()
}

fn emerald(cx: &Context<CodeColumn>) -> Hsla {
    if dark(cx) {
        tw::EMERALD_400
    } else {
        tw::EMERALD_600
    }
    .hsla()
}

fn centered(text: SharedString, color: Hsla) -> AnyElement {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .text_center()
        .text_size(px(14.0))
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// The shield: Guarded Undo can (or cannot) put this turn back, and why.
fn shield(review: &TurnReviewSummary, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let capture = resolve_guarded_undo_capture(review.guarded_undo.as_ref());
    let eligible = capture.state == "eligible";
    let expires = review
        .guarded_undo
        .as_ref()
        .and_then(|capture| capture.expires_at.clone())
        .map(|at| {
            cx.t_with(
                "turnReview.guardedUndo.validUntil",
                &Args::default().with("expiresAt", at),
            )
            .to_string()
        })
        .unwrap_or_default();
    let title = cx.t(&format!("turnReview.guardedUndo.states.{}", capture.state));
    let reason = cx.t_with(
        &format!("turnReview.guardedUndo.reasons.{}", capture.reason),
        &Args::count(
            review
                .guarded_undo
                .as_ref()
                .map_or(0, |capture| i64::from(capture.file_count)),
        )
        .with("expiresAt", expires),
    );
    let badge = div()
        .debug_selector(|| "turn-shield".into())
        .flex()
        .size(px(20.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded_full()
        .map(|badge| {
            if eligible {
                badge
                    .bg(tw::EMERALD_500.alpha(0.1))
                    .text_color(tw::EMERALD_600.hsla())
            } else {
                badge.bg(colors.muted).text_color(colors.muted_foreground)
            }
        })
        .child(
            Icon::new(if eligible {
                "shield-check"
            } else {
                "shield-alert"
            })
            .size(14.0),
        );
    Tooltip::new("turn-shield-tip", badge, move |_, _| {
        div()
            .max_w(px(288.0))
            .child(div().font_weight(FontWeight::MEDIUM).child(title.clone()))
            .child(div().mt(px(4.0)).opacity(0.7).child(reason.clone()))
            .into_any_element()
    })
    .side(Side::Bottom)
    .into_any_element()
}

/// `activeUndo`: an undo that stopped midway, and reading it again.
fn active_undo(review: &TurnReviewSummary, cx: &mut Context<CodeColumn>) -> Option<AnyElement> {
    let active = review.active_undo.as_ref()?;
    let colors = cx.theme().colors.clone();
    let recovery = active.status == "recovery_required";
    let (border, background, text) = if recovery {
        (
            colors.destructive.opacity(0.4),
            colors.destructive.opacity(0.05),
            colors.destructive,
        )
    } else {
        (
            tw::AMBER_500.alpha(0.4),
            tw::AMBER_500.alpha(0.05),
            if dark(cx) {
                tw::AMBER_400
            } else {
                tw::AMBER_700
            }
            .hsla(),
        )
    };
    Some(
        div()
            .debug_selector(|| "turn-active-undo".into())
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(background)
            .px(px(8.0))
            .py(px(6.0))
            .text_color(text)
            .child(if recovery {
                Icon::new("shield-alert").size(14.0).into_any_element()
            } else {
                spinner("turn-active-undo-busy", 14.0).into_any_element()
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(cx.t(&format!(
                        "turnReview.guardedUndo.operationStates.{}",
                        active.status
                    ))),
            )
            .child(
                Button::new("turn-refresh-recovery")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Xs)
                    .on_click(cx.listener(|column, _, _, cx| column.read_last_turn(cx)))
                    .child(cx.t("turnReview.guardedUndo.refreshRecovery")),
            )
            .into_any_element(),
    )
}

/// The block over the files: state, outcome, count, shield and notices.
fn summary(
    column: &CodeColumn,
    review: &TurnReviewSummary,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let outcome = resolve_turn_review_outcome(
        review.turn_outcome.as_deref(),
        review.outcome_reason.as_deref(),
    );
    let single = &column.turn.single;
    let stats = review
        .files
        .iter()
        .all(has_turn_review_line_stats)
        .then(|| {
            div()
                .flex()
                .gap(px(4.0))
                .child(" · ")
                .child(
                    div()
                        .text_color(tw::EMERALD_600.hsla())
                        .child(format!("+{}", review.insertions)),
                )
                .child(
                    div()
                        .text_color(colors.destructive)
                        .child(format!("−{}", review.deletions)),
                )
        });
    let line = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(cx.t(&format!("turnReview.states.{}", review.state))),
        )
        .when_some(outcome, |line, (outcome, _)| {
            let aborted = outcome.id() == "aborted";
            line.child(
                div()
                    .text_color(if aborted {
                        amber(cx)
                    } else {
                        colors.muted_foreground
                    })
                    .child(format!(
                        "· {}",
                        cx.t(&format!("turnReview.outcomes.{}", outcome.id()))
                    )),
            )
        })
        .child(
            div()
                .flex()
                .flex_1()
                .justify_end()
                .text_color(colors.muted_foreground)
                .child(cx.t_with(
                    "turnReview.fileCount",
                    &Args::count(review.files.len() as i64),
                ))
                .children(stats),
        )
        .child(shield(review, cx));
    let notice = |text: SharedString, color: Hsla| div().text_color(color).child(text);
    let outcome_line = single
        .outcome
        .as_ref()
        .filter(|_| !single.dialog)
        .map(|(status, _)| {
            notice(
                cx.t(&format!("turnReview.guardedUndo.results.{status}")),
                if *status == "completed" {
                    tw::EMERALD_600.hsla()
                } else {
                    amber(cx)
                },
            )
        });
    let problem = single.problem.as_ref().map(|(_, reason)| {
        notice(
            cx.t(&format!(
                "turnReview.guardedUndo.failureReasons.{}",
                resolve_guarded_undo_failure_reason(Some(reason))
            )),
            amber(cx),
        )
    });
    div()
        .debug_selector(|| "turn-summary".into())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(6.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.5))
        .px(px(12.0))
        .py(px(8.0))
        .text_size(px(10.5))
        .child(line)
        .child(notice(
            cx.t("turnReview.attributionNotice"),
            colors.muted_foreground,
        ))
        .when(review.compatibility == "diverged", |block| {
            block.child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .text_color(amber(cx))
                    .child(
                        div()
                            .mt(px(2.0))
                            .child(Icon::new("triangle-alert").size(14.0)),
                    )
                    .child(cx.t("turnReview.divergedNotice")),
            )
        })
        .children(active_undo(review, cx))
        .when(single.prepare_failed, |block| {
            block.child(notice(
                cx.t("turnReview.guardedUndo.prepareFailed"),
                colors.destructive,
            ))
        })
        .children(problem)
        .children(outcome_line)
        .into_any_element()
}

fn card_body(column: &CodeColumn, at: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let card = &column.turn.cards[at];
    let file = column
        .turn
        .summary()
        .and_then(|review| review.files.iter().find(|file| file.path == card.path));
    let (insertions, deletions) = file.map_or((0, 0), |file| (file.insertions, file.deletions));
    let small = |text: SharedString, color: Hsla| {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .p(px(16.0))
            .text_center()
            .text_size(px(12.0))
            .text_color(color)
            .child(text)
            .into_any_element()
    };
    let content = match &card.diff {
        TurnDiff::Ready(diff) => diff.clone().into_any_element(),
        TurnDiff::Unavailable => small(
            cx.t("turnReview.previewUnavailable"),
            colors.muted_foreground,
        ),
        TurnDiff::Failed => div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .p(px(16.0))
            .text_size(px(12.0))
            .child(
                div()
                    .text_color(colors.destructive)
                    .child(cx.t("turnReview.diffFailed")),
            )
            .child(
                Button::new(("turn-diff-retry", at))
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Sm)
                    .on_click(cx.listener(move |column, _, _, cx| column.retry_turn_diff(at, cx)))
                    .child(cx.t("turnReview.timeline.retry")),
            )
            .into_any_element(),
        TurnDiff::Idle | TurnDiff::Loading(_) => div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .child(spinner(("turn-diff-loading", at), 16.0))
            .child(cx.t("turnReview.loading"))
            .into_any_element(),
    };
    div()
        .debug_selector(move || format!("turn-card-body-{at}"))
        .h(px(card_diff_height(insertions, deletions)))
        .overflow_hidden()
        .bg(colors.background)
        .child(content)
        .into_any_element()
}

/// `TurnReviewCard`
fn card(column: &CodeColumn, at: usize, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let card = &column.turn.cards[at];
    let file = column
        .turn
        .summary()
        .and_then(|review| review.files.iter().find(|file| file.path == card.path))
        .cloned();
    let (name, folder) = match card.path.rsplit_once('/') {
        Some((folder, name)) => (name.to_string(), folder.to_string()),
        None => (card.path.clone(), String::new()),
    };
    let open = card.open;
    let (insertions, deletions, status) = file.map_or((0, 0, String::new()), |file| {
        (file.insertions, file.deletions, file.status)
    });
    let header = div()
        .flex()
        .min_h(px(44.0))
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.45))
        .px(px(10.0))
        .py(px(6.0))
        .child(
            div()
                .id(("turn-card-toggle", at))
                .debug_selector(move || format!("turn-card-{at}"))
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .on_click(cx.listener(move |column, _, _, cx| column.toggle_turn_card(at, cx)))
                .child(
                    Icon::new(if open {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    })
                    .size(14.0)
                    .color(colors.muted_foreground),
                )
                .child(img(file_icon(&name)).size(px(14.0)).flex_shrink_0())
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_baseline()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(
                            div()
                                .flex_shrink_0()
                                .max_w_full()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(name),
                        )
                        .when(!folder.is_empty(), |line| {
                            line.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .ml(px(6.0))
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(colors.muted_foreground)
                                    .child(folder),
                            )
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0))
                .text_size(px(10.0))
                .when(insertions > 0, |row| {
                    row.child(
                        div()
                            .text_color(emerald(cx))
                            .child(format!("+{insertions}")),
                    )
                })
                .when(deletions > 0, |row| {
                    row.child(
                        div()
                            .text_color(colors.destructive)
                            .child(format!("−{deletions}")),
                    )
                })
                .child(
                    div()
                        .min_w(px(16.0))
                        .text_center()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.muted_foreground)
                        .child(status),
                ),
        );
    div()
        .overflow_hidden()
        .rounded(px(12.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background)
        .shadow(super::super::changes::card_shadow())
        .child(header)
        .when(open, |article| article.child(card_body(column, at, cx)))
        .into_any_element()
}

/// The floating pill (`rounded-full … shadow-lg`) with one ghost button.
fn pill(
    id: &'static str,
    label: SharedString,
    busy: bool,
    on_click: impl Fn(&mut CodeColumn, &mut Context<CodeColumn>) + 'static,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(12.0))
        .flex()
        .justify_center()
        .px(px(12.0))
        .child(
            div()
                .debug_selector(move || id.to_string())
                .flex()
                .items_center()
                .rounded_full()
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.background.opacity(0.95))
                .p(px(4.0))
                .shadow_lg()
                .child(
                    div()
                        .id(id)
                        .flex()
                        .h(px(32.0))
                        .items_center()
                        .gap(px(6.0))
                        .rounded_full()
                        .px(px(12.0))
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .map(|button| {
                            if busy {
                                button.opacity(0.5)
                            } else {
                                let hover = colors.accent;
                                button
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(hover))
                                    .on_click(
                                        cx.listener(move |column, _, _, cx| on_click(column, cx)),
                                    )
                            }
                        })
                        .child(if busy {
                            spinner((id, 0usize), 14.0).into_any_element()
                        } else {
                            Icon::new("rotate-ccw").size(14.0).into_any_element()
                        })
                        .child(label),
                ),
        )
        .into_any_element()
}

/// "Desfazer" for one project, or `TurnUndoAcrossProjects` for several.
fn undo_controls(
    column: &CodeColumn,
    review: &TurnReviewSummary,
    cx: &mut Context<CodeColumn>,
) -> Vec<AnyElement> {
    if column.turn.members.len() > 1 {
        let plan = plan_turn_undo(&column.turn_projects());
        if plan.targets.is_empty() {
            return Vec::new();
        }
        let projects = &column.turn.projects;
        let blocked = projects.blocked.as_ref().map(|(project, reason)| {
            let colors = cx.theme().colors.clone();
            let text = format!(
                "{} {}",
                cx.t_with(
                    "turnReview.turnUndo.blocked",
                    &Args::default().with("project", project.clone())
                ),
                cx.t(&format!(
                    "turnReview.guardedUndo.failureReasons.{}",
                    resolve_guarded_undo_failure_reason(Some(reason))
                ))
            );
            div()
                .debug_selector(|| "turn-projects-blocked".into())
                .absolute()
                .left(px(12.0))
                .right(px(12.0))
                .bottom(px(56.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(tw::AMBER_500.alpha(0.4))
                .bg(colors.background.opacity(0.95))
                .px(px(10.0))
                .py(px(6.0))
                .text_size(px(10.5))
                .text_color(
                    if dark(cx) {
                        tw::AMBER_400
                    } else {
                        tw::AMBER_700
                    }
                    .hsla(),
                )
                .shadow_sm()
                .child(text)
                .into_any_element()
        });
        let label = cx.t_with(
            "turnReview.turnUndo.action",
            &Args::count(plan.targets.len() as i64),
        );
        let busy = projects.preparing;
        return blocked
            .into_iter()
            .chain([pill(
                "turn-undo-projects",
                label,
                busy,
                |column, cx| column.prepare_projects_undo(cx),
                cx,
            )])
            .collect();
    }
    if !can_prepare_guarded_undo(review.guarded_undo.as_ref(), review.active_undo.as_ref()) {
        return Vec::new();
    }
    vec![pill(
        "turn-undo",
        cx.t("turnReview.guardedUndo.action"),
        column.turn.single.preparing,
        |column, cx| column.prepare_undo(cx),
        cx,
    )]
}

/// The surface itself, in place of the change list.
pub(crate) fn surface(column: &mut CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let review = match &column.turn.review {
        None => {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_size(px(14.0))
                .text_color(colors.muted_foreground)
                .child(spinner("turn-loading", 16.0))
                .child(cx.t("turnReview.loading"))
                .into_any_element();
        }
        Some(Err(())) => {
            return div()
                .m(px(16.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.destructive.opacity(0.3))
                .bg(colors.destructive.opacity(0.05))
                .p(px(12.0))
                .text_size(px(14.0))
                .text_color(colors.destructive)
                .child(cx.t("turnReview.failed"))
                .into_any_element();
        }
        Some(Ok(None)) => {
            return centered(cx.t("turnReview.unavailable"), colors.muted_foreground);
        }
        Some(Ok(Some(review))) => review.clone(),
    };
    for at in 0..column.turn.cards.len() {
        if column.turn.cards[at].open {
            column.load_turn_diff(at, cx);
        }
    }
    let files = if review.files.is_empty() {
        div()
            .flex()
            .min_h(px(144.0))
            .items_center()
            .justify_center()
            .px(px(16.0))
            .text_center()
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .child(cx.t(if review.state == "collecting" {
                "turnReview.collectingHint"
            } else {
                "turnReview.noChanges"
            }))
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children((0..column.turn.cards.len()).map(|at| card(column, at, cx)))
            .into_any_element()
    };
    let controls = undo_controls(column, &review, cx);
    div()
        .debug_selector(|| "turn-review".into())
        .relative()
        .flex()
        .flex_1()
        .min_h_0()
        .flex_col()
        .bg(colors.background)
        .child(summary(column, &review, cx))
        .child(
            div()
                .id("turn-files")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .bg(colors.muted.opacity(0.08))
                .px(px(8.0))
                .pt(px(8.0))
                .pb(px(80.0))
                .child(files),
        )
        .children(controls)
        .into_any_element()
}
