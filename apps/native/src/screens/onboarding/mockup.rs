//! `OnboardingMockup`: a quiet, scaled-down sketch of the shell (1300×900,
//! shrunk to fit). Its labels come from the app's own texts. Each step
//! lifts one region and dims the rest. GPUI has no `transform: scale`, so
//! every measure here is multiplied by the scale.

use gpui::{AnyElement, App, FontWeight, Hsla, Pixels, SharedString, div, prelude::*, px};

use crate::data::onboarding::Step;
use crate::i18n::Translate;
use crate::project::{GlyphSize, ProjectGlyph};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::Icon;

pub const WIDTH: f32 = 1300.0;
pub const HEIGHT: f32 = 900.0;

#[derive(Clone, Copy)]
struct Scale(f32);

impl Scale {
    fn u(self, value: f32) -> Pixels {
        px(value * self.0)
    }
}

fn region(lit: bool, s: Scale, cx: &App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_col()
        .min_h_0()
        .rounded(s.u(16.0))
        .border_1()
        .bg(colors.background.opacity(0.8))
        .p(s.u(12.0))
        .map(|region| {
            if lit {
                region
                    .border_color(colors.foreground.opacity(0.25))
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::black().opacity(0.14),
                        offset: gpui::point(px(0.0), s.u(18.0)),
                        blur_radius: s.u(60.0),
                        spread_radius: px(0.0),
                        inset: false,
                    }])
            } else {
                region
                    .border_color(colors.border.opacity(0.5))
                    .opacity(0.45)
            }
        })
}

fn icon(name: &'static str, size: f32, s: Scale) -> Icon {
    Icon::new(name).size(size * s.0)
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Running,
    Waiting,
    Done,
}

