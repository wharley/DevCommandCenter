//! The app's look: colors, radii and fonts, light and dark.
//! Views read it with `cx.theme()`; nothing paints a color that is not here.

pub mod color;
pub mod density;
pub mod fonts;
pub mod metrics;
pub mod palette;
pub mod state;
pub mod tailwind;

use gpui::{App, Global, Window, WindowAppearance};
use serde::{Deserialize, Serialize};

pub use density::Density;
pub use fonts::Fonts;
pub use metrics::Radius;
pub use palette::Palette;
pub use state::StateTone;

/// What the person picked, like the desktop's `dcc-theme` preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    /// The appearance forced on native chrome; `None` hands it back to the OS.
    pub fn window_appearance(self) -> Option<WindowAppearance> {
        match self {
            Self::System => None,
            Self::Light => Some(WindowAppearance::Light),
            Self::Dark => Some(WindowAppearance::Dark),
        }
    }
}

/// What is painted on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    pub fn resolve(preference: ThemePreference, system: WindowAppearance) -> Self {
        match preference {
            ThemePreference::Light => Self::Light,
            ThemePreference::Dark => Self::Dark,
            ThemePreference::System => match system {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
                WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub preference: ThemePreference,
    pub appearance: Appearance,
    pub colors: Palette,
    pub radius: Radius,
    pub fonts: Fonts,
}

impl Global for Theme {}

impl Theme {
    pub fn new(preference: ThemePreference, system: WindowAppearance) -> Self {
        let appearance = Appearance::resolve(preference, system);
        Self {
            preference,
            appearance,
            colors: match appearance {
                Appearance::Light => Palette::light(),
                Appearance::Dark => Palette::dark(),
            },
            radius: Radius::default(),
            fonts: Fonts::default(),
        }
    }

    /// Installs the theme for `preference`, following the OS while it is `System`.
    /// An explicit choice also drives the native chrome (title bar), like the
    /// desktop's `setTheme`.
    pub fn init(preference: ThemePreference, cx: &mut App) {
        cx.set_window_appearance(preference.window_appearance());
        let system = cx.window_appearance();
        cx.set_global(Self::new(preference, system));
    }

    /// Re-resolves after the OS appearance changes; returns whether anything changed.
    pub fn sync_with_window(window: &Window, cx: &mut App) -> bool {
        let preference = cx.theme().preference;
        let next = Self::new(preference, window.appearance());
        if next.appearance == cx.theme().appearance {
            return false;
        }
        cx.set_global(next);
        true
    }
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_follows_the_os() {
        assert_eq!(
            Appearance::resolve(ThemePreference::System, WindowAppearance::VibrantDark),
            Appearance::Dark
        );
        assert_eq!(
            Appearance::resolve(ThemePreference::System, WindowAppearance::Light),
            Appearance::Light
        );
    }

    #[test]
    fn an_explicit_choice_ignores_the_os() {
        assert_eq!(
            Appearance::resolve(ThemePreference::Light, WindowAppearance::Dark),
            Appearance::Light
        );
        let theme = Theme::new(ThemePreference::Dark, WindowAppearance::Light);
        assert_eq!(theme.colors, Palette::dark());
    }

    #[test]
    fn only_an_explicit_choice_pins_the_title_bar() {
        assert_eq!(ThemePreference::System.window_appearance(), None);
        assert_eq!(
            ThemePreference::Dark.window_appearance(),
            Some(WindowAppearance::Dark)
        );
    }

    #[test]
    fn preference_reads_the_desktop_spelling() {
        let parsed: ThemePreference = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(parsed, ThemePreference::System);
    }
}
