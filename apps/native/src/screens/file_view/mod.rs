//! A file on the stage, editable (`WorkspaceFileEditor` on `@pierre/diffs`'
//! `File` with its `Editor`): numbered lines with the grammar's colors in
//! the diff's monospace and Pierre theme, a 2px blinking caret, selections,
//! the active line marked, sideways scroll (no wrapping), the platform's
//! text input (IME) and the clipboard, 200 undo steps, Enter keeping the
//! indentation (one level in after an opening bracket) and Tab typing the
//! project's indent. Tabs are drawn to their stop. A file opened at a line
//! shows that line.

mod buffer;
mod commands;
mod comments;
mod find;
mod find_view;
mod handler;
mod keys;
mod lines;
mod pick;
mod search;
mod view;

#[cfg(test)]
mod tests;

pub use buffer::EditBuffer;
/// Registers the editor's keys and its search panel's; call once at startup.
pub fn bind_keys(cx: &mut gpui::App) {
    keys::bind_keys(cx);
    commands::bind_keys(cx);
    find::bind_keys(cx);
}

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dcc_core::domain::indentation::FileIndentation;
use gpui::{
    Bounds, Context, EventEmitter, FocusHandle, Focusable, ListAlignment, ListState, Pixels, Task,
    px,
};

use crate::data::highlight::{Span, highlight};

/// `tokenizeMaxLength`: a bigger file is shown without colors.
const MAX_TOKENIZED: usize = 250_000;
/// The pause after an edit before the colors are worked out again.
const RECOLOR_AFTER: Duration = Duration::from_millis(120);
pub(crate) const LINE: f32 = 20.0;

#[derive(Clone, Debug, PartialEq)]
pub enum FileViewEvent {
    /// The text changed (the tab pins itself and shows unsaved).
    Edited,
    /// Code picked for the agent.
    Annotate(crate::screens::annotation::AnnotationRequest),
}

/// Where the text sits on screen, from the last frame (for the pointer
/// and the IME's candidate window).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Geometry {
    pub bounds: Bounds<Pixels>,
    /// The code's left edge, past the numbers, from the view's left.
    pub text_left: f32,
    pub cell: f32,
}

pub struct FileView {
    pub(crate) path: String,
    pub(crate) dark: bool,
    pub(crate) buffer: EditBuffer,
    /// What the disk had when last read or saved (unsaved = differs).
    pub(crate) base: String,
    pub(crate) indentation: FileIndentation,
    pub(crate) spans: Vec<Vec<Span>>,
    pub(crate) list: ListState,
    pub(crate) scroll_x: f32,
    pub(crate) focus_line: Option<usize>,
    pub(crate) focus: FocusHandle,
    /// The screen column up and down keep.
    pub(crate) goal_column: Option<f32>,
    pub(crate) geometry: Rc<Cell<Geometry>>,
    /// A drag with the pointer down is selecting.
    pub(crate) selecting: bool,
    /// The caret shows steadily for a moment after any change.
    pub(crate) caret_moved: Instant,
    /// Keep the caret in view at the next frame.
    pub(crate) reveal: bool,
    /// The search panel, while open.
    pub(crate) search: Option<find::Search>,
    /// Code can go to the agent from here (`annotatable`).
    pub(crate) annotating: Option<pick::Annotating>,
    _colors: Option<Task<()>>,
    _blink: Option<Task<()>>,
}

impl EventEmitter<FileViewEvent> for FileView {}

impl Focusable for FileView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl FileView {
    pub fn new(
        path: String,
        text: String,
        indentation: FileIndentation,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = EditBuffer::new(text.clone());
        let mut view = Self {
            path,
            dark,
            list: ListState::new(buffer.line_count(), ListAlignment::Top, px(400.0)),
            buffer,
            base: text,
            indentation,
            spans: Vec::new(),
            scroll_x: 0.0,
            focus_line: None,
            focus: cx.focus_handle(),
            goal_column: None,
            geometry: Rc::default(),
            selecting: false,
            caret_moved: Instant::now(),
            reveal: false,
            search: None,
            annotating: None,
            _colors: None,
            _blink: None,
        };
        view.recolor(Duration::ZERO, cx);
        // Repaints so the caret blinks (1.2s, after 0.8s still).
        view._blink = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                let alive = this.update(cx, |view, cx| {
                    if view.caret_moved.elapsed() > Duration::from_millis(700) {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        }));
        view
    }

    /// Another theme: the colors are worked out again.
    pub fn set_dark(&mut self, dark: bool, cx: &mut Context<Self>) {
        self.dark = dark;
        self.recolor(Duration::ZERO, cx);
        cx.notify();
    }

