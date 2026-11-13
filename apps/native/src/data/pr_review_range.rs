//! A comment over several lines of a diff: the lines above the commented
//! one it can take in (the same side of the same hunk), and the
//! `suggestion` block GitHub turns into a change the author applies with
//! one click.

use super::pr_review::{PatchRow, RowKind};

/// The lines of `side` from `line` upward within its hunk, nearest first,
/// the line itself included: `(number, text)`.
pub fn lines_up<'a>(rows: &'a [PatchRow], line: u32, side: &str) -> Vec<(u32, &'a str)> {
    let Some(at) = rows
        .iter()
        .position(|row| row.review == Some((line, side_of(side))))
    else {
        return Vec::new();
    };
    rows[..=at]
        .iter()
        .rev()
        .take_while(|row| row.kind != RowKind::Hunk)
        .filter_map(|row| match row.review {
            Some((number, row_side)) if row_side == side => Some((number, row.content.as_str())),
            _ => None,
        })
        .collect()
}

fn side_of(side: &str) -> &'static str {
    if side == "left" { "left" } else { "right" }
}

/// A comment on `line` reaching `span` lines above it: where it starts
/// (`None` for one line, or with nothing above) and the text it covers,
/// top to bottom. `span` past the hunk's start stops there.
pub fn range(rows: &[PatchRow], line: u32, side: &str, span: u32) -> (Option<u32>, Vec<String>) {
    let mut taken: Vec<(u32, &str)> = lines_up(rows, line, side)
        .into_iter()
        .take(span as usize + 1)
        .collect();
    taken.reverse();
    let start = taken
        .first()
        .map(|(number, _)| *number)
        .filter(|start| *start != line);
    (
        start,
        taken
            .into_iter()
            .map(|(_, text)| text.to_string())
            .collect(),
    )
}

/// How far a comment on `line` can reach upward.
pub fn max_span(rows: &[PatchRow], line: u32, side: &str) -> u32 {
    (lines_up(rows, line, side).len() as u32).saturating_sub(1)
}

/// The block GitHub reads as a suggested change, starting from the lines
/// as they are, for the person to edit.
pub fn suggestion(lines: &[String]) -> String {
    format!("```suggestion\n{}\n```", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::pr_review::parse_patch;

    const PATCH: &str = "@@ -1,3 +1,4 @@\n fn a() {\n-    old();\n+    new();\n+    more();\n }\n@@ -10,2 +11,2 @@\n fn b() {\n+    x();";

    #[test]
    fn a_comment_reaches_up_its_side_of_its_hunk_only() {
        let rows = parse_patch(Some(PATCH));
        // Line 3 on the right: 3, 2, 1 — the removed line is the other side.
        let up: Vec<u32> = lines_up(&rows, 3, "right")
            .iter()
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(up, [3, 2, 1]);
        assert_eq!(max_span(&rows, 3, "right"), 2);
        let (start, text) = range(&rows, 3, "right", 1);
        assert_eq!(start, Some(2));
        assert_eq!(text, ["    new();", "    more();"]);
        // One line: no start, its own text.
        assert_eq!(
            range(&rows, 3, "right", 0),
            (None, vec!["    more();".to_string()])
        );
        // Past the hunk's first line it stops there.
        assert_eq!(range(&rows, 3, "right", 9).0, Some(1));
        // The second hunk does not reach into the first.
        assert_eq!(max_span(&rows, 12, "right"), 1);
        assert!(lines_up(&rows, 99, "right").is_empty());
    }

    #[test]
    fn a_suggestion_starts_from_the_lines_as_they_are() {
        assert_eq!(
            suggestion(&["    new();".into(), "    more();".into()]),
            "```suggestion\n    new();\n    more();\n```"
        );
    }
}
