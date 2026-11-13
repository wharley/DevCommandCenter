//! The conflict blocks where they are, in the file's own editor: each
//! block tinted by side (the person's lines, then the incoming ones) and
//! its three choices on its first line. The file is the result: taking a
//! side rewrites that block in place and what is left stays editable,
//! like the rest of the text.

use std::rc::Rc;

use dcc_core::domain::merge_conflict::{ConflictResolution, parse_conflict_hunks};
use gpui::{AnyElement, App, Context, Hsla, WeakEntity, div, prelude::*, px};

use super::MergeResolver;
use crate::i18n::Translate;
use crate::screens::file_view::bands::{LineActions, LineBand};
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant};

/// A block's lines in the text (zero-based): its opening marker, where the
/// incoming side starts (the `=======` line) and its closing marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockLines {
    pub start: usize,
    pub divider: usize,
    pub end: usize,
}

/// Every block of `text`, in order. One whose divider cannot be found is
/// taken whole as the person's side.
pub(crate) fn block_lines(text: &str) -> Vec<BlockLines> {
    let lines: Vec<&str> = text.lines().collect();
    parse_conflict_hunks(text)
        .iter()
        .map(|hunk| {
            let start = hunk.start_line.saturating_sub(1);
            let end = hunk.end_line.saturating_sub(1).max(start);
            let divider = (start + 1..end)
                .find(|line| {
                    lines
                        .get(*line)
                        .is_some_and(|text| text.starts_with("======="))
                })
                .unwrap_or(end);
            BlockLines {
                start,
                divider,
                end,
            }
        })
        .collect()
}

/// The tints of every block: the person's side, then the incoming one.
pub(crate) fn bands(blocks: &[BlockLines], mine: Hsla, incoming: Hsla) -> Vec<LineBand> {
    blocks
        .iter()
        .flat_map(|block| {
            [
                LineBand {
                    lines: block.start..block.divider,
                    color: mine,
                },
                LineBand {
                    lines: block.divider..block.end + 1,
                    color: incoming,
                },
            ]
        })
        .filter(|band| !band.lines.is_empty())
        .collect()
}

fn choices(
    resolver: WeakEntity<MergeResolver>,
    index: usize,
    busy: bool,
    cx: &mut App,
) -> AnyElement {
    let choice =
        |id: &'static str, key: &'static str, resolution: ConflictResolution, cx: &mut App| {
            let resolver = resolver.clone();
            div().debug_selector(move || format!("{id}-{index}")).child(
                Button::new((id, index))
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Xs)
                    .text_size(11.0)
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        let _ = resolver.update(cx, |resolver, cx| {
                            resolver.apply_hunk_at(index, resolution, cx)
                        });
                    })
                    .child(cx.t(key)),
            )
        };
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(choice(
            "merge-accept-current",
            "mergeConflict.hunk.acceptMine",
            ConflictResolution::Current,
            cx,
        ))
        .child(choice(
            "merge-accept-incoming",
            "mergeConflict.hunk.acceptIncoming",
            ConflictResolution::Incoming,
            cx,
        ))
        .child(choice(
            "merge-accept-both",
            "mergeConflict.hunk.acceptBoth",
            ConflictResolution::Both,
            cx,
        ))
        .into_any_element()
}

impl MergeResolver {
    /// Paints the open file's blocks and pins each one's choices to its
    /// first line. Called when the text changes.
    pub(crate) fn decorate(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let blocks = block_lines(&open.buffer);
        let bands = bands(&blocks, tw::SKY_500.alpha(0.14), tw::AMBER_500.alpha(0.14));
        let starts: Vec<usize> = blocks.iter().map(|block| block.start).collect();
        let resolver = cx.entity().downgrade();
        let actions: LineActions = Rc::new(move |line, cx| {
            let index = starts.iter().position(|start| *start == line)?;
            let busy = resolver
                .read_with(cx, |resolver, _| resolver.busy.is_some())
                .unwrap_or(true);
            Some(choices(resolver.clone(), index, busy, cx))
        });
        open.editor.clone().update(cx, |editor, cx| {
            editor.set_bands(bands, cx);
            editor.set_line_actions(Some(actions), cx);
        });
    }

    /// One of a block's choices, picked on the block itself.
    pub(crate) fn apply_hunk_at(
        &mut self,
        index: usize,
        resolution: ConflictResolution,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = self.open.as_mut() {
            open.active_hunk = index;
        }
        self.apply_hunk(resolution, cx);
    }

    /// The block on screen brought into view in the editor.
    pub(crate) fn reveal_hunk(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let blocks = block_lines(&open.buffer);
        let Some(block) = blocks.get(open.active_hunk.min(blocks.len().saturating_sub(1))) else {
            return;
        };
        // A couple of lines above it, so the block does not hug the top.
        let line = block.start.saturating_sub(2) as u32 + 1;
        open.editor
            .clone()
            .update(cx, |editor, cx| editor.focus(Some(line), cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "a\n<<<<<<< HEAD\nmine 1\nmine 2\n=======\ntheirs\n>>>>>>> origin/main\nb\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> origin/main\n";

    #[test]
    fn a_block_is_found_by_its_markers_lines() {
        assert_eq!(
            block_lines(TEXT),
            [
                BlockLines {
                    start: 1,
                    divider: 4,
                    end: 6
                },
                BlockLines {
                    start: 8,
                    divider: 10,
                    end: 12
                },
            ]
        );
        assert!(block_lines("no conflict here\n").is_empty());
    }

    #[test]
    fn each_side_gets_its_tint_and_the_text_between_blocks_none() {
        let (mine, incoming) = (gpui::blue(), gpui::yellow());
        let bands = bands(&block_lines(TEXT), mine, incoming);
        let tint = |line| crate::screens::file_view::bands::band_at(&bands, line);
        assert_eq!(tint(0), None);
        assert_eq!((tint(1), tint(3)), (Some(mine), Some(mine)));
        assert_eq!((tint(4), tint(6)), (Some(incoming), Some(incoming)));
        assert_eq!(tint(7), None);
        assert_eq!((tint(8), tint(12)), (Some(mine), Some(incoming)));
    }
}
