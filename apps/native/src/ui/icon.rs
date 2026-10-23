//! A Lucide icon, like `<Plus className="size-4" strokeWidth={1.9} />`.

use gpui::{App, Hsla, Window, prelude::*, px, svg};

use crate::assets::icon_path;

#[derive(IntoElement)]
pub struct Icon {
    name: &'static str,
    size: f32,
    stroke: f32,
    color: Option<Hsla>,
    rotation_deg: f32,
}

impl Icon {
    /// `name` is the Lucide file name (`panel-left`, `circle-check-big`…).
    pub fn new(name: &'static str) -> Self {
        debug_assert!(crate::assets::has_icon(name), "missing icon {name}");
        Self {
            name,
            size: 16.0,
            stroke: 2.0,
            color: None,
            rotation_deg: 0.0,
        }
    }

    /// Side in px (`size-4` = 16).
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Lucide `strokeWidth`.
    pub fn stroke(mut self, stroke: f32) -> Self {
        self.stroke = stroke;
        self
    }

    /// Without a color the icon takes the surrounding text color.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// `rotate-90`, `rotate-[-12deg]`.
    pub fn rotate(mut self, degrees: f32) -> Self {
        self.rotation_deg = degrees;
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // GPUI's svg paints only with its own color; without one the icon
        // takes the inherited text color, like CSS `currentColor`.
        let color = self.color.unwrap_or_else(|| window.text_style().color);
        let icon = svg()
            .path(icon_path(self.name, self.stroke))
            .size(px(self.size))
            .flex_shrink_0()
            .text_color(color);
        if self.rotation_deg == 0.0 {
            icon
        } else {
            icon.with_transformation(gpui::Transformation::rotate(gpui::radians(
                self.rotation_deg.to_radians(),
            )))
        }
    }
}
