//! Guarded Undo's confirmations: what each restore puts back (path, kind,
//! size and a text preview), the warning, the preview's time, the run and
//! its result — for one project, or for every project of the turn.

use dcc_core::domain::turn_review::{
    GuardedUndoPreviewFile, plan_turn_undo, resolve_guarded_undo_failure_reason,
};
use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use super::super::CodeColumn;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_footer, dialog_header, spinner,
};

fn dark(cx: &Context<CodeColumn>) -> bool {
    cx.theme().appearance == Appearance::Dark
}

/// `Intl.NumberFormat(…, { style: "unit", unit: "byte", unitDisplay: "short" })`.
fn bytes(count: u64, cx: &Context<CodeColumn>) -> String {
    let separator = match cx.global::<I18n>().locale() {
        Locale::PtBr => '.',
        Locale::En => ',',
    };
    let digits = count.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    format!("{grouped} byte")
}

/// `rounded-md border border-amber-500/40 bg-amber-500/5 p-3` with the icon.
fn warning(text: SharedString, cx: &Context<CodeColumn>) -> AnyElement {
    div()
        .flex()
        .gap(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(tw::AMBER_500.alpha(0.4))
        .bg(tw::AMBER_500.alpha(0.05))
        .p(px(12.0))
        .text_color(
            if dark(cx) {
                tw::AMBER_300
            } else {
                tw::AMBER_800
            }
            .hsla(),
        )
        .child(
            div()
                .mt(px(2.0))
                .child(Icon::new("triangle-alert").size(16.0)),
        )
        .child(div().flex_1().min_w_0().child(text))
        .into_any_element()
}

/// A file the restore puts back.
fn file_row(file: &GuardedUndoPreviewFile, cx: &Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let kind = cx.t(if file.binary {
        "turnReview.guardedUndo.binary"
    } else {
        "turnReview.guardedUndo.text"
    });
    div()
        .overflow_hidden()
        .rounded(px(6.0))
        .border_1()
        .border_color(colors.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .bg(colors.muted.opacity(0.4))
                .px(px(10.0))
                .py(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(fonts::MONO)
                        .child(file.display_path.clone()),
                )
                .child(div().text_color(colors.muted_foreground).child(kind))
                .child(
                    div()
                        .text_color(colors.muted_foreground)
                        .child(format!("{} B", file.size)),
                ),
        )
        .when_some(
            file.preview.clone().filter(|_| !file.binary),
            |row, preview| {
                row.child(
                    div()
                        .max_h(px(208.0))
                        .overflow_hidden()
                        .p(px(10.0))
                        .font_family(fonts::MONO)
                        .text_size(px(10.5))
                        .line_height(px(20.0))
                        .child(preview),
                )
            },
        )
        .into_any_element()
}

fn executing(steps: bool, cx: &Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .flex()
        .items_start()
        .gap(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(colors.primary.opacity(0.3))
        .bg(colors.primary.opacity(0.05))
        .p(px(12.0))
        .child(
            div()
                .mt(px(2.0))
                .text_color(colors.primary)
                .child(spinner("turn-undo-executing", 16.0)),
        )
        .child(
            div()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("turnReview.guardedUndo.executing")),
                )
                .when(steps, |text| {
                    text.child(
                        div()
                            .mt(px(4.0))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("turnReview.guardedUndo.executingSteps")),
                    )
                }),
        )
        .into_any_element()
}

/// The result box's colors (`completed` emerald, recovery destructive, else amber).
fn result_colors(status: &str, cx: &Context<CodeColumn>) -> (Hsla, Hsla, Hsla) {
    let colors = cx.theme().colors.clone();
    match status {
        "completed" => (
            tw::EMERALD_500.alpha(0.4),
            tw::EMERALD_500.alpha(0.05),
            if dark(cx) {
                tw::EMERALD_400
            } else {
                tw::EMERALD_700
            }
            .hsla(),
        ),
        "recovery_required" => (
            colors.destructive.opacity(0.4),
            colors.destructive.opacity(0.05),
            colors.destructive,
        ),
        _ => (
            tw::AMBER_500.alpha(0.4),
            tw::AMBER_500.alpha(0.05),
            if dark(cx) {
                tw::AMBER_400
            } else {
                tw::AMBER_700
            }
            .hsla(),
        ),
    }
}

