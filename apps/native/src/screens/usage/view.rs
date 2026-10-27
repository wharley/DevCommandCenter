//! The dialog's frame: `sm:max-w-6xl`, the header with the title, the
//! periods, "Projeto atual" and ⟳; the body scrolls under it, loading, the
//! error with "Tentar novamente", empty, or the dashboard (`sections.rs`).

use gpui::{AnyElement, Context, FontWeight, Render, Window, div, prelude::*, px};

use super::{Usage, UsageEvent};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, spinner};

/// `[7, 30, 90, null]`
const PERIODS: [Option<u32>; 4] = [Some(7), Some(30), Some(90), None];

impl Usage {
    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let periods = PERIODS.iter().map(|period| {
            let period = *period;
            let label = match period {
                Some(days) => cx.t_with("usage.period.days", &Args::count(i64::from(days))),
                None => cx.t("usage.period.all"),
            };
            let id = format!("usage-period-{}", period.map_or(0, |days| days));
            Button::new(gpui::SharedString::from(id))
                .variant(if self.period == period {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Xs)
                .text_size(12.0)
                .on_click(cx.listener(move |usage, _, _, cx| usage.set_period(period, cx)))
                .child(label)
        });
        let project = self.project_id.is_some().then(|| {
            Button::new("usage-project")
                .variant(if self.project_only {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Xs)
                .text_size(12.0)
                .on_click(cx.listener(|usage, _, _, cx| usage.toggle_project(cx)))
                .child(cx.t("usage.period.project"))
        });
        div()
            .border_b_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(20.0))
            .pt(px(20.0))
            .pb(px(16.0))
            .pr(px(56.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(16.0))
                                    .line_height(px(16.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(
                                        Icon::new("chart-column")
                                            .size(16.0)
                                            .color(tw::EMERALD_400.hsla()),
                                    )
                                    .child(cx.t("usage.title")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("usage.description")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(px(6.0))
                            .children(periods)
                            .children(project)
                            .child(
                                Button::new("usage-refresh")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::IconXs)
                                    .disabled(self.fetching)
                                    .on_click(cx.listener(|usage, _, _, cx| usage.refresh(cx)))
                                    .child(Icon::new("refresh-cw").size(14.0)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn state(&self, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let centered = || {
            div()
                .flex()
                .flex_col()
                .min_h(px(320.0))
                .items_center()
                .justify_center()
                .text_center()
        };
        match &self.dashboard {
            Some(dashboard) if !dashboard.providers.is_empty() => {
                self.content(dashboard, width, cx)
            }
            None if self.failed => centered()
                .gap(px(12.0))
                .child(
                    div()
                        .text_color(colors.destructive)
                        .child(cx.t("usage.error")),
                )
                .child(
                    Button::new("usage-try-again")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|usage, _, _, cx| usage.refresh(cx)))
                        .child(cx.t("usage.tryAgain")),
                )
                .into_any_element(),
            None => centered()
                .flex_row()
                .gap(px(8.0))
                .text_color(colors.muted_foreground)
                .child(spinner("usage-loading", 16.0))
                .child(cx.t("usage.loading"))
                .into_any_element(),
            Some(_) => centered()
                .gap(px(8.0))
                .child(
                    Icon::new("chart-column")
                        .size(32.0)
                        .color(colors.muted_foreground.opacity(0.5)),
                )
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("usage.empty.title")),
                )
                .child(
                    div()
                        .max_w(px(384.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("usage.empty.body")),
                )
                .into_any_element(),
        }
    }
}

impl Render for Usage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
        let close = cx.entity().downgrade();
        let header = self.header(cx);
        let body = self.state(width, cx);
        Dialog::new("usage-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |_, cx| cx.emit(UsageEvent::Closed));
        })
        .max_width(1152.0)
        .max_height(height - 32.0)
        .padding(0.0)
        .child(header)
        .child(
            div()
                .id("usage-body")
                .debug_selector(|| "usage-dialog".into())
                .min_h_0()
                .max_h(px(height - 144.0))
                .overflow_y_scroll()
                .p(px(20.0))
                .child(body),
        )
    }
}
