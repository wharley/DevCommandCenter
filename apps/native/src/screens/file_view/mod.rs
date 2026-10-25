//! A file read on the stage (`WorkspaceFileEditor` on `@pierre/diffs`'
//! `File`): its lines with their numbers and the grammar's colors, in the
//! diff's monospace and Pierre theme, the line under the pointer
//! highlighted, scrolled sideways like `overflow: scroll`. A file opened
//! at a line shows that line.

mod view;

#[cfg(test)]
mod tests;

use gpui::{Context, ListAlignment, ListState, Task, px};

use crate::data::highlight::{Span, highlight};

/// `tab-size: 2`
const TAB: &str = "  ";
/// `tokenizeMaxLength`: a bigger file is shown without colors.
const MAX_TOKENIZED: usize = 250_000;

pub struct FileView {
    pub(crate) dark: bool,
    pub(crate) lines: Vec<String>,
    pub(crate) spans: Vec<Vec<Span>>,
    pub(crate) list: ListState,
    pub(crate) scroll_x: f32,
    /// The line to show, once laid out (1-based).
    pub(crate) focus_line: Option<usize>,
    _colors: Option<Task<()>>,
}

impl FileView {
    pub fn new(path: String, text: String, dark: bool, cx: &mut Context<Self>) -> Self {
        let text = text.replace('\t', TAB);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        // A trailing newline is a last, empty line in the editor.
        if text.is_empty() || text.ends_with('\n') {
            lines.push(String::new());
        }
        let mut view = Self {
            dark,
            list: ListState::new(lines.len(), ListAlignment::Top, px(400.0)),
            lines,
            spans: Vec::new(),
            scroll_x: 0.0,
            focus_line: None,
            _colors: None,
        };
        if text.len() <= MAX_TOKENIZED {
            view._colors = Some(cx.spawn(async move |this, cx| {
                let spans = cx
                    .background_executor()
                    .spawn(async move { highlight(&text, &path, dark, usize::MAX) })
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.spans = spans;
                    cx.notify();
                });
            }));
        }
        view
    }

    /// Shows `line` (1-based) at the top, as a reference opens it.
    pub fn focus(&mut self, line: Option<u32>, cx: &mut Context<Self>) {
        self.focus_line = line.map(|line| line.max(1) as usize);
        if let Some(line) = self.focus_line {
            let index = (line - 1).min(self.lines.len().saturating_sub(1));
            // Every row is one 20px line: the line goes straight to the top.
            self.list.scroll_to(gpui::ListOffset {
                item_ix: index,
                offset_in_item: px(0.0),
            });
        }
        cx.notify();
    }
}
