//! The person's message in the team room: their words in a bubble, the
//! images they attached as thumbnails (`room_images`) and their links as
//! chips (`room_links`) under it, and who took it.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::theme::ActiveTheme;

impl TeamRoom {
    pub(super) fn person_line(
        &self,
        key: String,
        text: String,
        note: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        // Images and links go under the words.
        let (text, images) = crate::data::message_links::split_images(&text);
        let thumbs = self.image_thumbs(&key, images, cx);
        let (words, links) = crate::data::message_links::split_links(&text);
        let chips = self.link_chips(&key, links, cx);
        let bubble = (!words.is_empty()).then(|| {
            div()
                .max_w(px(520.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(14.0))
                .bg(colors.accent)
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(colors.foreground)
                .child(words)
        });
        div()
            .debug_selector(move || key.clone())
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.0))
            .children(bubble)
            .children(thumbs)
            .children(chips)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(note),
            )
            .into_any_element()
    }
}
