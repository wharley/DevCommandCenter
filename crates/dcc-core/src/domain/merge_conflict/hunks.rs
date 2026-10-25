//! Conflict blocks in a file (`merge-conflict-hunks.ts`): `<<<<<<<`, an
//! optional diff3 `|||||||` base, `=======`, `>>>>>>>`, with their labels,
//! and a block replaced by one side, the other, or both.

/// `MergeConflictResolution`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictResolution {
    Current,
    Incoming,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictHunk {
    /// `${startOffset}:${endOffset}`
    pub id: String,
    /// Byte offsets of the whole block, markers included.
    pub start_offset: usize,
    pub end_offset: usize,
    /// One-based lines of the start and end markers.
    pub start_line: usize,
    pub end_line: usize,
    pub current_label: Option<String>,
    pub incoming_label: Option<String>,
    pub current_text: String,
    pub base_text: Option<String>,
    pub incoming_text: String,
    pub line_ending: String,
}

struct SourceLine<'a> {
    start: usize,
    end: usize,
    body: &'a str,
    line_ending: &'a str,
}

/// Lines with their own ending (`\n`, `\r\n` or a lone `\r`).
fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let mut cursor = start;
        while cursor < bytes.len() && bytes[cursor] != b'\n' && bytes[cursor] != b'\r' {
            cursor += 1;
        }
        let body_end = cursor;
        if cursor + 1 < bytes.len() && bytes[cursor] == b'\r' && bytes[cursor + 1] == b'\n' {
            cursor += 2;
        } else if cursor < bytes.len() {
            cursor += 1;
        }
        lines.push(SourceLine {
            start,
            end: cursor,
            body: &source[start..body_end],
            line_ending: &source[body_end..cursor],
        });
        start = cursor;
    }
    lines
}

/// Seven or more `mark`s, then the end or (when labelled) a space and more.
fn is_marker(body: &str, mark: char, labelled: bool) -> bool {
    let run = body.chars().take_while(|c| *c == mark).count();
    if run < 7 {
        return false;
    }
    let rest = &body[run..];
    rest.is_empty() || (labelled && rest.starts_with(' '))
}

fn is_start(body: &str) -> bool {
    is_marker(body, '<', true)
}
fn is_base(body: &str) -> bool {
    is_marker(body, '|', true)
}
fn is_separator(body: &str) -> bool {
    is_marker(body, '=', false)
}
fn is_end(body: &str) -> bool {
    is_marker(body, '>', true)
}

fn marker_label(body: &str) -> Option<String> {
    let space = body.find(' ')?;
    let label = body[space + 1..].trim();
    (!label.is_empty()).then(|| label.to_string())
}

fn slice_lines(source: &str, lines: &[SourceLine], start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    source[lines[start].start..lines[end - 1].end].to_string()
}

/// `parseMergeConflictHunks`
pub fn parse_conflict_hunks(source: &str) -> Vec<ConflictHunk> {
    let lines = source_lines(source);
    let mut hunks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let start_line = &lines[index];
        if !is_start(start_line.body) {
            index += 1;
            continue;
        }
        let current_start = index + 1;
        let mut cursor = current_start;
        while cursor < lines.len()
            && !is_base(lines[cursor].body)
            && !is_separator(lines[cursor].body)
        {
            cursor += 1;
        }
        if cursor >= lines.len() {
            index += 1;
            continue;
        }
        let current_end = cursor;
        let mut base = None;
        if is_base(lines[cursor].body) {
            let base_start = cursor + 1;
            cursor = base_start;
            while cursor < lines.len() && !is_separator(lines[cursor].body) {
                cursor += 1;
            }
            if cursor >= lines.len() {
                index += 1;
                continue;
            }
            base = Some((base_start, cursor));
        }
        let incoming_start = cursor + 1;
        cursor = incoming_start;
        while cursor < lines.len() && !is_end(lines[cursor].body) {
            cursor += 1;
        }
        if cursor >= lines.len() {
            index += 1;
            continue;
        }
        let end_marker = &lines[cursor];
        hunks.push(ConflictHunk {
            id: format!("{}:{}", start_line.start, end_marker.end),
            start_offset: start_line.start,
            end_offset: end_marker.end,
            start_line: index + 1,
            end_line: cursor + 1,
            current_label: marker_label(start_line.body),
            incoming_label: marker_label(end_marker.body),
            current_text: slice_lines(source, &lines, current_start, current_end),
            base_text: base.map(|(start, end)| slice_lines(source, &lines, start, end)),
            incoming_text: slice_lines(source, &lines, incoming_start, cursor),
            line_ending: if start_line.line_ending.is_empty() {
                "\n".to_string()
            } else {
                start_line.line_ending.to_string()
            },
        });
        index = cursor + 1;
    }
    hunks
}

