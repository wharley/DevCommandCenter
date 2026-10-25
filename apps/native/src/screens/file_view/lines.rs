//! The Pierre editor's line commands, as pure rules over the text and its
//! selection: indent in and out (`resolveIndentEdits`), move and copy the
//! selected lines, ⌘Enter's blank line, ⌃K to the line's end, Tab over a
//! selection on one line, and typing a bracket or quote over a selection
//! (`getAutoSurroundReplacementTexts`). Each returns the edits and the
//! selection after them; no GPUI here.

use std::ops::Range;

/// One replacement, in offsets of the text before any edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

/// The edits (in order, not overlapping) and the selection in the text after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub edits: Vec<Edit>,
    pub selection: Range<usize>,
    pub reversed: bool,
}

impl Change {
    /// The edits as one replacement: the range they span and its new text.
    pub fn splice(&self, text: &str) -> (Range<usize>, String) {
        let start = self.edits.first().map_or(0, |edit| edit.range.start);
        let end = self.edits.last().map_or(0, |edit| edit.range.end);
        let mut out = String::new();
        let mut at = start;
        for edit in &self.edits {
            out.push_str(&text[at..edit.range.start]);
            out.push_str(&edit.text);
            at = edit.range.end;
        }
        out.push_str(&text[at..end]);
        (start..end, out)
    }

    #[cfg(test)]
    pub fn apply(&self, text: &str) -> String {
        let (range, replacement) = self.splice(text);
        format!(
            "{}{replacement}{}",
            &text[..range.start],
            &text[range.end..]
        )
    }
}

/// The text's lines: where each starts and where its content ends.
pub struct Doc<'a> {
    pub text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Doc<'a> {
    pub fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self { text, starts }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    pub fn start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.text.len())
    }

    /// Where the line's content ends (before its `\n` or `\r\n`).
    pub fn end(&self, line: usize) -> usize {
        let Some(next) = self.starts.get(line + 1) else {
            return self.text.len();
        };
        let end = next - 1;
        if end > self.start(line) && self.text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        }
    }

    pub fn line(&self, line: usize) -> &'a str {
        &self.text[self.start(line)..self.end(line)]
    }

    pub fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        }
    }

    /// The text's line break: its first one (`\n` without any).
    pub fn eol(&self) -> &'static str {
        match self.text.find('\n') {
            Some(at) if at > 0 && self.text.as_bytes()[at - 1] == b'\r' => "\r\n",
            _ => "\n",
        }
    }

    /// The selection's lines; a selection ending at a line's start leaves
    /// that line out (`getSelectedLineBlocks`).
    pub fn block(&self, selection: &Range<usize>) -> (usize, usize) {
        let first = self.line_of(selection.start);
        let mut last = self.line_of(selection.end);
        if last > first && selection.end == self.start(last) {
            last -= 1;
        }
        (first, last)
    }
}

/// The leading whitespace's length.
pub fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// `remapOffsetThroughEdits`: an offset carried past the edits before it.
pub fn remap(offset: usize, edits: &[Edit]) -> usize {
    let mut delta: isize = 0;
    for edit in edits {
        if offset < edit.range.start {
            break;
        }
        if offset >= edit.range.end {
            delta += edit.text.len() as isize - edit.range.len() as isize;
        } else {
            return (edit.range.start as isize + delta) as usize + edit.text.len();
        }
    }
    (offset as isize + delta) as usize
}

fn shifted(selection: &Range<usize>, delta: isize, limit: usize) -> Range<usize> {
    let at = |offset: usize| ((offset as isize + delta).max(0) as usize).min(limit);
    at(selection.start)..at(selection.end)
}

/// Tab and Shift+Tab over lines, ⌘] and ⌘[: each line one level in (a tab
/// when it starts with one, else `tab_size` spaces) or out. Blank lines of
/// a block of several are left alone.
pub fn indent_lines(
    doc: &Doc,
    selection: &Range<usize>,
    reversed: bool,
    tab_size: usize,
    outdent: bool,
) -> Option<Change> {
    let (first, last) = doc.block(selection);
    let several = doc.line_of(selection.start) != doc.line_of(selection.end);
    let mut edits: Vec<(usize, isize, Edit)> = Vec::new();
    for line in first..=last {
        let text = doc.line(line);
        if several && text.trim().is_empty() {
            continue;
        }
        let unit = if text.starts_with('\t') {
            "\t".to_string()
        } else {
            " ".repeat(tab_size)
        };
        let at = doc.start(line);
        let (removed, inserted) = if outdent {
            let removed = if text.starts_with('\t') {
                1
            } else {
                (text.len() - text.trim_start_matches(' ').len()).min(unit.len())
            };
            if removed == 0 {
                continue;
            }
            (removed, String::new())
        } else {
            (0, unit)
        };
        let delta = inserted.len() as isize - removed as isize;
        let edit = Edit {
            range: at..at + removed,
            text: inserted,
        };
        edits.push((line, delta, edit));
    }
    if edits.is_empty() {
        return None;
    }
    // Each end keeps its column, moved by its own line's change.
    let place = |offset: usize| {
        let line = doc.line_of(offset);
        let before: isize = edits
            .iter()
            .filter(|(at, _, _)| *at < line)
            .map(|(_, delta, _)| delta)
            .sum();
        let own = edits
            .iter()
            .find(|(at, _, _)| *at == line)
            .map_or(0, |(_, delta, _)| *delta);
        let column = (offset - doc.start(line)) as isize;
        (doc.start(line) as isize + before + (column + own).max(0)) as usize
    };
    let (start, end) = (place(selection.start), place(selection.end));
    let edits = edits.into_iter().map(|(_, _, edit)| edit).collect();
    Some(Change {
        edits,
        selection: start.min(end)..start.max(end),
        reversed,
    })
}

