//! The terminal's colors: the desktop's `--terminal-*` tokens
//! (`dcc-shell-tokens.css`, the dark block only overriding what differs),
//! xterm's 256-color cube and grays past the 16 ANSI colors.

use gpui::{Hsla, Rgba};

use crate::theme::color::oklch;
use crate::theme::palette::Palette;

#[derive(Clone, Debug, PartialEq)]
pub struct TerminalPalette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub cursor: Hsla,
    pub selection: Hsla,
    /// black, red, green, yellow, blue, magenta, cyan, white, then the brights.
    pub ansi: [Hsla; 16],
}

impl TerminalPalette {
    pub fn new(dark: bool, colors: &Palette) -> Self {
        let ansi = if dark {
            [
                oklch(0.21, 0.0, 0.0),
                colors.destructive,
                oklch(0.72, 0.14, 155.0),
                oklch(0.82, 0.13, 85.0),
                oklch(0.72, 0.13, 250.0),
                oklch(0.72, 0.14, 310.0),
                oklch(0.78, 0.09, 192.0),
                oklch(0.87, 0.0, 0.0),
                oklch(0.5, 0.0, 0.0),
                oklch(0.78, 0.15, 18.0),
                oklch(0.78, 0.12, 150.0),
                oklch(0.88, 0.11, 85.0),
                oklch(0.78, 0.11, 250.0),
                oklch(0.78, 0.12, 310.0),
                oklch(0.84, 0.07, 192.0),
                oklch(0.985, 0.0, 0.0),
            ]
        } else {
            [
                oklch(0.26, 0.0, 0.0),
                colors.destructive,
                oklch(0.5, 0.15, 155.0),
                oklch(0.53, 0.14, 75.0),
                oklch(0.5, 0.16, 255.0),
                oklch(0.52, 0.17, 315.0),
                oklch(0.5, 0.1, 195.0),
                oklch(0.87, 0.0, 0.0),
                oklch(0.556, 0.0, 0.0),
                oklch(0.6, 0.2, 22.0),
                oklch(0.55, 0.14, 150.0),
                oklch(0.58, 0.12, 75.0),
                oklch(0.55, 0.14, 255.0),
                oklch(0.57, 0.15, 310.0),
                oklch(0.55, 0.09, 192.0),
                oklch(0.95, 0.0, 0.0),
            ]
        };
        Self {
            background: colors.sidebar,
            foreground: colors.foreground,
            cursor: colors.foreground,
            selection: if dark {
                oklch(0.35, 0.03, 250.0)
            } else {
                oklch(0.87, 0.03, 250.0)
            },
            ansi,
        }
    }

    /// Color `index` of the 256 (xterm's cube and gray ramp past 16).
    pub fn indexed(&self, index: u8) -> Hsla {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                let index = index - 16;
                let level = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
                rgb(level(index / 36), level((index / 6) % 6), level(index % 6))
            }
            _ => {
                let gray = 8 + (index - 232) * 10;
                rgb(gray, gray, gray)
            }
        }
    }
}

pub fn rgb(r: u8, g: u8, b: u8) -> Hsla {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

/// The 8-bit channels of a color, for a program asking which one it is.
pub fn channels(color: Hsla) -> (u8, u8, u8) {
    let rgba = color.to_rgb();
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    (byte(rgba.r), byte(rgba.g), byte(rgba.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_and_the_grays_follow_xterm() {
        let palette = TerminalPalette::new(true, &Palette::dark());
        assert_eq!(channels(palette.indexed(16)), (0, 0, 0));
        assert_eq!(channels(palette.indexed(196)), (255, 0, 0));
        assert_eq!(channels(palette.indexed(231)), (255, 255, 255));
        assert_eq!(channels(palette.indexed(232)), (8, 8, 8));
        assert_eq!(channels(palette.indexed(255)), (238, 238, 238));
        assert_eq!(palette.indexed(1), Palette::dark().destructive);
    }

    #[test]
    fn the_surface_is_the_sidebar() {
        let light = Palette::light();
        assert_eq!(
            TerminalPalette::new(false, &light).background,
            light.sidebar
        );
    }
}
