//! A long answer in the room shows its opening — the outcome the agents are
//! told to lead with — and "ver mais" for the rest, so the room reads as
//! what happened and what to do, not as walls of text. Opened by hand,
//! it stays open while the room lives.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_rows::Row;
use crate::i18n::Translate;
use crate::markdown::Block;
use crate::theme::ActiveTheme;

/// Longer than this, and in more than one block, it folds.
const FOLD_CHARS: usize = 320;

/// How many of `blocks` show: all, or the first while folded.
pub(crate) fn shown(text: &str, blocks: &[Block], open: bool) -> usize {
    if open || blocks.len() < 2 || text.chars().count() <= FOLD_CHARS {
        blocks.len()
    } else {
        1
    }
}

impl TeamRoom {
    fn toggle_answer(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.answers_open.remove(key) {
            self.answers_open.insert(key.to_string());
        }
        let model = self.list.model();
        if let Some(row) = model
            .rows
            .iter()
            .position(|row| matches!(row, Row::Line { line, .. } if line.key == key))
        {
            self.list.remeasure_row(row);
        }
        cx.notify();
    }

    /// "ver mais" / "ver menos" under a folded answer; none when it fits.
    pub(super) fn fold_toggle(
        &self,
        key: &str,
        folded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let target = key.to_string();
        div()
            .id(SharedString::from(format!("room-fold-{key}")))
            .debug_selector({
                let key = key.to_string();
                move || format!("room-fold-{key}")
            })
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .cursor_pointer()
            .hover(|style| style.text_color(colors.foreground))
            .on_click(cx.listener(move |room, _, _, cx| room.toggle_answer(&target, cx)))
            .child(if folded {
                cx.t("agents.room.showMore")
            } else {
                cx.t("agents.room.showLess")
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse;

    #[test]
    fn a_long_answer_shows_its_opening_and_a_short_one_shows_whole() {
        let long = format!(
            "**PR #1060 não foi aprovado.**\n\n{}",
            "detalhe ".repeat(60)
        );
        let blocks = parse(&long);
        assert_eq!(shown(&long, &blocks, false), 1);
        assert_eq!(shown(&long, &blocks, true), blocks.len());
        let short = "Feito.\n\nNada mais.";
        assert_eq!(shown(short, &parse(short), false), 2);
    }
}
