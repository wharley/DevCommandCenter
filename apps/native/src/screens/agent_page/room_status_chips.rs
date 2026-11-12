//! Beside the room's title, what the person needs at a glance: "2
//! trabalhando" and, in amber, "1 esperando você" (approvals, questions,
//! tasks waiting on them, replies to send). A click goes to the room's end,
//! where the live work and those cards are. None when nothing moves.

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_rows::Row;
use crate::data::room_inbox::CardKind;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};

impl TeamRoom {
    /// The agents at work in this room, and what waits on the person here.
    fn status_counts(&self, cx: &mut Context<Self>) -> (usize, usize) {
        let model = self.list.model();
        let mut working: Vec<&str> = self
            .live_feeds(cx)
            .into_iter()
            .map(|feed| feed.agent_id.as_str())
            .collect();
        working.sort_unstable();
        working.dedup();
        let tasks = model
            .rows
            .iter()
            .filter(|row| {
                matches!(row, Row::Card { card, .. } if matches!(card.kind, CardKind::TaskWaiting { .. }))
            })
            .count();
        let replies = cx
            .try_global::<crate::data::agent_replies::Replies>()
            .map_or(0, |replies| replies.in_room(&self.room).count());
        let waiting = self.approvals(&model.agents).len()
            + self.questions(&model.agents).len()
            + tasks
            + replies;
        (working.len(), waiting)
    }

    pub(super) fn status_chips(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (working, waiting) = self.status_counts(cx);
        if working == 0 && waiting == 0 {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let dark = cx.theme().appearance == Appearance::Dark;
        let amber = if dark { tw::AMBER_400 } else { tw::AMBER_600 }.hsla();
        let chip = |id: &'static str, dot: Hsla, text: SharedString, cx: &mut Context<Self>| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(2.0))
                .rounded_full()
                .border_1()
                .border_color(dot.opacity(0.3))
                .bg(dot.opacity(0.08))
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(dot)
                .cursor_pointer()
                .on_click(cx.listener(|room, _, _, cx| {
                    room.list.follow();
                    cx.notify();
                }))
                .child(div().size(px(6.0)).rounded_full().bg(dot))
                .child(text)
        };
        let mut chips: Vec<AnyElement> = Vec::new();
        if working > 0 {
            let text = cx.t_with("agents.room.status.working", &Args::count(working as i64));
            chips.push(chip("room-status-working", colors.primary, text, cx).into_any_element());
        }
        if waiting > 0 {
            let text = cx.t_with("agents.room.status.waiting", &Args::count(waiting as i64));
            chips.push(chip("room-status-waiting", amber, text, cx).into_any_element());
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .children(chips)
                .into_any_element(),
        )
    }
}
