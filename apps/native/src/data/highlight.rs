//! Syntax colors for code the app shows (the diffs first): TextMate grammars
//! like the desktop's Shiki (`two-face`, bat's set) with the Pierre themes the
//! desktop's `@pierre/diffs` uses, read from its JSON (`gen-diff-themes.mjs`).

use std::ops::Range;
use std::str::FromStr;
use std::sync::OnceLock;

use serde::Deserialize;
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// `tokenizeMaxLineLength`: longer lines stay plain.
pub const MAX_TOKENIZED_LINE: usize = 2_000;

/// One colored run of a line, in bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    /// `0xRRGGBBAA`
    pub color: u32,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Deserialize)]
struct VsTheme {
    colors: std::collections::HashMap<String, String>,
    #[serde(rename = "tokenColors")]
    token_colors: Vec<TokenColor>,
}

#[derive(Deserialize)]
struct TokenColor {
    #[serde(default)]
    scope: Option<Scopes>,
    settings: TokenSettings,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Scopes {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize)]
struct TokenSettings {
    #[serde(default)]
    foreground: Option<String>,
    #[serde(default, rename = "fontStyle")]
    font_style: Option<String>,
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(value: &str) -> Option<Color> {
    let hex = value.trim().strip_prefix('#')?;
    let expanded: String = match hex.len() {
        3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => hex.to_string(),
        _ => return None,
    };
    let byte = |index: usize| u8::from_str_radix(expanded.get(index..index + 2)?, 16).ok();
    Some(Color {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
        a: if expanded.len() == 8 { byte(6)? } else { 255 },
    })
}

fn font_style(style: &str) -> FontStyle {
    let mut font = FontStyle::empty();
    for word in style.split_whitespace() {
        match word {
            "bold" => font |= FontStyle::BOLD,
            "italic" => font |= FontStyle::ITALIC,
            "underline" => font |= FontStyle::UNDERLINE,
            _ => {}
        }
    }
    font
}

/// A VS Code theme (the Pierre JSON) as a syntect theme.
fn theme_from_json(source: &str) -> Theme {
    let theme: VsTheme = serde_json::from_str(source).expect("diff theme");
    let color = |key: &str| theme.colors.get(key).and_then(|value| parse_hex(value));
    let scopes = theme
        .token_colors
        .iter()
        .filter_map(|token| {
            let selector = match token.scope.as_ref()? {
                Scopes::One(scope) => scope.clone(),
                Scopes::Many(scopes) => scopes.join(", "),
            };
            Some(ThemeItem {
                scope: ScopeSelectors::from_str(&selector).ok()?,
                style: StyleModifier {
                    foreground: token.settings.foreground.as_deref().and_then(parse_hex),
                    background: None,
                    font_style: token.settings.font_style.as_deref().map(font_style),
                },
            })
        })
        .collect();
    Theme {
        name: None,
        author: None,
        settings: ThemeSettings {
            foreground: color("editor.foreground"),
            background: color("editor.background"),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

/// The theme's own surface colors the diff paints with (`getGitVariables`:
/// the git decoration colors, else the terminal's green/red/blue).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorColors {
    pub background: u32,
    pub foreground: u32,
    pub added: u32,
    pub deleted: u32,
    pub modified: u32,
}

fn editor_colors_from_json(source: &str) -> EditorColors {
    let theme: VsTheme = serde_json::from_str(source).expect("diff theme");
    let color = |keys: &[&str], fallback: u32| {
        keys.iter()
            .find_map(|key| theme.colors.get(*key).and_then(|value| parse_hex(value)))
            .map(rgba)
            .unwrap_or(fallback)
    };
    EditorColors {
        background: color(&["editor.background"], 0xffffffff),
        foreground: color(&["editor.foreground"], 0x000000ff),
        added: color(
            &[
                "gitDecoration.addedResourceForeground",
                "terminal.ansiGreen",
            ],
            0x0dbe4eff,
        ),
        deleted: color(
            &[
                "gitDecoration.deletedResourceForeground",
                "terminal.ansiRed",
            ],
            0xff2e3fff,
        ),
        modified: color(
            &[
                "gitDecoration.modifiedResourceForeground",
                "terminal.ansiBlue",
            ],
            0x009fffff,
        ),
    }
}

pub fn editor_colors(dark: bool) -> EditorColors {
    static LIGHT: OnceLock<EditorColors> = OnceLock::new();
    static DARK: OnceLock<EditorColors> = OnceLock::new();
    if dark {
        *DARK.get_or_init(|| {
            editor_colors_from_json(include_str!("../../assets/diff-themes/pierre-dark.json"))
        })
    } else {
        *LIGHT.get_or_init(|| {
            editor_colors_from_json(include_str!("../../assets/diff-themes/pierre-light.json"))
        })
    }
}

pub fn theme(dark: bool) -> &'static Theme {
    static LIGHT: OnceLock<Theme> = OnceLock::new();
    static DARK: OnceLock<Theme> = OnceLock::new();
    if dark {
        DARK.get_or_init(|| {
            theme_from_json(include_str!("../../assets/diff-themes/pierre-dark.json"))
        })
    } else {
        LIGHT.get_or_init(|| {
            theme_from_json(include_str!("../../assets/diff-themes/pierre-light.json"))
        })
    }
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// The grammar for a file: by extension, then by its whole name
/// (`Dockerfile`, `Makefile`), then by its first line (`#!/bin/bash`).
pub fn syntax_for(path: &str, first_line: &str) -> &'static SyntaxReference {
    let set = syntaxes();
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    extension
        .and_then(|extension| set.find_syntax_by_extension(extension))
        .or_else(|| set.find_syntax_by_extension(name))
        .or_else(|| set.find_syntax_by_first_line(first_line))
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

fn rgba(color: Color) -> u32 {
    u32::from_be_bytes([color.r, color.g, color.b, color.a])
}

/// The first `until` lines of `text` colored, in order (a grammar's state
/// carries from line to line, so it reads from the top).
pub fn highlight(text: &str, path: &str, dark: bool, until: usize) -> Vec<Vec<Span>> {
    let first = text.lines().next().unwrap_or_default();
    let syntax = syntax_for(path, first);
    let theme = theme(dark);
    let mut lines = HighlightLines::new(syntax, theme);
    let set = syntaxes();
    let mut out = Vec::new();
    for line in syntect::util::LinesWithEndings::from(text).take(until) {
        let content_len = line.trim_end_matches(['\n', '\r']).len();
        if content_len > MAX_TOKENIZED_LINE {
            out.push(Vec::new());
            // Keep the grammar's state moving on a plain copy of the line.
            let _ = lines.highlight_line("\n", set);
            continue;
        }
        let Ok(regions) = lines.highlight_line(line, set) else {
            out.push(Vec::new());
            continue;
        };
        let mut start = 0;
        let mut spans = Vec::new();
        for (style, piece) in regions {
            let end = (start + piece.len()).min(content_len);
            if start < end {
                spans.push(Span {
                    range: start..end,
                    color: rgba(style.foreground),
                    bold: style.font_style.contains(FontStyle::BOLD),
                    italic: style.font_style.contains(FontStyle::ITALIC),
                });
            }
            start += piece.len();
        }
        out.push(spans);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_languages_have_a_grammar() {
        for (path, expected) in [
            ("src/main.rs", "Rust"),
            ("App.tsx", "TypeScriptReact"),
            ("index.ts", "TypeScript"),
            ("package.json", "JSON"),
            ("README.md", "Markdown"),
            ("Cargo.toml", "TOML"),
            ("styles.css", "CSS"),
            ("script.py", "Python"),
        ] {
            assert_eq!(syntax_for(path, "").name, expected, "{path}");
        }
        assert_eq!(
            syntax_for("run", "#!/bin/bash").name,
            "Bourne Again Shell (bash)"
        );
        assert_eq!(syntax_for("notes.unknownext", "").name, "Plain Text");
    }

    #[test]
    fn rust_keywords_take_the_theme_color() {
        let lines = highlight(
            "fn main() {\n    let x = 1;\n}\n",
            "a.rs",
            false,
            usize::MAX,
        );
        assert_eq!(lines.len(), 3);
        let first = &lines[0];
        assert_eq!(first[0].range, 0..2, "`fn`");
        let plain = theme(false).settings.foreground.map(rgba);
        assert_ne!(Some(first[0].color), plain, "a keyword is colored");
        let long = "x".repeat(MAX_TOKENIZED_LINE + 1);
        assert!(highlight(&long, "a.rs", true, usize::MAX)[0].is_empty());
        assert_eq!(highlight("a\nb\nc\n", "a.rs", true, 2).len(), 2);
    }

    #[test]
    fn theme_colors_parse() {
        assert_eq!(
            parse_hex("#0a0a0a"),
            Some(Color {
                r: 10,
                g: 10,
                b: 10,
                a: 255
            })
        );
        assert_eq!(
            parse_hex("#fff8"),
            Some(Color {
                r: 255,
                g: 255,
                b: 255,
                a: 0x88
            })
        );
        assert!(theme(true).scopes.len() > 200);
    }
}
