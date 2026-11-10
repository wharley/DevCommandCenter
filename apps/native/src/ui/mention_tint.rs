//! How an `@Agent` reads: the agent's mascot color as a faint fill under
//! text in the same color, pulled toward the foreground so it stays legible
//! in the light and the dark theme. Shared by the room's messages, the
//! markdown of an answer and the composer's field.

use gpui::{App, Hsla};

use crate::theme::color::{hex, mix_oklab};
use crate::theme::{ActiveTheme, Appearance};

/// Fill and text colors of a mention in `rgb` (`0xRRGGBB`).
pub fn colors(rgb: u32, cx: &App) -> (Hsla, Hsla) {
    let color = hex(rgb);
    let theme = cx.theme();
    // Light text needs more ink to read on the faint fill; dark needs less.
    let toward = match theme.appearance {
        Appearance::Light => 0.5,
        Appearance::Dark => 0.25,
    };
    (
        color.opacity(0.14),
        mix_oklab(color, 1.0 - toward, theme.colors.foreground),
    )
}