fn restore_button(
    id: &'static str,
    label: SharedString,
    busy: bool,
    disabled: bool,
    on_click: impl Fn(&mut CodeColumn, &mut Context<CodeColumn>) + 'static,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    Button::new(id)
        .variant(ButtonVariant::Destructive)
        .size(ButtonSize::Default)
        .disabled(disabled)
        .on_click(cx.listener(move |column, _, _, cx| on_click(column, cx)))
        .child(if busy {
            spinner((id, 0usize), 16.0).into_any_element()
        } else {
            Icon::new("rotate-ccw").size(16.0).into_any_element()
        })
        .child(label)
        .into_any_element()
}

fn single_dialog(
    column: &CodeColumn,
    tall: f32,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let single = &column.turn.single;
    if !single.dialog {
        return None;
    }
    let prepared = single.prepared.as_ref()?;
    let colors = cx.theme().colors.clone();
    let header = dialog_header(
        cx.t("turnReview.guardedUndo.confirmTitle"),
        Some(
            cx.t_with(
                "turnReview.guardedUndo.confirmSummary",
                &Args::count(i64::from(prepared.file_count))
                    .with("bytes", bytes(prepared.total_bytes, cx)),
            ),
        ),
        cx,
    );
    let outcome = single.outcome.as_ref().map(|(status, reason)| {
        let (border, background, text) = result_colors(status, cx);
        let reason = reason
            .as_deref()
            .map(|reason| resolve_guarded_undo_failure_reason(Some(reason)));
        div()
            .debug_selector(|| "turn-undo-result".into())
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(background)
            .p(px(12.0))
            .text_color(text)
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(cx.t(&format!("turnReview.guardedUndo.results.{status}"))),
            )
            .when_some(reason, |block, reason| {
                block.child(
                    div()
                        .mt(px(4.0))
                        .child(cx.t(&format!("turnReview.guardedUndo.failureReasons.{reason}"))),
                )
            })
    });
    let body = div()
        .id("turn-undo-files")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(px(12.0))
        .overflow_y_scroll()
        .pr(px(4.0))
        .py(px(12.0))
        .text_size(px(12.0))
        .child(warning(cx.t("turnReview.guardedUndo.scopeWarning"), cx))
        .child(div().text_color(colors.muted_foreground).child(cx.t_with(
            "turnReview.guardedUndo.previewExpires",
            &Args::default().with("expiresAt", prepared.expires_at.clone()),
        )))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(prepared.files.iter().map(|file| file_row(file, cx))),
        )
        .when(single.executing, |body| body.child(executing(true, cx)))
        .when(single.expired && single.outcome.is_none(), |body| {
            body.child(
                div()
                    .text_color(colors.destructive)
                    .child(cx.t("turnReview.guardedUndo.previewExpired")),
            )
        })
        .when(single.execute_failed, |body| {
            body.child(
                div()
                    .text_color(colors.destructive)
                    .child(cx.t("turnReview.guardedUndo.executeFailed")),
            )
        })
        .children(outcome);
    let mut buttons = vec![
        Button::new("turn-undo-cancel")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Default)
            .disabled(single.executing)
            .on_click(cx.listener(|column, _, _, cx| column.close_undo(cx)))
            .child(cx.t(if single.outcome.is_some() {
                "turnReview.guardedUndo.closeResult"
            } else {
                "turnReview.guardedUndo.cancel"
            }))
            .into_any_element(),
    ];
    if single.outcome.is_none() && !single.execute_failed {
        buttons.push(restore_button(
            "turn-undo-confirm",
            cx.t_with(
                "turnReview.guardedUndo.confirmAction",
                &Args::count(i64::from(prepared.file_count)),
            ),
            single.executing,
            single.executing || single.expired,
            |column, cx| column.execute_undo(cx),
            cx,
        ));
    }
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("turn-undo-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.close_undo(cx));
        })
        .max_width(672.0)
        .max_height(tall)
        .close_button(!single.executing)
        .gap(0.0)
        .child(header)
        .child(body)
        .child(dialog_footer(buttons, cx))
        .into_any_element(),
    )
}

