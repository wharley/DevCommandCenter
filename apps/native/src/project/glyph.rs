//! `ProjectIdentityGlyph`: a tinted tile with the project's mascot, symbol
//! or logo. The mascot is painted cell by cell, snapped to device pixels like
//! the desktop's `shape-rendering: crispEdges`, and alternates frames while
//! an agent runs in the project.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use gpui::{
    App, Bounds, Hsla, Image, ImageFormat, Pixels, Window, canvas, div, fill, img, point,
    prelude::*, px, size, svg,
};

use super::{MascotRows, ProjectIcon, colors, resolve_color, resolve_icon};
use crate::assets::icon_path;
use crate::theme::color::mix_oklab;
use crate::theme::{ActiveTheme, Appearance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphSize {
    /// `size="sm" className="size-4"`: the 16px tile under the composer.
    Tiny,
    /// `size="sm"`: 20px tile.
    Small,
    /// `size="md"`: 28px tile.
    Medium,
    /// `size="lg"`: 36px tile.
    Large,
}

#[derive(IntoElement)]
pub struct ProjectGlyph {
    icon: Option<String>,
    color: Option<String>,
    seed: Option<String>,
    size: GlyphSize,
    tile: Option<f32>,
    /// Shows the second frame (the parent flips it while an agent runs).
    moving: bool,
}

impl ProjectGlyph {
    pub fn new(icon: Option<&str>, color: Option<&str>, seed: Option<&str>) -> Self {
        Self {
            icon: icon.map(str::to_string),
            color: color.map(str::to_string),
            seed: seed.map(str::to_string),
            size: GlyphSize::Small,
            tile: None,
            moving: false,
        }
    }

    pub fn size(mut self, size: GlyphSize) -> Self {
        self.size = size;
        self
    }

    /// Overrides the tile side, like the rail's `size-[18px]`.
    pub fn tile(mut self, side: f32) -> Self {
        self.tile = Some(side);
        self
    }

    pub fn moving(mut self, moving: bool) -> Self {
        self.moving = moving;
        self
    }
}

impl RenderOnce for ProjectGlyph {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let appearance = theme.appearance;
        let seed = self.seed.as_deref();
        let palette = colors(resolve_color(self.color.as_deref(), seed), appearance);
        let (tile, radius, symbol) = match self.size {
            GlyphSize::Tiny => (16.0, theme.radius.md, 12.0),
            GlyphSize::Small => (20.0, theme.radius.md, 12.0),
            GlyphSize::Medium => (28.0, theme.radius.lg, 14.0),
            GlyphSize::Large => (36.0, theme.radius.xl, 18.0),
        };
        let tile = self.tile.unwrap_or(tile);
        // 80% of the content box (inside the 1px border), as the SVG's width.
        let content = (tile - 2.0) * 0.8;

        let inner = match resolve_icon(self.icon.as_deref(), seed) {
            ProjectIcon::Mascot { rest, moving } => {
                let rows = if self.moving { *moving } else { *rest };
                sprite(rows, palette.foreground, appearance, content).into_any_element()
            }
            ProjectIcon::Symbol(name) => svg()
                .path(icon_path(name, 1.9))
                .size(px(symbol))
                .text_color(palette.foreground)
                .into_any_element(),
            ProjectIcon::Logo(uri) => match decode_logo(&uri) {
                Some(image) => img(image)
                    .size(px(content))
                    .rounded(px(3.0))
                    .into_any_element(),
                None => div().into_any_element(),
            },
        };

        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(tile))
            .rounded(radius)
            .border_1()
            .border_color(palette.border)
            .bg(palette.background)
            .child(inner)
    }
}

/// `.dcc-mascot-accent`: half opacity on light, mixed halfway to white on dark.
fn accent_color(body: Hsla, appearance: Appearance) -> Hsla {
    match appearance {
        Appearance::Light => body.opacity(0.5),
        Appearance::Dark => mix_oklab(body, 0.5, gpui::white()),
    }
}

pub(crate) fn sprite(
    rows: MascotRows,
    body: Hsla,
    appearance: Appearance,
    side: f32,
) -> impl IntoElement {
    let accent = accent_color(body, appearance);
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let scale = window.scale_factor();
            let cell = f32::from(bounds.size.width) / 10.0;
            let origin = bounds.origin;
            // Snaps an edge to the device pixel grid, like crispEdges.
            let snap = |logical: f32| (logical * scale).round() / scale;
            for (y, row) in rows.iter().enumerate() {
                for (x, glyph) in row.chars().enumerate() {
                    let color = match glyph {
                        '#' => body,
                        '+' => accent,
                        _ => continue,
                    };
                    let left = snap(f32::from(origin.x) + x as f32 * cell);
                    let top = snap(f32::from(origin.y) + y as f32 * cell);
                    let right = snap(f32::from(origin.x) + (x + 1) as f32 * cell);
                    let bottom = snap(f32::from(origin.y) + (y + 1) as f32 * cell);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(left), px(top)),
                            size(px(right - left), px(bottom - top)),
                        ),
                        color,
                    ));
                }
            }
        },
    )
    .size(px(side))
}

/// Decoded logos, kept for the life of the app (a few KB each).
fn decode_logo(uri: &str) -> Option<Arc<Image>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Image>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(image) = cache.lock().ok()?.get(uri) {
        return Some(image.clone());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(uri.strip_prefix("data:image/png;base64,")?)
        .ok()?;
    let image = Arc::new(Image::from_bytes(ImageFormat::Png, bytes));
    cache.lock().ok()?.insert(uri.to_string(), image.clone());
    Some(image)
}
