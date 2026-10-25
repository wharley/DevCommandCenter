//! A file's diff as `@pierre/diffs` shows it (`workspaceDiffViewOptions`):
//! every line with its numbers, the changed words of a changed line
//! (`lineDiffType: "word"`), three lines of context around each change, and
//! longer unchanged runs folded into a separator ("N unmodified lines") that
//! opens 20 lines at a time (`expansionLineCount`), unless the run is short
//! (`collapsedContextThreshold: 8`).

use std::ops::Range;

use similar::{ChangeTag, TextDiff};

/// Lines kept around a change.
pub const CONTEXT: usize = 3;
/// A hidden run this short shows whole instead.
pub const COLLAPSED_THRESHOLD: usize = 8;
/// Lines one click on a separator reveals.
pub const EXPANSION: usize = 20;
/// `maxLineDiffLength`: longer lines get no word emphasis.
pub const MAX_WORD_DIFF_LINE: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Deletion,
    Addition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// 1-based, in the original file (context and deletions).
    pub old: Option<usize>,
    /// 1-based, in the modified file (context and additions).
    pub new: Option<usize>,
    pub text: String,
    /// Changed words, in bytes of `text`.
    pub emphasis: Vec<Range<usize>>,
}

/// A row on screen: a line, or a separator for a folded run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Line(usize),
    Separator {
        gap: usize,
        hidden: usize,
        /// Where the run sits: before the first change, between, after the last.
        place: GapPlace,
    },
    /// Lines a patch leaves out between its hunks: told, not openable.
    Skipped {
        skip: usize,
        hidden: usize,
        place: GapPlace,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapPlace {
    First,
    Middle,
    Last,
}

/// An unchanged run folded away, and how much of it was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gap {
    /// Line indices `start..end` the run hides.
    pub start: usize,
    pub end: usize,
    /// Opened from its top and from its bottom.
    pub top: usize,
    pub bottom: usize,
    pub place: GapPlace,
}

impl Gap {
    pub fn hidden(&self) -> usize {
        (self.end - self.start).saturating_sub(self.top + self.bottom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    pub lines: Vec<DiffLine>,
    pub gaps: Vec<Gap>,
    /// A patch's left-out lines: (the line they come before, how many).
    pub skipped: Vec<(usize, usize)>,
    pub insertions: usize,
    pub deletions: usize,
}

fn strip_newline(line: &str) -> String {
    line.trim_end_matches(['\n', '\r']).to_string()
}

/// Words, whitespace runs and each punctuation mark on its own, as jsdiff's
/// word diff splits a line.
fn tokens(line: &str) -> Vec<&str> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Space,
        Mark,
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Mark
        }
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut current: Option<Class> = None;
    for (index, c) in line.char_indices() {
        let next = class(c);
        let split = match &current {
            None => false,
            Some(Class::Mark) => true,
            Some(previous) => *previous != next,
        };
        if split {
            out.push(&line[start..index]);
            start = index;
        }
        current = Some(next);
    }
    if start < line.len() {
        out.push(&line[start..]);
    }
    out
}

/// The changed words between a deleted and an added line.
fn word_emphasis(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    if old.len() > MAX_WORD_DIFF_LINE || new.len() > MAX_WORD_DIFF_LINE {
        return (Vec::new(), Vec::new());
    }
    let (old_tokens, new_tokens) = (tokens(old), tokens(new));
    let diff = TextDiff::from_slices(&old_tokens, &new_tokens);
    let (mut old_at, mut new_at) = (0, 0);
    let (mut old_ranges, mut new_ranges): (Vec<Range<usize>>, Vec<Range<usize>>) =
        (Vec::new(), Vec::new());
    let push = |ranges: &mut Vec<Range<usize>>, range: Range<usize>| {
        // Neighbouring changed words read as one run.
        match ranges.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => ranges.push(range),
        }
    };
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            ChangeTag::Equal => {
                old_at += len;
                new_at += len;
            }
            ChangeTag::Delete => {
                push(&mut old_ranges, old_at..old_at + len);
                old_at += len;
            }
            ChangeTag::Insert => {
                push(&mut new_ranges, new_at..new_at + len);
                new_at += len;
            }
        }
    }
    // A line rewritten whole has nothing worth emphasizing.
    let whole = |ranges: &[Range<usize>], text: &str| {
        ranges.len() == 1 && ranges[0].start == 0 && ranges[0].end >= text.trim_end().len()
    };
    if whole(&old_ranges, old) && whole(&new_ranges, new) {
        return (Vec::new(), Vec::new());
    }
    (old_ranges, new_ranges)
}

