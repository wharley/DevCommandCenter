//! The DCC's surface tokens shared by cards and panels: `--dcc-surface-edge`
//! (the border at 80%) and `--dcc-shadow-card`.

use gpui::{BoxShadow, Hsla, point, px};

use crate::theme::Theme;

/// shadcn's Input fill (transparent, `dark:bg-input/30`) made opaque over a
/// dialog's `popover`: GPUI paints a focus ring's shadow as a filled shape
/// under its element, so a see-through field would fill with the ring.
pub fn input_fill(theme: &Theme) -> Hsla {
    let colors = &theme.colors;
    if theme.appearance == crate::theme::Appearance::Dark {
        crate::theme::color::mix_srgb(colors.input, 0.3, colors.popover)
    } else {
        colors.popover
    }
}

/// `--dcc-surface-edge`: the border at 80%.
pub fn surface_edge(theme: &Theme) -> Hsla {
    theme.colors.border.opacity(0.8)
}

/// `--dcc-shadow-card`: `0 2px 3px rgb(0 0 0/.025), 0 8px 24px rgb(0 0 0/.04)`.
pub fn card_shadow() -> Vec<BoxShadow> {
    let shadow = |alpha: f32, y: f32, blur: f32| BoxShadow {
        color: Hsla::black().opacity(alpha),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.0),
        inset: false,
    };
    vec![shadow(0.025, 2.0, 3.0), shadow(0.04, 8.0, 24.0)]
}
