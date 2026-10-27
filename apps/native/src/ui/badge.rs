//! shadcn's `Badge`: a pill (`rounded-4xl`, 26px) that fits its text, h20,
//! px8, 12px medium. Callers that override the size (`h-7 px-2.5
//! text-[11px] font-normal`) say so with the builders.

use gpui::{App, FontWeight, SharedString, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeVariant {
    /// `border-border bg-transparent text-foreground`
    Outline,
    /// `bg-secondary text-secondary-foreground`
    Secondary,
    /// `bg-destructive/10 text-destructive`
    Destructive,
    /// `bg-transparent text-foreground`
    Ghost,
    /// `color-mix(in oklab, var(--success) 20%, var(--secondary))`
    Success,
    /// `color-mix(in oklab, var(--warning) 20%, var(--secondary))`
    Warn,
}

#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    variant: BadgeVariant,
    height: f32,
    padding_x: f32,
    text_size: f32,
    weight: FontWeight,
    icon: Option<&'static str>,
}

impl Badge {
    pub fn new(label: impl Into<SharedString>, variant: BadgeVariant) -> Self {
        Self {
            label: label.into(),
            variant,
            height: 20.0,
            padding_x: 8.0,
            text_size: 12.0,
            weight: FontWeight::MEDIUM,
            icon: None,
        }
    }

    /// A 12px icon before the text (`gap-1`).
    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn padding_x(mut self, padding: f32) -> Self {
        self.padding_x = padding;
        self
    }

    pub fn text_size(mut self, size: f32) -> Self {
        self.text_size = size;
        self
    }

    /// `font-normal` and the like.
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = &cx.theme().colors;
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(self.height))
            .px(px(self.padding_x))
            .rounded(px(26.0))
            .border_1()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(self.text_size))
            .font_weight(self.weight)
            .map(|badge| match self.variant {
                BadgeVariant::Outline => badge
                    .border_color(colors.border)
                    .text_color(colors.foreground),
                BadgeVariant::Secondary => badge
                    .border_color(gpui::transparent_black())
                    .bg(colors.secondary)
                    .text_color(colors.secondary_foreground),
                BadgeVariant::Destructive => badge
                    .border_color(gpui::transparent_black())
                    .bg(colors.destructive.opacity(0.1))
                    .text_color(colors.destructive),
                BadgeVariant::Ghost => badge
                    .border_color(gpui::transparent_black())
                    .text_color(colors.foreground),
                BadgeVariant::Success => badge
                    .border_color(gpui::transparent_black())
                    .bg(mix_oklab(colors.success, 0.2, colors.secondary))
                    .text_color(colors.foreground),
                BadgeVariant::Warn => badge
                    .border_color(gpui::transparent_black())
                    .bg(mix_oklab(colors.warning, 0.2, colors.secondary))
                    .text_color(colors.foreground),
            })
            .gap(px(4.0))
            .children(self.icon.map(|icon| crate::ui::Icon::new(icon).size(12.0)))
            .child(self.label)
    }
}
