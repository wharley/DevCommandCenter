//! Small state marks: a checkbox and the spinning loader (`Loader2 animate-spin`).

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ClickEvent, ElementId, Transformation, Window, div, percentage,
    prelude::*, px,
};

use super::Icon;
use crate::theme::ActiveTheme;

type OnToggle = Box<dyn Fn(bool, &mut Window, &mut App) + 'static>;

/// A 16px checkbox in the primary color (`size-4 accent-primary`).
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    /// The side: 16px (`size-4`) unless set (`size-3.5`).
    side: f32,
    checked: bool,
    disabled: bool,
    on_toggle: Option<OnToggle>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, checked: bool) -> Self {
        Self {
            id: id.into(),
            side: 16.0,
            checked,
            disabled: false,
            on_toggle: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn side(mut self, side: f32) -> Self {
        self.side = side;
        self
    }

    pub fn on_toggle(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = &cx.theme().colors;
        let checked = self.checked;
        let on_toggle = self.on_toggle;
        div()
            .id(self.id)
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(self.side))
            .rounded(px(self.side / 4.0))
            .border_1()
            .border_color(if checked {
                colors.primary
            } else {
                colors.input
            })
            .when(checked, |box_| {
                box_.bg(colors.primary)
                    .text_color(colors.primary_foreground)
            })
            .when(!checked, |box_| box_.bg(colors.background))
            .when(self.disabled, |box_| box_.opacity(0.5))
            .when(!self.disabled, |box_| box_.cursor_pointer())
            .when_some(on_toggle.filter(|_| !self.disabled), |box_, handler| {
                box_.on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    handler(!checked, window, cx)
                })
            })
            .when(checked, |box_| {
                box_.child(Icon::new("check").size(self.side * 0.75).stroke(3.0))
            })
    }
}

/// `Loader2` turning once a second, in the text color around it.
pub fn spinner(id: impl Into<ElementId>, size: f32) -> Spinner {
    Spinner {
        id: id.into(),
        size,
    }
}

#[derive(IntoElement)]
pub struct Spinner {
    id: ElementId,
    size: f32,
}

impl RenderOnce for Spinner {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // An svg is drawn in its own text color, not inherited (see `Icon`).
        let color = window.text_style().color;
        div().flex_shrink_0().child(
            gpui::svg()
                .path(crate::assets::icon_path("loader-circle", 2.0))
                .size(px(self.size))
                .text_color(color)
                .with_animation(
                    self.id,
                    Animation::new(Duration::from_secs(1)).repeat(),
                    |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
                ),
        )
    }
}

/// `DccThinkingIndicator`: a ring with three dots pulsing one after another
/// (`pulse 1.2s`, 180ms apart).
pub fn thinking_indicator(id: impl Into<ElementId>, size: f32) -> ThinkingIndicator {
    ThinkingIndicator {
        id: id.into(),
        size,
    }
}

#[derive(IntoElement)]
pub struct ThinkingIndicator {
    id: ElementId,
    size: f32,
}

impl RenderOnce for ThinkingIndicator {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = &cx.theme().colors;
        let foreground = colors.foreground;
        let dots = [(0.8, 0.0, 0u64), (0.55, -5.0, 180), (0.4, 5.0, 360)];
        div()
            .relative()
            .flex_shrink_0()
            .size(px(self.size))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border.opacity(0.7)),
            )
            .children(
                dots.into_iter()
                    .enumerate()
                    .map(move |(index, (alpha, offset, delay))| {
                        let dot = 6.0;
                        let center = (self.size - dot) / 2.0;
                        div()
                            .absolute()
                            .left(px(center))
                            .top(px(center + offset))
                            .size(px(dot))
                            .rounded_full()
                            .bg(foreground.opacity(alpha))
                            .with_animation(
                                ElementId::NamedInteger(
                                    format!("{:?}-dot", self.id).into(),
                                    index as u64,
                                ),
                                Animation::new(Duration::from_millis(1200)).repeat(),
                                move |dot, delta| {
                                    // Tailwind `pulse`: opacity 1 → .5 → 1, shifted by the delay.
                                    let phase = (delta + delay as f32 / 1200.0).fract();
                                    let fade = 1.0 - 0.5 * (1.0 - (2.0 * phase - 1.0).abs());
                                    dot.opacity(fade)
                                },
                            )
                    }),
            )
    }
}
