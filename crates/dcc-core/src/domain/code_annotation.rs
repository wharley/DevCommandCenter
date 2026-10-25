//! A stretch of code sent to the agent from a diff or a file (the desktop's
//! `DiffAnnotationRequest`): which file, which side of the change, which
//! lines and their text, and how it is written into the prompt
//! (`buildAnnotationContextBlock` / `buildAnnotationContent`).

/// The side of a change a selection was made on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationSide {
    /// The removed (old) side.
    Original,
    /// The new side, or a file as it is.
    Modified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeAnnotation {
    pub path: String,
    pub side: AnnotationSide,
    /// One-based, inclusive.
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
}

/// A column of the diff a line was picked in (`SelectedLineRange.side`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffColumn {
    Deletions,
    Additions,
}

/// Lines picked in a diff, from where the gesture began to where it ended
/// (`SelectedLineRange`): either end can be the higher one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffLineRange {
    pub start: usize,
    pub end: usize,
    pub side: DiffColumn,
    pub end_side: DiffColumn,
}

/// `L3`, or `L3–5` for several lines.
pub fn line_label(start_line: usize, end_line: usize) -> String {
    if start_line == end_line {
        format!("L{start_line}")
    } else {
        format!("L{start_line}–{end_line}")
    }
}

/// The text of one-based lines `start..=end` (`snippetForOneBasedLines`).
pub fn snippet_for_lines(text: &str, start_line: usize, end_line: usize) -> String {
    let start = start_line.min(end_line).max(1);
    let end = start.max(start_line.max(end_line));
    text.split('\n')
        .skip(start - 1)
        .take(end - start + 1)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `annotationPayloadFromPierreRange`: a pick that crossed from one column
/// to the other keeps the side where it ended.
pub fn from_diff_range(
    path: &str,
    range: DiffLineRange,
    original: &str,
    modified: &str,
) -> CodeAnnotation {
    let side = range.end_side;
    let raw_start = if range.side == side {
        range.start
    } else {
        range.end
    };
    let raw_end = range.end;
    let start_line = raw_start.min(raw_end).max(1);
    let end_line = start_line.max(raw_start.max(raw_end));
    let (side, source) = match side {
        DiffColumn::Deletions => (AnnotationSide::Original, original),
        DiffColumn::Additions => (AnnotationSide::Modified, modified),
    };
    CodeAnnotation {
        path: path.to_string(),
        side,
        start_line,
        end_line,
        snippet: snippet_for_lines(source, start_line, end_line),
    }
}

/// `buildAnnotationContextBlock`: where the code is and the code itself; a
/// removed stretch says so, so the agent doesn't edit lines that are gone.
pub fn context_block(annotation: &CodeAnnotation) -> String {
    let lines = if annotation.start_line == annotation.end_line {
        format!("linha {}", annotation.start_line)
    } else {
        format!("linhas {}–{}", annotation.start_line, annotation.end_line)
    };
    let side = match annotation.side {
        AnnotationSide::Original => ", código removido nesta mudança",
        AnnotationSide::Modified => "",
    };
    [
        format!("Sobre `{}` ({lines}{side}):", annotation.path),
        String::new(),
        "```".to_string(),
        annotation.snippet.clone(),
        "```".to_string(),
    ]
    .join("\n")
}

/// `buildAnnotationContent`: the instruction, if any, before the block.
pub fn content(annotation: &CodeAnnotation, instruction: &str) -> String {
    let block = context_block(annotation);
    let instruction = instruction.trim();
    if instruction.is_empty() {
        block
    } else {
        format!("{instruction}\n\n{block}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: usize, end: usize, side: DiffColumn, end_side: DiffColumn) -> DiffLineRange {
        DiffLineRange {
            start,
            end,
            side,
            end_side,
        }
    }

    #[test]
    fn a_reversed_pick_on_the_new_side_takes_its_lines() {
        let annotation = from_diff_range(
            "a.rs",
            range(3, 2, DiffColumn::Additions, DiffColumn::Additions),
            "old one\nold two\nold three",
            "new one\nnew two\nnew three",
        );
        assert_eq!(annotation.side, AnnotationSide::Modified);
        assert_eq!((annotation.start_line, annotation.end_line), (2, 3));
        assert_eq!(annotation.snippet, "new two\nnew three");
    }

    #[test]
    fn the_removed_side_reads_the_old_text() {
        let annotation = from_diff_range(
            "a.rs",
            range(1, 2, DiffColumn::Deletions, DiffColumn::Deletions),
            "old one\nold two\nold three",
            "new one\nnew two",
        );
        assert_eq!(annotation.side, AnnotationSide::Original);
        assert_eq!(annotation.snippet, "old one\nold two");
    }

    #[test]
    fn a_pick_across_the_columns_keeps_the_side_it_ended_on() {
        let annotation = from_diff_range(
            "a.rs",
            range(2, 4, DiffColumn::Deletions, DiffColumn::Additions),
            "a\nb\nc\nd",
            "A\nB\nC\nD",
        );
        assert_eq!(annotation.side, AnnotationSide::Modified);
        assert_eq!((annotation.start_line, annotation.end_line), (4, 4));
        assert_eq!(annotation.snippet, "D");
    }

    #[test]
    fn the_prompt_names_the_file_lines_and_side() {
        let mut annotation = CodeAnnotation {
            path: "src/lib.rs".into(),
            side: AnnotationSide::Modified,
            start_line: 4,
            end_line: 4,
            snippet: "let x = 1;".into(),
        };
        assert_eq!(
            content(&annotation, "  Renomeie  "),
            "Renomeie\n\nSobre `src/lib.rs` (linha 4):\n\n```\nlet x = 1;\n```"
        );
        annotation.side = AnnotationSide::Original;
        annotation.end_line = 6;
        assert_eq!(
            context_block(&annotation).lines().next(),
            Some("Sobre `src/lib.rs` (linhas 4–6, código removido nesta mudança):")
        );
        assert_eq!(content(&annotation, " "), context_block(&annotation));
        assert_eq!(line_label(4, 4), "L4");
        assert_eq!(line_label(4, 6), "L4–6");
    }

    #[test]
    fn lines_are_one_based_and_ordered() {
        assert_eq!(snippet_for_lines("a\nb\nc", 3, 2), "b\nc");
        assert_eq!(snippet_for_lines("a\nb\nc", 0, 1), "a");
        assert_eq!(snippet_for_lines("a\nb", 2, 9), "b");
    }
}