/// Pairs each block of deletions with the additions right after it, line
/// by line, for the word emphasis.
fn emphasize(lines: &mut [DiffLine]) {
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind != LineKind::Deletion {
            index += 1;
            continue;
        }
        let deleted_start = index;
        while index < lines.len() && lines[index].kind == LineKind::Deletion {
            index += 1;
        }
        let added_start = index;
        while index < lines.len() && lines[index].kind == LineKind::Addition {
            index += 1;
        }
        let pairs = (added_start - deleted_start).min(index - added_start);
        for offset in 0..pairs {
            let (old, new) = (deleted_start + offset, added_start + offset);
            let (old_ranges, new_ranges) = word_emphasis(&lines[old].text, &lines[new].text);
            lines[old].emphasis = old_ranges;
            lines[new].emphasis = new_ranges;
        }
    }
}

/// The unchanged runs too long to show: everything farther than `CONTEXT`
/// from a change, when more than `COLLAPSED_THRESHOLD` lines.
fn fold(lines: &[DiffLine]) -> Vec<Gap> {
    let changed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.kind != LineKind::Context)
        .map(|(index, _)| index)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }
    let mut visible = vec![false; lines.len()];
    for &index in &changed {
        let start = index.saturating_sub(CONTEXT);
        let end = (index + CONTEXT + 1).min(lines.len());
        visible[start..end].iter_mut().for_each(|line| *line = true);
    }
    let mut gaps = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if visible[index] {
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && !visible[index] {
            index += 1;
        }
        if index - start > COLLAPSED_THRESHOLD {
            let place = if start == 0 {
                GapPlace::First
            } else if index == lines.len() {
                GapPlace::Last
            } else {
                GapPlace::Middle
            };
            gaps.push(Gap {
                start,
                end: index,
                top: 0,
                bottom: 0,
                place,
            });
        }
    }
    gaps
}

impl FileDiff {
    pub fn new(original: &str, modified: &str) -> Self {
        let diff = TextDiff::from_lines(original, modified);
        let (mut old, mut new) = (0, 0);
        let mut lines = Vec::new();
        let (mut insertions, mut deletions) = (0, 0);
        for change in diff.iter_all_changes() {
            let text = strip_newline(change.value());
            let line = match change.tag() {
                ChangeTag::Equal => {
                    old += 1;
                    new += 1;
                    DiffLine {
                        kind: LineKind::Context,
                        old: Some(old),
                        new: Some(new),
                        text,
                        emphasis: Vec::new(),
                    }
                }
                ChangeTag::Delete => {
                    old += 1;
                    deletions += 1;
                    DiffLine {
                        kind: LineKind::Deletion,
                        old: Some(old),
                        new: None,
                        text,
                        emphasis: Vec::new(),
                    }
                }
                ChangeTag::Insert => {
                    new += 1;
                    insertions += 1;
                    DiffLine {
                        kind: LineKind::Addition,
                        old: None,
                        new: Some(new),
                        text,
                        emphasis: Vec::new(),
                    }
                }
            };
            lines.push(line);
        }
        emphasize(&mut lines);
        let gaps = fold(&lines);
        Self {
            lines,
            gaps,
            skipped: Vec::new(),
            insertions,
            deletions,
        }
    }

