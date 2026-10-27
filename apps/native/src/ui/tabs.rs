//! shadcn's `Tabs` (default variant): a muted pill (`rounded-lg bg-muted
//! p-1`) holding equal triggers (`flex-1 rounded-md text-sm font-medium`);
//! the active one on `bg-background`, the others muted until hovered.

use std::rc::Rc;

use gpui::{App, ElementId, FontWeight, SharedString, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;

type OnSelect = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Tabs {
    id: ElementId,
    labels: Vec<SharedString>,
    active: usize,
    /// `py-*` on each trigger (`py-0.5` by default).
    padding_y: f32,
    disabled: bool,
    on_select: Option<OnSelect>,
}

impl Tabs {
    pub fn new(id: impl Into<ElementId>, labels: Vec<SharedString>, active: usize) -> Self {
        Self {
            id: id.into(),
            labels,
            active,
            padding_y: 2.0,
            disabled: false,
            on_select: None,
        }
    }

    pub fn padding_y(mut self, padding: f32) -> Self {
        self.padding_y = padding;
        self
    }

    /// Triggers that do nothing while an action runs.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Tabs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let base = self.id.to_string();
        let triggers = self.labels.into_iter().enumerate().map(|(index, label)| {
            let active = index == self.active;
            let handler = self.on_select.clone().filter(|_| !self.disabled);
            let selector = format!("{base}-{index}");
            div()
                .id(SharedString::from(format!("{base}-{index}")))
                .debug_selector(move || selector.clone())
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .rounded(theme.radius.md)
                .border_1()
                .border_color(gpui::transparent_black())
                .px(px(6.0))
                .py(px(self.padding_y))
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .map(|trigger| {
                    if active {
                        trigger.bg(colors.background).text_color(colors.foreground)
                    } else {
                        let hover = colors.foreground;
                        trigger
                            .text_color(colors.muted_foreground)
                            .hover(move |style| style.text_color(hover))
                    }
                })
                .when_some(handler, |trigger, handler| {
                    trigger.on_click(move |_, window, cx| handler(index, window, cx))
                })
                .child(label)
        });
        div()
            .flex()
            .w_full()
            .items_center()
            .justify_center()
            .rounded(theme.radius.lg)
            .bg(colors.muted)
            .p(px(4.0))
            .children(triggers)
    }
}