/// A project's result chip (`turnReview.turnUndo.results.*`).
fn step_status(status: &str, cx: &Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let color = match status {
        "completed" => tw::EMERALD_600.hsla(),
        "not_attempted" => colors.muted_foreground,
        "recovery_required" => colors.destructive,
        _ => tw::AMBER_600.hsla(),
    };
    div()
        .flex()
        .flex_1()
        .justify_end()
        .items_center()
        .gap(px(4.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .when(status == "completed", |chip| {
            chip.child(Icon::new("circle-check").size(14.0))
        })
        .child(cx.t(&format!("turnReview.turnUndo.results.{status}")))
        .into_any_element()
}

fn projects_dialog(
    column: &CodeColumn,
    tall: f32,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let projects = &column.turn.projects;
    if !projects.dialog {
        return None;
    }
    let prepared = projects.prepared.as_ref()?;
    let colors = cx.theme().colors.clone();
    let files: u32 = prepared
        .iter()
        .map(|project| project.preview.file_count)
        .sum();
    let header = dialog_header(
        cx.t("turnReview.turnUndo.confirmTitle"),
        Some(cx.t_with(
            "turnReview.turnUndo.confirmSummary",
            &Args::count(i64::from(files)).with("projects", prepared.len().to_string()),
        )),
        cx,
    );
    let uncovered = plan_turn_undo(&column.turn_projects()).uncovered;
    let sections: Vec<AnyElement> = prepared
        .iter()
        .map(|project| {
            let result = projects.results.as_ref().and_then(|results| {
                results
                    .iter()
                    .find(|result| result.workspace_id == project.target.workspace_id)
            });
            let reason = result
                .filter(|result| !matches!(result.status, "completed" | "not_attempted"))
                .map(|result| resolve_guarded_undo_failure_reason(result.reason_code.as_deref()));
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(project.target.name.clone()),
                        )
                        .child(div().text_color(colors.muted_foreground).child(cx.t_with(
                            "turnReview.fileCount",
                            &Args::count(i64::from(project.preview.file_count)),
                        )))
                        .children(result.map(|result| step_status(result.status, cx))),
                )
                .when_some(reason, |section, reason| {
                    section.child(
                        div()
                            .text_color(
                                if dark(cx) {
                                    tw::AMBER_400
                                } else {
                                    tw::AMBER_700
                                }
                                .hsla(),
                            )
                            .child(
                                cx.t(&format!("turnReview.guardedUndo.failureReasons.{reason}")),
                            ),
                    )
                })
                .children(project.preview.files.iter().map(|file| file_row(file, cx)))
                .into_any_element()
        })
        .collect();
    let body = div()
        .id("turn-projects-files")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(px(12.0))
        .overflow_y_scroll()
        .pr(px(4.0))
        .py(px(12.0))
        .text_size(px(12.0))
        .child(warning(cx.t("turnReview.turnUndo.orderWarning"), cx))
        .when(!uncovered.is_empty(), |body| {
            let names: Vec<String> = uncovered.iter().map(|(_, name)| name.clone()).collect();
            body.child(div().text_color(colors.muted_foreground).child(cx.t_with(
                "turnReview.turnUndo.uncovered",
                &Args::default().with("projects", names.join(", ")),
            )))
        })
        .children(sections)
        .when(projects.executing, |body| body.child(executing(false, cx)))
        .when(projects.expired && projects.results.is_none(), |body| {
            body.child(
                div()
                    .text_color(colors.destructive)
                    .child(cx.t("turnReview.guardedUndo.previewExpired")),
            )
        })
        .when(projects.execute_failed, |body| {
            body.child(
                div()
                    .text_color(colors.destructive)
                    .child(cx.t("turnReview.guardedUndo.executeFailed")),
            )
        });
    let mut buttons = vec![
        Button::new("turn-projects-cancel")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Default)
            .disabled(projects.executing)
            .on_click(cx.listener(|column, _, _, cx| column.close_projects_undo(cx)))
            .child(cx.t(if projects.results.is_some() {
                "turnReview.guardedUndo.closeResult"
            } else {
                "turnReview.guardedUndo.cancel"
            }))
            .into_any_element(),
    ];
    if projects.results.is_none() && !projects.execute_failed {
        buttons.push(restore_button(
            "turn-projects-confirm",
            cx.t_with(
                "turnReview.turnUndo.confirmAction",
                &Args::count(prepared.len() as i64),
            ),
            projects.executing,
            projects.executing || projects.expired,
            |column, cx| column.execute_projects_undo(cx),
            cx,
        ));
    }
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("turn-projects-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.close_projects_undo(cx));
        })
        .max_width(672.0)
        .max_height(tall)
        .close_button(!projects.executing)
        .gap(0.0)
        .child(header)
        .child(body)
        .child(dialog_footer(buttons, cx))
        .into_any_element(),
    )
}

/// The open confirmation, if any.
pub(crate) fn dialogs(
    column: &CodeColumn,
    window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Vec<AnyElement> {
    // `max-h-[86vh]`
    let tall = f32::from(window.viewport_size().height) * 0.86;
    single_dialog(column, tall, cx)
        .into_iter()
        .chain(projects_dialog(column, tall, cx))
        .collect()
}
