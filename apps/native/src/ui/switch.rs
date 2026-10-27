//! shadcn's `Switch`: a pill (`h-[18.4px] w-8`, `sm` 14×24) on `bg-primary`
//! when on and `bg-input` when off, its thumb (`bg-background`, 16px or
//! 12px) slid to the end when on.

use std::rc::Rc;

use gpui::{App, ElementId, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;

type OnToggle = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwitchSize {
    #[default]
    Default,
    Sm,
}

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    size: SwitchSize,
    on_toggle: Option<OnToggle>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>, checked: bool) -> Self {
        Self {
            id: id.into(),
            checked,
            disabled: false,
            size: SwitchSize::Default,
            on_toggle: None,
        }
    }

    pub fn size(mut self, size: SwitchSize) -> Self {
        self.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called with the new state.
    pub fn on_toggle(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = &cx.theme().colors;
        let (width, height, thumb) = match self.size {
            SwitchSize::Default => (32.0, 18.4, 16.0),
            SwitchSize::Sm => (24.0, 14.0, 12.0),
        };
        // `translate-x-[calc(100%-2px)]` inside the 1px border.
        let travel = if self.checked { thumb - 2.0 } else { 0.0 };
        let selector = self.id.to_string();
        let checked = self.checked;
        div()
            .id(self.id)
            .debug_selector(move || selector.clone())
            .flex()
            .flex_none()
            .items_center()
            .w(px(width))
            .h(px(height))
            .rounded_full()
            .border_1()
            .border_color(gpui::transparent_black())
            .bg(if checked {
                colors.primary
            } else {
                colors.input
            })
            .when(self.disabled, |switch| switch.opacity(0.5))
            .when(!self.disabled, |switch| switch.cursor_pointer())
            .when_some(
                self.on_toggle.filter(|_| !self.disabled),
                |switch, handler| {
                    switch.on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        handler(!checked, window, cx)
                    })
                },
            )
            .child(
                div()
                    .ml(px(travel))
                    .size(px(thumb))
                    .rounded_full()
                    .bg(colors.background),
            )
    }
}