    /// The lines as typed now.
    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    /// Unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.buffer.text() != self.base
    }

    /// Saved: what is on disk now.
    pub fn mark_saved(&mut self, text: String, cx: &mut Context<Self>) {
        self.base = text;
        cx.notify();
    }

    /// Other text in place of all of it, as one step (the merge resolver's
    /// accepted block or side); what is on disk stays the base.
    pub fn replace_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.buffer.text() == text {
            return;
        }
        let lines = self.buffer.line_count();
        self.buffer.set_text(text);
        self.edited(lines, 0, cx);
    }

    /// "Recarregar do disco": the disk's text replaces the edit.
    pub fn take_disk(&mut self, disk: String, cx: &mut Context<Self>) {
        let lines = self.buffer.line_count();
        self.buffer.set_text(&disk);
        self.base = disk;
        self.edited(lines, 0, cx);
    }

    /// Shows `line` (1-based) at the top with the caret on it, as a
    /// reference opens it.
    pub fn focus(&mut self, line: Option<u32>, cx: &mut Context<Self>) {
        self.focus_line = line.map(|line| line.max(1) as usize);
        if let Some(line) = self.focus_line {
            let index = (line - 1).min(self.buffer.line_count().saturating_sub(1));
            self.list.scroll_to(gpui::ListOffset {
                item_ix: index,
                offset_in_item: px(0.0),
            });
            let at = self.buffer.line_start(index);
            self.buffer.go(at, false);
        }
        cx.notify();
    }

    /// The colors worked out again a moment after the text settles.
    fn recolor(&mut self, after: Duration, cx: &mut Context<Self>) {
        let text = self.buffer.text().to_string();
        if text.len() > MAX_TOKENIZED {
            self.spans.clear();
            return;
        }
        let (path, dark, version) = (self.path.clone(), self.dark, self.buffer.version);
        self._colors = Some(cx.spawn(async move |this, cx| {
            if !after.is_zero() {
                cx.background_executor().timer(after).await;
            }
            let spans = cx
                .background_executor()
                .spawn(async move { highlight(&text, &path, dark, usize::MAX) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.buffer.version == version {
                    view.spans = spans;
                    cx.notify();
                }
            });
        }));
    }

    /// After any edit: the list follows the line count (the colors shift
    /// with the lines until they are worked out again), the caret shows.
    pub(crate) fn edited(&mut self, lines_before: usize, at_line: usize, cx: &mut Context<Self>) {
        let lines = self.buffer.line_count();
        if lines != lines_before {
            let at = (at_line + 1).min(self.spans.len());
            if lines > lines_before {
                let added = vec![Vec::new(); lines - lines_before];
                self.spans.splice(at..at, added);
            } else {
                let removed = (lines_before - lines).min(self.spans.len() - at);
                self.spans.drain(at..at + removed);
            }
            let top = self.list.logical_scroll_top();
            self.list.reset(lines);
            self.list.scroll_to(top);
        }
        self.recolor(RECOLOR_AFTER, cx);
        // The panel's matches follow the text (the selection stays).
        if self.search.is_some() {
            self.refresh_matches(false, cx);
        }
        self.moved(cx);
        cx.emit(FileViewEvent::Edited);
    }

    /// The caret moved: it shows steadily and stays in view.
    pub(crate) fn moved(&mut self, cx: &mut Context<Self>) {
        self.caret_moved = Instant::now();
        self.reveal = true;
        cx.notify();
    }

    /// Scrolls so the caret's line and column show in a view this big.
    pub(crate) fn reveal_caret(&mut self, height: f32, width: f32, cell: f32) {
        let line = self.buffer.line_of(self.buffer.cursor());
        let rows = ((height / LINE).floor() as usize).max(1);
        let top = self.list.logical_scroll_top().item_ix;
        if line < top {
            self.list.scroll_to(gpui::ListOffset {
                item_ix: line,
                offset_in_item: px(0.0),
            });
        } else if line + 1 > top + rows {
            self.list.scroll_to(gpui::ListOffset {
                item_ix: line + 1 - rows,
                offset_in_item: px(0.0),
            });
        }
        let text = self.buffer.line(line);
        let start = self.buffer.line_start(line);
        let column = buffer::column_of(text, self.buffer.cursor() - start, self.indentation.size);
        let x = column as f32 * cell;
        if x < self.scroll_x {
            self.scroll_x = x;
        } else if x + cell * 2.0 > self.scroll_x + width {
            self.scroll_x = x + cell * 2.0 - width;
        }
    }
}

#[cfg(test)]
#[path = "pick_tests.rs"]
mod pick_tests;