    /// A captured unified patch (`parsePatchFiles`): only its hunks' lines,
    /// with the lines it leaves out before and between them told.
    pub fn from_patch(patch: &str) -> Self {
        let (mut old, mut new) = (0usize, 0usize);
        let mut lines = Vec::new();
        let mut skipped = Vec::new();
        let (mut insertions, mut deletions) = (0, 0);
        let mut in_hunk = false;
        for raw in patch.lines() {
            if let Some(header) = raw.strip_prefix("@@ ") {
                let Some((old_start, new_start)) = hunk_starts(header) else {
                    in_hunk = false;
                    continue;
                };
                let left_out = old_start.saturating_sub(old + 1);
                if left_out > 0 {
                    skipped.push((lines.len(), left_out));
                }
                old = old_start.saturating_sub(1);
                new = new_start.saturating_sub(1);
                in_hunk = true;
                continue;
            }
            if !in_hunk {
                continue;
            }
            let (kind, text) = match raw.as_bytes().first() {
                Some(b' ') => (LineKind::Context, &raw[1..]),
                Some(b'-') => (LineKind::Deletion, &raw[1..]),
                Some(b'+') => (LineKind::Addition, &raw[1..]),
                // "\ No newline at end of file"
                Some(b'\\') => continue,
                None => (LineKind::Context, ""),
                _ => {
                    in_hunk = false;
                    continue;
                }
            };
            let (old_number, new_number) = match kind {
                LineKind::Context => {
                    old += 1;
                    new += 1;
                    (Some(old), Some(new))
                }
                LineKind::Deletion => {
                    old += 1;
                    deletions += 1;
                    (Some(old), None)
                }
                LineKind::Addition => {
                    new += 1;
                    insertions += 1;
                    (None, Some(new))
                }
            };
            lines.push(DiffLine {
                kind,
                old: old_number,
                new: new_number,
                text: text.to_string(),
                emphasis: Vec::new(),
            });
        }
        emphasize(&mut lines);
        Self {
            lines,
            gaps: Vec::new(),
            skipped,
            insertions,
            deletions,
        }
    }

    /// The rows on screen with the runs opened so far.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.lines.len());
        let mut index = 0;
        let mut gaps = self.gaps.iter().enumerate().peekable();
        let mut skipped = self.skipped.iter().enumerate().peekable();
        while index < self.lines.len() {
            if let Some((skip, (_, hidden))) = skipped.next_if(|(_, (at, _))| *at == index) {
                let place = if index == 0 {
                    GapPlace::First
                } else {
                    GapPlace::Middle
                };
                rows.push(Row::Skipped {
                    skip,
                    hidden: *hidden,
                    place,
                });
            }
            match gaps.peek() {
                Some((gap_index, gap)) if gap.start == index => {
                    let (gap_index, gap) = (*gap_index, **gap);
                    gaps.next();
                    rows.extend((gap.start..gap.start + gap.top).map(Row::Line));
                    let hidden = gap.hidden();
                    if hidden > 0 {
                        rows.push(Row::Separator {
                            gap: gap_index,
                            hidden,
                            place: gap.place,
                        });
                    }
                    rows.extend(
                        (gap.end - gap.bottom.min(gap.end - gap.start - gap.top)..gap.end)
                            .map(Row::Line),
                    );
                    index = gap.end;
                }
                _ => {
                    rows.push(Row::Line(index));
                    index += 1;
                }
            }
        }
        rows
    }

    /// One click on a separator: 20 more lines next to the changes (both
    /// sides for a run between two), or the whole run.
    pub fn expand(&mut self, gap: usize, all: bool) {
        let Some(gap) = self.gaps.get_mut(gap) else {
            return;
        };
        let hidden = gap.hidden();
        if all || hidden <= EXPANSION {
            gap.top += hidden;
            return;
        }
        match gap.place {
            // Before the first change the lines nearest it are at the bottom.
            GapPlace::First => gap.bottom += EXPANSION,
            GapPlace::Last => gap.top += EXPANSION,
            GapPlace::Middle => {
                let top = EXPANSION.min(hidden);
                gap.top += top;
                gap.bottom += EXPANSION.min(hidden - top);
            }
        }
    }
}

/// `-12,3 +14,5 @@ …`: where a hunk starts on each side.
fn hunk_starts(header: &str) -> Option<(usize, usize)> {
    let mut parts = header.split_whitespace();
    let start = |part: Option<&str>, sign: char| -> Option<usize> {
        part?.strip_prefix(sign)?.split(',').next()?.parse().ok()
    };
    Some((start(parts.next(), '-')?, start(parts.next(), '+')?))
}

