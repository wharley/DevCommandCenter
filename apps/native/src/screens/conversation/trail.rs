//! `ConversationTrail` on the task's conversation: a stop per prompt with
//! its turn's answer (`ui::trail` draws it), a click scrolls the thread there.

use std::rc::Rc;

use dcc_core::domain::timeline::{Role, TimelineMessage};
use gpui::{AnyElement, Context, ListOffset, px};

use super::ConversationView;
pub(super) use crate::ui::trail::TrailState;
use crate::ui::trail::{TrailItem, preview};

/// `deriveConversationTrailItems`: one stop per prompt (the backend's own
/// `[DCC]` turns are not the person's), each with the last answer of its
/// turn, or of the prompt before it when the history has no turn ids.
pub(super) fn trail_items(rows: &[TimelineMessage]) -> Vec<TrailItem> {
    let mut items: Vec<TrailItem> = Vec::new();
    let mut turns: Vec<(&str, usize)> = Vec::new();
    for (row, message) in rows.iter().enumerate() {
        match message.role {
            Role::User if message.hand_back.is_none() => {
                if let Some(turn) = message.turn_id.as_deref() {
                    turns.push((turn, items.len()));
                }
                items.push(TrailItem {
                    row,
                    prompt: preview(&message.content),
                    response: String::new(),
                });
            }
            Role::Assistant => {
                let response = preview(&message.content);
                if response.is_empty() {
                    continue;
                }
                let index = message
                    .turn_id
                    .as_deref()
                    .and_then(|turn| turns.iter().rev().find(|(id, _)| *id == turn))
                    .map(|(_, index)| *index)
                    .or_else(|| items.len().checked_sub(1));
                if let Some(item) = index.and_then(|index| items.get_mut(index)) {
                    item.response = response;
                }
            }
            _ => {}
        }
    }
    items
}

impl ConversationView {
    /// The prompt being read: the last one at or above the thread's top.
    fn trail_current(&self, items: &[TrailItem]) -> usize {
        let top = self.list.logical_scroll_top().item_ix;
        // The list's first item is the top room; row `n` is item `n + 1`.
        items.iter().rposition(|item| item.row < top).unwrap_or(0)
    }

    /// The trail, or nothing below two prompts.
    pub(super) fn trail(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.data.clone()?;
        let items = Rc::new(trail_items(&data.rows));
        let current = self.trail_current(&items);
        let width = self.viewport_width;
        crate::ui::trail::trail(
            "conversation-trail",
            items,
            current,
            width,
            |view: &mut Self| &mut view.trail,
            |view, item, _| {
                view.list.scroll_to(ListOffset {
                    item_ix: item.row + 1,
                    offset_in_item: px(0.0),
                });
            },
            |_, cx| cx.notify(),
            self,
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, role: Role, turn: Option<&str>, content: &str) -> TimelineMessage {
        TimelineMessage {
            id: id.into(),
            role,
            turn_id: turn.map(str::to_string),
            turn_settled: true,
            turn_started_at: None,
            turn_ended_at: None,
            assistant_phase: None,
            content: content.into(),
            label: String::new(),
            model: None,
            streaming: false,
            created_at: None,
            status: None,
            annotations: Vec::new(),
            plan_mode: false,
            answers_plan_mode: false,
            evidence: None,
            retry_of_turn_id: None,
            action: None,
            delegation: None,
            delegation_turn_id: None,
            hand_back: None,
        }
    }

    #[test]
    fn a_stop_per_prompt_with_its_turns_answer() {
        let rows = vec![
            row("u1", Role::User, Some("t1"), "Primeira  pergunta\n"),
            row("a1", Role::Assistant, Some("t1"), "resposta um"),
            row("s", Role::System, None, "nota"),
            row("u2", Role::User, Some("t2"), "Segunda"),
            row("a0", Role::Assistant, Some("t1"), "fim do um"),
        ];
        let items = trail_items(&rows);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].row, 0);
        assert_eq!(items[0].prompt, "Primeira pergunta");
        assert_eq!(items[0].response, "fim do um");
        assert_eq!(items[1].row, 3);
        assert_eq!(items[1].response, "");
    }

    #[test]
    fn without_turn_ids_the_answer_goes_to_the_prompt_before_it() {
        let rows = vec![
            row("u1", Role::User, None, "a"),
            row("a1", Role::Assistant, None, "b"),
        ];
        assert_eq!(trail_items(&rows)[0].response, "b");
    }
}
