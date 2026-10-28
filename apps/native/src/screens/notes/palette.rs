//! A note's colors (`[data-color]` in `notes.css`): its accent, fill and
//! line, light and dark.

use gpui::Hsla;

use crate::theme::color::hex;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteColors {
    pub accent: Hsla,
    pub fill: Hsla,
    pub line: Hsla,
}

pub fn note_colors(color: &str, dark: bool) -> NoteColors {
    let (accent, fill, line) = match (color, dark) {
        ("mint", false) => (0x27876c, 0xeffbf5, 0xc2e4d5),
        ("violet", false) => (0x8461bd, 0xf7f1ff, 0xddccf1),
        ("sky", false) => (0x3b83b0, 0xeff8ff, 0xc8e1f2),
        (_, false) => (0xb77a21, 0xfff8e8, 0xecd8ad),
        ("mint", true) => (0x81cab0, 0x1e2925, 0x324b40),
        ("violet", true) => (0xbea0ec, 0x282330, 0x49395c),
        ("sky", true) => (0x8bc2e8, 0x202831, 0x344a5c),
        (_, true) => (0xe1b66f, 0x29261f, 0x4b4030),
    };
    NoteColors {
        accent: hex(accent),
        fill: hex(fill),
        line: hex(line),
    }
}
