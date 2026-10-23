//! Geist and Geist Mono, the desktop's `--font-sans` and `--font-mono`.
//! Static weights are embedded so the app looks the same on every machine.

use std::borrow::Cow;

use gpui::{App, SharedString};

pub const SANS: &str = "Geist";
pub const MONO: &str = "Geist Mono";

const FILES: [&[u8]; 8] = [
    include_bytes!("../../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Geist-Bold.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/GeistMono-Bold.ttf"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    pub sans: SharedString,
    pub mono: SharedString,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            sans: SANS.into(),
            mono: MONO.into(),
        }
    }
}

pub fn register(cx: &App) -> anyhow::Result<()> {
    cx.text_system()
        .add_fonts(FILES.iter().map(|bytes| Cow::Borrowed(*bytes)).collect())
}
