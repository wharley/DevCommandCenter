//! "Experimental": the local UX signals — how many times each thing was
//! found or used and how long into the run the first time came — with
//! "Limpar métricas".

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::Settings;
use crate::data::ux_metrics::SHOWN;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant};

impl Settings {
    pub(super) fn experimental(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let rows = SHOWN.iter().enumerate().map(|(index, name)| {
            let value = match self.metrics.get(*name) {
                Some(metric) => cx.t_with(
                    "settings.experimental.metricValue",
                    &Args::count(metric.count as i64).with(
                        "seconds",
                        format!("{:.1}", metric.first_elapsed_ms as f64 / 1000.0),
                    ),
                ),
                None => cx.t("settings.experimental.metricEmpty"),
            };
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .py(px(10.0))
                .text_size(px(12.0))
                // `divide-y divide-border/40`
                .when(index > 0, |row| {
                    row.border_t_1().border_color(colors.border.opacity(0.4))
                })
                .child(
                    div()
                        .text_color(colors.foreground)
                        .child(cx.t(&format!("settings.experimental.metrics.{name}"))),
                )
                .child(
                    div()
                        .font_features(crate::ui::tabular_nums())
                        .text_color(colors.muted_foreground)
                        .child(value),
                )
        });
        div()
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.15))
            .p(px(16.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("settings.experimental.uxMetricsTitle")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("settings.experimental.uxMetricsHint")),
                            ),
                    )
                    .child(
                        Button::new("settings-reset-metrics")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(|settings, _, _, cx| settings.reset_metrics(cx)))
                            .child(cx.t("settings.experimental.resetMetrics")),
                    ),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .rounded(px(10.0))
                    .bg(colors.background)
                    .px(px(12.0))
                    .children(rows),
            )
            .into_any_element()
    }
}
