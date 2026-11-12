//! A run of a project's configured checks (`WorkspaceGitValidationReport`):
//! passed or failed, then each command with ✓/✕, its time and exit code,
//! and what it printed. The merge resolver shows it after "Validar", and
//! the delivery after a push the before-push checks blocked.

use dcc_core::domain::merge_conflict::{
    WorkspaceGitValidationReport, WorkspaceGitValidationStatus,
};
use gpui::{AnyElement, App, FontWeight, SharedString, div, prelude::*, px};

use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::Icon;

pub(crate) fn report_card(report: &WorkspaceGitValidationReport, cx: &App) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let failed = report.status == WorkspaceGitValidationStatus::Failed;
    let headline = match report.status {
        WorkspaceGitValidationStatus::Failed => cx.t("mergeConflict.validation.failed"),
        WorkspaceGitValidationStatus::Passed => cx.t("mergeConflict.validation.passed"),
        WorkspaceGitValidationStatus::NotConfigured => {
            cx.t("mergeConflict.validation.notConfigured")
        }
    };
    let tone = if failed {
        colors.destructive
    } else {
        tw::EMERALD_500.hsla()
    };
    div()
        .debug_selector(|| "merge-validation-report".into())
        .mt(px(16.0))
        .w_full()
        .rounded(px(8.0))
        .border_1()
        .border_color(tone.opacity(0.3))
        .bg(tone.opacity(0.05))
        .p(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(
                    div()
                        .text_color(if failed {
                            colors.destructive
                        } else {
                            tw::EMERALD_600.hsla()
                        })
                        .child(Icon::new(if failed { "circle-x" } else { "check" }).size(16.0)),
                )
                .child(headline),
        )
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(report.steps.iter().map(|step| {
                    let exit = step
                        .exit_code
                        .map(|code| {
                            cx.t_with(
                                "mergeConflict.validation.exitCode",
                                &Args::default().with("code", code.to_string()),
                            )
                            .to_string()
                        })
                        .unwrap_or_default();
                    let meta = cx.t_with(
                        "mergeConflict.validation.stepMeta",
                        &Args::default()
                            .with(
                                "seconds",
                                format!("{:.1}", step.duration_ms as f64 / 1000.0),
                            )
                            .with("exitCode", exit),
                    );
                    div()
                        .overflow_hidden()
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.5))
                        .bg(colors.background.opacity(0.6))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .px(px(10.0))
                                .py(px(6.0))
                                .text_size(px(11.0))
                                .child(
                                    div()
                                        .text_color(if step.success {
                                            tw::EMERALD_600.hsla()
                                        } else {
                                            colors.destructive
                                        })
                                        .child(if step.success { "✓" } else { "✕" }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(fonts::MONO)
                                        .child(step.command.clone()),
                                )
                                .child(div().text_color(colors.muted_foreground).child(meta)),
                        )
                        .when(!step.output.is_empty(), |row| {
                            row.child(
                                div()
                                    .id(SharedString::from(format!("step-{}", step.command)))
                                    .max_h(px(192.0))
                                    .overflow_y_scroll()
                                    .border_t_1()
                                    .border_color(colors.border.opacity(0.4))
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .font_family(fonts::MONO)
                                    .text_size(px(10.0))
                                    .line_height(px(16.0))
                                    .text_color(colors.muted_foreground)
                                    .child(step.output.clone()),
                            )
                        })
                })),
        )
        .into_any_element()
}
