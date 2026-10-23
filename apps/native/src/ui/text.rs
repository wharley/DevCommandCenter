//! Text helpers GPUI lacks: CSS `letter-spacing` (Tailwind `tracking-*`)
//! and `tabular-nums`.

use std::sync::Arc;

use gpui::{App, FontFeatures, SharedString, Window, div, prelude::*, px};

/// `font-variant-numeric: tabular-nums`.
pub fn tabular_nums() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

/// Text with `letter-spacing` in em, like `tracking-[0.08em]`. GPUI has no
/// letter spacing, so each character is laid out on its own with the extra
/// space after it, as CSS adds it. Meant for short labels.
#[derive(IntoElement)]
pub struct Tracked {
    text: SharedString,
    font_size: f32,
    tracking_em: f32,
}

impl Tracked {
    pub fn new(text: impl Into<SharedString>, font_size: f32, tracking_em: f32) -> Self {
        Self {
            text: text.into(),
            font_size,
            tracking_em,
        }
    }
}

impl RenderOnce for Tracked {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let spacing = px(self.font_size * self.tracking_em);
        div()
            .flex()
            .flex_shrink_0()
            .whitespace_nowrap()
            .text_size(px(self.font_size))
            .children(self.text.chars().map(move |character| {
                // A lone space would collapse; a no-break space keeps its width.
                let glyph = if character == ' ' {
                    '\u{a0}'
                } else {
                    character
                };
                div()
                    .mr(spacing)
                    .child(SharedString::from(glyph.to_string()))
            }))
    }
}
