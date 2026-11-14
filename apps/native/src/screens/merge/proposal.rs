//! The agent's answer, in the editor, to look at block by block. The whole
//! answer is already in the file (the agent resolved the blocks together);
//! each block it wrote is tinted, with "Aceitar" and "Recusar" on its first
//! line. Accepting keeps the text; refusing puts that block's conflict back
//! where it was, to resolve by hand. A block the person edits is theirs:
//! it is no longer offered. When the answer could not be told apart by
//! block, it is one proposal for the file, kept or discarded whole.

use std::ops::Range;

use gpui::{AnyElement, App, Context, WeakEntity, div, prelude::*, px};

use super::MergeResolver;
use crate::i18n::Translate;
use crate::ui::{Button, ButtonSize, ButtonVariant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockState {
    Pending,
    Accepted,
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProposalBlock {
    /// The conflict as it was, markers included.
    pub original: String,
    pub proposed: String,
    /// What the agent says it did there.
    pub note: Option<String>,
    pub state: BlockState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Proposal {
    pub path: String,
    pub explanation: String,
    /// The file before the answer went in.
    pub before: String,
    /// Empty when the answer is one proposal for the whole file.
    pub blocks: Vec<ProposalBlock>,
    /// The block the editor was last taken to.
    pub at: usize,
}

impl Proposal {
    /// Where each block still offered sits in `text` (bytes), in the
    /// file's order: its proposed text, looked for after the one before.
    /// One that proposes nothing, or that was edited since, has no place.
    pub fn located(&self, text: &str) -> Vec<Option<Range<usize>>> {
        let mut from = 0;
        self.blocks
            .iter()
            .map(|block| {
                if block.state != BlockState::Pending || block.proposed.is_empty() {
                    return None;
                }
                let at = from + text.get(from..)?.find(&block.proposed)?;
                from = at + block.proposed.len();
                Some(at..from)
            })
            .collect()
    }

    /// The blocks still to accept or refuse in `text`.
    pub fn offered(&self, text: &str) -> usize {
        self.located(text).iter().flatten().count()
    }

    /// The offered block after (or before) the one the editor is on;
    /// `None` at the end it would step past.
    pub fn step(&self, text: &str, forward: bool) -> Option<usize> {
        let offered: Vec<usize> = self
            .lines(text)
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        if forward {
            offered.into_iter().find(|index| *index > self.at)
        } else {
            offered.into_iter().rev().find(|index| *index < self.at)
        }
    }

    /// Where the editor is among the blocks still offered: `(position,
    /// total)`, the position one-based.
    pub fn position(&self, text: &str) -> Option<(usize, usize)> {
        let offered: Vec<usize> = self
            .lines(text)
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        if offered.is_empty() {
            return None;
        }
        let at = offered
            .iter()
            .position(|index| *index >= self.at)
            .unwrap_or(offered.len() - 1);
        Some((at + 1, offered.len()))
    }

    /// The lines (zero-based) of each block still offered.
    pub fn lines(&self, text: &str) -> Vec<(usize, Range<usize>)> {
        self.located(text)
            .into_iter()
            .enumerate()
            .filter_map(|(index, range)| {
                let range = range?;
                let first = text[..range.start].matches('\n').count();
                let count = text[range.clone()].lines().count().max(1);
                Some((index, first..first + count))
            })
            .collect()
    }
}

/// "Aceitar" and "Recusar", on the first line of a proposed block.
pub(super) fn choices(
    resolver: WeakEntity<MergeResolver>,
    index: usize,
    cx: &mut App,
) -> AnyElement {
    let button = |id: &'static str, key: &'static str, accept: bool, cx: &mut App| {
        let resolver = resolver.clone();
        div().debug_selector(move || format!("{id}-{index}")).child(
            Button::new((id, index))
                .variant(if accept {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Xs)
                .text_size(11.0)
                .on_click(move |_, _, cx| {
                    let _ = resolver.update(cx, |resolver, cx| {
                        if accept {
                            resolver.accept_proposed(index, cx)
                        } else {
                            resolver.refuse_proposed(index, cx)
                        }
                    });
                })
                .child(cx.t(key)),
        )
    };
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(button(
            "merge-proposal-refuse",
            "mergeConflict.proposal.refuse",
            false,
            cx,
        ))
        .child(button(
            "merge-proposal-accept",
            "mergeConflict.proposal.accept",
            true,
            cx,
        ))
        .into_any_element()
}

impl MergeResolver {
    /// The editor taken to a block the agent proposed.
    pub(crate) fn reveal_proposed(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(proposal), Some(open)) = (self.proposal.as_mut(), self.open.as_ref()) else {
            return;
        };
        let Some((_, lines)) = proposal
            .lines(&open.buffer)
            .into_iter()
            .find(|(offered, _)| *offered == index)
        else {
            return;
        };
        proposal.at = index;
        // A couple of lines above it, so the block does not hug the top.
        let line = lines.start.saturating_sub(2) as u32 + 1;
        open.editor
            .clone()
            .update(cx, |editor, cx| editor.focus(Some(line), cx));
        cx.notify();
    }

    /// The bar's arrows, while a proposal is offered.
    pub(crate) fn step_proposed(&mut self, forward: bool, cx: &mut Context<Self>) {
        let next = match (&self.proposal, &self.open) {
            (Some(proposal), Some(open)) => proposal.step(&open.buffer, forward),
            _ => None,
        };
        if let Some(next) = next {
            self.reveal_proposed(next, cx);
        }
    }

    /// The proposal leaves once nothing of it is left to decide.
    pub(crate) fn settle_proposal(&mut self, cx: &mut Context<Self>) {
        let done = match (&self.proposal, &self.open) {
            (Some(proposal), Some(open)) => {
                !proposal.blocks.is_empty() && proposal.offered(&open.buffer) == 0
            }
            _ => false,
        };
        if done {
            self.proposal = None;
        }
        self.decorate(cx);
        cx.notify();
    }

    pub(crate) fn accept_proposed(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(block) = self
            .proposal
            .as_mut()
            .and_then(|proposal| proposal.blocks.get_mut(index))
        {
            block.state = BlockState::Accepted;
        }
        self.settle_proposal(cx);
    }

    /// That block's conflict back in the file, in place of what the agent
    /// wrote, to resolve by hand.
    pub(crate) fn refuse_proposed(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(proposal), Some(open)) = (self.proposal.as_mut(), self.open.as_ref()) else {
            return;
        };
        let Some(range) = proposal.located(&open.buffer).get(index).cloned().flatten() else {
            return;
        };
        let Some(block) = proposal.blocks.get_mut(index) else {
            return;
        };
        block.state = BlockState::Refused;
        let mut text = open.buffer.clone();
        text.replace_range(range, &block.original);
        self.set_result(text, cx);
        self.settle_proposal(cx);
    }

    /// "Aceitar todos": the answer stays as it is in the file.
    pub(crate) fn accept_proposal(&mut self, cx: &mut Context<Self>) {
        self.proposal = None;
        self.decorate(cx);
        cx.notify();
    }

    /// "Descartar": every block still offered goes back to its conflict; an
    /// answer for the whole file goes back to the file as it was.
    pub(crate) fn discard_proposal(&mut self, cx: &mut Context<Self>) {
        let (Some(proposal), Some(open)) = (self.proposal.take(), self.open.as_ref()) else {
            return;
        };
        let text = if proposal.blocks.is_empty() {
            proposal.before
        } else {
            let mut text = open.buffer.clone();
            // From the end, so the places before stay where they were found.
            for (index, range) in proposal.located(&text).into_iter().enumerate().rev() {
                if let Some(range) = range {
                    text.replace_range(range, &proposal.blocks[index].original);
                }
            }
            text
        };
        self.set_result(text, cx);
        self.decorate(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(proposed: &str) -> ProposalBlock {
        ProposalBlock {
            original: "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> main\n".into(),
            proposed: proposed.into(),
            note: None,
            state: BlockState::Pending,
        }
    }

    fn proposal(blocks: Vec<ProposalBlock>) -> Proposal {
        Proposal {
            path: "a.rs".into(),
            explanation: "both".into(),
            before: String::new(),
            blocks,
            at: 0,
        }
    }

    #[test]
    fn a_block_is_offered_where_its_text_sits_until_decided_or_edited() {
        let text = "top\nboth a\nmiddle line here\nboth b\nbottom\n";
        let mut proposal = proposal(vec![block("both a\n"), block("both b\n")]);
        assert_eq!(proposal.lines(text), [(0, 1..2), (1, 3..4)]);
        assert_eq!(proposal.offered(text), 2);
        // Decided: no longer offered; the other stays where it is.
        proposal.blocks[0].state = BlockState::Accepted;
        assert_eq!(proposal.lines(text), [(1, 3..4)]);
        // Edited by hand: it is the person's text now.
        let edited = "top\nboth a\nmiddle line here\nboth B!\nbottom\n";
        assert_eq!(proposal.offered(edited), 0);
    }

    #[test]
    fn the_arrows_walk_the_blocks_still_offered() {
        let text = "x\none\ny\ntwo\nz\nthree\n";
        let mut proposal = proposal(vec![block("one\n"), block("two\n"), block("three\n")]);
        assert_eq!(proposal.position(text), Some((1, 3)));
        assert_eq!(proposal.step(text, false), None, "already on the first");
        // The second was decided: the arrows skip it.
        proposal.blocks[1].state = BlockState::Accepted;
        assert_eq!(proposal.step(text, true), Some(2));
        proposal.at = 2;
        assert_eq!(proposal.position(text), Some((2, 2)));
        assert_eq!(proposal.step(text, true), None);
        assert_eq!(proposal.step(text, false), Some(0));
    }

    #[test]
    fn the_same_text_twice_is_found_in_order_and_nothing_proposed_has_no_place() {
        let text = "x\nsame\ny\nsame\nz\n";
        let twice = proposal(vec![block("same\n"), block("same\n")]);
        assert_eq!(twice.lines(text), [(0, 1..2), (1, 3..4)]);
        let removed = proposal(vec![block("")]);
        assert_eq!(removed.offered(text), 0);
    }
}
