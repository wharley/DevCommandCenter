//! The agent's resolution of a whole file, read as one proposal per
//! conflict block. The agent resolves every block together, with the
//! whole file in view, and answers the whole file; what it did to each
//! block is found by the text between the blocks, which it is told to
//! keep as it is. When that text was changed too, the answer cannot be
//! told apart by block and stays one proposal for the file.

use super::hunks::parse_conflict_hunks;

/// What the agent proposes in place of one block (or of a run of blocks
/// with too little between them to tell where one ends).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedBlock {
    /// The block as it was in the file, markers included.
    pub original: String,
    pub proposed: String,
}

/// The least text between two blocks that tells one from the next: under
/// it (a blank line, a brace) the two are proposed as one.
const MIN_BETWEEN: usize = 12;

/// `resolved` split by `source`'s blocks, in order. `None` when the text
/// around the blocks is not found as it was, or there is no block.
pub fn split_proposal(source: &str, resolved: &str) -> Option<Vec<ProposedBlock>> {
    let hunks = parse_conflict_hunks(source);
    let first = hunks.first()?;
    let last = hunks.last()?;
    let (before, after) = (&source[..first.start_offset], &source[last.end_offset..]);
    if resolved.len() < before.len() + after.len()
        || !resolved.starts_with(before)
        || !resolved.ends_with(after)
    {
        return None;
    }
    // Runs of blocks, each closed by enough text to find it again.
    let mut runs: Vec<(usize, usize, &str)> = Vec::new();
    let mut run_start = first.start_offset;
    for (index, hunk) in hunks.iter().enumerate() {
        let Some(next) = hunks.get(index + 1) else {
            runs.push((run_start, hunk.end_offset, after));
            break;
        };
        let between = &source[hunk.end_offset..next.start_offset];
        if between.trim().len() >= MIN_BETWEEN {
            runs.push((run_start, hunk.end_offset, between));
            run_start = next.start_offset;
        }
    }
    let end = resolved.len() - after.len();
    let mut cursor = before.len();
    let mut blocks = Vec::with_capacity(runs.len());
    for (index, (start, stop, closing)) in runs.iter().enumerate() {
        let until = if index + 1 == runs.len() {
            end
        } else {
            cursor + resolved.get(cursor..end)?.find(closing)?
        };
        blocks.push(ProposedBlock {
            original: source[*start..*stop].to_string(),
            proposed: resolved.get(cursor..until)?.to_string(),
        });
        cursor = until
            + if index + 1 == runs.len() {
                0
            } else {
                closing.len()
            };
    }
    Some(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "<<<<<<< HEAD\nmine a\n=======\ntheirs a\n>>>>>>> main\n";
    const B: &str = "<<<<<<< HEAD\nmine b\n=======\ntheirs b\n>>>>>>> main\n";

    #[test]
    fn each_block_gets_what_the_agent_put_in_its_place() {
        let source = format!("top\n{A}fn between() {{ body(); }}\n{B}bottom\n");
        let resolved = "top\nboth a\nfn between() { body(); }\ntheirs b\nbottom\n";
        let blocks = split_proposal(&source, resolved).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            (blocks[0].original.as_str(), blocks[0].proposed.as_str()),
            (A, "both a\n")
        );
        assert_eq!(
            (blocks[1].original.as_str(), blocks[1].proposed.as_str()),
            (B, "theirs b\n")
        );
        // Put back together, it is the agent's file.
        let rebuilt = format!(
            "top\n{}fn between() {{ body(); }}\n{}bottom\n",
            blocks[0].proposed, blocks[1].proposed
        );
        assert_eq!(rebuilt, resolved);
    }

    #[test]
    fn blocks_with_almost_nothing_between_them_are_one_proposal() {
        let source = format!("top\n{A}\n{B}bottom\n");
        let blocks = split_proposal(&source, "top\na\n\nb\nbottom\n").unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].original, format!("{A}\n{B}"));
        assert_eq!(blocks[0].proposed, "a\n\nb\n");
    }

    #[test]
    fn a_block_the_agent_removed_proposes_nothing_in_its_place() {
        let source = format!("top\n{A}bottom\n");
        let blocks = split_proposal(&source, "top\nbottom\n").unwrap();
        assert_eq!(blocks[0].proposed, "");
    }

    #[test]
    fn text_changed_outside_the_blocks_is_not_split() {
        let source = format!("top\n{A}fn between() {{ body(); }}\n{B}bottom\n");
        // The agent rewrote the top, then the middle.
        assert_eq!(
            split_proposal(&source, "TOP\nx\nfn between() { body(); }\ny\nbottom\n"),
            None
        );
        assert_eq!(
            split_proposal(&source, "top\nx\nfn other() {}\ny\nbottom\n"),
            None
        );
        assert_eq!(split_proposal("no blocks\n", "no blocks\n"), None);
    }
}
