//! `WorkspaceDeliveryFailureSection`: the last failed delivery, folded to
//! its title and class; open, the hint, branch/commit/remote/target, the
//! captured output and changed files, and the safe recoveries.

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use super::super::CodeColumn;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, Icon, Tracked, dialog_footer, dialog_header, spinner,
};

fn amber(dark: bool, light: tw::Oklch, dark_shade: tw::Oklch) -> Hsla {
    if dark { dark_shade } else { light }.hsla()
}

/// A 2-column cell: an icon and one truncated line.
fn cell(icon: &'static str, text: SharedString, cx: &Context<CodeColumn>) -> AnyElement {
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
        .child(Icon::new(icon).size(14.0))
        .child(div().min_w_0().truncate().child(text))
        .into_any_element()
}

fn label(icon: &'static str, text: SharedString, cx: &Context<CodeColumn>) -> AnyElement {
    div()
        .mb(px(4.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_color(cx.theme().colors.muted_foreground)
        .font_weight(FontWeight::MEDIUM)
        .child(Icon::new(icon).size(12.0))
        .child(Tracked::new(text.to_uppercase(), 9.0, 0.08))
        .into_any_element()
}

fn action(
    column: &CodeColumn,
    id: &'static str,
    icon: &'static str,
    text: SharedString,
    ghost: bool,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let running = column.review.recovering.as_deref() == Some(id);
    let busy = column.review.recovering.is_some();
    Button::new(SharedString::from(format!("failure-{id}")))
        .variant(if ghost {
            ButtonVariant::Ghost
        } else {
            ButtonVariant::Outline
        })
        .size(ButtonSize::Sm)
        .disabled(busy)
        .on_click(cx.listener(move |column, _, _, cx| column.recover(id, false, cx)))
        .child(if running {
            spinner(SharedString::from(format!("failure-{id}-busy")), 12.0).into_any_element()
        } else {
            Icon::new(icon).size(12.0).into_any_element()
        })
        .child(div().text_size(px(9.5)).child(text))
        .into_any_element()
}

pub(crate) fn failure_section(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let failure = column.review.failure.as_ref()?;
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let open = column.review.failure_open;
    let known = failure.classification != "unknown";
    let class = cx.t(&format!(
        "inspector.deliveryFailure.classification.{}",
        failure.classification
    ));
    let badge = div()
        .flex()
        .h(px(20.0))
        .flex_shrink_0()
        .items_center()
        .rounded_full()
        .border_1()
        .px(px(6.0))
        .text_size(px(9.0))
        .font_weight(FontWeight::MEDIUM)
        .map(|badge| {
            if known {
                badge
                    .border_color(tw::AMBER_500.alpha(0.25))
                    .bg(tw::AMBER_500.alpha(0.1))
                    .text_color(amber(dark, tw::AMBER_700, tw::AMBER_300))
            } else {
                badge
                    .border_color(colors.border.opacity(0.7))
                    .bg(colors.muted.opacity(0.6))
                    .text_color(colors.muted_foreground)
            }
        })
        .child(class);
    let header = div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(8.0))
        .child(
            div()
                .id("failure-toggle")
                .debug_selector(|| "failure-toggle".into())
                .flex()
                .h(px(24.0))
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .ml(px(-4.0))
                .px(px(4.0))
                .cursor_pointer()
                .on_click(cx.listener(|column, _, _, cx| column.toggle_failure(cx)))
                .child(
                    Icon::new("chevron-right")
                        .size(14.0)
                        .rotate(if open { 90.0 } else { 0.0 }),
                )
                .child(Icon::new("triangle-alert").size(14.0).color(amber(
                    dark,
                    tw::AMBER_600,
                    tw::AMBER_400,
                )))
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("inspector.deliveryFailure.title")),
                ),
        )
        .child(badge);
    let body = open.then(|| {
        let mut actions = Vec::new();
        if failure.offers("retry") {
            actions.push(action(
                column,
                "retry",
                "rotate-ccw",
                cx.t("inspector.deliveryFailure.retry"),
                false,
                cx,
            ));
        }
        if failure.offers("synchronize") {
            actions.push(action(
                column,
                "synchronize",
                "refresh-cw",
                cx.t("inspector.deliveryFailure.synchronize"),
                false,
                cx,
            ));
        }
        if failure.offers("send-to-agent") {
            actions.push(action(
                column,
                "send-to-agent",
                "bot",
                cx.t("inspector.deliveryFailure.sendToAgent"),
                false,
                cx,
            ));
        }
        if failure.offers("open-external") {
            actions.push(action(
                column,
                "open-external",
                "external-link",
                cx.t("inspector.deliveryFailure.openExternal"),
                true,
                cx,
            ));
        }
        let unavailable = |value: Option<&String>, key: &str| -> SharedString {
            value
                .cloned()
                .map(SharedString::from)
                .unwrap_or_else(|| cx.t(key))
        };
        let push_target = failure
            .push_target
            .as_ref()
            .map(|target| format!("{}/{}", target.remote, target.branch));
        let sha = failure
            .head_sha
            .as_ref()
            .map(|sha| sha.chars().take(8).collect::<String>());
        let captured = cx.t_with(
            "inspector.deliveryFailure.capturedContext",
            &Args::default()
                .with(
                    "operation",
                    cx.t(&format!(
                        "inspector.deliveryFailure.operation.{}",
                        failure.operation
                    ))
                    .to_string(),
                )
                .with(
                    "time",
                    crate::data::time::local_date_time(&failure.created_at),
                ),
        );
        let note = |text: SharedString| {
            div()
                .mt(px(4.0))
                .text_size(px(9.0))
                .text_color(amber(dark, tw::AMBER_700, tw::AMBER_300))
                .child(text)
        };
        let files: Vec<AnyElement> = failure
            .changed_files
            .iter()
            .map(|path| {
                div()
                    .truncate()
                    .font_family(fonts::MONO)
                    .text_size(px(9.5))
                    .text_color(colors.muted_foreground)
                    .child(path.clone())
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .border_t_1()
            .border_color(tw::AMBER_500.alpha(0.15))
            .px(px(10.0))
            .py(px(8.0))
            .child(
                div()
                    .text_size(px(9.5))
                    .line_height(px(15.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(&format!(
                        "inspector.deliveryFailure.classificationHint.{}",
                        failure.classification
                    ))),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(6.0))
                    .child(cell(
                        "git-branch",
                        unavailable(
                            failure.branch.as_ref(),
                            "inspector.deliveryFailure.branchUnavailable",
                        ),
                        cx,
                    ))
                    .child(cell(
                        "git-commit-horizontal",
                        unavailable(sha.as_ref(), "inspector.deliveryFailure.commitUnavailable"),
                        cx,
                    ))
                    .child(cell(
                        "server",
                        unavailable(
                            failure.remote.as_ref(),
                            "inspector.deliveryFailure.remoteUnavailable",
                        ),
                        cx,
                    ))
                    .child(cell(
                        "git-branch",
                        unavailable(
                            push_target.as_ref(),
                            "inspector.deliveryFailure.pushTargetUnavailable",
                        ),
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(colors.muted_foreground)
                    .child(captured),
            )
            .child(
                div()
                    .child(label(
                        "square-terminal",
                        cx.t("inspector.deliveryFailure.output"),
                        cx,
                    ))
                    .child(
                        div()
                            .id("failure-output")
                            .max_h(px(144.0))
                            .overflow_y_scroll()
                            .rounded(px(6.0))
                            .bg(colors.background.opacity(0.7))
                            .p(px(8.0))
                            .font_family(fonts::MONO)
                            .text_size(px(9.5))
                            .line_height(px(13.8))
                            .text_color(colors.foreground.opacity(0.85))
                            .child(failure.output.clone()),
                    )
                    .when(failure.output_truncated, |output| {
                        output.child(note(cx.t("inspector.deliveryFailure.outputTruncated")))
                    }),
            )
            .child(
                div()
                    .child(label(
                        "file-exclamation-point",
                        cx.t_with(
                            "inspector.deliveryFailure.changedFiles",
                            &Args::count(failure.changed_files.len() as i64),
                        ),
                        cx,
                    ))
                    .child(if files.is_empty() {
                        div()
                            .text_size(px(9.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("inspector.deliveryFailure.noChangedFiles"))
                            .into_any_element()
                    } else {
                        div()
                            .id("failure-files")
                            .max_h(px(96.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .rounded(px(6.0))
                            .bg(colors.background.opacity(0.5))
                            .px(px(8.0))
                            .py(px(6.0))
                            .children(files)
                            .into_any_element()
                    })
                    .when(failure.changed_files_truncated, |files| {
                        files.child(note(cx.t("inspector.deliveryFailure.filesTruncated")))
                    }),
            )
            .when(!actions.is_empty(), |body| {
                body.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .border_t_1()
                        .border_color(tw::AMBER_500.alpha(0.15))
                        .pt(px(8.0))
                        .children(actions),
                )
            })
    });
    Some(
        div()
            .debug_selector(|| "delivery-failure".into())
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(px(6.0))
            .border_1()
            .border_color(tw::AMBER_500.alpha(0.3))
            .bg(tw::AMBER_500.alpha(0.045))
            .child(header)
            .children(body)
            .into_any_element(),
    )
}

/// `window.confirm` before repeating or synchronizing a delivery.
pub(crate) fn recovery_confirm(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let action = column.review.confirm_recovery.clone()?;
    let failure = column.review.failure.as_ref()?;
    let repeats_update =
        action == "retry" && matches!(failure.operation.as_str(), "fetch" | "pull");
    let key = if action == "retry" && !repeats_update {
        "inspector.deliveryFailure.confirmRetry"
    } else {
        "inspector.deliveryFailure.confirmSynchronize"
    };
    let header = dialog_header(cx.t(key), None, cx);
    let confirmed = action.clone();
    let footer = dialog_footer(
        [
            Button::new("failure-confirm-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .child(cx.t("inspector.gitConfirmation.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.cancel_recovery(cx)))
                .into_any_element(),
            Button::new("failure-confirm-run")
                .size(ButtonSize::Default)
                .child("OK")
                .on_click(cx.listener(move |column, _, _, cx| {
                    let action = confirmed.clone();
                    column.recover(&action, true, cx)
                }))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("failure-confirm", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.cancel_recovery(cx));
        })
        .close_button(false)
        .child(header)
        .child(footer)
        .into_any_element(),
    )
}
