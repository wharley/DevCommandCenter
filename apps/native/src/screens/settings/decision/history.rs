//! The history tab: each decision with its status, mode, point, the
//! completion review's verdict, the model, when, the counts, "Ver critérios
//! avaliados" (`DecisionEvaluationDetails`), what was selected and the error.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::Settings;
use crate::data::decision_provider::{HistoryEntry, medium_date_time};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

/// `Badge` at `h-6 px-2 text-[10px] font-normal`.
fn badge(
    text: SharedString,
    tone: Option<gpui::Hsla>,
    outline: bool,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    let (background, foreground, border) = match (tone, outline) {
        (Some(tone), _) => (tone.opacity(0.1), tone, gpui::transparent_black()),
        (None, true) => (gpui::transparent_black(), colors.foreground, colors.border),
        (None, false) => (
            colors.secondary,
            colors.secondary_foreground,
            gpui::transparent_black(),
        ),
    };
    div()
        .flex()
        .items_center()
        .h(px(24.0))
        .px(px(8.0))
        .rounded(px(26.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_size(px(10.0))
        .text_color(foreground)
        .child(text)
        .into_any_element()
}

impl Settings {
    pub(super) fn decision_history(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let decision = &self.decision;
        let loading = decision.history_loading;
        let body: AnyElement = match &decision.history {
            None => div()
                .mt(px(16.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("settings.decisionProvider.historyLoading"))
                .into_any_element(),
            Some(entries) if entries.is_empty() => div()
                .mt(px(16.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("settings.decisionProvider.historyEmpty"))
                .into_any_element(),
            Some(entries) => {
                let rows: Vec<AnyElement> = entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| self.decision_row(index, entry, cx))
                    .collect();
                div()
                    .id("settings-decision-history-list")
                    .mt(px(16.0))
                    .max_h(px(480.0))
                    .overflow_y_scroll()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(colors.border.opacity(0.5))
                    .bg(colors.background)
                    .children(rows)
                    .into_any_element()
            }
        };
        div()
            .debug_selector(|| "settings-decision-history".into())
            .flex()
            .flex_col()
            .min_h_0()
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
                                    .child(cx.t("settings.decisionProvider.historyTitle")),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(12.0))
                                    .line_height(px(19.5))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("settings.decisionProvider.historyHint")),
                            ),
                    )
                    .child(
                        Button::new("settings-decision-refresh")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .disabled(loading)
                            .on_click(cx.listener(|settings, _, _, cx| {
                                settings.refresh_decision_history(cx)
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(if loading {
                                        spinner("settings-decision-refreshing", 14.0)
                                            .into_any_element()
                                    } else {
                                        Icon::new("refresh-cw").size(14.0).into_any_element()
                                    })
                                    .child(cx.t("settings.decisionProvider.refreshHistory")),
                            ),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    fn decision_row(
        &self,
        index: usize,
        entry: &HistoryEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<crate::i18n::I18n>().locale() == crate::i18n::Locale::En;
        let failed = entry.status != "completed";
        let completion = entry.completion_key().map(|(key, needs_review)| {
            badge(
                cx.t(key),
                needs_review.then_some(colors.destructive),
                false,
                cx,
            )
        });
        let meta = if entry.status == "failed" {
            cx.t_with(
                "settings.decisionProvider.historyFailedMeta",
                &Args::default()
                    .with("candidates", entry.candidate_count.to_string())
                    .with("duration", entry.duration_ms.to_string()),
            )
        } else {
            cx.t_with(
                "settings.decisionProvider.historyMeta",
                &Args::default()
                    .with("candidates", entry.candidate_count.to_string())
                    .with("selected", entry.selected_count.to_string())
                    .with("threshold", entry.threshold.to_string())
                    .with("duration", entry.duration_ms.to_string()),
            )
        };
        let selected = (entry.status == "completed").then(|| {
            let text = if entry.selected_labels.is_empty() {
                let indices = if entry.selected_indices.is_empty() {
                    cx.t("settings.decisionProvider.none").to_string()
                } else {
                    entry
                        .selected_indices
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                cx.t_with(
                    "settings.decisionProvider.selectedIndices",
                    &Args::default().with("indices", indices),
                )
            } else {
                cx.t_with(
                    "settings.decisionProvider.selectedLabels",
                    &Args::default().with("labels", entry.selected_labels.join(", ")),
                )
            };
            div()
                .font_family(cx.theme().fonts.mono.clone())
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(text)
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
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
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(badge(
                                cx.t(&format!(
                                    "settings.decisionProvider.historyStatus.{}",
                                    entry.status
                                )),
                                failed.then_some(colors.destructive),
                                false,
                                cx,
                            ))
                            .child(badge(
                                cx.t(&format!(
                                    "settings.decisionProvider.historyModes.{}",
                                    entry.mode
                                )),
                                None,
                                true,
                                cx,
                            ))
                            .child(badge(
                                cx.t(&format!(
                                    "settings.decisionProvider.historyPoints.{}",
                                    entry.decision_point
                                )),
                                None,
                                true,
                                cx,
                            ))
                            .children(completion)
                            .child(
                                div()
                                    .font_family(cx.theme().fonts.mono.clone())
                                    .text_size(px(11.0))
                                    .text_color(colors.foreground)
                                    .child(entry.model.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(medium_date_time(&entry.created_at, english)),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(meta),
            )
            .children(self.evaluation_details(entry, cx))
            .children(selected)
            .children(entry.error.clone().map(|error| {
                div()
                    .text_size(px(11.0))
                    .text_color(colors.destructive)
                    .child(error)
            }))
            .into_any_element()
    }

    /// `DecisionEvaluationDetails`: a `<details>` of the criteria scores.
    fn evaluation_details(
        &self,
        entry: &HistoryEntry,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let evaluation = entry.evaluation.as_ref()?;
        let colors = cx.theme().colors.clone();
        let open = self.decision.open_details.contains(&entry.id);
        let id = entry.id;
        let scores = evaluation.scores.iter().map(|score| {
            let key = format!("settings.decisionProvider.criteria.{}", score.key);
            let name: SharedString = if cx.global::<crate::i18n::I18n>().has(&key, &Args::default())
            {
                cx.t(&key)
            } else {
                score.key.clone().into()
            };
            div()
                .flex()
                .justify_between()
                .gap(px(12.0))
                .child(div().min_w_0().child(name))
                .child(
                    div()
                        .font_family(cx.theme().fonts.mono.clone())
                        .child(format!("{}%", (score.probability * 100.0).round())),
                )
        });
        Some(
            div()
                .mt(px(8.0))
                .min_w_0()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "settings-decision-details-{id}"
                        )))
                        .debug_selector(move || format!("settings-decision-details-{id}"))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |settings, _, _, cx| {
                            let details = &mut settings.decision.open_details;
                            if !details.remove(&id) {
                                details.insert(id);
                            }
                            cx.notify();
                        }))
                        .child(
                            Icon::new(if open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            })
                            .size(12.0),
                        )
                        .child(cx.t("settings.decisionProvider.evaluationDetails")),
                )
                .when(open, |details| {
                    details
                        .child(
                            div()
                                .mt(px(4.0))
                                .child(cx.t("settings.decisionProvider.probabilityHint")),
                        )
                        .when(evaluation.context_truncated, |details| {
                            details.child(
                                div()
                                    .mt(px(4.0))
                                    .child(cx.t("settings.decisionProvider.contextIncomplete")),
                            )
                        })
                        .child(
                            div()
                                .mt(px(8.0))
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .children(scores),
                        )
                })
                .into_any_element(),
        )
    }
}
