//! Markdown lists and emphasis in the composer, kept as the Markdown it
//! sends (the desktop's Lexical composer showed them rendered and sent the
//! same `- item` / `1. item` / `**bold**`). In a list, Enter starts the next
//! item and on an empty one leaves the list; ⌘Enter sends anyway. Tab and
//! Shift+Tab nest an item under the one above or bring it back out; ⌘B and
//! ⌘I wrap the selection. Plain functions over the text and selection; the
//! field applies what they return as one undo step.

use std::ops::Range;

use gpui::{Context, Window};

use super::buffer::CHIP;
use super::{Bold, Italic, Outdent, SubmitNow, TextArea, TextAreaEvent};

/// Edits (non-overlapping, never over a chip) and the selection after.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Change {
    pub edits: Vec<(Range<usize>, String)>,
    pub selection: Range<usize>,
}

/// A list item's start: `  - `, `* `, `+ `, `12. ` or `3) `.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Marker {
    /// Leading spaces.
    indent: usize,
    /// `-` / `*` / `+`, or the number and its `.` / `)`.
    kind: Kind,
    /// Through the space after the marker: where the item's text starts.
    len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Bullet(char),
    Number(u64, char),
}

impl Marker {
    fn text(&self) -> String {
        match self.kind {
            Kind::Bullet(bullet) => format!("{bullet} "),
            Kind::Number(number, delimiter) => format!("{number}{delimiter} "),
        }
    }

    /// The same marker at another indent, numbered `number` if it counts.
    fn at(&self, indent: usize, number: u64) -> Marker {
        let kind = match self.kind {
            Kind::Bullet(bullet) => Kind::Bullet(bullet),
            Kind::Number(_, delimiter) => Kind::Number(number, delimiter),
        };
        let mut marker = Marker {
            indent,
            kind,
            len: 0,
        };
        marker.len = indent + marker.text().len();
        marker
    }

    fn prefix(&self) -> String {
        format!("{}{}", " ".repeat(self.indent), self.text())
    }

    fn number(&self) -> Option<u64> {
        match self.kind {
            Kind::Number(number, _) => Some(number),
            Kind::Bullet(_) => None,
        }
    }
}

fn marker(line: &str) -> Option<Marker> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let mut chars = rest.chars();
    let kind = match chars.next()? {
        bullet @ ('-' | '*' | '+') => Kind::Bullet(bullet),
        '0'..='9' => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 9 {
                return None;
            }
            let delimiter = rest[digits..].chars().next()?;
            if !matches!(delimiter, '.' | ')') {
                return None;
            }
            Kind::Number(rest[..digits].parse().ok()?, delimiter)
        }
        _ => return None,
    };
    let mut found = Marker {
        indent,
        kind,
        len: 0,
    };
    let text = found.text();
    // The marker must be followed by its space (`**bold**` is not a list).
    if !rest.starts_with(&text) {
        return None;
    }
    found.len = indent + text.len();
    Some(found)
}

fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |index| index + 1)
}

fn line_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index)
}

/// The lines above `start`, nearest first, each with its marker.
fn above(text: &str, start: usize) -> impl Iterator<Item = Option<Marker>> + '_ {
    text[..start.saturating_sub(1)]
        .rsplit('\n')
        .take_while(move |_| start > 0)
        .map(marker)
}

/// What a numbered item at `indent` counts from: one past the item above
/// it at that indent, before the list goes shallower.
fn number_at(text: &str, start: usize, indent: usize) -> u64 {
    for above in above(text, start) {
        match above {
            Some(item) if item.indent == indent => return item.number().map_or(1, |n| n + 1),
            Some(item) if item.indent > indent => continue,
            _ => break,
        }
    }
    1
}

/// The item on the caret's line, with that line's start.
fn item_at(text: &str, offset: usize) -> Option<(usize, Marker)> {
    let start = line_start(text, offset);
    marker(&text[start..line_end(text, offset)]).map(|marker| (start, marker))
}

pub(super) fn in_list(text: &str, offset: usize) -> bool {
    item_at(text, offset.min(text.len())).is_some()
}

/// Enter on a list item: the next item, or out of the list from an empty one
/// (one level up first when nested). `None` outside a list.
pub(super) fn enter(text: &str, caret: usize) -> Option<Change> {
    let (start, item) = item_at(text, caret)?;
    let end = line_end(text, caret);
    if text[start + item.len..end].trim().is_empty() {
        if item.indent == 0 {
            return Some(Change {
                edits: vec![(start..end, String::new())],
                selection: start..start,
            });
        }
        return level(text, start..start, false);
    }
    let at = caret.max(start + item.len);
    let next = item.at(item.indent, item.number().map_or(1, |n| n + 1));
    let insert = format!("\n{}", next.prefix());
    let after = at + insert.len();
    Some(Change {
        edits: vec![(at..at, insert)],
        selection: after..after,
    })
}

