//! The text a multi-line field edits: content, selection (in UTF-8 byte
//! offsets on grapheme boundaries), the IME's marked range and undo history.
//! An inline chip (an attachment, a mentioned file) is one `CHIP` character
//! in the text; its data sits in `chips`, the n-th chip for the n-th `CHIP`,
//! and travels with the text through undo. No GPUI here.

use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

/// Typing within this pause joins the previous undo step.
const UNDO_GROUP: Duration = Duration::from_millis(600);
const UNDO_LIMIT: usize = 200;

/// The character that stands for a chip in the text.
pub const CHIP: char = '\u{FFFC}';

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot<C> {
    text: String,
    selection: Range<usize>,
    chips: Vec<C>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Typing,
    Other,
}

#[derive(Debug)]
pub struct Buffer<C = ()> {
    text: String,
    chips: Vec<C>,
    pub selection: Range<usize>,
    pub reversed: bool,
    pub marked: Option<Range<usize>>,
    undo: Vec<Snapshot<C>>,
    redo: Vec<Snapshot<C>>,
    last_edit: Option<(EditKind, Instant)>,
}

impl<C> Default for Buffer<C> {
    fn default() -> Self {
        Self {
            text: String::new(),
            chips: Vec::new(),
            selection: 0..0,
            reversed: false,
            marked: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
        }
    }
}

impl<C: Clone> Buffer<C> {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replaces everything (a draft restored, the field cleared after send),
    /// with the caret at the end and no history.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.replace(CHIP, "");
        self.chips.clear();
        self.selection = self.text.len()..self.text.len();
        self.reversed = false;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
    }

    /// Drops undo and redo (after filling the field programmatically).
    pub fn forget_history(&mut self) {
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

    fn snapshot(&self) -> Snapshot<C> {
        Snapshot {
            text: self.text.clone(),
            selection: self.selection.clone(),
            chips: self.chips.clone(),
        }
    }

    pub fn chips(&self) -> &[C] {
        &self.chips
    }

    /// The chips in text order with the byte offset of each.
    pub fn chip_offsets(&self) -> impl Iterator<Item = (usize, &C)> {
        self.text
            .char_indices()
            .filter(|(_, c)| *c == CHIP)
            .map(|(index, _)| index)
            .zip(self.chips.iter())
    }

    fn chips_before(&self, offset: usize) -> usize {
        self.text[..offset].matches(CHIP).count()
    }

    /// Drops the chips whose characters `range` covers.
    fn remove_chips_in(&mut self, range: &Range<usize>) {
        let first = self.chips_before(range.start);
        let count = self.text[range.clone()].matches(CHIP).count();
        self.chips
            .drain(first..(first + count).min(self.chips.len()));
    }

    /// Puts a chip at the caret (replacing the selection), as one edit.
    pub fn insert_chip(&mut self, chip: C) {
        let range = self
            .marked
            .clone()
            .unwrap_or_else(|| self.selection.clone());
        self.record(EditKind::Other, Instant::now());
        self.remove_chips_in(&range);
        let index = self.chips_before(range.start);
        self.chips.insert(index, chip);
        let mut buffer = [0u8; 4];
        self.text
            .replace_range(range.clone(), CHIP.encode_utf8(&mut buffer));
        let caret = range.start + CHIP.len_utf8();
        self.selection = caret..caret;
        self.reversed = false;
        self.marked = None;
        self.last_edit = Some((EditKind::Other, Instant::now()));
    }

    /// Removes the chip that starts at `offset`.
    pub fn remove_chip_at(&mut self, offset: usize) {
        if self.text[offset..].starts_with(CHIP) {
            self.replace(Some(offset..offset + CHIP.len_utf8()), "");
            self.last_edit = Some((EditKind::Other, Instant::now()));
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
        // Chips only enter through `insert_chip`.
        let cleaned = text.replace(CHIP, "");
        let text = cleaned.as_str();
        let typing = !text.contains('\n') && text.chars().count() <= 1;
        self.record(
            if typing {
                EditKind::Typing
            } else {
                EditKind::Other
            },
            Instant::now(),
        );
        self.remove_chips_in(&range);
        self.text.replace_range(range.clone(), text);
        let caret = range.start + text.len();
        self.selection = caret..caret;
        self.reversed = false;
        self.marked = None;
    }

    /// Several edits as one undo step (a list marker, emphasis marks). They
    /// must not overlap or cover a chip; `selection` is set after them.
    pub fn splice(&mut self, mut edits: Vec<(Range<usize>, String)>, selection: Range<usize>) {
        self.record(EditKind::Other, Instant::now());
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        for (range, text) in edits {
            let range = self.clamp(range.start)..self.clamp(range.end);
            debug_assert!(!self.text[range.clone()].contains(CHIP));
            self.text.replace_range(range, &text.replace(CHIP, ""));
        }
        self.selection = self.clamp(selection.start)..self.clamp(selection.end);
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
        let text = text.replace(CHIP, "");
        let text = text.as_str();
        self.remove_chips_in(&range);
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

    fn restore(&mut self, snapshot: Snapshot<C>) {
        self.text = snapshot.text;
        self.chips = snapshot.chips;
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

    fn buffer(text: &str) -> Buffer<&'static str> {
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
    fn chips_move_with_the_text_and_undo() {
        let mut b = buffer("ver  agora");
        b.move_to(4);
        b.insert_chip("a.rs");
        b.insert_chip("b.png");
        assert_eq!(b.chips(), &["a.rs", "b.png"]);
        b.move_to(0);
        b.insert_chip("first");
        assert_eq!(b.chips(), &["first", "a.rs", "b.png"]);
        // Backspace over a chip removes it whole.
        let after_a = b.chip_offsets().nth(1).unwrap().0 + CHIP.len_utf8();
        b.move_to(after_a);
        b.delete_to(b.previous_boundary(after_a));
        assert_eq!(b.chips(), &["first", "b.png"]);
        assert!(b.undo());
        assert_eq!(b.chips(), &["first", "a.rs", "b.png"]);
        // Selecting across chips and typing drops them.
        b.select_all();
        b.replace(None, "x");
        assert!(b.chips().is_empty());
        b.replace(None, "\u{FFFC}");
        assert_eq!(b.text(), "x");
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
