//! The export history tab: the sessions waiting in the queue (attempts,
//! events, next retry, the error, "Reprocessar"), then the exports kept for
//! audit.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use crate::data::ai_memory_settings::short_session;
use crate::data::decision_provider::medium_date_time;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

impl Settings {
    pub(super) fn memory_history(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let memory = &self.memory;
        let english = cx.global::<crate::i18n::I18n>().locale() == crate::i18n::Locale::En;
        let loading = memory.history_loading;
        let outbox = memory.outbox.clone().unwrap_or_default();
        let exports = memory.exports.clone().unwrap_or_default();
        let queue: AnyElement = if outbox.is_empty() {
            div()
                .mt(px(16.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .bg(colors.background)
                .p(px(12.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("settings.aiMemory.outboxEmpty"))
                .into_any_element()
        } else {
            let rows = outbox.iter().enumerate().map(|(index, entry)| {
                let session = entry.session_id.clone();
                let retrying = memory.retrying.as_deref() == Some(entry.session_id.as_str());
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .p(px(12.0))
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(colors.border.opacity(0.4))
                    })
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .font_family(cx.theme().fonts.mono.clone())
                                    .text_size(px(11.0))
                                    .text_color(colors.foreground)
                                    .child(entry.session_id.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(11.0))
                                    .text_color(colors.muted_foreground)
                                    .child(
                                        cx.t_with(
                                            "settings.aiMemory.outboxMeta",
                                            &Args::default()
                                                .with("attempts", entry.attempts.to_string())
                                                .with("next", entry.next_attempt_at.clone())
                                                .with("events", entry.event_count.to_string()),
                                        ),
                                    ),
                            )
                            .children(entry.last_error.clone().map(|error| {
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(11.0))
                                    .text_color(colors.destructive)
                                    .child(error)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-memory-retry-{}",
                            entry.session_id
                        )))
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .disabled(retrying)
                        .on_click(cx.listener(move |settings, _, _, cx| {
                            settings.retry_memory(session.clone(), cx)
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .children(
                                    retrying.then(|| spinner("settings-memory-retrying", 14.0)),
                                )
                                .child(cx.t("settings.aiMemory.retry")),
                        ),
                    )
            });
            div()
                .mt(px(16.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .bg(colors.background)
                .children(rows)
                .into_any_element()
        };
        let past: AnyElement = if exports.is_empty() {
            div()
                .mt(px(12.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("settings.aiMemory.historyEmpty"))
                .into_any_element()
        } else {
            let rows = exports.iter().enumerate().map(|(index, entry)| {
                let completed = entry.status == "completed";
                div()
                    .p(px(12.0))
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(colors.border.opacity(0.4))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .truncate()
                                    .font_family(cx.theme().fonts.mono.clone())
                                    .text_size(px(11.0))
                                    .text_color(colors.foreground)
                                    .child(
                                        cx.t_with(
                                            "settings.aiMemory.historySession",
                                            &Args::default()
                                                .with("id", short_session(&entry.session_id)),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(24.0))
                                    .px(px(8.0))
                                    .rounded(px(26.0))
                                    .border_1()
                                    .border_color(if completed {
                                        gpui::transparent_black()
                                    } else {
                                        colors.border
                                    })
                                    .when(completed, |badge| badge.bg(colors.secondary))
                                    .text_size(px(10.0))
                                    .text_color(colors.foreground)
                                    .child(cx.t(&format!(
                                        "settings.aiMemory.historyStatus.{}",
                                        entry.status
                                    ))),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                cx.t_with(
                                    "settings.aiMemory.historyMeta",
                                    &Args::default()
                                        .with("attempts", entry.attempts.to_string())
                                        .with("accepted", entry.accepted_count.to_string())
                                        .with("events", entry.event_count.to_string())
                                        .with(
                                            "finished",
                                            medium_date_time(&entry.finished_at, english),
                                        ),
                                ),
                            ),
                    )
                    .children(entry.error.clone().map(|error| {
                        div()
                            .mt(px(4.0))
                            .text_size(px(11.0))
                            .text_color(colors.destructive)
                            .child(error)
                    }))
            });
            div()
                .id("settings-memory-exports")
                .mt(px(12.0))
                .max_h(px(288.0))
                .overflow_y_scroll()
                .rounded(px(10.0))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .bg(colors.background)
                .children(rows)
                .into_any_element()
        };
        div()
            .debug_selector(|| "settings-memory-history".into())
            .rounded(px(14.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .p(px(16.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t("settings.aiMemory.outboxTitle")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("settings.aiMemory.outboxHint")),
                            ),
                    )
                    .child(
                        Button::new("settings-memory-refresh-history")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .disabled(loading)
                            .on_click(
                                cx.listener(|settings, _, _, cx| settings.load_memory_history(cx)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(if loading {
                                        spinner("settings-memory-history-loading", 14.0)
                                            .into_any_element()
                                    } else {
                                        Icon::new("refresh-cw").size(14.0).into_any_element()
                                    })
                                    .child(cx.t("settings.aiMemory.refresh")),
                            ),
                    ),
            )
            .child(queue)
            .child(
                div()
                    .mt(px(16.0))
                    .border_t_1()
                    .border_color(colors.border.opacity(0.4))
                    .pt(px(16.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t("settings.aiMemory.historyTitle")),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(11.0))
                            .line_height(px(17.9))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("settings.aiMemory.historyHint")),
                    )
                    .child(past),
            )
            .into_any_element()
    }
}
