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

use crate::data::diff::{DiffLine, FileDiff, LineKind, Row, SplitRow, split_rows};
use crate::data::highlight::{Span, highlight};

pub use palette::DiffPalette;
pub(crate) use view::styled_runs;

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
    /// Side by side (`diffStyle: "split"`): the rows paired, and the list
    /// holds them instead.
    pub(crate) split: bool,
    pub(crate) split_rows: Vec<SplitRow>,
    pub(crate) list: ListState,
    /// How far the lines are scrolled sideways (`overflow: scroll`); side by
    /// side, the old side's, and the new side's apart.
    pub(crate) scroll_x: f32,
    pub(crate) scroll_x_new: f32,
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
        Self::start(path, dark, false, cx, move || lines(&original, &modified))
    }

    /// The stage's view of a modified file: the old side beside the new.
    pub fn side_by_side(
        path: String,
        original: String,
        modified: String,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::start(path, dark, true, cx, move || lines(&original, &modified))
    }

    /// A captured patch (`WorkspacePatchDiff`): its hunks only.
    pub fn from_patch(path: String, patch: String, dark: bool, cx: &mut Context<Self>) -> Self {
        Self::start(path, dark, false, cx, move || patch_lines(&patch))
    }

    fn start(
        path: String,
        dark: bool,
        split: bool,
        cx: &mut Context<Self>,
        compute: impl FnOnce() -> (Computed, String, String) + Send + 'static,
    ) -> Self {
        let mut view = Self {
            dark,
            state: State::Loading,
            rows: Vec::new(),
            split,
            split_rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.0)),
            scroll_x: 0.0,
            scroll_x_new: 0.0,
            seen_top: None,
            _compute: None,
        };
        view._compute = Some(cx.spawn(async move |this, cx| {
            let (computed, original, modified) = cx
                .background_executor()
                .spawn(async move { compute() })
                .await;
            // Side by side the old side shows its context too.
            let old_until = if split {
                usize::MAX
            } else {
                old_lines_to_color(&computed.diff)
            };
            let shown = this.update(cx, |view, cx| {
                view.rows = computed.diff.rows();
                view.split_rows = split_rows(&computed.diff, &view.rows);
                view.list.reset(view.shown_rows());
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

    /// The rows the list holds.
    pub(crate) fn shown_rows(&self) -> usize {
        if self.split {
            self.split_rows.len()
        } else {
            self.rows.len()
        }
    }

    /// A separator clicked: 20 more lines, or the whole run with Shift.
    pub(crate) fn expand(&mut self, gap: usize, all: bool, cx: &mut Context<Self>) {
        let State::Ready(computed) = &mut self.state else {
            return;
        };
        computed.diff.expand(gap, all);
        let next = computed.diff.rows();
        let next_split = split_rows(&computed.diff, &next);
        fn first_changed<T: PartialEq>(old: &[T], new: &[T]) -> usize {
            old.iter().zip(new).take_while(|(a, b)| a == b).count()
        }
        let (kept, old_len, new_len) = if self.split {
            (
                first_changed(&self.split_rows, &next_split),
                self.split_rows.len(),
                next_split.len(),
            )
        } else {
            (
                first_changed(&self.rows, &next),
                self.rows.len(),
                next.len(),
            )
        };
        self.list.splice(kept..old_len, new_len - kept);
        self.rows = next;
        self.split_rows = next_split;
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

/// A patch's lines, and each side as the grammar reads it: the lines the
/// patch has at their own numbers, the rest blank.
fn patch_lines(patch: &str) -> (Computed, String, String) {
    let diff = FileDiff::from_patch(&patch.replace('\t', TAB));
    let side = |number: fn(&DiffLine) -> Option<usize>| {
        let mut text: Vec<&str> = Vec::new();
        for line in &diff.lines {
            if let Some(at) = number(line) {
                if text.len() < at {
                    text.resize(at, "");
                }
                text[at - 1] = &line.text;
            }
        }
        text.join("\n")
    };
    let (original, modified) = (side(|line| line.old), side(|line| line.new));
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
