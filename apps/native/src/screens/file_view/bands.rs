//! Tints behind runs of lines, and an element pinned to the right of some:
//! how a view that owns the file marks parts of it without leaving the
//! editor (the merge resolver's conflict blocks, each with its choices on
//! its first line). The text under a band is edited like any other; the
//! owner sets the bands again when the text changes.
//!
//! Lines that are not in the text can show above one that is ("ghost"
//! lines: what a proposal took out, read-only, in the place it was). They
//! make that row taller, so where a click lands and where the caret is
//! drawn count them (`rows_to`, `line_at_row`).

use std::ops::Range;
use std::rc::Rc;

use gpui::{AnyElement, App, Context, Hsla, SharedString};

use super::FileView;

/// A tint behind the lines `lines` (zero-based, end excluded).
#[derive(Clone, Debug, PartialEq)]
pub struct LineBand {
    pub lines: Range<usize>,
    pub color: Hsla,
}

/// A line shown as a label instead of its text (a conflict marker read as
/// "Seu código", say). The text is still there: the line shows it again
/// while the caret is on it, so it is edited as what it is.
#[derive(Clone, Debug, PartialEq)]
pub struct LineLabel {
    pub line: usize,
    pub text: SharedString,
    pub color: Hsla,
}

/// Lines drawn above `before` that are not part of the text.
#[derive(Clone, Debug, PartialEq)]
pub struct GhostLines {
    pub before: usize,
    pub lines: Vec<SharedString>,
    pub color: Hsla,
}

/// How many rows are drawn above `line`'s own text, in its item.
pub(crate) fn ghost_rows(ghosts: &[GhostLines], line: usize) -> usize {
    ghosts
        .iter()
        .filter(|ghost| ghost.before == line)
        .map(|ghost| ghost.lines.len())
        .sum()
}

/// Rows from the top of item `from` down to `line`'s own text.
pub(crate) fn rows_to(ghosts: &[GhostLines], from: usize, line: usize) -> f32 {
    if ghosts.is_empty() || line < from {
        return line as f32 - from as f32;
    }
    let above: usize = (from..line).map(|at| 1 + ghost_rows(ghosts, at)).sum();
    (above + ghost_rows(ghosts, line)) as f32
}

/// The line whose item holds the row `rows` below the top of item `from`
/// (a ghost row belongs to the line under it).
pub(crate) fn line_at_row(ghosts: &[GhostLines], from: usize, rows: f32) -> usize {
    if ghosts.is_empty() || rows < 0.0 {
        return (from as f32 + rows.floor()).max(0.0) as usize;
    }
    let mut top = 0.0;
    let mut line = from;
    loop {
        let height = (1 + ghost_rows(ghosts, line)) as f32;
        if rows < top + height {
            return line;
        }
        top += height;
        line += 1;
    }
}

/// What sits at the right of a line (zero-based), if anything.
pub type LineActions = Rc<dyn Fn(usize, &mut App) -> Option<AnyElement>>;

/// The band a line is under: the first that holds it.
pub(crate) fn band_at(bands: &[LineBand], line: usize) -> Option<Hsla> {
    bands
        .iter()
        .find(|band| band.lines.contains(&line))
        .map(|band| band.color)
}

impl FileView {
    pub fn set_bands(&mut self, bands: Vec<LineBand>, cx: &mut Context<Self>) {
        if self.bands != bands {
            self.bands = bands;
            cx.notify();
        }
    }

    pub fn set_line_labels(&mut self, labels: Vec<LineLabel>, cx: &mut Context<Self>) {
        if self.line_labels != labels {
            self.line_labels = labels;
            cx.notify();
        }
    }

    /// The rows change height: the list measures them again.
    pub fn set_ghosts(&mut self, ghosts: Vec<GhostLines>, cx: &mut Context<Self>) {
        if self.ghosts != ghosts {
            self.ghosts = ghosts;
            self.list.remeasure();
            cx.notify();
        }
    }

    pub fn set_line_actions(&mut self, actions: Option<LineActions>, cx: &mut Context<Self>) {
        self.line_actions = actions;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_takes_the_first_band_that_holds_it() {
        let (blue, amber) = (gpui::blue(), gpui::yellow());
        let bands = [
            LineBand {
                lines: 2..5,
                color: blue,
            },
            LineBand {
                lines: 5..8,
                color: amber,
            },
        ];
        assert_eq!(band_at(&bands, 1), None);
        assert_eq!(band_at(&bands, 2), Some(blue));
        assert_eq!(band_at(&bands, 5), Some(amber), "the end is excluded");
        assert_eq!(band_at(&bands, 8), None);
    }

    #[test]
    fn rows_count_the_lines_that_are_not_in_the_text() {
        let ghost = |before, count| GhostLines {
            before,
            lines: vec!["x".into(); count],
            color: gpui::red(),
        };
        // Two ghost rows above line 3, one above line 5.
        let ghosts = [ghost(3, 2), ghost(5, 1)];
        // Without ghosts every line is one row.
        assert_eq!(rows_to(&[], 2, 6), 4.0);
        assert_eq!(line_at_row(&[], 2, 4.7), 6);
        // Line 3's text sits under its two ghosts; line 4 after them.
        assert_eq!(rows_to(&ghosts, 2, 3), 3.0);
        assert_eq!(rows_to(&ghosts, 2, 4), 4.0);
        assert_eq!(rows_to(&ghosts, 2, 5), 6.0);
        // A click on a ghost row is a click on the line under it.
        assert_eq!(line_at_row(&ghosts, 2, 0.5), 2);
        assert_eq!(line_at_row(&ghosts, 2, 1.2), 3, "first ghost of 3");
        assert_eq!(line_at_row(&ghosts, 2, 3.9), 3, "line 3 itself");
        assert_eq!(line_at_row(&ghosts, 2, 4.1), 4);
        assert_eq!(line_at_row(&ghosts, 2, 5.5), 5, "ghost of 5");
        // Both ways agree.
        for line in 2..8 {
            assert_eq!(line_at_row(&ghosts, 2, rows_to(&ghosts, 2, line)), line);
        }
    }
}
