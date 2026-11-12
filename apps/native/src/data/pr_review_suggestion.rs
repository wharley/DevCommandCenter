//! Code the Reviewer suggests in a pull request's review: a better way to
//! write what the author changed, as GitHub shows it. Replacing lines the
//! patch shows, it is a ```suggestion block on a comment covering exactly
//! those lines, which the author applies with one click ("Commit
//! suggestion"); a new function or a change elsewhere is a plain code block
//! to copy. Only when it is worth it — a bug, a duplicate of what exists,
//! much simpler code — never for taste.

use std::collections::HashSet;

/// What the review prompt asks about suggesting code.
pub const SUGGESTION_RULES: &str = "Suggest code only when a better way is worth it: it fixes a bug, reuses something that already exists instead of duplicating it, or makes the changed code much simpler. Never for taste, naming or formatting.\n- When the better version replaces lines the patch shows on the right side, make the comment cover exactly those lines (startLine to line, one hunk) and put the replacement in the body as a block fenced with ```suggestion, holding only the new lines with their indentation; one short sentence before it says why. The author applies it with one click, so it must be complete and correct as written.\n- When it is a new function, a change in another place or file, or too large to replace in place, put the code in a normal fenced block with its language, to copy and adapt.\nA suggestion that fixes nothing blocking is one of the optional points.";

/// The first line of a comment on several lines, when it stands: before
/// `line`, on the same side, every line between them shown by the patch.
pub fn start_line(
    raw: Option<u64>,
    path: &str,
    line: u32,
    side: &str,
    reviewable: &HashSet<String>,
) -> Option<u32> {
    let start = u32::try_from(raw?).ok()?;
    if start == 0 || start >= line {
        return None;
    }
    (start..=line)
        .all(|at| reviewable.contains(&super::pr_review::draft_key(path, at, side)))
        .then_some(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::pr_review::draft_key;

    #[test]
    fn a_range_stands_only_inside_what_the_patch_shows() {
        let shown: HashSet<String> = (10..=14)
            .map(|line| draft_key("src/a.ts", line, "right"))
            .collect();
        assert_eq!(
            start_line(Some(10), "src/a.ts", 14, "right", &shown),
            Some(10)
        );
        // One line, backwards, out of the hunk, another side: a single line.
        assert_eq!(start_line(Some(14), "src/a.ts", 14, "right", &shown), None);
        assert_eq!(start_line(Some(16), "src/a.ts", 14, "right", &shown), None);
        assert_eq!(start_line(Some(8), "src/a.ts", 14, "right", &shown), None);
        assert_eq!(start_line(Some(10), "src/a.ts", 14, "left", &shown), None);
        assert_eq!(start_line(None, "src/a.ts", 14, "right", &shown), None);
    }
}