/// Nests the item under the one above (`deeper`) or brings it a level out.
pub(super) fn level(text: &str, selection: Range<usize>, deeper: bool) -> Option<Change> {
    let (start, item) = item_at(text, selection.start)?;
    let indent = if deeper {
        // Under the item above at this level, at its text's column.
        let mut parent = None;
        for above in above(text, start) {
            match above {
                Some(above) if above.indent == item.indent => {
                    parent = Some(above.len);
                    break;
                }
                Some(above) if above.indent > item.indent => continue,
                _ => break,
            }
        }
        parent?
    } else {
        if item.indent == 0 {
            return None;
        }
        above(text, start)
            .flatten()
            .find(|above| above.indent < item.indent)
            .map_or(0, |above| above.indent)
    };
    let moved = item.at(indent, number_at(text, start, indent));
    let shift = |offset: usize| {
        if offset < start + item.len {
            start + moved.len
        } else {
            offset + moved.len - item.len
        }
    };
    Some(Change {
        edits: vec![(start..start + item.len, moved.prefix())],
        selection: shift(selection.start)..shift(selection.end.max(selection.start)),
    })
}

/// ⌘B / ⌘I: wraps the selection in `mark` (spaces stay outside), unwraps it
/// when already wrapped, or puts the pair at the caret.
pub(super) fn emphasis(text: &str, selection: Range<usize>, mark: &str) -> Option<Change> {
    let width = mark.len();
    let selected = &text[selection.clone()];
    if selected.contains('\n') {
        return None;
    }
    let start = selection.start + (selected.len() - selected.trim_start().len());
    let end = (selection.end - (selected.len() - selected.trim_end().len())).max(start);
    if start == end {
        let pair = format!("{mark}{mark}");
        return Some(Change {
            edits: vec![(selection.end..selection.end, pair)],
            selection: selection.end + width..selection.end + width,
        });
    }
    let inner = &text[start..end];
    if start >= width && text[..start].ends_with(mark) && text[end..].starts_with(mark) {
        return Some(Change {
            edits: vec![
                (end..end + width, String::new()),
                (start - width..start, String::new()),
            ],
            selection: start - width..end - width,
        });
    }
    if inner.len() >= 2 * width && inner.starts_with(mark) && inner.ends_with(mark) {
        return Some(Change {
            edits: vec![
                (end - width..end, String::new()),
                (start..start + width, String::new()),
            ],
            selection: start..end - 2 * width,
        });
    }
    Some(Change {
        edits: vec![(end..end, mark.into()), (start..start, mark.into())],
        selection: start + width..end + width,
    })
}

/// Each item's marker (`-`, `1.`), drawn fainter than the text.
pub(super) fn marker_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        if let Some(item) = marker(line) {
            spans.push(start + item.indent..start + item.len - 1);
        }
        start += line.len() + 1;
    }
    spans
}

impl TextArea {
    fn apply(&mut self, change: Change, cx: &mut Context<Self>) {
        if self.disabled
            || change
                .edits
                .iter()
                .any(|(range, _)| self.buffer.text()[range.clone()].contains(CHIP))
        {
            return;
        }
        self.buffer.splice(change.edits, change.selection);
        self.after_edit(cx);
    }

    /// Enter in a list: handled here unless the caret is outside one.
    pub(super) fn continue_list(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.lists || !in_list(self.buffer.text(), self.buffer.selection.start) {
            return false;
        }
        if !self.buffer.selection.is_empty() {
            self.edit(|buffer| buffer.replace(None, ""), cx);
        }
        match enter(self.buffer.text(), self.buffer.cursor()) {
            Some(change) => self.apply(change, cx),
            None => return false,
        }
        true
    }

    /// Tab on a list item nests it.
    pub(super) fn nest(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.lists {
            return false;
        }
        match level(self.buffer.text(), self.buffer.selection.clone(), true) {
            Some(change) => self.apply(change, cx),
            // On an item that cannot nest, Tab stays in the field.
            None => return in_list(self.buffer.text(), self.buffer.selection.start),
        }
        true
    }

    pub(super) fn outdent(&mut self, _: &Outdent, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.buffer.text();
        match level(text, self.buffer.selection.clone(), false) {
            Some(change) => self.apply(change, cx),
            None if !in_list(text, self.buffer.selection.start) => window.focus_prev(cx),
            None => {}
        }
    }

    pub(super) fn submit_now(&mut self, _: &SubmitNow, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.marked.is_none() {
            cx.emit(TextAreaEvent::Submitted);
        }
    }

    fn emphasize(&mut self, mark: &str, cx: &mut Context<Self>) {
        if let Some(change) = emphasis(self.buffer.text(), self.buffer.selection.clone(), mark) {
            self.apply(change, cx);
        }
    }

    pub(super) fn bold(&mut self, _: &Bold, _: &mut Window, cx: &mut Context<Self>) {
        self.emphasize("**", cx);
    }

    pub(super) fn italic(&mut self, _: &Italic, _: &mut Window, cx: &mut Context<Self>) {
        self.emphasize("*", cx);
    }
}

#[cfg(test)]
#[path = "lists_tests.rs"]
mod tests;