/// `hasMergeConflictMarkerFragments`: any marker line left at all.
pub fn has_conflict_marker_fragments(source: &str) -> bool {
    source_lines(source).iter().any(|line| {
        is_start(line.body) || is_base(line.body) || is_separator(line.body) || is_end(line.body)
    })
}

fn combine_both(hunk: &ConflictHunk) -> String {
    if hunk.current_text.is_empty() {
        return hunk.incoming_text.clone();
    }
    if hunk.incoming_text.is_empty() {
        return hunk.current_text.clone();
    }
    let separator =
        !hunk.current_text.ends_with(['\n', '\r']) && !hunk.incoming_text.starts_with(['\n', '\r']);
    format!(
        "{}{}{}",
        hunk.current_text,
        if separator {
            hunk.line_ending.as_str()
        } else {
            ""
        },
        hunk.incoming_text
    )
}

/// `applyMergeConflictResolution`
pub fn apply_conflict_resolution(
    source: &str,
    hunk: &ConflictHunk,
    resolution: ConflictResolution,
) -> String {
    let replacement = match resolution {
        ConflictResolution::Current => hunk.current_text.clone(),
        ConflictResolution::Incoming => hunk.incoming_text.clone(),
        ConflictResolution::Both => combine_both(hunk),
    };
    apply_conflict_replacement(source, hunk, &replacement)
}

/// `applyMergeConflictReplacement`
pub fn apply_conflict_replacement(source: &str, hunk: &ConflictHunk, replacement: &str) -> String {
    format!(
        "{}{replacement}{}",
        &source[..hunk.start_offset],
        &source[hunk.end_offset..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = "before\n<<<<<<< HEAD\nconst current = true;\n=======\nconst incoming = true;\n>>>>>>> origin/main\nafter\n";

    #[test]
    fn parses_labels_ranges_and_both_sides() {
        let hunk = &parse_conflict_hunks(SIMPLE)[0];
        assert_eq!((hunk.start_line, hunk.end_line), (2, 6));
        assert_eq!(hunk.current_label.as_deref(), Some("HEAD"));
        assert_eq!(hunk.incoming_label.as_deref(), Some("origin/main"));
        assert_eq!(hunk.current_text, "const current = true;\n");
        assert_eq!(hunk.incoming_text, "const incoming = true;\n");
    }

    #[test]
    fn accepts_current_incoming_or_both() {
        let hunk = &parse_conflict_hunks(SIMPLE)[0];
        let apply = |resolution| apply_conflict_resolution(SIMPLE, hunk, resolution);
        assert_eq!(
            apply(ConflictResolution::Current),
            "before\nconst current = true;\nafter\n"
        );
        assert_eq!(
            apply(ConflictResolution::Incoming),
            "before\nconst incoming = true;\nafter\n"
        );
        assert_eq!(
            apply(ConflictResolution::Both),
            "before\nconst current = true;\nconst incoming = true;\nafter\n"
        );
        assert_eq!(
            apply_conflict_replacement(SIMPLE, hunk, "const merged = true;\n"),
            "before\nconst merged = true;\nafter\n"
        );
    }

    #[test]
    fn supports_diff3_bases_and_crlf() {
        let source = "<<<<<<< feature\r\ncurrent\r\n||||||| base\r\nancestor\r\n=======\r\nincoming\r\n>>>>>>> main\r\n";
        let hunk = &parse_conflict_hunks(source)[0];
        assert_eq!(hunk.base_text.as_deref(), Some("ancestor\r\n"));
        assert_eq!(
            apply_conflict_resolution(source, hunk, ConflictResolution::Both),
            "current\r\nincoming\r\n"
        );
    }

    #[test]
    fn re_parses_what_is_left_after_one_block() {
        let source = format!("{SIMPLE}{SIMPLE}");
        let first = &parse_conflict_hunks(&source)[0];
        let next = apply_conflict_resolution(&source, first, ConflictResolution::Current);
        assert_eq!(parse_conflict_hunks(&next).len(), 1);
    }

    #[test]
    fn detects_stray_marker_fragments() {
        assert!(has_conflict_marker_fragments(
            "<<<<<<< HEAD\nmissing separator\n"
        ));
        assert!(!has_conflict_marker_fragments("const value = '<<<<<<<';\n"));
    }
}
