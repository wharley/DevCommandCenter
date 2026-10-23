//! shadcn's `Button` (`components/ui/button.tsx`): the variants and sizes the
//! app uses. Content goes in as children, like the React component.

use gpui::{AnyElement, App, ClickEvent, ElementId, Hsla, Window, div, prelude::*, px};
use smallvec::SmallVec;

use crate::theme::{ActiveTheme, Appearance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Default,
    Ghost,
    Outline,
    Destructive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// `h-8 px-3`, shadcn's default.
    Default,
    /// `h-7 px-2.5`
    Sm,
    /// `size-8`
    Icon,
    /// `size-6`
    IconXs,
    /// `size-7`
    IconSm,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    variant: Variant,
    size: Size,
    disabled: bool,
    /// `text-muted-foreground hover:text-foreground`, the rail's icon buttons.
    muted: bool,
    /// `aria-current="page"`: `bg-accent text-foreground`.
    current: bool,
    /// A text color of its own, at rest and on hover (`text-muted-foreground/60 hover:text-emerald-600`).
    text: Option<Hsla>,
    hover_text: Option<Hsla>,
    children: SmallVec<[AnyElement; 2]>,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: Variant::Default,
            size: Size::Sm,
            disabled: false,
            muted: false,
            current: false,
            text: None,
            hover_text: None,
            children: SmallVec::new(),
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }

    pub fn text_color(mut self, color: Hsla) -> Self {
        self.text = Some(color);
        self
    }

    pub fn hover_text_color(mut self, color: Hsla) -> Self {
        self.hover_text = Some(color);
        self
    }

    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = &theme.colors;
        let dark = theme.appearance == Appearance::Dark;
        let (background, foreground, border, hover_background) = match self.variant {
            Variant::Default => (
                Some(colors.primary),
                colors.primary_foreground,
                None,
                Some(colors.primary.opacity(0.8)),
            ),
            Variant::Ghost => (
                None,
                colors.foreground,
                None,
                Some(if dark {
                    colors.muted.opacity(0.5)
                } else {
                    colors.muted
                }),
            ),
            Variant::Destructive => (
                Some(colors.destructive.opacity(if dark { 0.2 } else { 0.1 })),
                colors.destructive,
                None,
                Some(colors.destructive.opacity(if dark { 0.3 } else { 0.2 })),
            ),
            Variant::Outline => (
                Some(if dark {
                    colors.input.opacity(0.3)
                } else {
                    colors.background
                }),
                colors.foreground,
                Some(if dark { colors.input } else { colors.border }),
                Some(if dark {
                    colors.input.opacity(0.5)
                } else {
                    colors.muted
                }),
            ),
        };
        let foreground = match self.text {
            Some(color) => color,
            None if self.muted => colors.muted_foreground,
            None => foreground,
        };
        let hover_foreground = self.hover_text.unwrap_or(colors.foreground);
        let (width, height, padding) = match self.size {
            Size::Default => (None, 32.0, 12.0),
            Size::Sm => (None, 28.0, 10.0),
            Size::Icon => (Some(32.0), 32.0, 0.0),
            Size::IconXs => (Some(24.0), 24.0, 0.0),
            Size::IconSm => (Some(28.0), 28.0, 0.0),
        };
        let on_click = self.on_click;
        let disabled = self.disabled;
        let current_background = self.current.then_some(colors.accent);

        div()
            .id(self.id)
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .h(px(height))
            .when_some(width, |button, width| button.w(px(width)))
            .px(px(padding))
            .rounded(theme.radius.lg)
            .border_1()
            .border_color(border.unwrap_or(gpui::transparent_black()))
            .text_size(px(14.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if self.current {
                colors.foreground
            } else {
                foreground
            })
            .when_some(current_background.or(background), |button, background| {
                button.bg(background)
            })
            .when(disabled, |button| button.opacity(0.5))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| {
                        let style = style.text_color(hover_foreground);
                        match hover_background {
                            Some(background) if current_background.is_none() => {
                                style.bg(background)
                            }
                            _ => style,
                        }
                    })
                    // `active:translate-y-px`
                    .active(|style| style.top(px(1.0)))
            })
            .when_some(on_click.filter(|_| !disabled), |button, handler| {
                button.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    handler(event, window, cx)
                })
            })
            .children(self.children)
    }
}
