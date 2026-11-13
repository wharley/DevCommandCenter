//! A section's header in the hub's list: a chevron that folds it, its name
//! and how many, green when about to be merged, blue when it waits on
//! someone else, amber when it needs the person.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::PrHub;
use crate::data::pr_sections::{Section, Tone};
use crate::i18n::Translate;
use crate::theme::{ActiveTheme, StateTone};
use crate::ui::Icon;

impl PrHub {
    pub(super) fn section_header(
        &self,
        section: Section,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let tone = match section.tone() {
            Tone::Done => StateTone::Done,
            Tone::Waiting => StateTone::Waiting,
            Tone::NeedsYou => StateTone::NeedsYou,
            Tone::Quiet => StateTone::Quiet,
        }
        .text(cx.theme());
        let folded = self.section_folded(section);
        let key = section.key();
        div()
            .id(SharedString::from(format!("pr-section-{key}")))
            .debug_selector(move || format!("pr-section-{key}"))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(16.0))
            .pt(px(12.0))
            .pb(px(4.0))
            .text_size(px(10.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(tone)
            .cursor_pointer()
            .on_click(cx.listener(move |hub, _, _, cx| hub.toggle_section(section, cx)))
            .child(
                Icon::new(if folded {
                    "chevron-right"
                } else {
                    "chevron-down"
                })
                .size(12.0)
                .color(tone),
            )
            .child(cx.t(&format!("pullRequests.sections.{key}")).to_uppercase())
            .child(
                div()
                    .text_color(colors.muted_foreground.opacity(0.7))
                    .child(count.to_string()),
            )
            .into_any_element()
    }
}
