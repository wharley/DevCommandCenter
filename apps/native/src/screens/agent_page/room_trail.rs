//! The task conversation's trail in the team room (`ui::trail`): a tick per
//! message of the person's down the left margin, the one under the pointer
//! shows the message and the answer it got, a click goes there. In a room
//! narrower than its column and two margins, the rows make room for it.

use gpui::{AnyElement, Context, ListOffset, px};

use super::room::TeamRoom;
use super::room_rows::Row;
use crate::data::team_room::Speaker;
use crate::ui::trail::{TrailItem, preview};

/// The room's column (760) and a gutter on each side.
const TRAIL_WIDTH: f32 = 880.0;
/// Narrower than this, no trail: the room is too tight for a gutter.
const MIN_WIDTH: f32 = 420.0;

/// Whether the rows move right to keep the trail's margin free.
pub(crate) fn needs_gutter(width: f32) -> bool {
    (MIN_WIDTH..TRAIL_WIDTH).contains(&width)
}

/// A stop per message of the person's, with the last answer after it.
pub(crate) fn room_trail_items(rows: &[Row]) -> Vec<TrailItem> {
    let mut items: Vec<TrailItem> = Vec::new();
    for (row, item) in rows.iter().enumerate() {
        let Row::Line { line, said, .. } = item else {
            continue;
        };
        match (said, line.speaker) {
            (Some(said), _) => items.push(TrailItem {
                row,
                prompt: preview(&said.words),
                response: String::new(),
            }),
            (None, Speaker::Agent) => {
                if let Some(last) = items.last_mut() {
                    let response = preview(&line.text);
                    if !response.is_empty() {
                        last.response = response;
                    }
                }
            }
            _ => {}
        }
    }
    items
}

impl TeamRoom {
    pub(super) fn room_trail(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let width = self.width.get();
        if width < MIN_WIDTH {
            return None;
        }
        let items = self.list.model().trail.clone();
        let top = self.list.state.logical_scroll_top().item_ix;
        let current = items.iter().rposition(|item| item.row <= top).unwrap_or(0);
        crate::ui::trail::trail(
            "room-trail",
            items,
            current,
            width,
            |room: &mut Self| &mut room.trail,
            |room, item, _| {
                room.list.state.scroll_to(ListOffset {
                    item_ix: item.row,
                    offset_in_item: px(0.0),
                });
            },
            // The pointer over the trail redraws it, never the rows.
            |room, cx| {
                room.list.hold_rows();
                cx.notify();
            },
            self,
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::super::room_said::Said;
    use super::*;
    use crate::data::agent_mentions::MentionNames;
    use crate::data::team_room::RoomLine;

    fn line(key: &str, speaker: Speaker, text: &str) -> RoomLine {
        RoomLine {
            key: key.into(),
            agent_id: "a".into(),
            session_id: "s".into(),
            speaker,
            text: text.into(),
            at: "2026-11-12T10:00:00Z".into(),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }
    }

    fn row(line: RoomLine) -> Row {
        let names = MentionNames::new(std::iter::empty::<(&str, &str)>(), |_| 0);
        let said =
            (line.speaker == Speaker::Person).then(|| Rc::new(Said::new(&line.text, &names)));
        Row::Line {
            index: 0,
            line,
            first: true,
            said,
            steps: None,
        }
    }

    #[test]
    fn a_stop_per_message_with_the_last_answer_after_it() {
        let rows = vec![
            row(line("p1", Speaker::Person, "revisa o #1060")),
            row(line("a1", Speaker::Agent, "Olhando.")),
            row(line("a2", Speaker::Agent, "**Não aprovado.**")),
            row(line("p2", Speaker::Person, "e o deploy?")),
        ];
        let items = room_trail_items(&rows);
        assert_eq!(items.len(), 2);
        assert_eq!(
            (items[0].row, items[0].response.as_str()),
            (0, "**Não aprovado.**")
        );
        assert_eq!((items[1].row, items[1].response.as_str()), (3, ""));
    }
}