/// `"N unmodified lines"`, as the desktop's diff writes it.
pub fn unmodified_lines(count: usize) -> String {
    format!(
        "{count} unmodified line{}",
        if count == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,3 +10,3 @@ fn main() {
 let a = 1;
-let b = 2;
+let b = 3;
 let c = 4;
@@ -40,2 +40,3 @@
 end();
+more();
 done();
\\ No newline at end of file
";

    #[test]
    fn a_patch_keeps_its_hunks_and_tells_what_it_leaves_out() {
        let diff = FileDiff::from_patch(PATCH);
        assert_eq!((diff.insertions, diff.deletions), (2, 1));
        let numbers: Vec<(Option<usize>, Option<usize>)> =
            diff.lines.iter().map(|line| (line.old, line.new)).collect();
        assert_eq!(
            numbers,
            vec![
                (Some(10), Some(10)),
                (Some(11), None),
                (None, Some(11)),
                (Some(12), Some(12)),
                (Some(40), Some(40)),
                (None, Some(41)),
                (Some(41), Some(42)),
            ]
        );
        // 9 lines before the first hunk, 27 between the two.
        let rows = diff.rows();
        assert_eq!(
            rows[0],
            Row::Skipped {
                skip: 0,
                hidden: 9,
                place: GapPlace::First
            }
        );
        assert!(rows.contains(&Row::Skipped {
            skip: 1,
            hidden: 27,
            place: GapPlace::Middle
        }));
        assert_eq!(rows.len(), diff.lines.len() + 2);
        // The changed words of "let b = 2" → "let b = 3".
        assert!(!diff.lines[1].emphasis.is_empty());
    }

    fn numbered(count: usize) -> String {
        (1..=count).map(|n| format!("line {n}\n")).collect()
    }

    #[test]
    fn lines_carry_their_numbers_and_counts() {
        let diff = FileDiff::new("a\nb\nc\n", "a\nB\nc\nd\n");
        let kinds: Vec<(LineKind, Option<usize>, Option<usize>)> = diff
            .lines
            .iter()
            .map(|line| (line.kind, line.old, line.new))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (LineKind::Context, Some(1), Some(1)),
                (LineKind::Deletion, Some(2), None),
                (LineKind::Addition, None, Some(2)),
                (LineKind::Context, Some(3), Some(3)),
                (LineKind::Addition, None, Some(4)),
            ]
        );
        assert_eq!((diff.insertions, diff.deletions), (2, 1));
    }

    #[test]
    fn a_changed_word_is_emphasized_not_the_whole_line() {
        let diff = FileDiff::new("let total = price * 2;\n", "let total = price * 3;\n");
        let deleted = &diff.lines[0];
        let added = &diff.lines[1];
        assert_eq!(&deleted.text[deleted.emphasis[0].clone()], "2");
        assert_eq!(&added.text[added.emphasis[0].clone()], "3");
        let rewritten = FileDiff::new("alpha\n", "omega\n");
        assert!(rewritten.lines.iter().all(|line| line.emphasis.is_empty()));
    }

    #[test]
    fn long_unchanged_runs_fold_and_open_twenty_at_a_time() {
        let original = numbered(100);
        let modified = original.replace("line 50\n", "line fifty\n");
        let mut diff = FileDiff::new(&original, &modified);
        let rows = diff.rows();
        let separators: Vec<(usize, GapPlace)> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Separator { hidden, place, .. } => Some((*hidden, *place)),
                _ => None,
            })
            .collect();
        assert_eq!(
            separators,
            vec![(46, GapPlace::First), (47, GapPlace::Last)]
        );
        assert_eq!(rows.len(), 2 + 3 + 2 + 3);
        diff.expand(0, false);
        assert_eq!(diff.gaps[0].hidden(), 26);
        // The opened lines sit next to the change, the separator stays above.
        let rows = diff.rows();
        assert!(matches!(rows[0], Row::Separator { hidden: 26, .. }));
        assert_eq!(rows[1], Row::Line(26));
        diff.expand(1, true);
        assert_eq!(diff.gaps[1].hidden(), 0);
        assert!(
            !diff
                .rows()
                .iter()
                .any(|row| matches!(row, Row::Separator { gap: 1, .. }))
        );
        assert_eq!(unmodified_lines(1), "1 unmodified line");
    }

    #[test]
    fn a_short_run_shows_whole() {
        let original = numbered(12);
        let modified = original
            .replace("line 1\n", "first\n")
            .replace("line 12\n", "last\n");
        let diff = FileDiff::new(&original, &modified);
        assert!(
            diff.gaps.is_empty(),
            "4 unchanged lines between the context"
        );
        assert!(diff.rows().iter().all(|row| matches!(row, Row::Line(_))));
    }
}
