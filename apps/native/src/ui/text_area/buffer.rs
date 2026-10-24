//! The text a multi-line field edits: content, selection (in UTF-8 byte
//! offsets on grapheme boundaries), the IME's marked range and undo history.
//! No GPUI here; the view maps keys and layout onto these operations.

use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

/// Typing within this pause joins the previous undo step.
const UNDO_GROUP: Duration = Duration::from_millis(600);
const UNDO_LIMIT: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    text: String,
    selection: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Typing,
    Other,
}

#[derive(Debug, Default)]
pub struct Buffer {
    text: String,
    pub selection: Range<usize>,
    pub reversed: bool,
    pub marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<(EditKind, Instant)>,
}

impl Buffer {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replaces everything (a draft restored, the field cleared after send),
    /// with the caret at the end and no history.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.selection = self.text.len()..self.text.len();
        self.reversed = false;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
    }

    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    /// The other end of the selection.
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

    pub fn move_to(&mut self, offset: usize) {
        let offset = self.clamp(offset);
        self.selection = offset..offset;
        self.reversed = false;
    }

    pub fn select_to(&mut self, offset: usize) {
        let offset = self.clamp(offset);
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

    /// Moves the caret, or extends the selection when `extend`.
    pub fn go(&mut self, offset: usize, extend: bool) {
        if extend {
            self.select_to(offset);
        } else {
            self.move_to(offset);
        }
    }

    fn clamp(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    pub fn previous_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    pub fn next_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.text.len())
    }

    /// Start of the word before `offset` (macOS option-left).
    pub fn previous_word(&self, offset: usize) -> usize {
        self.text
            .unicode_word_indices()
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    /// End of the word after `offset` (macOS option-right).
    pub fn next_word(&self, offset: usize) -> usize {
        self.text
            .unicode_word_indices()
            .map(|(index, word)| index + word.len())
            .find(|end| *end > offset)
            .unwrap_or(self.text.len())
    }

    /// Start of the hard line `offset` is on.
    pub fn line_start(&self, offset: usize) -> usize {
        self.text[..self.clamp(offset)]
            .rfind('\n')
            .map_or(0, |index| index + 1)
    }

    pub fn line_end(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |index| offset + index)
    }

    /// The word around `offset`, for a double click.
    pub fn word_at(&self, offset: usize) -> Range<usize> {
        self.text
            .unicode_word_indices()
            .map(|(index, word)| index..index + word.len())
            .find(|range| range.start <= offset && offset <= range.end)
            .unwrap_or(offset..offset)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            selection: self.selection.clone(),
        }
    }

    fn record(&mut self, kind: EditKind, now: Instant) {
        let joins = matches!(self.last_edit, Some((EditKind::Typing, at)) if kind == EditKind::Typing && now.duration_since(at) < UNDO_GROUP);
        if !joins {
            self.undo.push(self.snapshot());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some((kind, now));
    }

    /// Replaces `range` (or the marked text, or the selection) with `text`,
    /// leaving the caret after it. Typed text joins one undo step.
    pub fn replace(&mut self, range: Option<Range<usize>>, text: &str) {
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let range = self.clamp(range.start)..self.clamp(range.end);
        let typing = !text.contains('\n') && text.chars().count() <= 1;
        self.record(
            if typing {
                EditKind::Typing
            } else {
                EditKind::Other
            },
            Instant::now(),
        );
        self.text.replace_range(range.clone(), text);
        let caret = range.start + text.len();
        self.selection = caret..caret;
        self.reversed = false;
        self.marked = None;
    }

    /// IME composition: the text stays marked until committed.
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
        if self.marked.is_none() {
            self.record(EditKind::Other, Instant::now());
        }
        self.text.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selection = match selected {
            Some(selected) => range.start + selected.start..range.start + selected.end,
            None => range.start + text.len()..range.start + text.len(),
        };
        self.reversed = false;
    }

    /// Deletes the selection, or from the caret to `to` when nothing is selected.
    pub fn delete_to(&mut self, to: usize) {
        if self.selection.is_empty() {
            let cursor = self.cursor();
            if to == cursor {
                return;
            }
            let range = cursor.min(to)..cursor.max(to);
            self.replace(Some(range), "");
        } else {
            self.replace(None, "");
        }
        self.last_edit = Some((EditKind::Other, Instant::now()));
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(next);
        true
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.selection = snapshot.selection;
        self.reversed = false;
        self.marked = None;
        self.last_edit = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> Buffer {
        let mut buffer = Buffer::default();
        buffer.set_text(text);
        buffer
    }

    #[test]
    fn edits_lines_and_words() {
        let mut b = buffer("olá mundo\nsegunda linha");
        assert_eq!(b.cursor(), b.text().len());
        b.move_to(b.line_start(b.cursor()));
        assert_eq!(&b.text()[b.cursor()..], "segunda linha");
        assert_eq!(b.line_end(0), "olá mundo".len());
        assert_eq!(b.previous_word("olá mundo".len()), "olá ".len());
        assert_eq!(b.next_word(0), "olá".len());
        b.move_to(0);
        b.select_to(b.next_word(0));
        assert_eq!(b.selected_text(), "olá");
        b.replace(None, "oi");
        assert_eq!(b.text(), "oi mundo\nsegunda linha");
        assert_eq!(b.word_at(4), 3..8);
    }

    #[test]
    fn selection_flips_around_the_anchor() {
        let mut b = buffer("abcdef");
        b.move_to(3);
        b.select_to(1);
        assert_eq!(
            (b.selection.clone(), b.reversed, b.cursor()),
            (1..3, true, 1)
        );
        b.select_to(5);
        assert_eq!((b.selection.clone(), b.reversed), (3..5, false));
    }

    #[test]
    fn typing_undoes_in_one_step_and_redo_returns() {
        let mut b = buffer("");
        for c in ["o", "i", " ", "x"] {
            b.replace(None, c);
        }
        b.replace(None, "\n");
        assert_eq!(b.text(), "oi x\n");
        assert!(b.undo());
        assert_eq!(b.text(), "oi x");
        assert!(b.undo());
        assert_eq!(b.text(), "");
        assert!(b.redo());
        assert_eq!(b.text(), "oi x");
    }

    #[test]
    fn deletes_graphemes_and_ranges() {
        let mut b = buffer("ação👍");
        let before = b.previous_boundary(b.cursor());
        b.delete_to(before);
        assert_eq!(b.text(), "ação");
        b.move_to(0);
        b.delete_to(b.next_word(0));
        assert_eq!(b.text(), "");
    }

    #[test]
    fn utf16_offsets_round_trip() {
        let b = buffer("a👍b");
        assert_eq!(b.offset_to_utf16(b.text().len()), 4);
        assert_eq!(b.offset_from_utf16(3), "a👍".len());
    }

    #[test]
    fn composition_marks_then_commits() {
        let mut b = buffer("x");
        b.replace_and_mark(None, "´", None);
        assert_eq!(b.marked, Some(1..3));
        b.replace(None, "é");
        assert_eq!((b.text(), b.marked.clone()), ("xé", None));
    }
}
