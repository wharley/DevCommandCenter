//! Semantic colors, one to one with the desktop CSS tokens
//! (`apps/desktop/src/styles/dcc-shell-tokens.css`, `dcc-status-tokens.css`
//! and the accents in `dcc-design.css`). Field names follow the CSS variables.

use gpui::Hsla;

use super::color::{hex, oklch, oklch_alpha, rgb_triplet};

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub conversation_body_foreground: Hsla,
    pub card: Hsla,
    pub card_foreground: Hsla,
    pub popover: Hsla,
    pub popover_foreground: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub secondary: Hsla,
    pub secondary_foreground: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub destructive: Hsla,
    pub destructive_foreground: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub ring: Hsla,
    pub sidebar: Hsla,
    pub sidebar_foreground: Hsla,
    pub sidebar_primary: Hsla,
    pub sidebar_primary_foreground: Hsla,
    pub sidebar_accent: Hsla,
    pub sidebar_accent_foreground: Hsla,
    pub sidebar_border: Hsla,
    pub sidebar_ring: Hsla,
    pub info: Hsla,
    pub info_foreground: Hsla,
    pub success: Hsla,
    pub success_foreground: Hsla,
    pub warning: Hsla,
    pub warning_foreground: Hsla,
    /// `--dcc-accent-idea`
    pub accent_idea: Hsla,
    /// `--dcc-accent-calm`
    pub accent_calm: Hsla,
}

impl Palette {
    /// `:root`
    pub fn light() -> Self {
        Self {
            background: oklch(1.0, 0.0, 0.0),
            foreground: oklch(0.145, 0.0, 0.0),
            conversation_body_foreground: rgb_triplet(44, 40, 38),
            card: oklch(1.0, 0.0, 0.0),
            card_foreground: oklch(0.145, 0.0, 0.0),
            popover: oklch(1.0, 0.0, 0.0),
            popover_foreground: oklch(0.145, 0.0, 0.0),
            primary: oklch(0.205, 0.0, 0.0),
            primary_foreground: oklch(0.985, 0.0, 0.0),
            secondary: oklch(0.97, 0.0, 0.0),
            secondary_foreground: oklch(0.205, 0.0, 0.0),
            muted: oklch(0.97, 0.0, 0.0),
            muted_foreground: oklch(0.556, 0.0, 0.0),
            accent: oklch(0.97, 0.0, 0.0),
            accent_foreground: oklch(0.205, 0.0, 0.0),
            destructive: oklch(0.577, 0.245, 27.325),
            destructive_foreground: oklch(0.45, 0.19, 27.0),
            border: oklch(0.922, 0.0, 0.0),
            input: oklch(0.922, 0.0, 0.0),
            ring: oklch(0.708, 0.0, 0.0),
            sidebar: oklch(0.985, 0.0, 0.0),
            sidebar_foreground: oklch(0.145, 0.0, 0.0),
            sidebar_primary: oklch(0.205, 0.0, 0.0),
            sidebar_primary_foreground: oklch(0.985, 0.0, 0.0),
            sidebar_accent: oklch(0.97, 0.0, 0.0),
            sidebar_accent_foreground: oklch(0.205, 0.0, 0.0),
            sidebar_border: oklch(0.922, 0.0, 0.0),
            sidebar_ring: oklch(0.708, 0.0, 0.0),
            info: oklch(0.58, 0.2, 250.0),
            info_foreground: oklch(0.42, 0.15, 250.0),
            success: oklch(0.62, 0.17, 165.0),
            success_foreground: oklch(0.38, 0.12, 165.0),
            warning: oklch(0.78, 0.16, 85.0),
            warning_foreground: oklch(0.38, 0.1, 75.0),
            accent_idea: hex(0xac751c),
            accent_calm: hex(0x31816d),
        }
    }

    /// `.dark`
    pub fn dark() -> Self {
        Self {
            background: oklch(0.165, 0.0, 0.0),
            foreground: oklch(0.985, 0.0, 0.0),
            conversation_body_foreground: rgb_triplet(234, 232, 230),
            card: oklch(0.205, 0.0, 0.0),
            card_foreground: oklch(0.985, 0.0, 0.0),
            popover: oklch(0.205, 0.0, 0.0),
            popover_foreground: oklch(0.985, 0.0, 0.0),
            primary: oklch(0.922, 0.0, 0.0),
            primary_foreground: oklch(0.205, 0.0, 0.0),
            secondary: oklch(0.269, 0.0, 0.0),
            secondary_foreground: oklch(0.985, 0.0, 0.0),
            muted: oklch(0.269, 0.0, 0.0),
            muted_foreground: oklch(0.708, 0.0, 0.0),
            accent: oklch(0.269, 0.0, 0.0),
            accent_foreground: oklch(0.985, 0.0, 0.0),
            destructive: oklch(0.704, 0.191, 22.216),
            destructive_foreground: oklch(0.78, 0.12, 25.0),
            border: oklch_alpha(1.0, 0.0, 0.0, 0.1),
            input: oklch_alpha(1.0, 0.0, 0.0, 0.15),
            ring: oklch(0.556, 0.0, 0.0),
            sidebar: oklch(0.205, 0.0, 0.0),
            sidebar_foreground: oklch(0.985, 0.0, 0.0),
            sidebar_primary: oklch(0.488, 0.243, 264.376),
            sidebar_primary_foreground: oklch(0.985, 0.0, 0.0),
            sidebar_accent: oklch(0.269, 0.0, 0.0),
            sidebar_accent_foreground: oklch(0.985, 0.0, 0.0),
            sidebar_border: oklch_alpha(1.0, 0.0, 0.0, 0.1),
            sidebar_ring: oklch(0.556, 0.0, 0.0),
            info: oklch(0.58, 0.18, 250.0),
            info_foreground: oklch(0.82, 0.08, 250.0),
            success: oklch(0.62, 0.16, 165.0),
            success_foreground: oklch(0.84, 0.1, 165.0),
            warning: oklch(0.78, 0.14, 85.0),
            warning_foreground: oklch(0.86, 0.12, 85.0),
            accent_idea: hex(0xe8b65e),
            accent_calm: hex(0x83cbb7),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_differ_where_the_css_differs() {
        let light = Palette::light();
        let dark = Palette::dark();
        assert_ne!(light.background, dark.background);
        assert_ne!(light.sidebar, dark.sidebar);
        // `--destructive-foreground` and `--warning` really change between modes.
        assert_ne!(light.warning, dark.warning);
    }

    #[test]
    fn dark_borders_are_translucent_white() {
        let dark = Palette::dark();
        assert!((dark.border.a - 0.1).abs() < 1e-6);
        assert!((dark.input.a - 0.15).abs() < 1e-6);
        assert!((Palette::light().border.a - 1.0).abs() < 1e-6);
    }
}
