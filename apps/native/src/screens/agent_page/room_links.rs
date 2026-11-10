//! The chips under the person's words in the team room, one per link
//! (`data::message_links`): a pull request with its icon and `app#924`,
//! another link with its host and path; a click opens it.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::message_links::MessageLink;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::Icon;

impl TeamRoom {
    pub(super) fn link_chips(
        &self,
        key: &str,
        links: Vec<MessageLink>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if links.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let chips = links.into_iter().enumerate().map(|(index, link)| {
            let url = link.url.clone();
            let icon = if link.pull_request {
                "git-pull-request"
            } else {
                "link-2"
            };
            div()
                .id(SharedString::from(format!("{key}-link-{index}")))
                .debug_selector({
                    let key = key.to_string();
                    move || format!("{key}-link-{index}")
                })
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.0))
                .h(px(24.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.border.opacity(0.8))
                .bg(colors.card)
                .text_size(px(12.0))
                .text_color(colors.foreground)
                .cursor_pointer()
                .hover(|chip| chip.bg(colors.accent.opacity(0.6)))
                .on_click(move |_, _, cx| cx.open_url(&url))
                .child(Icon::new(icon).size(12.0).color(if link.pull_request {
                    tw::GREEN_500.hsla()
                } else {
                    colors.muted_foreground
                }))
                .child(link.label)
        });
        Some(
            div()
                .flex()
                .flex_wrap()
                .justify_end()
                .gap(px(6.0))
                .max_w(px(520.0))
                .children(chips)
                .into_any_element(),
        )
    }
}