/// Tab over a selection within one line: a tab when the line starts with
/// one, else `tab_size` spaces, the caret after it.
pub fn tab_over_selection(doc: &Doc, selection: &Range<usize>, tab_size: usize) -> Change {
    let line = doc.line_of(selection.start);
    let text = if doc.line(line).starts_with('\t') {
        "\t".to_string()
    } else {
        " ".repeat(tab_size)
    };
    let caret = selection.start + text.len();
    Change {
        edits: vec![Edit {
            range: selection.clone(),
            text,
        }],
        selection: caret..caret,
        reversed: false,
    }
}

/// ⌥↑ / ⌥↓: the selected lines swap with the line above or below; nothing
/// at the file's edge.
pub fn move_lines(doc: &Doc, selection: &Range<usize>, reversed: bool, up: bool) -> Option<Change> {
    let (first, last) = doc.block(selection);
    let eol = doc.eol();
    let block = &doc.text[doc.start(first)..doc.end(last)];
    let (range, text, delta) = if up {
        let above = first.checked_sub(1)?;
        let line = doc.line(above);
        (
            doc.start(above)..doc.end(last),
            format!("{block}{eol}{line}"),
            -((line.len() + eol.len()) as isize),
        )
    } else {
        let below = last + 1;
        if below >= doc.line_count() {
            return None;
        }
        let line = doc.line(below);
        (
            doc.start(first)..doc.end(below),
            format!("{line}{eol}{block}"),
            (line.len() + eol.len()) as isize,
        )
    };
    let limit = doc.text.len() - range.len() + text.len();
    Some(Change {
        edits: vec![Edit { range, text }],
        selection: shifted(selection, delta, limit),
        reversed,
    })
}

/// ⇧⌥↓ / ⇧⌥↑: the selected lines copied; down keeps the selection on the
/// lower copy, up on the upper one.
pub fn copy_lines(doc: &Doc, selection: &Range<usize>, reversed: bool, down: bool) -> Change {
    let (first, last) = doc.block(selection);
    let eol = doc.eol();
    let block = &doc.text[doc.start(first)..doc.end(last)];
    let (at, text) = if down {
        (doc.start(first), format!("{block}{eol}"))
    } else if last + 1 < doc.line_count() {
        (doc.start(last + 1), format!("{block}{eol}"))
    } else {
        (doc.end(last), format!("{eol}{block}"))
    };
    let delta = if down { text.len() as isize } else { 0 };
    let limit = doc.text.len() + text.len();
    Change {
        edits: vec![Edit {
            range: at..at,
            text,
        }],
        selection: shifted(selection, delta, limit),
        reversed,
    }
}

/// ⌘Enter: a new line under the caret's, with its indentation.
pub fn blank_line(doc: &Doc, caret: usize) -> Change {
    let line = doc.line_of(caret);
    let text = doc.line(line);
    let indent = &text[..indent_of(text)];
    let at = doc.end(line);
    let inserted = format!("{}{indent}", doc.eol());
    let caret = at + inserted.len();
    Change {
        edits: vec![Edit {
            range: at..at,
            text: inserted,
        }],
        selection: caret..caret,
        reversed: false,
    }
}

/// ⌃K: the selection, else to the line's end, else its line break.
pub fn delete_to_line_end(doc: &Doc, selection: &Range<usize>) -> Option<Change> {
    let range = if !selection.is_empty() {
        selection.clone()
    } else {
        let line = doc.line_of(selection.start);
        let end = doc.end(line);
        if selection.start < end {
            selection.start..end
        } else if line + 1 < doc.line_count() {
            selection.start..doc.start(line + 1)
        } else {
            return None;
        }
    };
    let caret = range.start;
    Some(Change {
        edits: vec![Edit {
            range,
            text: String::new(),
        }],
        selection: caret..caret,
        reversed: false,
    })
}

/// `AUTO_SURROUND_CLOSE_CHARS`
fn closing(open: &str) -> Option<&'static str> {
    Some(match open {
        "'" => "'",
        "\"" => "\"",
        "`" => "`",
        "{" => "}",
        "[" => "]",
        "<" => ">",
        "(" => ")",
        _ => return None,
    })
}

/// A bracket or quote typed over a selection wraps it, the text inside
/// still selected.
pub fn surround(doc: &Doc, selection: &Range<usize>, typed: &str) -> Option<Change> {
    if selection.is_empty() {
        return None;
    }
    let close = closing(typed)?;
    let inner = &doc.text[selection.clone()];
    let start = selection.start + typed.len();
    Some(Change {
        edits: vec![Edit {
            range: selection.clone(),
            text: format!("{typed}{inner}{close}"),
        }],
        selection: start..start + inner.len(),
        reversed: false,
    })
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
