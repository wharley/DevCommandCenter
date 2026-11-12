//! The filters row: the period, the runtimes, main agents or subagents,
//! the ways in ("Via", only those the period has) and the metric the
//! charts count.

use dcc_core::domain::usage_logs::{GroupBy, Metric, Runtime, Scope, Via};
use gpui::{AnyElement, Context, ElementId, SharedString, div, prelude::*, px};

use super::Usage;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::{Button, ButtonSize, ButtonVariant};

/// `[7, 30, 90, null]`
const PERIODS: [Option<u32>; 4] = [Some(7), Some(30), Some(90), None];

pub(super) fn metric_key(metric: Metric) -> &'static str {
    match metric {
        Metric::TotalTokens => "usage.metric.totalTokens",
        Metric::NewTokens => "usage.metric.newTokens",
        Metric::Output => "usage.metric.output",
        Metric::Cost => "usage.metric.cost",
        Metric::Calls => "usage.metric.calls",
        Metric::Turns => "usage.metric.turns",
    }
}

pub(super) fn group_key(group_by: GroupBy) -> &'static str {
    match group_by {
        GroupBy::Runtime => "usage.group.runtime",
        GroupBy::Model => "usage.group.model",
        GroupBy::Via => "usage.group.via",
        GroupBy::Project => "usage.group.project",
        GroupBy::Agent => "usage.group.agent",
    }
}

pub(super) fn via_key(via: Via) -> String {
    format!("usage.via.{}", via.as_str())
}

fn scope_key(scope: Scope) -> &'static str {
    match scope {
        Scope::All => "usage.scope.all",
        Scope::Main => "usage.scope.main",
        Scope::Subagents => "usage.scope.subagents",
    }
}

/// A choice in a row: filled when on.
pub(super) fn chip(
    id: impl Into<ElementId>,
    label: SharedString,
    on: bool,
    dot: Option<u32>,
) -> Button {
    Button::new(id)
        .variant(if on {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        })
        .size(ButtonSize::Xs)
        .text_size(12.0)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .children(dot.map(|color| {
                    div()
                        .flex_none()
                        .size(px(8.0))
                        .rounded_full()
                        .bg(hex(color).opacity(if on { 1.0 } else { 0.4 }))
                }))
                .child(label),
        )
}

/// A labeled group of chips, kept on one line.
fn group(label: Option<SharedString>, chips: Vec<AnyElement>, cx: &Context<Usage>) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .children(label.map(|label| {
            div()
                .mr(px(4.0))
                .text_size(px(11.0))
                .text_color(cx.theme().colors.muted_foreground)
                .child(label)
        }))
        .children(chips)
}

impl Usage {
    pub(super) fn filters(&self, cx: &mut Context<Self>) -> AnyElement {
        let periods = PERIODS
            .iter()
            .map(|period| {
                let period = *period;
                let label = match period {
                    Some(days) => cx.t_with("usage.period.days", &Args::count(i64::from(days))),
                    None => cx.t("usage.period.all"),
                };
                chip(
                    SharedString::from(format!("usage-period-{}", period.unwrap_or(0))),
                    label,
                    self.filter.period_days == period,
                    None,
                )
                .on_click(cx.listener(move |usage, _, _, cx| usage.set_period(period, cx)))
                .into_any_element()
            })
            .collect();
        let present = self
            .summary
            .as_ref()
            .map(|summary| summary.runtimes_present);
        let runtimes = Runtime::ALL
            .into_iter()
            .filter(|runtime| present.is_none_or(|present| present[runtime.index()]))
            .map(|runtime| {
                let index = runtime.index();
                chip(
                    SharedString::from(format!("usage-runtime-{}", runtime.as_str())),
                    cx.t(&format!("usage.runtime.{}", runtime.as_str())),
                    self.filter.runtimes[index],
                    Some(crate::data::usage::runtime_color(runtime.as_str())),
                )
                .on_click(cx.listener(move |usage, _, _, cx| usage.toggle_runtime(index, cx)))
                .into_any_element()
            })
            .collect();
        let scopes = [Scope::All, Scope::Main, Scope::Subagents]
            .into_iter()
            .enumerate()
            .map(|(index, scope)| {
                chip(
                    SharedString::from(format!("usage-scope-{index}")),
                    cx.t(scope_key(scope)),
                    self.filter.scope == scope,
                    None,
                )
                .on_click(cx.listener(move |usage, _, _, cx| usage.set_scope(scope, cx)))
                .into_any_element()
            })
            .collect();
        let vias: Vec<AnyElement> = self
            .summary
            .as_ref()
            .map(|summary| summary.vias_present.clone())
            .unwrap_or_default()
            .into_iter()
            .chain(self.filter.hidden_vias.iter().copied())
            .fold(Vec::new(), |mut list, via| {
                if !list.contains(&via) {
                    list.push(via);
                }
                list
            })
            .into_iter()
            .map(|via| {
                chip(
                    SharedString::from(format!("usage-via-{}", via.as_str())),
                    cx.t(&via_key(via)),
                    !self.filter.hidden_vias.contains(&via),
                    None,
                )
                .on_click(cx.listener(move |usage, _, _, cx| usage.toggle_via(via, cx)))
                .into_any_element()
            })
            .collect();
        let metrics = Metric::ALL
            .into_iter()
            .enumerate()
            .map(|(index, metric)| {
                chip(
                    SharedString::from(format!("usage-metric-{index}")),
                    cx.t(metric_key(metric)),
                    self.metric == metric,
                    None,
                )
                .on_click(cx.listener(move |usage, _, _, cx| usage.set_metric(metric, cx)))
                .into_any_element()
            })
            .collect();
        let via_label = cx.t("usage.via.label");
        div()
            .debug_selector(|| "usage-filters".into())
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(16.0))
                    .gap_y(px(6.0))
                    .child(group(None, periods, cx))
                    .child(group(None, runtimes, cx))
                    .child(group(None, scopes, cx))
                    .when(!vias.is_empty(), |row| {
                        row.child(group(Some(via_label), vias, cx))
                    }),
            )
            .child(group(None, metrics, cx).flex_wrap())
            .into_any_element()
    }
}
