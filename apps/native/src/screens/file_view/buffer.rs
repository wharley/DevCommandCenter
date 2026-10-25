//! The text the file editor edits: the content with its line starts, the
//! selection (UTF-8 byte offsets on grapheme boundaries), the IME's marked
//! range, and an undo history of the edits themselves (not copies of the
//! file: a big file keeps 200 steps cheaply). No GPUI here.

use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

/// Typing within this pause joins the previous undo step.
const UNDO_GROUP: Duration = Duration::from_millis(600);
/// `historyMaxEntries`
const UNDO_LIMIT: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    start: usize,
    removed: String,
    inserted: String,
    before: Range<usize>,
    after: Range<usize>,
}

/// An IME composition under way: where it began and what it replaced.
#[derive(Clone, Debug)]
struct Composition {
    start: usize,
    removed: String,
    before: Range<usize>,
}

#[derive(Debug)]
pub struct EditBuffer {
    text: String,
    line_starts: Vec<usize>,
    pub selection: Range<usize>,
    pub reversed: bool,
    pub marked: Option<Range<usize>>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    last_typing: Option<Instant>,
    composition: Option<Composition>,
    /// Bumped on every change of the text.
    pub version: u64,
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

impl EditBuffer {
    pub fn new(text: String) -> Self {
        Self {
            line_starts: line_starts(&text),
            text,
            selection: 0..0,
            reversed: false,
            marked: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_typing: None,
            composition: None,
            version: 0,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Where line `index` starts.
    pub fn line_start(&self, index: usize) -> usize {
        self.line_starts
            .get(index)
            .copied()
            .unwrap_or(self.text.len())
    }

    /// Line `index` without its line break (nor a CR before it).
    pub fn line(&self, index: usize) -> &str {
        let start = self.line_start(index);
        let end = self
            .line_starts
            .get(index + 1)
            .map_or(self.text.len(), |next| next - 1);
        self.text[start..end].trim_end_matches('\r')
    }

    /// The line an offset sits on.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        }
    }

    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    pub fn anchor(&self) -> usize {
        if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        }
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.clone()]
    }

    fn clamp(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// The caret at `offset`, or the selection grown to it.
    pub fn go(&mut self, offset: usize, extend: bool) {
        let offset = self.clamp(offset);
        if !extend {
            self.selection = offset..offset;
            self.reversed = false;
            return;
        }
        let anchor = self.anchor();
        if offset < anchor {
            self.selection = offset..anchor;
            self.reversed = true;
        } else {
            self.selection = anchor..offset;
            self.reversed = false;
        }
    }

    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
        self.reversed = false;
    }

    pub fn select(&mut self, range: Range<usize>) {
        self.selection = self.clamp(range.start)..self.clamp(range.end);
        self.reversed = false;
    }

