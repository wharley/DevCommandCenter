//! A draft per room (Tucano's way): the words and chips left in the field
//! stay with the room they were written in — Geral, a project's room, or
//! one agent's "Conversa" in either — and come back when it opens again.
//! Kept in memory for the session only; sending empties the field, so that
//! room keeps nothing.

use std::collections::HashMap;

use gpui::Context;

use super::room::TeamRoom;
use crate::ui::text_area::{CHIP, Chip, Part};

/// Where a draft belongs: the room (none: Geral) and the agent shown alone.
pub(crate) type DraftKey = (Option<String>, Option<String>);

#[derive(Default)]
pub(crate) struct RoomDrafts {
    kept: HashMap<DraftKey, Vec<Part>>,
}

impl RoomDrafts {
    /// Keeps `parts` for `key`; an empty field keeps nothing.
    fn keep(&mut self, key: DraftKey, parts: Vec<Part>) {
        if parts.is_empty() {
            self.kept.remove(&key);
        } else {
            self.kept.insert(key, parts);
        }
    }

    fn take(&mut self, key: &DraftKey) -> Vec<Part> {
        self.kept.remove(key).unwrap_or_default()
    }
}

/// The field's content as parts: each `CHIP` mark in `text` is the next of
/// `chips`, in order. Blank text alone is nothing.
pub(crate) fn parts_of(text: &str, chips: &[Chip]) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut chips = chips.iter();
    for (index, piece) in text.split(CHIP).enumerate() {
        if index > 0
            && let Some(chip) = chips.next()
        {
            parts.push(Part::Chip(chip.clone()));
        }
        if !piece.is_empty() {
            parts.push(Part::Text(piece.to_string()));
        }
    }
    let blank = parts
        .iter()
        .all(|part| matches!(part, Part::Text(text) if text.trim().is_empty()));
    if blank { Vec::new() } else { parts }
}

impl TeamRoom {
    pub(crate) fn draft_key(&self) -> DraftKey {
        (self.room.clone(), self.focus.clone())
    }

    /// The room or agent shown changed from `before`: the field's content
    /// stays there, and what was left here comes back.
    pub(crate) fn swap_draft(&mut self, before: DraftKey, cx: &mut Context<Self>) {
        let now = self.draft_key();
        if now == before {
            return;
        }
        let input = self.input.read(cx);
        let chips: Vec<Chip> = input.chips().into_iter().map(|(_, chip)| chip).collect();
        let left = parts_of(input.text(), &chips);
        self.drafts.keep(before, left);
        let back = self.drafts.take(&now);
        self.input.update(cx, |area, cx| area.set_parts(back, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_splits_into_its_words_and_chips() {
        let chips = vec![
            Chip::Image("/t/a.png".into()),
            Chip::File("src/b.rs".into()),
        ];
        let text = format!("olha {CHIP} e {CHIP}");
        assert_eq!(
            parts_of(&text, &chips),
            vec![
                Part::Text("olha ".into()),
                Part::Chip(chips[0].clone()),
                Part::Text(" e ".into()),
                Part::Chip(chips[1].clone()),
            ]
        );
        assert_eq!(
            parts_of(&CHIP.to_string(), &chips[..1]),
            vec![Part::Chip(chips[0].clone())]
        );
        assert!(parts_of("  \n", &[]).is_empty());
        assert!(parts_of("", &[]).is_empty());
    }

    #[test]
    fn each_place_keeps_its_own() {
        let mut drafts = RoomDrafts::default();
        let geral: DraftKey = (None, None);
        let dev: DraftKey = (None, Some("dev".into()));
        drafts.keep(geral.clone(), vec![Part::Text("oi".into())]);
        assert!(drafts.take(&dev).is_empty());
        assert_eq!(drafts.take(&geral), vec![Part::Text("oi".into())]);
        drafts.keep(geral.clone(), vec![Part::Text("a".into())]);
        drafts.keep(geral.clone(), Vec::new());
        assert!(drafts.take(&geral).is_empty(), "sent: nothing kept");
    }
}
