//! shadcn's `ToggleGroup` (single choice) as Settings uses it: 32px items
//! (`h-8 rounded-lg border border-border/60 px-3 text-[12px]`), the chosen
//! one on `bg-muted` or, for the language, `bg-accent`.

use std::rc::Rc;

use gpui::{App, ElementId, FontWeight, SharedString, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;

type OnSelect = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ToggleGroup {
    id: ElementId,
    labels: Vec<SharedString>,
    selected: Option<usize>,
    /// `text-muted-foreground` at rest and `bg-accent` when on.
    accent: bool,
    on_select: Option<OnSelect>,
}

impl ToggleGroup {
    pub fn new(
        id: impl Into<ElementId>,
        labels: Vec<SharedString>,
        selected: Option<usize>,
    ) -> Self {
        Self {
            id: id.into(),
            labels,
            selected,
            accent: false,
            on_select: None,
        }
    }

    /// The language's look: muted until on, then `bg-accent text-foreground`.
    pub fn accent(mut self) -> Self {
        self.accent = true;
        self
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ToggleGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let base = self.id.to_string();
        let items = self.labels.into_iter().enumerate().map(|(index, label)| {
            let on = self.selected == Some(index);
            let handler = self.on_select.clone();
            let selector = format!("{base}-{index}");
            let (rest_text, on_background) = if self.accent {
                (colors.muted_foreground, colors.accent)
            } else {
                (colors.foreground, colors.muted)
            };
            let hover = colors.muted;
            let foreground = colors.foreground;
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .h(px(32.0))
                .px(px(12.0))
                .rounded(theme.radius.lg)
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .cursor_pointer()
                .map(|item| {
                    if on {
                        item.bg(on_background).text_color(foreground)
                    } else {
                        item.text_color(rest_text)
                            .hover(move |style| style.bg(hover).text_color(foreground))
                    }
                })
                .when_some(handler, |item, handler| {
                    item.on_click(move |_, window, cx| handler(index, window, cx))
                })
                .child(label)
        });
        div().flex().items_center().gap(px(4.0)).children(items)
    }
}