    pub fn previous_boundary(&self, offset: usize) -> usize {
        self.text[..self.clamp(offset)]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    pub fn next_boundary(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        self.text[offset..]
            .graphemes(true)
            .next()
            .map_or(self.text.len(), |grapheme| offset + grapheme.len())
    }

    /// The start of the word before `offset` (over spaces first).
    pub fn previous_word(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        self.text[..offset]
            .unicode_word_indices()
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    /// The end of the word after `offset`.
    pub fn next_word(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        self.text[offset..]
            .unicode_word_indices()
            .next()
            .map_or(self.text.len(), |(at, word)| offset + at + word.len())
    }

    pub fn line_start_of(&self, offset: usize) -> usize {
        self.line_start(self.line_of(self.clamp(offset)))
    }

    pub fn line_end_of(&self, offset: usize) -> usize {
        let line = self.line_of(self.clamp(offset));
        self.line_start(line) + self.line(line).len()
    }

    /// The word around `offset` (a double-click), or the character there.
    pub fn word_at(&self, offset: usize) -> Range<usize> {
        let offset = self.clamp(offset);
        let line = self.line_of(offset);
        let start = self.line_start(line);
        let text = self.line(line);
        for (at, word) in text.split_word_bound_indices() {
            let range = start + at..start + at + word.len();
            if range.contains(&offset) || range.end == offset && offset == start + text.len() {
                return range;
            }
        }
        offset..offset
    }

    fn apply(&mut self, range: Range<usize>, text: &str) {
        self.text.replace_range(range, text);
        self.line_starts = line_starts(&self.text);
        self.version += 1;
    }

    fn push(&mut self, step: Step) {
        self.redo.clear();
        self.undo.push(step);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }

    /// Replaces `range` (or the marked text, or the selection) with `text`,
    /// the caret after it. One typed character joins the step before it.
    pub fn replace(&mut self, range: Option<Range<usize>>, text: &str) {
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let range = self.clamp(range.start)..self.clamp(range.end);
        let before = self.selection.clone();
        let removed = self.text[range.clone()].to_string();
        self.apply(range.clone(), text);
        let caret = range.start + text.len();
        self.selection = caret..caret;
        self.reversed = false;
        self.marked = None;
        if let Some(composition) = self.composition.take() {
            // The composition ends: one step from where it began.
            let inserted = self.text[composition.start..caret].to_string();
            self.push(Step {
                start: composition.start,
                removed: composition.removed,
                inserted,
                before: composition.before,
                after: caret..caret,
            });
            self.last_typing = None;
            return;
        }
        let now = Instant::now();
        let typing = removed.is_empty() && !text.contains('\n') && text.chars().count() == 1;
        let joins = typing
            && self
                .last_typing
                .is_some_and(|last| now.duration_since(last) < UNDO_GROUP)
            && self.redo.is_empty()
            && self
                .undo
                .last()
                .is_some_and(|step| step.start + step.inserted.len() == range.start);
        if joins {
            let step = self.undo.last_mut().expect("joined step");
            step.inserted.push_str(text);
            step.after = caret..caret;
        } else {
            self.push(Step {
                start: range.start,
                removed,
                inserted: text.to_string(),
                before,
                after: caret..caret,
            });
        }
        self.last_typing = typing.then_some(now);
    }

    /// Replaces `range` with `text` as a step of its own, then selects
    /// `selection` (a command's edit: undo puts the old selection back,
    /// redo this one).
    pub fn replace_selecting(
        &mut self,
        range: Range<usize>,
        text: &str,
        selection: Range<usize>,
        reversed: bool,
    ) {
        self.last_typing = None;
        self.replace(Some(range), text);
        self.last_typing = None;
        self.selection = self.clamp(selection.start)..self.clamp(selection.end);
        self.reversed = reversed && !self.selection.is_empty();
        if let Some(step) = self.undo.last_mut() {
            step.after = self.selection.clone();
        }
    }

    /// IME composition: the text stays marked until committed; the whole
    /// composition is one undo step.
    pub fn replace_and_mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) {
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let range = self.clamp(range.start)..self.clamp(range.end);
        if self.composition.is_none() {
            self.composition = Some(Composition {
                start: range.start,
                removed: self.text[range.clone()].to_string(),
                before: self.selection.clone(),
            });
        }
        self.apply(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selection = match selected {
            Some(selected) => range.start + selected.start..range.start + selected.end,
            None => range.start + text.len()..range.start + text.len(),
        };
        self.reversed = false;
    }

    /// The marked text is kept as typed (`unmark_text`).
    pub fn unmark(&mut self) {
        self.marked = None;
        if let Some(composition) = self.composition.take() {
            let caret = self.cursor();
            let inserted = self.text[composition.start..caret.max(composition.start)].to_string();
            self.push(Step {
                start: composition.start,
                removed: composition.removed,
                inserted,
                before: composition.before,
                after: caret..caret,
            });
        }
    }

    /// Deletes the selection, or from the caret to `to`.
    pub fn delete_to(&mut self, to: usize) {
        if self.selection.is_empty() {
            let cursor = self.cursor();
            if to == cursor {
                return;
            }
            self.replace(Some(cursor.min(to)..cursor.max(to)), "");
        } else {
            self.replace(None, "");
        }
        self.last_typing = None;
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        let end = step.start + step.inserted.len();
        self.apply(step.start..end, &step.removed);
        self.selection = step.before.clone();
        self.reversed = false;
        self.marked = None;
        self.last_typing = None;
        self.redo.push(step);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        let end = step.start + step.removed.len();
        self.apply(step.start..end, &step.inserted);
        self.selection = step.after.clone();
        self.reversed = false;
        self.last_typing = None;
        self.undo.push(step);
        true
    }

    /// Puts other text in, as one step (the disk's version taken).
    pub fn set_text(&mut self, text: &str) {
        let all = 0..self.text.len();
        self.replace(Some(all), text);
        self.go(0, false);
    }

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..self.clamp(offset)].encode_utf16().count()
    }

