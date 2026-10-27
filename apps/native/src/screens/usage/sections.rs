//! `UsageContent`: four summary cards, then the providers' arena beside
//! the days (`lg:grid-cols-[1fr_1.15fr]`), then the providers' scores
//! beside the models' ranking. Under 1024px of window each pair stacks and
//! the cards go two by two, as the desktop's `lg:`.

use dcc_core::domain::usage::UsageDashboard;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::Usage;
use super::charts;
use crate::data::usage::{compact, dollars, percent, provider_color, short_date};
use crate::i18n::{Args, I18n, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::{Badge, BadgeVariant, Icon, Tracked, tabular_nums};

/// The desktop's `lg` breakpoint.
const LARGE: f32 = 1024.0;

/// `rounded-lg border border-border/60 p-4`
fn card(cx: &Context<Usage>) -> gpui::Div {
    div()
        .min_w_0()
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(cx.theme().colors.border.opacity(0.6))
        .p(px(16.0))
}

/// `h3 text-[13px] font-medium` with a 10px line under it.
fn card_title(
    title: SharedString,
    subtitle: Option<SharedString>,
    cx: &Context<Usage>,
) -> gpui::Div {
    div()
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

/// Two side by side (each `flex-1`, the right `grow` as given), or stacked.
fn pair(left: AnyElement, right: AnyElement, grow: f32, large: bool) -> AnyElement {
    if large {
        let mut right_box = div().min_w_0().flex_basis(px(0.0)).child(right);
        right_box.style().flex_grow = Some(grow);
        div()
            .flex()
            .gap(px(16.0))
            .child(div().flex_1().flex_basis(px(0.0)).min_w_0().child(left))
            .child(right_box)
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(left)
            .child(right)
            .into_any_element()
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

impl Usage {
    /// `width` is the window's (the breakpoints are the viewport's).
    pub(super) fn content(
        &self,
        dashboard: &UsageDashboard,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let large = width >= LARGE;
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(16.0))
            .child(self.summary(dashboard, large, cx))
            .child(pair(
                self.arena(dashboard, cx),
                self.days(dashboard, cx),
                1.15,
                large,
            ))
            .child(pair(
                self.scores(dashboard, cx),
                self.models(dashboard, cx),
                1.0,
                large,
            ))
            .into_any_element()
    }

    fn summary(
        &self,
        dashboard: &UsageDashboard,
        large: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let totals = &dashboard.totals;
        let coverage = percent(totals.measured_turns, totals.turns);
        let cost = totals
            .cost_usd
            .map_or_else(|| "—".to_string(), |cost| dollars(cost, locale));
        let cards = [
            (
                "sparkles",
                cx.t("usage.metrics.tokens"),
                compact(totals.total_tokens, locale),
                cx.t_with(
                    "usage.metrics.tokenBreakdown",
                    &Args::default()
                        .with("input", compact(totals.input_tokens, locale))
                        .with("output", compact(totals.output_tokens, locale)),
                ),
            ),
            (
                "activity",
                cx.t("usage.metrics.activity"),
                compact(totals.turns, locale),
                cx.t_with(
                    "usage.metrics.sessions",
                    &Args::count(totals.sessions as i64),
                ),
            ),
            (
                "gauge",
                cx.t("usage.metrics.coverage"),
                format!("{coverage:.0}%"),
                cx.t_with(
                    "usage.metrics.measuredTurns",
                    &Args::count(totals.measured_turns as i64),
                ),
            ),
            (
                "coins",
                cx.t("usage.metrics.cost"),
                cost,
                cx.t("usage.metrics.reportedCost"),
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

    fn arena(&self, dashboard: &UsageDashboard, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let tokens = dashboard.totals.total_tokens > 0;
        let total: u64 = dashboard
            .providers
            .iter()
            .map(|provider| charts::metric(provider, tokens))
            .sum();
        let ring = div()
            .debug_selector(|| "usage-ring".into())
            .relative()
            .mx_auto()
            .flex_none()
            .size(px(160.0))
            .child(charts::ring(
                charts::ring_stops(&dashboard.providers, tokens),
                colors.muted,
            ))
            .child(
                div()
                    .absolute()
                    .top(px(20.0))
                    .left(px(20.0))
                    .size(px(120.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(colors.popover)
                    .border_1()
                    .border_color(colors.border.opacity(0.5))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .line_height(px(28.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_features(tabular_nums())
                            .child(compact(total, locale)),
                    )
                    .child(
                        div()
                            .text_color(colors.muted_foreground)
                            .child(Tracked::new(
                                if tokens {
                                    cx.t("usage.units.tokens")
                                } else {
                                    cx.t("usage.units.turns")
                                }
                                .to_uppercase(),
                                10.0,
                                0.08,
                            )),
                    ),
            );
        let legend = dashboard
            .providers
            .iter()
            .enumerate()
            .map(|(index, provider)| {
                let share = percent(charts::metric(provider, tokens), total);
                let color = hex(provider_color(&provider.provider_id));
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(cx.theme().radius.md)
                    .px(px(8.0))
                    .py(px(6.0))
                    .hover(|row| row.bg(colors.muted.opacity(0.4)))
                    .child(
                        div()
                            .w(px(20.0))
                            .flex_none()
                            .text_center()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child((index + 1).to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(div().flex_none().size(px(8.0)).rounded_full().bg(color))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(self.label(&provider.provider_id)),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .h(px(4.0))
                                    .overflow_hidden()
                                    .rounded_full()
                                    .bg(colors.muted)
                                    .child(
                                        div()
                                            .h_full()
                                            .rounded_full()
                                            .bg(color)
                                            .w(gpui::relative(share as f32 / 100.0)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .font_features(tabular_nums())
                            .child(format!("{share:.1}%")),
                    )
            });
        let count = dashboard.providers.len() as i64;
        card(cx)
            .child(
                div()
                    .mb(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(card_title(
                        cx.t("usage.arena.title"),
                        Some(if tokens {
                            cx.t("usage.arena.byTokens")
                        } else {
                            cx.t("usage.arena.byTurns")
                        }),
                        cx,
                    ))
                    .child(
                        Badge::new(
                            cx.t_with("usage.arena.providerCount", &Args::count(count)),
                            BadgeVariant::Secondary,
                        )
                        .text_size(10.0),
                    ),
            )
            // `sm:grid-cols-[180px_minmax(0,1fr)] sm:items-center gap-5`
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(20.0))
                    .child(div().flex().flex_none().w(px(180.0)).child(ring))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(8.0))
                            .children(legend),
                    ),
            )
            .into_any_element()
    }

    fn days(&self, dashboard: &UsageDashboard, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let tokens = dashboard.totals.total_tokens > 0;
        let (dates, lines) = charts::timeline_lines(&dashboard.daily, &dashboard.providers, tokens);
        let body = match (dates.first(), dates.last()) {
            (Some(first), Some(last)) => div()
                .debug_selector(|| "usage-timeline".into())
                .min_w_0()
                .child(charts::timeline(lines, colors.border.opacity(0.5)))
                .child(
                    div()
                        .mt(px(4.0))
                        .flex()
                        .justify_between()
                        .text_size(px(10.0))
                        .text_color(colors.muted_foreground)
                        .child(short_date(first, locale))
                        .child(short_date(last, locale)),
                )
                .into_any_element(),
            _ => div()
                .flex()
                .h(px(176.0))
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child("—")
                .into_any_element(),
        };
        card(cx)
            .child(div().mb(px(8.0)).child(card_title(
                cx.t("usage.timeline.title"),
                Some(if tokens {
                    cx.t("usage.timeline.tokens")
                } else {
                    cx.t("usage.timeline.turns")
                }),
                cx,
            )))
            .child(body)
            .into_any_element()
    }

    fn scores(&self, dashboard: &UsageDashboard, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let stat = |value: String, unit: SharedString| {
            div()
                .flex_1()
                .flex_basis(px(0.0))
                .text_center()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::BOLD)
                        .font_features(tabular_nums())
                        .child(value),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(colors.muted_foreground)
                        .child(unit),
                )
        };
        let tiles = dashboard
            .providers
            .iter()
            .map(|provider| {
                div()
                    .min_w_0()
                    .rounded(cx.theme().radius.md)
                    .bg(colors.muted.opacity(0.3))
                    .p(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(10.0))
                                    .rounded_full()
                                    .bg(hex(provider_color(&provider.provider_id))),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.label(&provider.provider_id)),
                            )
                            .when(provider.measured_turns == 0, |row| {
                                row.child(
                                    div().ml_auto().child(
                                        Badge::new(
                                            cx.t("usage.providers.noTelemetry"),
                                            BadgeVariant::Outline,
                                        )
                                        .text_size(9.0),
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .flex()
                            .gap(px(8.0))
                            .child(stat(
                                provider.sessions.to_string(),
                                cx.t("usage.units.sessions"),
                            ))
                            .child(stat(provider.turns.to_string(), cx.t("usage.units.turns")))
                            .child(stat(
                                compact(provider.total_tokens, locale),
                                cx.t("usage.units.tokens"),
                            )),
                    )
                    .into_any_element()
            })
            .collect();
        card(cx)
            .child(card_title(cx.t("usage.providers.title"), None, cx))
            .child(div().mt(px(12.0)).child(grid(tiles, 2, 8.0)))
            .into_any_element()
    }

    fn models(&self, dashboard: &UsageDashboard, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let locale = cx.global::<I18n>().locale();
        let top: Vec<AnyElement> = dashboard
            .models
            .iter()
            .take(8)
            .enumerate()
            .map(|(index, model)| {
                let turns = cx.t_with(
                    "usage.models.turnCount",
                    &Args::count(model.measured_turns as i64),
                );
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(cx.theme().radius.md)
                    .bg(colors.muted.opacity(0.25))
                    .px(px(10.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .w(px(20.0))
                            .flex_none()
                            .text_center()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child((index + 1).to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(model.model.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(9.0))
                                    .text_color(colors.muted_foreground)
                                    .child(format!("{} · {turns}", self.label(&model.provider_id))),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .font_features(tabular_nums())
                            .child(compact(model.total_tokens, locale)),
                    )
                    .into_any_element()
            })
            .collect();
        let body = if top.is_empty() {
            div()
                .py(px(40.0))
                .text_center()
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("usage.models.awaiting"))
                .into_any_element()
        } else {
            div()
                .mt(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(top)
                .into_any_element()
        };
        card(cx)
            .child(card_title(cx.t("usage.models.title"), None, cx))
            .child(body)
            .into_any_element()
    }
}
