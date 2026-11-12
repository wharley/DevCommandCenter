//! Every file resolved: the checks the project declares before the merge
//! commit (or that there are none, or that their config is broken), the
//! last run's report, and "Validar, concluir e enviar".

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::view::{note, title};
use super::{Busy, Load, MergeResolver};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, Icon, spinner};

pub(crate) fn completion(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let checks = match &resolver.validation {
        None | Some(Load::Pending) => div()
            .mt(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .child(spinner("merge-validation-loading", 14.0))
            .child(cx.t("mergeConflict.validation.loading"))
            .into_any_element(),
        Some(Load::Failed(error)) => div()
            .mt(px(20.0))
            .w_full()
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.destructive.opacity(0.3))
            .bg(colors.destructive.opacity(0.05))
            .p(px(12.0))
            .text_size(px(12.0))
            .text_color(colors.destructive)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Icon::new("circle-x").size(16.0))
                    .child(cx.t("mergeConflict.validation.invalidTitle")),
            )
            .child(div().mt(px(4.0)).child(error.clone()))
            .into_any_element(),
        Some(Load::Ready(config)) if !config.commands.is_empty() => div()
            .debug_selector(|| "merge-validation-commands".into())
            .mt(px(20.0))
            .w_full()
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.1))
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
                            .text_color(tw::VIOLET_500.hsla())
                            .child(Icon::new("list-checks").size(16.0)),
                    )
                    .child(cx.t("mergeConflict.validation.configuredTitle")),
            )
            .child(div().mt(px(8.0)).flex().flex_col().gap(px(6.0)).children(
                config.commands.iter().map(|command| {
                    div()
                        .rounded(px(4.0))
                        .bg(colors.muted)
                        .px(px(8.0))
                        .py(px(6.0))
                        .font_family(fonts::MONO)
                        .text_size(px(11.0))
                        .child(command.clone())
                }),
            ))
            .child(
                div()
                    .mt(px(8.0))
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t_with(
                        "mergeConflict.validation.timeout",
                        &Args::default().with("seconds", config.timeout_seconds.to_string()),
                    )),
            )
            .into_any_element(),
        Some(Load::Ready(_)) => div()
            .mt(px(20.0))
            .w_full()
            .rounded(px(8.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.1))
            .p(px(12.0))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .child(cx.t("mergeConflict.validation.empty"))
            .into_any_element(),
    };
    let report = resolver
        .report
        .as_ref()
        .map(|report| crate::screens::validation_report::report_card(report, cx));
    let with_checks =
        matches!(&resolver.validation, Some(Load::Ready(config)) if !config.commands.is_empty());
    let ready = matches!(&resolver.validation, Some(Load::Ready(_)));
    let busy = resolver.busy;
    div()
        .id("merge-completion")
        .flex()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .justify_center()
        .p(px(32.0))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .w_full()
                .max_w(px(672.0))
                .m_auto()
                .child(
                    div()
                        .flex()
                        .size(px(48.0))
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(tw::EMERALD_500.alpha(0.15))
                        .text_color(tw::EMERALD_600.hsla())
                        .child(Icon::new("check").size(24.0)),
                )
                .child(title(cx.t("mergeConflict.completion.title"), 16.0))
                .child(note(cx.t("mergeConflict.completion.description"), cx))
                .child(checks)
                .children(report)
                .child(
                    div().mt(px(20.0)).child(
                        Button::new("merge-complete")
                            .size(ButtonSize::Default)
                            .disabled(busy.is_some() || !ready)
                            .on_click(
                                cx.listener(|resolver, _, _, cx| resolver.request_complete(cx)),
                            )
                            .child(if busy == Some(Busy::Complete) {
                                spinner("merge-complete-busy", 16.0).into_any_element()
                            } else {
                                Icon::new("git-merge").size(16.0).into_any_element()
                            })
                            .child(cx.t(if with_checks {
                                "mergeConflict.completion.withValidation"
                            } else {
                                "mergeConflict.completion.withoutValidation"
                            })),
                    ),
                ),
        )
        .into_any_element()
}
