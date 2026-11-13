//! The colours of a state on the way to production, one name each, the same
//! wherever a pull request, a check or a delivery step is drawn: done
//! (green), waiting on someone else (blue), needs the person (amber), failed
//! (red), merged (violet), and quiet for what asks for nothing.

use gpui::Hsla;

use super::tailwind as tw;
use super::{Appearance, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateTone {
    Done,
    Waiting,
    NeedsYou,
    Failed,
    Merged,
    Quiet,
}

impl StateTone {
    /// Text and icons on the theme's surfaces.
    pub fn text(self, theme: &Theme) -> Hsla {
        let dark = theme.appearance == Appearance::Dark;
        let pick =
            |on_dark: tw::Oklch, on_light: tw::Oklch| if dark { on_dark } else { on_light }.hsla();
        match self {
            Self::Done => pick(tw::EMERALD_400, tw::EMERALD_600),
            Self::Waiting => pick(tw::SKY_400, tw::SKY_600),
            Self::NeedsYou => pick(tw::AMBER_400, tw::AMBER_600),
            Self::Failed => pick(tw::RED_400, tw::RED_600),
            Self::Merged => pick(tw::VIOLET_400, tw::VIOLET_600),
            Self::Quiet => theme.colors.muted_foreground,
        }
    }

    /// A solid that white text reads on (a filled button, a step's dot),
    /// the same in both themes.
    pub fn fill(self, theme: &Theme) -> Hsla {
        match self {
            Self::Done => tw::EMERALD_600.hsla(),
            Self::Waiting => tw::SKY_600.hsla(),
            Self::NeedsYou => tw::AMBER_600.hsla(),
            Self::Failed => tw::RED_600.hsla(),
            Self::Merged => tw::VIOLET_600.hsla(),
            Self::Quiet => theme.colors.muted_foreground,
        }
    }
}