    pub fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (index, character) in self.text.char_indices() {
            if utf16 >= offset {
                return index;
            }
            utf16 += character.len_utf16();
        }
        self.text.len()
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

/// The screen column of each byte boundary of a line, tabs going to the
/// next stop (`tab-size`): `columns[i]` for the i-th char boundary, plus
/// the end.
pub fn display_columns(line: &str, tab: usize) -> Vec<(usize, usize)> {
    let tab = tab.max(1);
    let mut column = 0;
    let mut out = Vec::with_capacity(line.len() + 1);
    for (at, character) in line.char_indices() {
        out.push((at, column));
        column = if character == '\t' {
            (column / tab + 1) * tab
        } else {
            column + 1
        };
    }
    out.push((line.len(), column));
    out
}

/// The screen column of byte `offset` in `line`.
pub fn column_of(line: &str, offset: usize, tab: usize) -> usize {
    display_columns(line, tab)
        .into_iter()
        .take_while(|(at, _)| *at <= offset)
        .last()
        .map_or(0, |(_, column)| column)
}

/// The byte offset nearest screen `column` in `line` (rounded to the
/// closer side of a character).
pub fn offset_at_column(line: &str, column: f32, tab: usize) -> usize {
    let columns = display_columns(line, tab);
    let mut best = (0usize, f32::MAX);
    for (at, at_column) in columns {
        let distance = (at_column as f32 - column).abs();
        if distance < best.1 {
            best = (at, distance);
        }
    }
    best.0
}

/// The line drawn: tabs spelled as spaces to their stop, and where each
/// byte of the line lands in it.
pub fn expand_tabs(line: &str, tab: usize) -> (String, Vec<usize>) {
    let tab = tab.max(1);
    let mut out = String::with_capacity(line.len());
    let mut map = vec![0; line.len() + 1];
    let mut column = 0;
    for (at, character) in line.char_indices() {
        map[at..at + character.len_utf8()].fill(out.len());
        if character == '\t' {
            let next = (column / tab + 1) * tab;
            out.extend(std::iter::repeat_n(' ', next - column));
            column = next;
        } else {
            out.push(character);
            column += 1;
        }
    }
    map[line.len()] = out.len();
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_offsets_follow_the_text() {
        let buffer = EditBuffer::new("ab\r\ncd\n\nlast".into());
        assert_eq!(buffer.line_count(), 4);
        assert_eq!(buffer.line(0), "ab");
        assert_eq!(buffer.line(2), "");
        assert_eq!(buffer.line(3), "last");
        assert_eq!(buffer.line_of(5), 1);
        assert_eq!(buffer.line_start(3), 8);
        assert_eq!(buffer.line_end_of(9), 12);
    }

    #[test]
    fn typing_joins_one_step_and_undo_redo_put_it_back() {
        let mut buffer = EditBuffer::new("fn a() {}".into());
        buffer.go(8, false);
        for character in ["x", "y", "z"] {
            buffer.replace(None, character);
        }
        assert_eq!(buffer.text(), "fn a() {xyz}");
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "fn a() {}");
        assert_eq!(buffer.cursor(), 8);
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "fn a() {xyz}");
        assert_eq!(buffer.cursor(), 11);
        // A paste is its own step.
        buffer.replace(None, "\n  body\n");
        buffer.replace(None, "!");
        assert!(buffer.undo());
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "fn a() {xyz}");
    }

    #[test]
    fn a_composition_is_one_step() {
        let mut buffer = EditBuffer::new("a".into());
        buffer.go(1, false);
        buffer.replace_and_mark(None, "´", None);
        buffer.replace_and_mark(None, "é", None);
        buffer.replace(None, "é");
        assert_eq!(buffer.text(), "aé");
        assert!(buffer.marked.is_none());
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "a");
        assert!(!buffer.undo());
    }

    #[test]
    fn words_graphemes_and_deletes() {
        let mut buffer = EditBuffer::new("let total = 3;".into());
        assert_eq!(buffer.previous_word(10), 4);
        assert_eq!(buffer.next_word(4), 9);
        assert_eq!(buffer.word_at(6), 4..9);
        buffer.go(9, false);
        buffer.delete_to(buffer.previous_word(9));
        assert_eq!(buffer.text(), "let  = 3;");
        let emoji = EditBuffer::new("a👍🏽b".into());
        assert_eq!(emoji.next_boundary(1), 1 + "👍🏽".len());
    }

    #[test]
    fn selections_grow_both_ways() {
        let mut buffer = EditBuffer::new("abcdef".into());
        buffer.go(3, false);
        buffer.go(5, true);
        assert_eq!((buffer.selection.clone(), buffer.reversed), (3..5, false));
        buffer.go(1, true);
        assert_eq!((buffer.selection.clone(), buffer.reversed), (1..3, true));
        assert_eq!(buffer.selected_text(), "bc");
    }

    #[test]
    fn tabs_go_to_their_stop() {
        assert_eq!(column_of("\tab", 1, 4), 4);
        assert_eq!(column_of("a\tb", 2, 4), 4);
        assert_eq!(offset_at_column("\tab", 4.4, 4), 1);
        assert_eq!(offset_at_column("\tab", 1.0, 4), 0);
        let (text, map) = expand_tabs("a\tb", 4);
        assert_eq!(text, "a   b");
        assert_eq!(map, vec![0, 1, 4, 5]);
    }

    #[test]
    fn utf16_offsets_map_back() {
        let buffer = EditBuffer::new("ação 👍".into());
        let end = buffer.text().len();
        assert_eq!(buffer.offset_from_utf16(buffer.offset_to_utf16(end)), end);
        assert_eq!(buffer.offset_to_utf16("ação".len()), 4);
    }
}
