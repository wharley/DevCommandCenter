//! A project's face: the mascot, symbol or logo and the color, picked by the
//! rules in `@dcc/mascots` so a project looks the same on every DCC surface.

mod glyph;
mod provider;

use gpui::Hsla;

pub(crate) use glyph::sprite;
pub use glyph::{GlyphSize, ProjectGlyph};
pub use provider::{ProviderIcon, model_label, provider_label};

use crate::theme::Appearance;
use crate::theme::tailwind::{self as tw, Oklch};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/mascots.rs"));
}

pub const SYMBOL_IDS: [&str; 12] = [
    "folder", "terminal", "code", "layers", "package", "database", "globe", "rocket", "branch",
    "cpu", "shield", "wrench",
];

pub const COLOR_IDS: [&str; 12] = [
    "slate", "sky", "cyan", "emerald", "amber", "orange", "rose", "violet", "indigo", "fuchsia",
    "lime", "pink",
];

const LOGO_PREFIX: &str = "data:image/png;base64,";
const FALLBACK_MASCOT: &str = "polvo";

/// One frame of a 10×10 sprite: `#` body, `+` accent, `.` transparent.
pub type MascotRows = [&'static str; 10];

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectIcon {
    Mascot {
        rest: &'static MascotRows,
        moving: &'static MascotRows,
    },
    /// A Lucide icon name.
    Symbol(&'static str),
    /// A PNG data URI.
    Logo(String),
}

/// `projectSeedHash`: 32-bit FNV-1a over UTF-16 code units, like `charCodeAt`.
pub fn seed_hash(seed: &str) -> u32 {
    seed.encode_utf16().fold(0x811c_9dc5_u32, |hash, unit| {
        (hash ^ u32::from(unit)).wrapping_mul(0x0100_0193)
    })
}

fn mascot(name: &str) -> Option<ProjectIcon> {
    generated::MASCOT_FRAMES
        .iter()
        .find(|(id, _, _)| *id == name)
        .map(|(_, rest, moving)| ProjectIcon::Mascot { rest, moving })
}

/// The fauna a project can pick, in `@dcc/mascots` order.
pub fn mascot_ids() -> impl Iterator<Item = &'static str> {
    generated::MASCOT_FRAMES.iter().map(|(id, _, _)| *id)
}

/// `isKnownProjectIcon`
pub fn is_known_icon(value: &str) -> bool {
    value == "logo" || SYMBOL_IDS.contains(&value) || mascot_ids().any(|id| id == value)
}

/// `projectIconValue`: the uploaded logo when the pick is `logo`.
pub fn icon_value<'a>(icon: Option<&'a str>, logo: Option<&'a str>) -> Option<&'a str> {
    match (icon, logo) {
        (Some("logo"), Some(logo)) if !logo.is_empty() => Some(logo),
        _ => icon,
    }
}

/// `resolveProjectIcon`: an explicit mascot or symbol wins; anything else
/// falls back to the mascot hashed from the seed (the project path).
pub fn resolve_icon(value: Option<&str>, seed: Option<&str>) -> ProjectIcon {
    if let Some(value) = value {
        if let Some(icon) = mascot(value) {
            return icon;
        }
        if let Some(lucide) = symbol_lucide_name(value) {
            return ProjectIcon::Symbol(lucide);
        }
        if value.starts_with(LOGO_PREFIX) {
            return ProjectIcon::Logo(value.to_string());
        }
    }
    let name = match seed {
        Some(seed) => {
            let frames = generated::MASCOT_FRAMES;
            frames[seed_hash(seed) as usize % frames.len()].0
        }
        None => FALLBACK_MASCOT,
    };
    mascot(name).expect("fallback mascot exists")
}

/// The Lucide icon each project symbol renders (`PROJECT_ICON_OPTIONS`).
fn symbol_lucide_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "folder" => "folder",
        "terminal" => "square-terminal",
        "code" => "code-xml",
        "layers" => "layers",
        "package" => "package",
        "database" => "database",
        "globe" => "earth",
        "rocket" => "rocket",
        "branch" => "git-branch",
        "cpu" => "cpu",
        "shield" => "shield-check",
        "wrench" => "wrench",
        _ => return None,
    })
}

