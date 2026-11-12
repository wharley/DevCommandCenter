//! The dashboard: eight KPI cards, the stats line (active days, streak,
//! peaks, subagents) with the cost's notes, the metric per day as stacked
//! bars grouped by runtime, model, via, project or agent, then "Quando
//! você usa" beside the four bands of the day with the hours' bars. Under
//! 1024px of window the pair stacks and the cards go two by two.

use dcc_core::domain::usage_logs::{GroupBy, Metric, OTHERS_GROUP, Runtime, UsageSummary, Via};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::Usage;
use super::charts::{self, Column};
use super::filters::{chip, group_key, metric_key, via_key};
use crate::data::usage::{
    compact, dollars_compact, group_color, percent, runtime_color, short_date,
};
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::{Icon, Tracked, tabular_nums};

/// The desktop's `lg` breakpoint.
const LARGE: f32 = 1024.0;

/// `rounded-lg border border-border/60 p-4`
pub(super) fn card(cx: &Context<Usage>) -> gpui::Div {
    div()
        .min_w_0()
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(cx.theme().colors.border.opacity(0.6))
        .p(px(16.0))
}

/// `h3 text-[13px] font-medium` with a 10px line under it.
pub(super) fn card_title(
    title: SharedString,
    subtitle: Option<SharedString>,
    cx: &Context<Usage>,
) -> gpui::Div {
    div()
        .min_w_0()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .children(subtitle.map(|subtitle| {
            div()
                .text_size(px(10.0))
                .line_height(px(15.0))
                .text_color(cx.theme().colors.muted_foreground)
                .child(subtitle)
        }))
}

/// Two side by side, or stacked.
fn pair(left: AnyElement, right: AnyElement, large: bool) -> AnyElement {
    let row = div().flex().gap(px(16.0));
    if large {
        row.child(div().flex_1().flex_basis(px(0.0)).min_w_0().child(left))
            .child(div().flex_1().flex_basis(px(0.0)).min_w_0().child(right))
            .into_any_element()
    } else {
        row.flex_col().child(left).child(right).into_any_element()
    }
}

/// `grid-cols-N gap-2`: `items` in rows of `columns`, each cell `flex-1`.
fn grid(items: Vec<AnyElement>, columns: usize, gap: f32) -> AnyElement {
    let mut rows = Vec::new();
    let mut items = items.into_iter().peekable();
    while items.peek().is_some() {
        let mut row = div().flex().gap(px(gap));
        for _ in 0..columns {
            row = row.child(
                div()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .min_w_0()
                    .children(items.next()),
            );
        }
        rows.push(row);
    }
    div()
        .flex()
        .flex_col()
        .gap(px(gap))
        .children(rows)
        .into_any_element()
}

/// A metric's value as the cards and charts write it.
pub(super) fn metric_value(metric: Metric, value: f64, locale: Locale) -> String {
    match metric {
        Metric::Cost => dollars_compact(value, locale),
        _ => compact(value.round().max(0.0) as u64, locale),
    }
}

/// One decimal in the language's style.
fn decimal(value: f64, locale: Locale) -> String {
    let text = format!("{value:.1}");
    match locale {
        Locale::PtBr => text.replace('.', ","),
        Locale::En => text,
    }
}

impl Usage {
    /// A legend group's name.
    pub(super) fn group_label(&self, key: &str, cx: &Context<Self>) -> SharedString {
        if key == OTHERS_GROUP {
            return cx.t("usage.group.others");
        }
        match self.group_by {
            GroupBy::Runtime => cx.t(&format!("usage.runtime.{key}")),
            GroupBy::Via => cx.t(&via_key(Via::parse(key))),
            GroupBy::Agent if key.is_empty() => cx.t("usage.group.main"),
            _ if key.is_empty() => cx.t("usage.group.unknown"),
            _ => SharedString::from(key.to_string()),
        }
    }

