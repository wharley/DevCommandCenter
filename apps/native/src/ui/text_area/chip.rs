//! `ComposerInlineBadge`: a chip inside the text for an attached image, an
//! attached or mentioned file, or a long paste. Measured and painted by the
//! field; this file knows what each chip says and how it looks.

use gpui::{
    App, Bounds, FontWeight, Hsla, Pixels, ShapedLine, SharedString, TextRun, TransformationMatrix,
    Window, point, px, size,
};

use crate::assets::icon_path;
use crate::theme::color::oklch;
use crate::theme::{ActiveTheme, Appearance};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chip {
    /// A file attached or mentioned, by the path the prompt will carry
    /// (relative to the workspace when inside it).
    File(String),
    Image(String),
    /// A long paste, sent as its full text.
    Snippet(String),
}

/// Chip height inside a 20px line.
pub const HEIGHT: f32 = 20.0;
const LABEL_SIZE: f32 = 14.0;
const ICON: f32 = 14.0;
const CLOSE: f32 = 16.0;
/// `mx-0.5`, border, `pl-2`, `pr-1`, the close button's `mr-1`.
const MARGIN: f32 = 2.0;
const PAD_LEFT: f32 = 8.0;
const PAD_RIGHT: f32 = 4.0;
const CLOSE_MARGIN: f32 = 4.0;
const FILE_LABEL_MAX: f32 = 260.0;
const TEXT_LABEL_MAX: f32 = 200.0;
const CODE_EXTENSIONS: &[&str] = &[
    "astro", "bash", "c", "cc", "cpp", "cs", "css", "cxx", "go", "graphql", "gql", "h", "hpp",
    "html", "java", "js", "jsx", "json", "kt", "kts", "less", "lua", "m", "mdx", "mts", "php",
    "py", "r", "rb", "rs", "sass", "scala", "scss", "sh", "sql", "svelte", "swift", "toml", "ts",
    "tsx", "vue", "xml", "yaml", "yml", "zsh",
];

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// `snippetUiLabel`, the count grouped like `toLocaleString`.
pub fn snippet_label(body: &str, english: bool) -> String {
    let first = body.trim_start().lines().next().unwrap_or("");
    let preview: String = if first.chars().count() > 44 {
        format!("{}…", first.chars().take(44).collect::<String>())
    } else {
        first.to_string()
    };
    let count = body.encode_utf16().count();
    if preview.is_empty() {
        format!("Pasted snippet · {} chars", group(count, english))
    } else {
        format!("{preview} · {} chars", group(count, english))
    }
}

fn group(value: usize, english: bool) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(if english { ',' } else { '.' });
        }
        out.push(digit);
    }
    out
}

/// The parts drawn after the icon: (text, weight, opacity) in order.
fn label_parts(chip: &Chip, english: bool) -> Vec<(String, FontWeight, f32)> {
    match chip {
        Chip::File(path) => {
            let normalized = path.replace('\\', "/");
            let mut segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
            let name = segments.pop().unwrap_or(path.as_str()).to_string();
            match segments.pop() {
                Some(folder) => vec![
                    (format!("{folder}/"), FontWeight::NORMAL, 0.55),
                    (name, FontWeight::MEDIUM, 1.0),
                ],
                None => vec![(name, FontWeight::MEDIUM, 1.0)],
            }
        }
        Chip::Image(path) => {
            let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
            vec![(name, FontWeight::NORMAL, 1.0)]
        }
        Chip::Snippet(body) => vec![(snippet_label(body, english), FontWeight::NORMAL, 1.0)],
    }
}

/// Icon, its color, and the gap after it.
fn icon(chip: &Chip, cx: &App) -> (&'static str, Hsla, f32) {
    let colors = &cx.theme().colors;
    let dark = cx.theme().appearance == Appearance::Dark;
    match chip {
        Chip::File(path) => {
            let ext = extension(path);
            let name =
                if ["mp3", "wav", "m4a", "ogg", "webm", "flac", "aac"].contains(&ext.as_str()) {
                    "file-headphone"
                } else if CODE_EXTENSIONS.contains(&ext.as_str()) {
                    "file-code-corner"
                } else {
                    "file-text"
                };
            (name, colors.muted_foreground.opacity(0.7), 4.0)
        }
        // `text-chart-3` / `text-chart-2`
        Chip::Image(_) => (
            "image",
            if dark {
                oklch(0.769, 0.188, 70.08)
            } else {
                oklch(0.398, 0.07, 227.392)
            },
            6.0,
        ),
        Chip::Snippet(_) => (
            "braces",
            if dark {
                oklch(0.696, 0.17, 162.48)
            } else {
                oklch(0.6, 0.118, 184.704)
            },
            6.0,
        ),
    }
}

