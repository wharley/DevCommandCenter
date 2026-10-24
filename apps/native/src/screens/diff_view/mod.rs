//! A read-only file diff (`WorkspaceChangesDiff` / `@pierre/diffs` with
//! `workspaceDiffViewOptions`): unified, one number column, `+`/`-` in front
//! of changed lines, the changed words emphasized, syntax colors from the
//! Pierre themes, and folded unchanged runs that open on click. The code
//! column's change cards use it now; the center's diff tabs will too.

mod palette;
#[cfg(test)]
mod tests;
mod view;

use gpui::{Context, ListAlignment, ListState, Task, px};

use crate::data::diff::{DiffLine, FileDiff, LineKind, Row};
use crate::data::highlight::{Span, highlight};

pub use palette::DiffPalette;

/// `tab-size: 2`
const TAB: &str = "  ";

/// The diff and the colors of both sides, computed off the UI thread: the
/// lines first, their colors after (a big file takes seconds to color).
pub(crate) struct Computed {
    pub diff: FileDiff,
    pub old_spans: Vec<Vec<Span>>,
    pub new_spans: Vec<Vec<Span>>,
    /// The biggest line number on either side (the number column's width).
    pub last_number: usize,
}

pub(crate) enum State {
    Loading,
    Ready(Box<Computed>),
}

pub struct DiffView {
    pub(crate) dark: bool,
    pub(crate) state: State,
    pub(crate) rows: Vec<Row>,
    pub(crate) list: ListState,
    /// How far the lines are scrolled sideways (`overflow: scroll`).
    pub(crate) scroll_x: f32,
    /// Where the list stood after the last wheel event (or frame).
    pub(crate) seen_top: Option<(usize, gpui::Pixels)>,
    _compute: Option<Task<()>>,
}

/// The colors of both sides, once the grammar went through them.
struct Colors {
    old_spans: Vec<Vec<Span>>,
    new_spans: Vec<Vec<Span>>,
}

impl DiffView {
    pub fn new(
        path: String,
        original: String,
        modified: String,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            dark,
            state: State::Loading,
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.0)),
            scroll_x: 0.0,
            seen_top: None,
            _compute: None,
        };
        view._compute = Some(cx.spawn(async move |this, cx| {
            let (computed, original, modified) = cx
                .background_executor()
                .spawn(async move { lines(&original, &modified) })
                .await;
            let old_until = old_lines_to_color(&computed.diff);
            let shown = this.update(cx, |view, cx| {
                view.rows = computed.diff.rows();
                view.list.reset(view.rows.len());
                view.state = State::Ready(Box::new(computed));
                cx.notify();
            });
            if shown.is_err() {
                return;
            }
            let colors = cx
                .background_executor()
                .spawn(async move {
                    Colors {
                        old_spans: highlight(&original, &path, dark, old_until),
                        new_spans: highlight(&modified, &path, dark, usize::MAX),
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if let State::Ready(computed) = &mut view.state {
                    computed.old_spans = colors.old_spans;
                    computed.new_spans = colors.new_spans;
                    cx.notify();
                }
            });
        }));
        view
    }

    pub(crate) fn list_top(&self) -> (usize, gpui::Pixels) {
        let top = self.list.logical_scroll_top();
        (top.item_ix, top.offset_in_item)
    }

    /// A separator clicked: 20 more lines, or the whole run with Shift.
    pub(crate) fn expand(&mut self, gap: usize, all: bool, cx: &mut Context<Self>) {
        let State::Ready(computed) = &mut self.state else {
            return;
        };
        computed.diff.expand(gap, all);
        let next = computed.diff.rows();
        let first_changed = self
            .rows
            .iter()
            .zip(&next)
            .take_while(|(a, b)| a == b)
            .count();
        let removed = first_changed..self.rows.len();
        self.list.splice(removed, next.len() - first_changed);
        self.rows = next;
        cx.notify();
    }
}

/// The lines of the diff, without colors yet (tabs as 2 spaces), and both
/// texts as the grammar will read them.
fn lines(original: &str, modified: &str) -> (Computed, String, String) {
    let (original, modified) = (original.replace('\t', TAB), modified.replace('\t', TAB));
    let diff = FileDiff::new(&original, &modified);
    let last = |side: fn(&DiffLine) -> Option<usize>| diff.lines.iter().rev().find_map(side);
    let last_number = last(|line| line.old)
        .unwrap_or(0)
        .max(last(|line| line.new).unwrap_or(0));
    let computed = Computed {
        diff,
        old_spans: Vec::new(),
        new_spans: Vec::new(),
        last_number,
    };
    (computed, original, modified)
}

/// The old side shows only its removed lines: the grammar stops after the
/// last one.
fn old_lines_to_color(diff: &FileDiff) -> usize {
    diff.lines
        .iter()
        .filter(|line| line.kind == LineKind::Deletion)
        .filter_map(|line| line.old)
        .max()
        .unwrap_or(0)
}

/// Lines and colors at once (the tests' way).
#[cfg(test)]
pub(crate) fn compute(path: &str, original: &str, modified: &str, dark: bool) -> Computed {
    let (mut computed, original, modified) = lines(original, modified);
    let old_until = old_lines_to_color(&computed.diff);
    computed.old_spans = highlight(&original, path, dark, old_until);
    computed.new_spans = highlight(&modified, path, dark, usize::MAX);
    computed
}
