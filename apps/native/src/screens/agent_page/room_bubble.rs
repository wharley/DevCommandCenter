//! The person's message in the team room: their words in a bubble (an
//! `@Agent` in them a pill in the agent's color), the images they attached
//! as thumbnails (`room_images`) and their links as chips (`room_links`)
//! under it, and who took it.

use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, SharedString, StyledText, div, prelude::*, px,
};

use super::room::TeamRoom;
use super::room_said::Said;
use crate::theme::ActiveTheme;

impl TeamRoom {
    pub(super) fn person_line(
        &self,
        key: String,
        said: &Said,
        note: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let thumbs = self.image_thumbs(&key, said.images.clone(), cx);
        let chips = self.link_chips(&key, said.links.clone(), cx);
        let bubble = (!said.words.is_empty()).then(|| {
            let highlights: Vec<_> = said
                .mentions
                .iter()
                .map(|(range, rgb)| {
                    let (fill, text) = crate::ui::mention_tint::colors(*rgb, cx);
                    (
                        range.clone(),
                        HighlightStyle {
                            color: Some(text),
                            background_color: Some(fill),
                            font_weight: Some(FontWeight::MEDIUM),
                            ..HighlightStyle::default()
                        },
                    )
                })
                .collect();
            div()
                .max_w(px(520.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(14.0))
                .bg(colors.accent)
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(colors.foreground)
                .child(StyledText::new(said.words.clone()).with_highlights(highlights))
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
