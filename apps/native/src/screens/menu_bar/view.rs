//! How the panel looks (`menu-bar.css`).

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, KeyDownEvent, Render, SharedString, Window, div,
    prelude::*, px,
};

use super::MenuBarPanel;
use crate::data::keep_awake::reason_key;
use crate::data::menu_bar::{Task as MenuTask, cpu, memory};
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::{Icon, spinner, tabular_nums};

const GREEN: u32 = 0x5aaf83;
const AMBER: u32 = 0xc78b33;
const BLUE: u32 = 0x5e9fea;

impl MenuBarPanel {
    fn task_row(&self, task: &MenuTask, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (icon, color, tint) = match task.status.as_str() {
            "running" => ("loader-circle", hex(BLUE), colors.muted),
            "permission" => (
                "shield-check",
                hex(AMBER),
                hex(AMBER).opacity(0x18 as f32 / 255.0),
            ),
            "input" => (
                "message-circle",
                hex(AMBER),
                hex(AMBER).opacity(0x18 as f32 / 255.0),
            ),
            "completed" => ("check", hex(GREEN), colors.muted),
            _ => ("circle-pause", colors.muted_foreground, colors.muted),
        };
        let glyph: AnyElement = if task.status == "running" {
            div()
                .text_color(color)
                .child(spinner(
                    SharedString::from(format!("menu-bar-spin-{index}")),
                    16.0,
                ))
                .into_any_element()
        } else {
            Icon::new(icon).size(16.0).color(color).into_any_element()
        };
        let status = cx.t(&format!("menuBar.status.{}", task.status));
        let opened = task.clone();
        let busy = self.busy;
        let muted = colors.muted;
        div()
            .id(SharedString::from(format!("menu-bar-task-{index}")))
            .debug_selector(move || format!("menu-bar-task-{index}"))
            .flex()
            .w_full()
            .items_center()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(11.0))
            .rounded(px(8.0))
            .map(|row| {
                if busy {
                    row.opacity(0.5)
                } else {
                    row.cursor_pointer()
                        .hover(move |row| row.bg(muted))
                        .on_click(cx.listener(move |panel, _, window, cx| {
                            panel.open(Some(opened.clone()), window, cx)
                        }))
                }
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .size(px(28.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .bg(tint)
                    .child(glyph),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(task.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(format!("{} · {status}", task.project)),
                    ),
            )
            .child(
                Icon::new("arrow-up-right")
                    .size(14.0)
                    .color(colors.muted_foreground),
            )
            .into_any_element()
    }

    fn link(
        &self,
        id: &'static str,
        icon: Option<&'static str>,
        label: SharedString,
        trailing: Option<&'static str>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let colors = &cx.theme().colors;
        let fg = colors.foreground;
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex()
            .items_center()
            .gap(px(5.0))
            .p(px(3.0))
            .text_size(px(10.0))
            .text_color(colors.muted_foreground)
            .map(|link| {
                if self.busy {
                    link.opacity(0.5)
                } else {
                    link.cursor_pointer().hover(move |link| link.text_color(fg))
                }
            })
            .children(icon.map(|icon| Icon::new(icon).size(12.0)))
            .child(label)
            .children(trailing.map(|icon| Icon::new(icon).size(13.0)))
    }
}

type App = gpui::App;

impl Render for MenuBarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let border = colors.border;
        let muted_fg = colors.muted_foreground;
        let (running, waiting) = (self.running(), self.waiting());
        let summary: AnyElement = match &self.snapshot {
            Some(_) => div()
                .flex()
                .items_center()
                .gap(px(14.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().size(px(6.0)).rounded_full().bg(hex(GREEN)))
                        .child(cx.t_with("menuBar.running", &Args::count(running as i64))),
                )
                .child(
                    div()
                        .when(waiting > 0, |span| span.text_color(hex(AMBER)))
                        .child(cx.t_with("menuBar.waiting", &Args::count(waiting as i64))),
                )
                .into_any_element(),
            None => div()
                .child(cx.t(if self.error {
                    "menuBar.unavailable"
                } else {
                    "menuBar.loading"
                }))
                .into_any_element(),
        };
        let keep_awake = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.keep_awake.as_ref())
            .map(|hold| {
                let (key, count, time) = reason_key(&hold.reason);
                let mut args = Args::default();
                if let Some(count) = count {
                    args = args.with_count(count);
                }
                if let Some(time) = time {
                    args = args.with("time", time);
                }
                let reason = cx.t_with(key, &args).to_string();
                div()
                    .debug_selector(|| "menu-bar-keep-awake".into())
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(18.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(border)
                    .text_size(px(11.0))
                    .text_color(muted_fg)
                    .child(Icon::new("coffee").size(13.0).color(colors.foreground))
                    .child(cx.t_with("menuBar.keepAwake", &Args::default().with("reason", reason)))
            });
        let alert = (self.error || self.action_error).then(|| {
            div()
                .px(px(18.0))
                .py(px(8.0))
                .text_size(px(10.0))
                .text_color(hex(AMBER))
                .bg(hex(AMBER).opacity(0x10 as f32 / 255.0))
                .child(cx.t(if self.action_error {
                    "menuBar.actionError"
                } else {
                    "menuBar.stale"
                }))
        });
        let tasks = self.tasks().to_vec();
        let active: Vec<(usize, MenuTask)> = tasks
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, task)| task.active())
            .collect();
        let recent: Vec<(usize, MenuTask)> = tasks
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, task)| !task.active())
            .collect();
        let empty =
            |icon: &'static str, spin: bool, title: Option<SharedString>, hint: SharedString| {
                let glyph: AnyElement = if spin {
                    spinner("menu-bar-loading", 22.0).into_any_element()
                } else {
                    Icon::new(icon).size(24.0).into_any_element()
                };
                div()
                    .debug_selector(|| "menu-bar-empty".into())
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .py(px(24.0))
                    .text_color(muted_fg)
                    .child(glyph)
                    .children(title.map(|title| {
                        div()
                            .text_color(colors.foreground)
                            .font_weight(FontWeight::MEDIUM)
                            .child(title)
                    }))
                    .child(
                        div()
                            .max_w(px(250.0))
                            .text_size(px(11.0))
                            .line_height(gpui::relative(1.5))
                            .text_center()
                            .child(hint),
                    )
            };
        let list = div()
            .id("menu-bar-tasks")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(8.0))
            .when(self.snapshot.is_none() && !self.error, |list| {
                list.child(empty("loader-circle", true, None, cx.t("menuBar.loading")))
            })
            .when(self.snapshot.is_some() && active.is_empty(), |list| {
                list.child(empty(
                    "check",
                    false,
                    Some(cx.t("menuBar.empty")),
                    cx.t("menuBar.emptyHint"),
                ))
            })
            .children(
                active
                    .iter()
                    .map(|(index, task)| self.task_row(task, *index, cx))
                    .collect::<Vec<_>>(),
            )
            .when(!recent.is_empty(), |list| {
                list.child(
                    div()
                        .mt(px(12.0))
                        .mb(px(6.0))
                        .mx(px(10.0))
                        .text_size(px(10.0))
                        .text_color(muted_fg)
                        .child(crate::ui::Tracked::new(
                            cx.t("menuBar.recent").to_uppercase(),
                            10.0,
                            0.07,
                        )),
                )
            })
            .children(
                recent
                    .iter()
                    .map(|(index, task)| self.task_row(task, *index, cx))
                    .collect::<Vec<_>>(),
            );
        let metrics = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.metrics.clone());
        let value = |label: SharedString, value: String| {
            div()
                .flex()
                .items_baseline()
                .gap(px(5.0))
                .child(label)
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .font_features(tabular_nums())
                        .text_color(colors.foreground)
                        .child(value),
                )
        };
        let resources = div()
            .border_t_1()
            .border_color(border)
            .px(px(18.0))
            .py(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(11.0))
                    .text_color(muted_fg)
                    .child(Icon::new("cpu").size(13.0))
                    .child(cx.t("menuBar.resources")),
            )
            .child(
                div()
                    .debug_selector(|| "menu-bar-metrics".into())
                    .mt(px(9.0))
                    .flex()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(muted_fg)
                    .child(value("CPU".into(), cpu(metrics.as_ref(), english)))
                    .child(value(
                        cx.t("menuBar.memory"),
                        memory(metrics.as_ref(), english),
                    )),
            )
            .child(
                crate::ui::Tooltip::text(
                    "menu-bar-metrics-tip",
                    div()
                        .mt(px(7.0))
                        .text_size(px(9.0))
                        .line_height(gpui::relative(1.4))
                        .text_color(muted_fg)
                        .child(cx.t("menuBar.metricsScope")),
                    cx.t("menuBar.metricsDetail"),
                )
                .side(crate::ui::tooltip::Side::Top),
            );
        let muted = colors.muted;
        let busy = self.busy;
        let footer = div()
            .px(px(14.0))
            .pb(px(12.0))
            .child(
                div()
                    .id("menu-bar-compose")
                    .debug_selector(|| "menu-bar-compose".into())
                    .flex()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .gap(px(7.0))
                    .p(px(9.0))
                    .border_1()
                    .border_color(border)
                    .rounded(px(8.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .map(|button| {
                        if busy {
                            button.opacity(0.5)
                        } else {
                            button
                                .cursor_pointer()
                                .hover(move |button| button.bg(muted))
                                .on_click(
                                    cx.listener(|panel, _, window, cx| panel.compose(window, cx)),
                                )
                        }
                    })
                    .child(Icon::new("plus").size(16.0))
                    .child(cx.t("menuBar.compose")),
            )
            .child(
                div()
                    .mt(px(10.0))
                    .flex()
                    .justify_between()
                    .child(
                        self.link(
                            "menu-bar-open",
                            None,
                            cx.t("menuBar.open"),
                            Some("arrow-up-right"),
                            cx,
                        )
                        .when(!busy, |link| {
                            link.on_click(
                                cx.listener(|panel, _, window, cx| panel.open(None, window, cx)),
                            )
                        }),
                    )
                    .child(
                        self.link(
                            "menu-bar-quit",
                            Some("power"),
                            cx.t("menuBar.quit"),
                            None,
                            cx,
                        )
                        .when(!busy, |link| {
                            link.on_click(cx.listener(|panel, _, _, cx| panel.quit(cx)))
                        }),
                    ),
            );
        div()
            .id("menu-bar")
            .debug_selector(|| "menu-bar".into())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|panel, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    panel.close(window, cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            .text_size(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt(px(18.0))
                    .px(px(18.0))
                    .pb(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(17.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(crate::ui::Tracked::new("DCC", 17.0, -0.04)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(muted_fg)
                                    .child(cx.t("menuBar.title")),
                            ),
                    )
                    .child(
                        div()
                            .id("menu-bar-refresh")
                            .debug_selector(|| "menu-bar-refresh".into())
                            .p(px(5.0))
                            .rounded(px(6.0))
                            .text_color(muted_fg)
                            .cursor_pointer()
                            .hover(move |button| button.bg(muted))
                            .on_click(cx.listener(|panel, _, _, cx| panel.refresh(cx)))
                            .child(Icon::new("refresh-cw").size(14.0)),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "menu-bar-summary".into())
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .px(px(18.0))
                    .pb(px(14.0))
                    .border_b_1()
                    .border_color(border)
                    .text_size(px(11.0))
                    .text_color(muted_fg)
                    .child(summary),
            )
            .children(keep_awake)
            .children(alert)
            .child(list)
            .child(resources)
            .child(footer)
    }
}
