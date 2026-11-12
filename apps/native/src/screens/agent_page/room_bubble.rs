//! The person's message in the team room: their words in a bubble, drawn
//! as the composer's chat draws a prompt (links inline with their site's
//! icon, an `@Agent` a pill in the agent's color), the images they attached
//! as thumbnails (`room_images`) under it, and who took it.

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_said::Said;
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::theme::ActiveTheme;

impl TeamRoom {
    pub(super) fn person_line(
        &self,
        key: String,
        said: &Said,
        note: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let thumbs = self.image_thumbs(&key, said.images.clone(), cx);
        let bubble = (!said.words.is_empty()).then(|| {
            div()
                .max_w(px(520.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(14.0))
                .bg(colors.accent)
                .text_size(px(13.0))
                .text_color(colors.foreground)
                .child(render_markdown(
                    &said.blocks,
                    MarkdownStyle::prompt(),
                    &format!("{key}-words"),
                    window,
                    cx,
                ))
        });
        div()
            .debug_selector(move || key.clone())
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.0))
            .children(bubble)
            .children(thumbs)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(note),
            )
            .into_any_element()
    }
}