/// `resolveProjectColor`: slate reads as unset and amber means "needs you",
/// so the automatic pick skips both.
pub fn resolve_color(value: Option<&str>, seed: Option<&str>) -> &'static str {
    if let Some(id) = value.and_then(|value| COLOR_IDS.iter().find(|id| **id == value)) {
        return id;
    }
    let Some(seed) = seed else { return "slate" };
    let auto: Vec<&'static str> = COLOR_IDS
        .iter()
        .copied()
        .filter(|id| *id != "slate" && *id != "amber")
        .collect();
    auto[seed_hash(&format!("color:{seed}")) as usize % auto.len()]
}

/// `PROJECT_COLOR_CLASSES`: tile border, tile background and glyph color.
pub struct ProjectColors {
    pub border: Hsla,
    pub background: Hsla,
    pub foreground: Hsla,
}

pub fn colors(color_id: &str, appearance: Appearance) -> ProjectColors {
    // (tile tint `-500`, light text `-600`, dark text `-300`)
    let (base, light, dark): (Oklch, Oklch, Oklch) = match color_id {
        "sky" => (tw::SKY_500, tw::SKY_600, tw::SKY_300),
        "cyan" => (tw::CYAN_500, tw::CYAN_600, tw::CYAN_300),
        "emerald" => (tw::EMERALD_500, tw::EMERALD_600, tw::EMERALD_300),
        // `text-amber-700`: amber-600 is too light on the pale tile.
        "amber" => (tw::AMBER_500, tw::AMBER_700, tw::AMBER_300),
        "orange" => (tw::ORANGE_500, tw::ORANGE_600, tw::ORANGE_300),
        "rose" => (tw::ROSE_500, tw::ROSE_600, tw::ROSE_300),
        "violet" => (tw::VIOLET_500, tw::VIOLET_600, tw::VIOLET_300),
        "indigo" => (tw::INDIGO_500, tw::INDIGO_600, tw::INDIGO_300),
        "fuchsia" => (tw::FUCHSIA_500, tw::FUCHSIA_600, tw::FUCHSIA_300),
        "lime" => (tw::LIME_500, tw::LIME_600, tw::LIME_300),
        "pink" => (tw::PINK_500, tw::PINK_600, tw::PINK_300),
        _ => (tw::SLATE_500, tw::SLATE_600, tw::SLATE_300),
    };
    // Slate is quieter: `/20` border and `/10` fill instead of `/25` and `/12`.
    let slate = !COLOR_IDS[1..].contains(&color_id);
    ProjectColors {
        border: base.alpha(if slate { 0.20 } else { 0.25 }),
        background: base.alpha(if slate { 0.10 } else { 0.12 }),
        foreground: match appearance {
            Appearance::Light => light.hsla(),
            Appearance::Dark => dark.hsla(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_the_typescript_fnv() {
        // Values computed with `projectSeedHash` in packages/mascots.
        assert_eq!(seed_hash(""), 0x811c_9dc5);
        assert_eq!(seed_hash("a"), 0xe40c_292c);
        assert_eq!(seed_hash("foobar"), 0xbf9c_f968);
    }

    #[test]
    fn explicit_picks_win_and_the_rest_hash_from_the_path() {
        assert!(matches!(
            resolve_icon(Some("tucano"), None),
            ProjectIcon::Mascot { .. }
        ));
        assert_eq!(
            resolve_icon(Some("globe"), None),
            ProjectIcon::Symbol("earth")
        );
        let seed = "/Users/me/projetos/dcc";
        assert_eq!(
            resolve_icon(None, Some(seed)),
            resolve_icon(Some("bogus"), Some(seed))
        );
        assert_eq!(resolve_color(Some("lime"), Some(seed)), "lime");
        let auto = resolve_color(None, Some(seed));
        assert!(auto != "slate" && auto != "amber");
        assert_eq!(resolve_color(None, None), "slate");
    }

    #[test]
    fn logo_needs_the_logo_pick() {
        assert_eq!(icon_value(Some("logo"), Some("data:x")), Some("data:x"));
        assert_eq!(
            icon_value(Some("capivara"), Some("data:x")),
            Some("capivara")
        );
        assert_eq!(icon_value(Some("logo"), None), Some("logo"));
    }

    #[test]
    fn every_mascot_is_parsed_from_the_shared_package() {
        assert_eq!(generated::MASCOT_FRAMES.len(), 10);
        assert_eq!(generated::MASCOT_FRAMES[0].0, "capivara");
        assert_eq!(generated::MASCOT_FRAMES[9].0, "polvo");
    }
}
