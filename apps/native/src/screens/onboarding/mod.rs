//! `OnboardingWizard`: over the whole window, the shell's sketch scaled to
//! fit (`mockup.rs`) with the step's region lit, the step's card over it
//! (`panel.rs`), and the foot — Voltar, "Pular por agora", Avançar, and on
//! the last step "Concluir e abrir a Ajuda" and Concluir. Esc closes it
//! without finishing (it comes back on the next start).

mod mockup;
mod panel;
#[cfg(test)]
mod tests;

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, Context, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, Render, Window, canvas, div, fill, point, prelude::*, px, size,
};

use crate::data::onboarding::Step;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::theme::color::hex;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster};

/// `top-11`
const TOP: f32 = 44.0;
/// The background grid's step.
const GRID: f32 = 72.0;

pub enum OnboardingEvent {
    /// Esc: closed, not finished.
    Closed,
    /// Finished or skipped.
    Finished,
    /// Finished into the Help.
    FinishedIntoHelp,
}

pub struct Onboarding {
    pub step: Step,
    focus: FocusHandle,
}

impl EventEmitter<OnboardingEvent> for Onboarding {}

impl Focusable for Onboarding {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Onboarding {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            step: Step::Project,
            focus: cx.focus_handle(),
        }
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.step.next() {
            self.step = next;
            cx.notify();
        }
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        if let Some(previous) = self.step.previous() {
            self.step = previous;
            cx.notify();
        }
    }

    /// "Abrir terminal": the system's terminal in the home folder.
    pub(super) fn open_cli_terminal(&mut self, cx: &mut Context<Self>) {
        let home = dirs::home_dir().unwrap_or_default();
        cx.spawn(async move |_, cx| {
            let opened = if cfg!(test) {
                Ok(())
            } else {
                cx.background_executor()
                    .spawn(async move { crate::data::system_terminal::open(&home) })
                    .await
            };
            cx.update(|cx| match opened {
                Ok(()) => Toaster::success(cx.t("onboarding.project.cliOpened"), cx),
                Err(error) if !error.is_empty() => Toaster::error(error, cx),
                Err(_) => Toaster::error(cx.t("onboarding.project.cliFailed"), cx),
            });
        })
        .detach();
    }

    fn foot(&self, width: f32, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = cx.theme().colors.clone();
        let last = self.step.is_last();
        let small = |button: Button| {
            button
                .size(ButtonSize::Sm)
                .height(32.0)
                .radius(9.0)
                .padding_x(10.0)
                .text_size(12.0)
        };
        let with_icon = |icon: &'static str, label: gpui::SharedString, trailing: bool| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .when(!trailing, |row| row.child(Icon::new(icon).size(14.0)))
                .child(label)
                .when(trailing, |row| row.child(Icon::new(icon).size(14.0)))
        };
        let left = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                small(Button::new("onboarding-back").variant(ButtonVariant::Outline))
                    .disabled(self.step.previous().is_none())
                    .on_click(cx.listener(|onboarding, _, _, cx| onboarding.back(cx)))
                    .child(with_icon("chevron-left", cx.t("onboarding.back"), false)),
            )
            .when(!last, |left| {
                left.child(
                    small(Button::new("onboarding-skip").variant(ButtonVariant::Ghost))
                        .text_color(colors.muted_foreground)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(OnboardingEvent::Finished)))
                        .child(cx.t("onboarding.skip")),
                )
            });
        let right = div().flex().items_center().gap(px(8.0)).map(|right| {
            if last {
                right
                    .child(
                        small(Button::new("onboarding-help").variant(ButtonVariant::Outline))
                            .on_click(
                                cx.listener(|_, _, _, cx| {
                                    cx.emit(OnboardingEvent::FinishedIntoHelp)
                                }),
                            )
                            .child(with_icon(
                                "circle-question-mark",
                                cx.t("onboarding.workbench.openHelp"),
                                false,
                            )),
                    )
                    .child(
                        small(Button::new("onboarding-finish"))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(OnboardingEvent::Finished)))
                            .child(cx.t("onboarding.finish")),
                    )
            } else {
                right.child(
                    small(Button::new("onboarding-next"))
                        .on_click(cx.listener(|onboarding, _, _, cx| onboarding.next(cx)))
                        .child(with_icon("chevron-right", cx.t("onboarding.next"), true)),
                )
            }
        });
        div()
            .debug_selector(|| "onboarding-foot".into())
            .w(px(640.0_f32.min(width - 32.0)))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(16.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background.opacity(0.9))
            .px(px(16.0))
            .py(px(12.0))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::black().opacity(0.16),
                offset: point(px(0.0), px(16.0)),
                blur_radius: px(48.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .child(left)
            .child(right)
            .into_any_element()
    }
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let viewport = window.viewport_size();
        let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
        let scale = (width / mockup::WIDTH)
            .min((height - TOP) / mockup::HEIGHT)
            .min(1.0);
        let line = colors.foreground;
        let accent = hex(0x238636);
        let key = self.step.key();
        let lift = if width >= 640.0 { 32.0 } else { 24.0 };
        div()
            .id("onboarding")
            .debug_selector(|| "onboarding".into())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    cx.emit(OnboardingEvent::Closed);
                }
            }))
            .absolute()
            .inset_0()
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            .occlude()
            .child(crate::ui::occlusion::marker())
            // The grid at 8%, fading out from the middle (`mask-image: radial-gradient`).
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let center = bounds.center();
                        let reach = (f32::from(bounds.size.width)
                            .hypot(f32::from(bounds.size.height)))
                            / 2.0
                            * 0.82;
                        let fade = |x: f32, y: f32| {
                            let distance = (x - f32::from(center.x)).hypot(y - f32::from(center.y));
                            (1.0 - distance / reach).clamp(0.0, 1.0)
                        };
                        let (left, top) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
                        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                        let segment = 24.0;
                        let mut x = 0.0;
                        while x <= w {
                            let mut y = 0.0;
                            while y < h {
                                let alpha = 0.08 * fade(left + x, top + y + segment / 2.0);
                                if alpha > 0.002 {
                                    window.paint_quad(fill(
                                        gpui::Bounds::new(
                                            point(px(left + x), px(top + y)),
                                            size(px(1.0), px(segment)),
                                        ),
                                        line.opacity(alpha),
                                    ));
                                }
                                y += segment;
                            }
                            x += GRID;
                        }
                        let mut y = 0.0;
                        while y <= h {
                            let mut x = 0.0;
                            while x < w {
                                let alpha = 0.08 * fade(left + x + segment / 2.0, top + y);
                                if alpha > 0.002 {
                                    window.paint_quad(fill(
                                        gpui::Bounds::new(
                                            point(px(left + x), px(top + y)),
                                            size(px(segment), px(1.0)),
                                        ),
                                        line.opacity(alpha),
                                    ));
                                }
                                x += segment;
                            }
                            y += GRID;
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            // The green glow up top and the sidebar's tone below.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(gpui::relative(0.4))
                    .bg(gpui::linear_gradient(
                        180.0,
                        gpui::linear_color_stop(accent.opacity(0.12), 0.0),
                        gpui::linear_color_stop(accent.opacity(0.0), 1.0),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(gpui::relative(0.4))
                    .bg(gpui::linear_gradient(
                        180.0,
                        gpui::linear_color_stop(colors.sidebar.opacity(0.0), 0.0),
                        gpui::linear_color_stop(colors.sidebar.opacity(0.88), 1.0),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(TOP))
                    .flex()
                    .justify_center()
                    .child(mockup::mockup(self.step, scale, cx)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(TOP))
                    .bottom(px(96.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .pt(px(lift * 2.0))
                    .child(
                        div()
                            .max_h(px((height - 224.0).max(120.0)))
                            .flex()
                            .child(self.panel(width, cx))
                            .with_animation(
                                gpui::SharedString::from(format!("onboarding-step-{key}")),
                                Animation::new(Duration::from_millis(700))
                                    .with_easing(gpui::ease_out_quint()),
                                move |panel, delta| {
                                    panel.opacity(delta).mt(px(lift * (1.0 - delta)))
                                },
                            ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(24.0))
                    .flex()
                    .justify_center()
                    .child(self.foot(width, cx)),
            )
    }
}
