//! The day separator: a small centered pill ("Hoje", "Ontem", or the date)
//! between the rows of different days (`room_flow`).

use chrono::NaiveDate;
use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_flow::{date_text, nearness};
use super::room_view::english;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;

impl TeamRoom {
    pub(super) fn day_row(&self, day: NaiveDate, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let today = chrono::Local::now().date_naive();
        let label = match nearness(day, today) {
            Some(key) => cx.t(key).to_string(),
            None => date_text(day, today, english(cx)),
        };
        div()
            .debug_selector(move || format!("room-day-{day}"))
            .flex()
            .justify_center()
            .child(
                div()
                    .px(px(10.0))
                    .py(px(2.0))
                    .rounded(px(10.0))
                    .bg(colors.muted.opacity(0.6))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .into_any_element()
    }
}
