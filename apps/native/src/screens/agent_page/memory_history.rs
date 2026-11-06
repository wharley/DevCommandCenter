//! "Histórico" of an agent's memory: each change, newest first, with what
//! it was about, when, and how many notes the memory had right after it.
//! "Voltar para esta" brings that version back (itself a new version, so it
//! can be undone the same way).

use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::memory::MemoryCard;
use crate::data::agent_memory::{AgentMemory, MemoryVersion};
use crate::data::time::medium_date_time;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_header};

fn change_icon(change: &str) -> &'static str {
    match change {
        "learned" => "sparkles",
        "edited" => "pencil",
        "removed" => "trash-2",
        "restored" => "rotate-ccw",
        _ => "plus",
    }
}

impl MemoryCard {
    fn version_row(
        &self,
        index: usize,
        version: &MemoryVersion,
        english: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let when = medium_date_time(&version.created_at, english);
        let (title, summary) = if version.change == "restored" {
            (
                cx.t_with(
                    "agents.memory.changes.restored",
                    &Args::default().with("when", medium_date_time(&version.summary, english)),
                ),
                None,
            )
        } else {
            (
                cx.t(&format!("agents.memory.changes.{}", version.change)),
                Some(version.summary.clone()),
            )
        };
        let count = cx.t_with(
            "agents.memory.noteCount",
            &Args::default().with_count(version.notes.len() as i64),
        );
        let id = version.id;
        // The first row is the memory as it is now.
        let restore = (index > 0).then(|| {
            Button::new(SharedString::from(format!("agent-memory-restore-{index}")))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .disabled(self.busy)
                .on_click(cx.listener(move |card, _, _, cx| card.restore(id, cx)))
                .child(cx.t("agents.memory.restore"))
        });
        div()
            .debug_selector(move || format!("agent-memory-version-{index}"))
            .flex()
            .items_start()
            .gap(px(10.0))
            .py(px(10.0))
            .when(index > 0, |row| {
                row.border_t_1().border_color(colors.border.opacity(0.7))
            })
            .child(
                div()
                    .flex_none()
                    .pt(px(2.0))
                    .text_color(colors.muted_foreground)
                    .child(Icon::new(change_icon(&version.change)).size(14.0)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(title),
                    )
                    .children(summary.map(|summary| {
                        div()
                            .mt(px(2.0))
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(colors.muted_foreground)
                            .child(summary)
                    }))
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(format!("{when} · {count}")),
                    ),
            )
            .children(restore)
            .into_any_element()
    }

    pub(crate) fn history_dialog(
        &self,
        memory: &AgentMemory,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.history_open {
            return None;
        }
        let english = cx.global::<I18n>().locale() == Locale::En;
        let rows: Vec<AnyElement> = memory
            .versions
            .iter()
            .enumerate()
            .map(|(index, version)| self.version_row(index, version, english, cx))
            .collect();
        let card = cx.entity().downgrade();
        Some(
            Dialog::new("agent-memory-history-dialog", move |_: &mut Window, cx| {
                let _ = card.update(cx, |card, cx| {
                    card.history_open = false;
                    cx.notify();
                });
            })
            .max_width(520.0)
            .max_height(560.0)
            .child(dialog_header(
                cx.t("agents.memory.historyTitle"),
                Some(cx.t("agents.memory.historyDescription")),
                cx,
            ))
            .child(
                div()
                    .id("agent-memory-history")
                    .debug_selector(|| "agent-memory-history".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element(),
        )
    }
}
