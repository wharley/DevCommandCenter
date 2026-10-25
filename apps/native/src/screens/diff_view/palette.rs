//! The diff's colors, as `@pierre/diffs`' stylesheet mixes them from the
//! theme (`--diffs-bg-addition`, `--diffs-fg-number`, …), in Lab.

use gpui::{Hsla, Rgba};

use crate::data::highlight::editor_colors;
use crate::theme::color::mix_lab;

fn color(rgba: u32) -> Hsla {
    Rgba {
        r: ((rgba >> 24) & 0xff) as f32 / 255.0,
        g: ((rgba >> 16) & 0xff) as f32 / 255.0,
        b: ((rgba >> 8) & 0xff) as f32 / 255.0,
        a: (rgba & 0xff) as f32 / 255.0,
    }
    .into()
}

#[derive(Clone, Copy, Debug)]
pub struct DiffPalette {
    pub dark: bool,
    pub background: Hsla,
    pub foreground: Hsla,
    /// `--diffs-fg-number`: the foreground 65% into the background.
    pub number: Hsla,
    /// `--diffs-bg-separator`
    pub separator: Hsla,
    /// `--diffs-bg-buffer`: the stripes of a side left empty.
    pub buffer: Hsla,
    /// `--diffs-bg-context-gutter`: the number column of an empty side.
    pub context_gutter: Hsla,
    pub added: Hsla,
    pub deleted: Hsla,
    pub added_line: Hsla,
    pub deleted_line: Hsla,
    pub added_gutter: Hsla,
    pub deleted_gutter: Hsla,
    /// `--diffs-bg-addition-emphasis`: the changed words.
    pub added_word: Hsla,
    pub deleted_word: Hsla,
    /// `lineHoverHighlight: "both"`: the line under the pointer, 3% (dark
    /// 9%) toward the mixer, or toward its own color on a changed line.
    pub hover: Hsla,
    pub added_line_hover: Hsla,
    pub deleted_line_hover: Hsla,
    pub added_gutter_hover: Hsla,
    pub deleted_gutter_hover: Hsla,
    /// `--diffs-selection-base`: the theme's modified color.
    pub selection_base: Hsla,
}

impl DiffPalette {
    pub fn new(dark: bool) -> Self {
        let colors = editor_colors(dark);
        let (background, foreground) = (color(colors.background), color(colors.foreground));
        let (added, deleted) = (color(colors.added), color(colors.deleted));
        // `--diffs-mixer: light-dark(#000, #fff)`
        let mixer = if dark { gpui::white() } else { gpui::black() };
        let (line, gutter, separator, word) = if dark {
            (0.80, 0.85, 0.85, 0.2)
        } else {
            (0.88, 0.91, 0.96, 0.15)
        };
        let hover = if dark { 0.91 } else { 0.97 };
        // `--diffs-bg-context`, then its gutter toward the background.
        let context = mix_lab(background, if dark { 0.925 } else { 0.985 }, mixer);
        let context_gutter = mix_lab(context, if dark { 0.45 } else { 0.90 }, background);
        let added_line = mix_lab(background, line, added);
        let deleted_line = mix_lab(background, line, deleted);
        let added_gutter = mix_lab(background, gutter, added);
        let deleted_gutter = mix_lab(background, gutter, deleted);
        Self {
            dark,
            selection_base: color(colors.modified),
            hover: mix_lab(background, hover, mixer),
            added_line_hover: mix_lab(added_line, hover, added),
            deleted_line_hover: mix_lab(deleted_line, hover, deleted),
            added_gutter_hover: mix_lab(added_gutter, hover, added),
            deleted_gutter_hover: mix_lab(deleted_gutter, hover, deleted),
            background,
            foreground,
            number: mix_lab(foreground, 0.65, background),
            separator: mix_lab(background, separator, mixer),
            buffer: mix_lab(background, 0.92, mixer),
            context_gutter,
            added,
            deleted,
            added_line,
            deleted_line,
            added_gutter,
            deleted_gutter,
            added_word: added.opacity(word),
            deleted_word: deleted.opacity(word),
        }
    }
}
