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
    checked: bool,
    disabled: bool,
    on_toggle: Option<OnToggle>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, checked: bool) -> Self {
        Self {
            id: id.into(),
            checked,
            disabled: false,
            on_toggle: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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
            .size(px(16.0))
            .rounded(px(4.0))
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
                box_.child(Icon::new("check").size(12.0).stroke(3.0))
            })
    }
}

/// `Loader2` turning once a second.
pub fn spinner(id: impl Into<ElementId>, size: f32) -> impl IntoElement {
    let id: ElementId = id.into();
    div().flex_shrink_0().child(
        gpui::svg()
            .path(crate::assets::icon_path("loader-circle", 2.0))
            .size(px(size))
            .with_animation(
                id,
                Animation::new(Duration::from_secs(1)).repeat(),
                |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
            ),
    )
}