fn task_row(
    name: &str,
    project: &str,
    status: SharedString,
    lit: bool,
    tone: Tone,
    s: Scale,
    cx: &App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    let dark = cx.theme().appearance == Appearance::Dark;
    let status_color = match tone {
        Tone::Running if dark => tw::EMERALD_300.hsla().opacity(0.9),
        Tone::Running => tw::EMERALD_700.hsla(),
        Tone::Waiting if dark => tw::AMBER_300.hsla().opacity(0.9),
        Tone::Waiting => tw::AMBER_700.hsla(),
        Tone::Done => colors.muted_foreground,
    };
    let seed = format!("/mockup/{project}");
    div()
        .flex()
        .items_start()
        .gap(s.u(8.0))
        .rounded(s.u(8.0))
        .border_1()
        .px(s.u(8.0))
        .py(s.u(6.0))
        .map(|row| {
            if lit {
                row.border_color(colors.foreground.opacity(0.3))
                    .bg(colors.background)
            } else {
                row.border_color(gpui::transparent_black())
            }
        })
        .child(
            div().mt(px(1.0)).child(
                ProjectGlyph::new(None, None, Some(&seed))
                    .size(GlyphSize::Small)
                    .tile(16.0 * s.0)
                    .moving(tone == Tone::Running),
            ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(s.u(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground.opacity(0.9))
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap(s.u(4.0))
                        .text_size(s.u(10.0))
                        .text_color(colors.muted_foreground)
                        .child(div().truncate().child(project.to_string()))
                        .child(div().opacity(0.4).child("·"))
                        .child(div().flex_none().text_color(status_color).child(status)),
                ),
        )
        .into_any_element()
}

fn callout(icon_name: &'static str, label: SharedString, s: Scale, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .mt(s.u(16.0))
        .flex()
        .items_center()
        .gap(s.u(8.0))
        .rounded(s.u(8.0))
        .border_1()
        .border_color(colors.foreground.opacity(0.3))
        .bg(colors.foreground)
        .px(s.u(10.0))
        .py(s.u(6.0))
        .text_size(s.u(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.background)
        .shadow(vec![gpui::BoxShadow {
            color: colors.foreground.opacity(0.1),
            offset: gpui::point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: s.u(6.0),
            inset: false,
        }])
        .child(icon(icon_name, 14.0, s))
        .child(label)
        .into_any_element()
}

fn bubble(end: bool, text: SharedString, s: Scale, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .when(end, |row| row.justify_end())
        .child(
            div()
                .max_w(relative_width(0.78))
                .rounded(s.u(16.0))
                .px(s.u(12.0))
                .py(s.u(8.0))
                .text_size(s.u(12.0))
                .line_height(s.u(20.0))
                .map(|bubble| {
                    if end {
                        bubble
                            .bg(colors.foreground.opacity(0.9))
                            .text_color(colors.background)
                    } else {
                        bubble
                            .bg(colors.muted.opacity(0.4))
                            .text_color(colors.foreground.opacity(0.9))
                    }
                })
                .child(text),
        )
        .into_any_element()
}

fn relative_width(fraction: f32) -> gpui::DefiniteLength {
    gpui::relative(fraction)
}

fn activity(text: SharedString, s: Scale, cx: &App) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(s.u(8.0))
        .text_size(s.u(11.0))
        .line_height(s.u(20.0))
        .text_color(cx.theme().colors.foreground.opacity(0.85))
        .child(
            div()
                .mt(s.u(8.0))
                .flex_none()
                .size(s.u(6.0))
                .rounded_full()
                .bg(tw::EMERALD_500.hsla()),
        )
        .child(text)
        .into_any_element()
}

fn diff_row(path: &str, added: u32, removed: u32, s: Scale, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let dark = cx.theme().appearance == Appearance::Dark;
    let (green, red): (Hsla, Hsla) = if dark {
        (tw::EMERALD_400.hsla(), tw::RED_400.hsla())
    } else {
        (tw::EMERALD_600.hsla(), tw::RED_600.hsla())
    };
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(s.u(8.0))
        .rounded(s.u(8.0))
        .border_1()
        .border_color(colors.border.opacity(0.5))
        .bg(colors.background.opacity(0.7))
        .px(s.u(10.0))
        .py(s.u(6.0))
        .font_family(fonts::MONO)
        .text_size(s.u(11.0))
        .child(
            div()
                .truncate()
                .text_color(colors.foreground.opacity(0.9))
                .child(path.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .gap(s.u(4.0))
                .child(div().text_color(green).child(format!("+{added}")))
                .child(div().text_color(red).child(format!("-{removed}"))),
        )
        .into_any_element()
}

/// The sketch at `scale`.
pub fn mockup(step: Step, scale: f32, cx: &App) -> AnyElement {
    let s = Scale(scale);
    let colors = cx.theme().colors.clone();
    let sidebar_lit = matches!(step, Step::Project | Step::Task);
    let workbench = step == Step::Workbench;
    let muted = colors.muted_foreground;
    let sidebar = region(sidebar_lit, s, cx)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(s.u(4.0))
                .child(
                    div()
                        .text_size(s.u(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("sidebar.tasksTitle")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(s.u(6.0))
                        .text_color(muted)
                        .child(
                            icon("folder-plus", 14.0, s)
                                .when(step == Step::Project, |icon| icon.color(colors.foreground)),
                        )
                        .child(icon("plus", 14.0, s)),
                ),
        )
        .child(
            div()
                .mt(s.u(12.0))
                .px(s.u(4.0))
                .text_size(s.u(9.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(muted.opacity(0.7))
                .child(cx.t("sidebar.taskSection.today").to_uppercase()),
        )
        .child(
            div()
                .mt(s.u(4.0))
                .flex()
                .flex_col()
                .gap(s.u(4.0))
                .child(task_row(
                    "feat/checkout-stripe",
                    "minha-api",
                    cx.t("sidebar.agentState.active"),
                    step == Step::Task,
                    Tone::Running,
                    s,
                    cx,
                ))
                .child(task_row(
                    "fix/login-500",
                    "minha-api",
                    cx.t("sidebar.agentState.waiting"),
                    false,
                    Tone::Waiting,
                    s,
                    cx,
                ))
                .child(task_row(
                    "feat/landing-hero",
                    "web-app",
                    cx.t("sidebar.agentState.completed"),
                    false,
                    Tone::Done,
                    s,
                    cx,
                )),
        )
        .when(step == Step::Project, |sidebar| {
            sidebar.child(callout("folder-open", cx.t("sidebar.openProject"), s, cx))
        })
        .when(step == Step::Task, |sidebar| {
            sidebar.child(callout("plus", cx.t("sidebar.newWorkspace"), s, cx))
        })
        .child(
            div()
                .mt_auto()
                .flex()
                .items_center()
                .gap(s.u(8.0))
                .px(s.u(4.0))
                .pt(s.u(12.0))
                .text_color(muted)
                .child(icon("chart-column", 16.0, s))
                .child(icon("sparkles", 16.0, s))
                .child(icon("settings-2", 16.0, s))
                .child(
                    div()
                        .flex()
                        .size(s.u(24.0))
                        .items_center()
                        .justify_center()
                        .rounded(s.u(6.0))
                        .when(workbench, |help| {
                            help.bg(colors.foreground)
                                .text_color(colors.background)
                                .shadow(vec![gpui::BoxShadow {
                                    color: colors.foreground.opacity(0.14),
                                    offset: gpui::point(px(0.0), px(0.0)),
                                    blur_radius: px(0.0),
                                    spread_radius: s.u(6.0),
                                    inset: false,
                                }])
                        })
                        .child(icon("circle-question-mark", 16.0, s)),
                ),
        );
    let lit_edge = |lit: bool| {
        if lit {
            colors.foreground.opacity(0.3)
        } else {
            colors.border.opacity(0.6)
        }
    };
    let center = region(workbench, s, cx)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(colors.border.opacity(0.5))
                .pb(s.u(8.0))
                .child(
                    div()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(s.u(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child("feat/checkout-stripe"),
                        )
                        .child(
                            div()
                                .text_size(s.u(11.0))
                                .text_color(muted)
                                .child("minha-api"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(s.u(8.0))
                        .text_color(muted)
                        .child(icon("square-terminal", 16.0, s))
                        .child(icon("globe", 16.0, s)),
                ),
        )
        .child(
            div()
                .mt(s.u(12.0))
                .flex()
                .flex_col()
                .flex_1()
                .gap(s.u(12.0))
                .overflow_hidden()
                .child(bubble(true, cx.t("onboarding.mockup.userMessage"), s, cx))
                .child(bubble(false, cx.t("onboarding.mockup.agentMessage"), s, cx)),
        )
        .child(
            div()
                .mt(s.u(12.0))
                .rounded(s.u(12.0))
                .border_1()
                .border_color(lit_edge(workbench))
                .bg(colors.background.opacity(0.8))
                .px(s.u(12.0))
                .py(s.u(8.0))
                .text_size(s.u(12.0))
                .text_color(muted)
                .child(cx.t("composer.placeholder.default")),
        )
        .child(
            div()
                .mt(s.u(12.0))
                .rounded(s.u(12.0))
                .border_1()
                .border_color(lit_edge(workbench))
                .bg(colors.sidebar.opacity(0.6))
                .p(s.u(12.0))
                .font_family(fonts::MONO)
                .text_size(s.u(11.0))
                .line_height(s.u(20.0))
                .text_color(muted)
                .child(
                    div()
                        .mb(s.u(4.0))
                        .flex()
                        .items_center()
                        .gap(s.u(8.0))
                        .font_family(cx.theme().fonts.sans.clone())
                        .child(icon("square-terminal", 14.0, s))
                        .child(cx.t("terminalDock.scopes.worktree")),
                )
                .child("$ yarn dev")
                .child("ready on http://localhost:3000"),
        );
    let tabs: Vec<AnyElement> = [
        cx.t("inspector.tabs.activity"),
        cx.t("inspector.tabs.context"),
        cx.t("inspector.tabs.spec"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, label)| {
        div()
            .flex_1()
            .rounded(s.u(6.0))
            .px(s.u(8.0))
            .py(s.u(4.0))
            .flex()
            .justify_center()
            .map(|tab| {
                if index == 0 {
                    tab.bg(colors.background).text_color(colors.foreground)
                } else {
                    tab.text_color(muted)
                }
            })
            .child(label)
            .into_any_element()
    })
    .collect();
    let inspector = region(workbench, s, cx)
        .child(
            div()
                .flex()
                .gap(s.u(4.0))
                .rounded(s.u(8.0))
                .bg(colors.muted.opacity(0.3))
                .p(s.u(4.0))
                .text_size(s.u(11.0))
                .children(tabs),
        )
        .child(
            div()
                .mt(s.u(12.0))
                .flex()
                .flex_col()
                .gap(s.u(8.0))
                .child(activity(cx.t("onboarding.mockup.activityRead"), s, cx))
                .child(activity(cx.t("onboarding.mockup.activityEdit"), s, cx))
                .child(activity(cx.t("onboarding.mockup.activityTest"), s, cx)),
        )
        .child(
            div()
                .mt_auto()
                .child(
                    div()
                        .px(s.u(4.0))
                        .text_size(s.u(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(muted)
                        .child(cx.t("inspector.modeDock.git")),
                )
                .child(
                    div()
                        .mt(s.u(8.0))
                        .flex()
                        .flex_col()
                        .gap(s.u(6.0))
                        .child(diff_row("src/checkout/stripe.ts", 42, 3, s, cx))
                        .child(diff_row("src/checkout/stripe.test.ts", 18, 0, s, cx)),
                ),
        );
    div()
        .debug_selector(|| "onboarding-mockup".into())
        .w(s.u(WIDTH))
        .h(s.u(HEIGHT))
        .overflow_hidden()
        .rounded(s.u(28.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(gpui::linear_gradient(
            180.0,
            gpui::linear_color_stop(colors.background.opacity(0.94), 0.0),
            gpui::linear_color_stop(colors.sidebar.opacity(0.96), 1.0),
        ))
        .p(s.u(16.0))
        .child(
            div()
                .flex()
                .size_full()
                .gap(s.u(12.0))
                .child(
                    div()
                        .w(s.u(236.0))
                        .flex_none()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(sidebar.h_full()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(center.h_full()),
                )
                .child(
                    div()
                        .w(s.u(252.0))
                        .flex_none()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(inspector.h_full()),
                ),
        )
        .into_any_element()
}
