//! Corner radii, one to one with the desktop tokens (`app.css` `@theme`
//! and `dcc-design.css`). GPUI's built-in `rounded_*` scale is different,
//! so views use these values instead.

use gpui::{Pixels, px};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radius {
    /// `--radius-sm`
    pub sm: Pixels,
    /// `--radius-md`
    pub md: Pixels,
    /// `--radius-lg` (`--radius`)
    pub lg: Pixels,
    /// `--radius-xl`
    pub xl: Pixels,
    /// `--radius-2xl`
    pub xxl: Pixels,
    /// `--dcc-radius-control`
    pub control: Pixels,
    /// `--dcc-radius-surface`
    pub surface: Pixels,
    /// `--dcc-radius-dialog`
    pub dialog: Pixels,
}

/// `--radius: 0.625rem` at the desktop's 16px root size.
const BASE_RADIUS: f32 = 10.0;

impl Default for Radius {
    fn default() -> Self {
        Self {
            sm: px(BASE_RADIUS * 0.6),
            md: px(BASE_RADIUS * 0.8),
            lg: px(BASE_RADIUS),
            xl: px(BASE_RADIUS * 1.4),
            xxl: px(BASE_RADIUS * 1.8),
            control: px(10.0),
            surface: px(18.0),
            dialog: px(22.0),
        }
    }
}
