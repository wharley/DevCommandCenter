//! "Quando você usa": weekdays (Seg..Dom) by local hours (00–23), each
//! cell's tint by its share of the busiest cell, each weekday's share of
//! the metric on the right.

use dcc_core::domain::usage_logs::UsageSummary;
use gpui::{AnyElement, Context, div, prelude::*, px};

use super::Usage;
use super::filters::metric_key;
use super::sections::{card, card_title};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::tabular_nums;

const WEEKDAYS: [&str; 7] = [
    "usage.heatmap.mon",
    "usage.heatmap.tue",
    "usage.heatmap.wed",
    "usage.heatmap.thu",
    "usage.heatmap.fri",
    "usage.heatmap.sat",
    "usage.heatmap.sun",
];

/// A cell's tint, 0 for nothing and up to 1 for the busiest cell.
pub(super) fn intensity(value: f64, busiest: f64) -> f32 {
    if busiest <= 0.0 || value <= 0.0 {
        0.0
    } else {
        (0.15 + 0.85 * value / busiest).min(1.0) as f32
    }
}

impl Usage {
    pub(super) fn heatmap(&self, summary: &UsageSummary, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let busiest = summary
            .heatmap
            .iter()
            .flat_map(|day| day.iter())
            .copied()
            .fold(0.0, f64::max);
        let accent = tw::EMERALD_400.hsla();
        let label = |text: gpui::SharedString| {
            div()
                .w(px(28.0))
                .flex_none()
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(text)
        };
        let hours = div()
            .flex()
            .gap(px(2.0))
            .child(label("".into()))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_between()
                    .text_size(px(9.0))
                    .text_color(colors.muted_foreground)
                    .children(["00", "06", "12", "18", "23"].map(|hour| div().child(hour))),
            )
            .child(div().w(px(36.0)).flex_none());
        let rows = WEEKDAYS.iter().enumerate().map(|(weekday, key)| {
            let share = if summary.metric_total > 0.0 {
                summary.weekdays[weekday] / summary.metric_total * 100.0
            } else {
                0.0
            };
            let cells = summary.heatmap[weekday].iter().map(|value| {
                let tint = intensity(*value, busiest);
                div()
                    .flex_1()
                    .h(px(14.0))
                    .rounded(px(2.0))
                    .bg(if tint > 0.0 {
                        accent.opacity(tint)
                    } else {
                        colors.muted.opacity(0.35)
                    })
            });
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(label(cx.t(key)))
                .child(div().flex_1().flex().gap(px(2.0)).children(cells))
                .child(
                    div()
                        .w(px(36.0))
                        .flex_none()
                        .text_right()
                        .text_size(px(10.0))
                        .font_features(tabular_nums())
                        .text_color(colors.muted_foreground)
                        .child(format!("{share:.0}%")),
                )
        });
        let metric = cx.t(metric_key(self.metric));
        card(cx)
            .debug_selector(|| "usage-heatmap".into())
            .child(div().mb(px(12.0)).child(card_title(
                cx.t("usage.heatmap.title"),
                Some(cx.t_with(
                    "usage.heatmap.subtitle",
                    &Args::default().with("metric", metric),
                )),
                cx,
            )))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(hours)
                    .children(rows),
            )
            .into_any_element()
    }
}
