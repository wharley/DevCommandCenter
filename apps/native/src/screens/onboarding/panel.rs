//! `WizardPanel`: the step's card — "1 / 3", its title and text, the three
//! chips, and what the step shows: opening or cloning a project and the
//! optional CLI login; the worktree, the branch and completing; chat,
//! terminal and Inspector, and where the Help is.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px, relative};

use super::Onboarding;
use crate::data::onboarding::Step;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Icon};

/// `sm:` in the desktop.
const SMALL: f32 = 640.0;

fn detail_card(
    title: SharedString,
    body: SharedString,
    icon: &'static str,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex_1()
        .min_w_0()
        .rounded(px(12.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.15))
        .p(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .size(px(32.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.6))
                        .bg(colors.background.opacity(0.8))
                        .text_color(colors.muted_foreground)
                        .child(Icon::new(icon).size(16.0)),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(title),
                ),
        )
        .child(
            div()
                .mt(px(8.0))
                .text_size(px(12.0))
                .line_height(relative(1.625))
                .text_color(colors.muted_foreground)
                .child(body),
        )
        .into_any_element()
}

fn chip(label: SharedString, active: bool, done: bool, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    let (border, bg, fg) = if active {
        (
            colors.foreground.opacity(0.2),
            colors.background,
            colors.foreground,
        )
    } else if done {
        (
            colors.border.opacity(0.6),
            colors.muted.opacity(0.3),
            colors.muted_foreground,
        )
    } else {
        (
            colors.border.opacity(0.4),
            gpui::transparent_black(),
            colors.muted_foreground.opacity(0.7),
        )
    };
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded_full()
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .px(px(12.0))
        .py(px(4.0))
        .text_size(px(11.0))
        .child(div().size(px(6.0)).rounded_full().bg(fg))
        .child(label)
        .into_any_element()
}

impl Onboarding {
    fn row(&self, cards: Vec<AnyElement>, wide: bool) -> AnyElement {
        div()
            .flex()
            .gap(px(12.0))
            .when(!wide, |row| row.flex_col())
            .children(cards)
            .into_any_element()
    }

    pub(super) fn panel(&self, viewport_width: f32, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let step = self.step;
        let wide = viewport_width >= SMALL;
        let key = step.key();
        let chips: Vec<AnyElement> = Step::ALL
            .iter()
            .map(|chip_step| {
                chip(
                    cx.t(&format!("onboarding.chips.{}", chip_step.key())),
                    *chip_step == step,
                    chip_step.index() < step.index(),
                    cx,
                )
            })
            .collect();
        let card = |title: &str, body: &str, icon: &'static str, cx: &gpui::App| {
            detail_card(cx.t(title), cx.t(body), icon, cx)
        };
        let extra: AnyElement = match step {
            Step::Project => div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(self.row(
                    vec![
                        card(
                            "onboarding.project.open.title",
                            "onboarding.project.open.body",
                            "folder-open",
                            cx,
                        ),
                        card(
                            "onboarding.project.clone.title",
                            "onboarding.project.clone.body",
                            "link-2",
                            cx,
                        ),
                    ],
                    wide,
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .map(|row| {
                            if wide {
                                row.items_center().justify_between()
                            } else {
                                row.flex_col()
                            }
                        })
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.6))
                        .bg(colors.sidebar.opacity(0.4))
                        .p(px(12.0))
                        .child(
                            div()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(cx.t("onboarding.project.cliTitle")),
                                )
                                .child(
                                    div()
                                        .mt(px(2.0))
                                        .text_size(px(12.0))
                                        .line_height(relative(1.625))
                                        .text_color(colors.muted_foreground)
                                        .child(cx.t("onboarding.project.cliBody")),
                                ),
                        )
                        .child(
                            div().flex_none().child(
                                Button::new("onboarding-cli")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .height(32.0)
                                    .radius(9.0)
                                    .padding_x(10.0)
                                    .text_size(12.0)
                                    .on_click(cx.listener(|onboarding, _, _, cx| {
                                        onboarding.open_cli_terminal(cx)
                                    }))
                                    .child(cx.t("onboarding.project.cliButton")),
                            ),
                        ),
                )
                .into_any_element(),
            Step::Task => self.row(
                vec![
                    card(
                        "onboarding.task.worktree.title",
                        "onboarding.task.worktree.body",
                        "folder-git-2",
                        cx,
                    ),
                    card(
                        "onboarding.task.branch.title",
                        "onboarding.task.branch.body",
                        "git-branch",
                        cx,
                    ),
                    card(
                        "onboarding.task.done.title",
                        "onboarding.task.done.body",
                        "circle-check-big",
                        cx,
                    ),
                ],
                wide,
            ),
            Step::Workbench => div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(self.row(
                    vec![
                        card(
                            "onboarding.workbench.chat.title",
                            "onboarding.workbench.chat.body",
                            "message-square-text",
                            cx,
                        ),
                        card(
                            "onboarding.workbench.terminal.title",
                            "onboarding.workbench.terminal.body",
                            "square-terminal",
                            cx,
                        ),
                        card(
                            "onboarding.workbench.inspector.title",
                            "onboarding.workbench.inspector.body",
                            "panel-right",
                            cx,
                        ),
                    ],
                    wide,
                ))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(12.0))
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(colors.foreground.opacity(0.2))
                        .bg(colors.sidebar.opacity(0.4))
                        .p(px(12.0))
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .size(px(32.0))
                                .items_center()
                                .justify_center()
                                .rounded(px(8.0))
                                .bg(colors.foreground)
                                .text_color(colors.background)
                                .child(Icon::new("circle-question-mark").size(16.0).stroke(1.9)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(cx.t("onboarding.workbench.helpTitle")),
                                )
                                .child(
                                    div()
                                        .mt(px(2.0))
                                        .text_size(px(12.0))
                                        .line_height(relative(1.625))
                                        .text_color(colors.muted_foreground)
                                        .child(cx.t("onboarding.workbench.helpBody")),
                                ),
                        ),
                )
                .into_any_element(),
        };
        div()
            .id(SharedString::from(format!("onboarding-panel-{key}")))
            .debug_selector(move || format!("onboarding-panel-{key}"))
            .w(px(640.0_f32.min(viewport_width - 32.0)))
            .overflow_y_scroll()
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background.opacity(0.9))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::black().opacity(0.18),
                offset: gpui::point(px(0.0), px(24.0)),
                blur_radius: px(90.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .p(px(24.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        Badge::new(
                            format!("{} / {}", step.index() + 1, Step::ALL.len()),
                            BadgeVariant::Outline,
                        )
                        .height(28.0)
                        .padding_x(10.0)
                        .text_size(11.0)
                        .weight(FontWeight::NORMAL),
                    )
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(crate::ui::Tracked::new(
                                cx.t(&format!("onboarding.steps.{key}.title")),
                                24.0,
                                -0.03,
                            )),
                    ),
            )
            .child(
                div()
                    .max_w(px(672.0))
                    .text_size(px(14.0))
                    .line_height(px(24.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(&format!("onboarding.steps.{key}.body"))),
            )
            .child(div().flex().flex_wrap().gap(px(8.0)).children(chips))
            .child(extra)
            .into_any_element()
    }
}
