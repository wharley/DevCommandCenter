//! The line under an answer: how long it took and how many tokens, "Abrir
//! conversa" and the 👍 / 👎 — all but a reaction the person gave show only
//! while the answer is hovered (Tucano's receipt), the line's height kept so
//! the rows do not jump. When the folded steps line is there it already says
//! how long the work took, so the receipt keeps the tokens only.

use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::team_room::{Receipt, RoomLine};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;

impl TeamRoom {
    /// "24 s · 42k tokens", the duration left out when `duration` is false.
    pub(super) fn receipt_text(receipt: Receipt, duration: bool, cx: &gpui::App) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(seconds) = receipt.seconds.filter(|_| duration) {
            parts.push(if seconds < 60 {
                format!("{seconds} s")
            } else {
                format!("{} min {} s", seconds / 60, seconds % 60)
            });
        }
        if let Some(tokens) = receipt.tokens.filter(|tokens| *tokens > 0) {
            parts.push(
                cx.t_with(
                    "agents.room.tokens",
                    &Args::default().with("count", crate::data::goal::format_tokens(tokens)),
                )
                .to_string(),
            );
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// The footer of an answer whose row is the hover `group`.
    pub(super) fn answer_footer(
        &self,
        index: usize,
        line: &RoomLine,
        group: &SharedString,
        steps_shown: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let receipt = line
            .receipt
            .and_then(|receipt| Self::receipt_text(receipt, !steps_shown, cx));
        let agent_id = line.agent_id.clone();
        div()
            .debug_selector(move || format!("room-footer-{index}"))
            .h(px(22.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .opacity(0.0)
                    .group_hover(group.clone(), |style| style.opacity(1.0))
                    .children(receipt)
                    .child(
                        div()
                            .id(SharedString::from(format!("room-open-{index}")))
                            .debug_selector(move || format!("room-open-{index}"))
                            .cursor_pointer()
                            .hover(|link| link.text_color(colors.foreground))
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.open_conversation(&agent_id, cx)
                            }))
                            .child(cx.t("agents.room.open")),
                    ),
            )
            .child(self.reaction_buttons(index, line, group, cx))
            .into_any_element()
    }
}
