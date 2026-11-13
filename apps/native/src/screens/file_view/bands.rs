//! Tints behind runs of lines, and an element pinned to the right of some:
//! how a view that owns the file marks parts of it without leaving the
//! editor (the merge resolver's conflict blocks, each with its choices on
//! its first line). The text under a band is edited like any other; the
//! owner sets the bands again when the text changes.

use std::ops::Range;
use std::rc::Rc;

use gpui::{AnyElement, App, Context, Hsla};

use super::FileView;

/// A tint behind the lines `lines` (zero-based, end excluded).
#[derive(Clone, Debug, PartialEq)]
pub struct LineBand {
    pub lines: Range<usize>,
    pub color: Hsla,
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
}