    /// The colors of the legend's groups, in order.
    fn group_colors(&self, summary: &UsageSummary) -> Vec<u32> {
        summary
            .groups
            .iter()
            .enumerate()
            .map(|(index, (key, _))| match self.group_by {
                GroupBy::Runtime => runtime_color(key),
                _ => group_color(index, key == OTHERS_GROUP),
            })
            .collect()
    }

    /// `width` is the window's (the breakpoints are the viewport's).
    pub(super) fn content(
        &self,
        summary: &UsageSummary,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let large = width >= LARGE;
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(16.0))
            .child(self.kpis(summary, large, cx))
            .child(self.stats(summary, cx))
            .child(self.days(summary, cx))
            .child(pair(
                self.heatmap(summary, cx),
                self.bands(summary, cx),
                large,
            ))
            .into_any_element()
    }

    fn kpis(&self, summary: &UsageSummary, large: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let tokens = &summary.tokens;
        let share = |part: u64, total: u64| format!("{:.0}", percent(part, total));
        let per = |part: u64, total: u64| {
            if total == 0 {
                0.0
            } else {
                part as f64 / total as f64
            }
        };
        let metric_name = cx.t(metric_key(self.metric)).to_lowercase();
        let cards = [
            (
                "sparkles",
                cx.t("usage.metric.totalTokens"),
                compact(tokens.total(), locale),
                cx.t_with(
                    "usage.kpi.cacheShare",
                    &Args::default().with("percent", share(tokens.cache_read, tokens.total())),
                ),
            ),
            (
                "zap",
                cx.t("usage.metric.newTokens"),
                compact(tokens.new_tokens(), locale),
                cx.t("usage.kpi.newHint"),
            ),
            (
                "message-square",
                cx.t("usage.metric.output"),
                compact(tokens.output, locale),
                cx.t_with(
                    "usage.kpi.reasoningShare",
                    &Args::default().with("percent", share(tokens.reasoning, tokens.output)),
                ),
            ),
            (
                "coins",
                cx.t("usage.metric.cost"),
                dollars_compact(summary.cost_usd, locale),
                cx.t_with(
                    "usage.kpi.costSplit",
                    &Args::default()
                        .with(
                            "claude",
                            dollars_compact(
                                summary.cost_by_runtime[Runtime::Claude.index()],
                                locale,
                            ),
                        )
                        .with(
                            "codex",
                            dollars_compact(
                                summary.cost_by_runtime[Runtime::Codex.index()],
                                locale,
                            ),
                        ),
                ),
            ),
            (
                "activity",
                cx.t("usage.metric.calls"),
                compact(summary.calls, locale),
                cx.t_with(
                    "usage.kpi.perCall",
                    &Args::default().with(
                        "value",
                        compact(per(tokens.total(), summary.calls).round() as u64, locale),
                    ),
                ),
            ),
            (
                "messages-square",
                cx.t("usage.metric.turns"),
                compact(summary.turns, locale),
                cx.t_with(
                    "usage.kpi.perTurn",
                    &Args::default()
                        .with("value", decimal(per(summary.calls, summary.turns), locale)),
                ),
            ),
            (
                "layers",
                cx.t("usage.kpi.sessions"),
                compact(summary.sessions, locale),
                cx.t_with(
                    "usage.kpi.subagentShare",
                    &Args::default().with(
                        "percent",
                        share(summary.subagent_sessions, summary.sessions),
                    ),
                ),
            ),
            (
                "workflow",
                cx.t("usage.kpi.viaDcc"),
                format!("{:.0}%", summary.dcc_percent),
                cx.t_with(
                    "usage.kpi.viaDccHint",
                    &Args::default().with("metric", metric_name),
                ),
            ),
        ]
        .into_iter()
        .map(|(icon, label, value, hint)| {
            div()
                .min_w_0()
                .rounded(cx.theme().radius.lg)
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.muted.opacity(0.15))
                .p(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_color(colors.muted_foreground)
                        .child(Icon::new(icon).size(14.0))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .overflow_hidden()
                                .child(Tracked::new(label.to_uppercase(), 10.0, 0.07)),
                        ),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .truncate()
                        .text_size(px(20.0))
                        .line_height(px(28.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .font_features(tabular_nums())
                        .child(value),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .truncate()
                        .text_size(px(10.0))
                        .line_height(px(15.0))
                        .text_color(colors.muted_foreground)
                        .child(hint),
                )
                .into_any_element()
        })
        .collect();
        grid(cards, if large { 4 } else { 2 }, 8.0)
    }

    /// "N de M dias ativos · maior sequência X dias · dia de pico … ·
    /// hora de pico … · Y% em subagentes", and the cost's notes.
    fn stats(&self, summary: &UsageSummary, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let mut parts: Vec<SharedString> = vec![
            cx.t_with(
                "usage.stats.activeDays",
                &Args::default()
                    .with("active", summary.active_days.to_string())
                    .with("total", summary.period_days.to_string()),
            ),
            cx.t_with(
                "usage.stats.streak",
                &Args::count(i64::from(summary.longest_streak)),
            ),
        ];
        if let Some(peak) = summary.peak_day {
            parts.push(
                cx.t_with(
                    "usage.stats.peakDay",
                    &Args::default()
                        .with("date", short_date(&peak.date.to_string(), locale))
                        .with("value", metric_value(self.metric, peak.value, locale)),
                ),
            );
        }
        if let Some(peak) = summary.peak_hour {
            parts.push(
                cx.t_with(
                    "usage.stats.peakHour",
                    &Args::default()
                        .with("date", short_date(&peak.date.to_string(), locale))
                        .with("hour", format!("{:02}", peak.hour))
                        .with("value", metric_value(self.metric, peak.value, locale)),
                ),
            );
        }
        parts.push(cx.t_with(
            "usage.stats.subagents",
            &Args::default().with("percent", format!("{:.0}", summary.subagent_percent)),
        ));
        let line = parts
            .iter()
            .map(SharedString::as_ref)
            .collect::<Vec<_>>()
            .join(" · ");
        let unpriced = (!summary.unknown_models.is_empty()).then(|| {
            cx.t_with(
                "usage.cost.unpriced",
                &Args::count(summary.unpriced_calls as i64)
                    .with("models", summary.unknown_models.join(", ")),
            )
        });
        div()
            .debug_selector(|| "usage-stats".into())
            .flex()
            .flex_col()
            .gap(px(2.0))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .font_features(tabular_nums())
            .child(div().text_color(colors.foreground).child(line))
            .child(div().text_size(px(11.0)).child(cx.t("usage.cost.note")))
            .children(unpriced.map(|text| div().text_size(px(11.0)).child(text)))
            .into_any_element()
    }

    fn legend(&self, summary: &UsageSummary, cx: &Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let palette = self.group_colors(summary);
        div()
            .flex()
            .flex_wrap()
            .gap_x(px(16.0))
            .gap_y(px(6.0))
            .children(
                summary
                    .groups
                    .iter()
                    .zip(palette)
                    .map(|((key, value), color)| {
                        let share = if summary.metric_total > 0.0 {
                            value / summary.metric_total * 100.0
                        } else {
                            0.0
                        };
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(11.0))
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(8.0))
                                    .rounded_full()
                                    .bg(hex(color)),
                            )
                            .child(
                                div()
                                    .max_w(px(220.0))
                                    .truncate()
                                    .child(self.group_label(key, cx)),
                            )
                            .child(
                                div()
                                    .text_color(colors.muted_foreground)
                                    .font_features(tabular_nums())
                                    .child(format!(
                                        "{} · {share:.0}%",
                                        metric_value(self.metric, *value, locale)
                                    )),
                            )
                    }),
            )
            .into_any_element()
    }

    fn days(&self, summary: &UsageSummary, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let palette: Vec<_> = self.group_colors(summary).into_iter().map(hex).collect();
        let columns: Vec<Column> = summary
            .days
            .iter()
            .map(|day| {
                day.parts
                    .iter()
                    .zip(&palette)
                    .map(|(value, color)| (*color, *value))
                    .collect()
            })
            .collect();
        let top = charts::tallest(&columns);
        let groups = GroupBy::ALL
            .into_iter()
            .enumerate()
            .map(|(index, group_by)| {
                chip(
                    SharedString::from(format!("usage-group-{index}")),
                    cx.t(group_key(group_by)),
                    self.group_by == group_by,
                    None,
                )
                .on_click(cx.listener(move |usage, _, _, cx| usage.set_group_by(group_by, cx)))
                .into_any_element()
            })
            .collect::<Vec<_>>();
        let metric = cx.t(metric_key(self.metric));
        let first = summary.days.first().map(|day| day.date.to_string());
        let last = summary.days.last().map(|day| day.date.to_string());
        card(cx)
            .debug_selector(|| "usage-days".into())
            .child(
                div()
                    .mb(px(12.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(card_title(
                        cx.t_with("usage.days.title", &Args::default().with("metric", metric)),
                        None,
                        cx,
                    ))
                    .child(div().flex().flex_wrap().gap(px(2.0)).children(groups)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .font_features(tabular_nums())
                    .child(metric_value(self.metric, top, locale)),
            )
            .child(charts::stacked_bars(
                columns,
                colors.border.opacity(0.5),
                176.0,
            ))
            .child(
                div()
                    .mt(px(4.0))
                    .flex()
                    .justify_between()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .children(first.map(|date| short_date(&date, locale)))
                    .children(last.map(|date| short_date(&date, locale))),
            )
            .child(div().mt(px(12.0)).child(self.legend(summary, cx)))
            .into_any_element()
    }

    fn bands(&self, summary: &UsageSummary, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let names = [
            ("usage.bands.night", "00–06h"),
            ("usage.bands.morning", "06–12h"),
            ("usage.bands.afternoon", "12–18h"),
            ("usage.bands.evening", "18–24h"),
        ];
        let tiles = summary
            .bands
            .iter()
            .zip(names)
            .map(|(band, (name, range))| {
                div()
                    .min_w_0()
                    .rounded(cx.theme().radius.md)
                    .bg(colors.muted.opacity(0.3))
                    .p(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .justify_between()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(cx.t(name)),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(9.0))
                                    .text_color(colors.muted_foreground)
                                    .child(range),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(18.0))
                            .line_height(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_features(tabular_nums())
                            .child(format!("{:.0}%", band.percent)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .font_features(tabular_nums())
                            .child(metric_value(self.metric, band.value, locale)),
                    )
                    .into_any_element()
            })
            .collect();
        let columns: Vec<Column> = summary
            .hours
            .iter()
            .map(|hour| {
                Runtime::ALL
                    .into_iter()
                    .map(|runtime| (hex(runtime_color(runtime.as_str())), hour[runtime.index()]))
                    .collect()
            })
            .collect();
        let metric = cx.t(metric_key(self.metric));
        card(cx)
            .debug_selector(|| "usage-bands".into())
            .child(div().mb(px(12.0)).child(card_title(
                cx.t("usage.bands.title"),
                Some(cx.t_with(
                    "usage.bands.subtitle",
                    &Args::default().with("metric", metric),
                )),
                cx,
            )))
            .child(grid(tiles, 4, 8.0))
            .child(div().mt(px(16.0)).child(charts::stacked_bars(
                columns,
                colors.border.opacity(0.5),
                96.0,
            )))
            .child(
                div()
                    .mt(px(4.0))
                    .flex()
                    .justify_between()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .children(["00", "06", "12", "18", "23"].map(|hour| div().child(hour))),
            )
            .into_any_element()
    }
}