/// The label shaped and cut to its maximum width with an ellipsis.
fn shape_label(chip: &Chip, window: &Window, cx: &App) -> ShapedLine {
    let colors = &cx.theme().colors;
    let base = window.text_style().font();
    let (color, max) = match chip {
        Chip::File(_) => (colors.foreground, FILE_LABEL_MAX - ICON - 4.0),
        _ => (colors.muted_foreground, TEXT_LABEL_MAX),
    };
    let english = cx.global::<crate::i18n::I18n>().locale() == crate::i18n::Locale::En;
    let parts = label_parts(chip, english);
    let shape = |parts: &[(String, FontWeight, f32)]| {
        let text: String = parts.iter().map(|(text, ..)| text.as_str()).collect();
        let runs: Vec<TextRun> = parts
            .iter()
            .map(|(text, weight, opacity)| TextRun {
                len: text.len(),
                font: gpui::Font {
                    weight: *weight,
                    ..base.clone()
                },
                color: color.opacity(*opacity),
                background_color: None,
                underline: None,
                strikethrough: None,
            })
            .collect();
        window
            .text_system()
            .shape_line(SharedString::from(text), px(LABEL_SIZE), &runs, None)
    };
    let full = shape(&parts);
    if full.width <= px(max) {
        return full;
    }
    // Cut the last part until it fits, like `truncate`.
    let mut parts = parts;
    loop {
        let Some(last) = parts.last_mut() else {
            return full;
        };
        let mut chars: Vec<char> = last.0.trim_end_matches('…').chars().collect();
        if chars.is_empty() {
            parts.pop();
            continue;
        }
        chars.pop();
        last.0 = chars.into_iter().collect::<String>() + "…";
        let only_ellipsis = last.0 == "…";
        let line = shape(&parts);
        if line.width <= px(max) || only_ellipsis {
            return line;
        }
    }
}

/// A chip ready to paint: its label and full width.
pub struct ChipLayout {
    pub label: ShapedLine,
    pub width: Pixels,
}

pub fn layout(chip: &Chip, window: &Window, cx: &App) -> ChipLayout {
    let label = shape_label(chip, window, cx);
    let (_, _, gap) = icon(chip, cx);
    let width = MARGIN * 2.0
        + 2.0
        + PAD_LEFT
        + ICON
        + gap
        + f32::from(label.width)
        + PAD_RIGHT
        + CLOSE
        + CLOSE_MARGIN;
    ChipLayout {
        label,
        width: px(width),
    }
}

/// Paints the chip at `origin` (the line's top-left at the chip) and returns
/// where its close button is.
pub fn paint(
    chip: &Chip,
    laid: &ChipLayout,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> Bounds<Pixels> {
    let colors = cx.theme().colors.clone();
    let (icon_name, icon_color, gap) = icon(chip, cx);
    let top = origin.y + (line_height - px(HEIGHT)) / 2.0;
    let left = origin.x + px(MARGIN);
    let width = laid.width - px(MARGIN * 2.0);
    let bounds = Bounds::new(point(left, top), size(width, px(HEIGHT)));
    window.paint_quad(gpui::PaintQuad {
        bounds,
        corner_radii: gpui::Corners::all(cx.theme().radius.sm),
        background: gpui::transparent_black().into(),
        border_widths: gpui::Edges::all(px(1.0)),
        border_color: colors.border.opacity(0.6),
        border_style: gpui::BorderStyle::Solid,
    });
    let center = top + px(HEIGHT) / 2.0;
    let icon_left = left + px(1.0 + PAD_LEFT);
    let _ = window.paint_svg(
        Bounds::new(
            point(icon_left, center - px(ICON / 2.0)),
            size(px(ICON), px(ICON)),
        ),
        icon_path(icon_name, 1.8),
        None,
        TransformationMatrix::unit(),
        icon_color,
        cx,
    );
    let label_left = icon_left + px(ICON + gap);
    let label_top = center - line_height / 2.0;
    let _ = laid.label.paint(
        point(label_left, label_top),
        line_height,
        gpui::TextAlign::Left,
        None,
        window,
        cx,
    );
    let close = Bounds::new(
        point(
            left + width - px(1.0 + CLOSE_MARGIN + CLOSE),
            center - px(CLOSE / 2.0),
        ),
        size(px(CLOSE), px(CLOSE)),
    );
    let x = 12.0;
    let _ = window.paint_svg(
        Bounds::new(
            point(
                close.left() + px((CLOSE - x) / 2.0),
                close.top() + px((CLOSE - x) / 2.0),
            ),
            size(px(x), px(x)),
        ),
        icon_path("x", 1.8),
        None,
        TransformationMatrix::unit(),
        colors.muted_foreground.opacity(0.4),
        cx,
    );
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_read_like_the_desktop() {
        assert_eq!(
            snippet_label("  linha um\nresto", false),
            format!("linha um · {} chars", 16)
        );
        assert_eq!(
            snippet_label(&"x".repeat(1500), false),
            format!("{}… · 1.500 chars", "x".repeat(44))
        );
        assert_eq!(
            snippet_label(&"x".repeat(1500), true),
            format!("{}… · 1,500 chars", "x".repeat(44))
        );
        let parts = label_parts(&Chip::File("src/screens/app.rs".into()), false);
        assert_eq!(parts[0].0, "screens/");
        assert_eq!(parts[1].0, "app.rs");
        assert_eq!(
            label_parts(&Chip::Image("/tmp/dcc/shot.png".into()), false)[0].0,
            "shot.png"
        );
    }
}
