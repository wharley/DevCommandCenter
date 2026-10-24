//! The "Objetivo" trigger in the composer toolbar and its panel
//! (`session-objective.css`): header, progress, the four fields, the limits
//! box, the persistence note and the actions.

use dcc_core::domain::objective::{ObjectiveStatus, ObjectiveTransition, SessionObjective};
use dcc_core::domain::objective_form::{available_transitions, summarize};
use gpui::{AnyElement, App, Context, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use super::ObjectiveBusy;
use crate::i18n::{Args, Translate};
use crate::screens::composer::Composer;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverSide};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

/// `.objective-panel`: `min(520px, 100vw - 24px)`.
const PANEL_WIDTH: f32 = 520.0;

fn status_key(status: ObjectiveStatus) -> &'static str {
    match status {
        ObjectiveStatus::Active => "active",
        ObjectiveStatus::Paused => "paused",
        ObjectiveStatus::Done => "done",
    }
}

fn transition_key(transition: ObjectiveTransition) -> &'static str {
    match transition {
        ObjectiveTransition::Pause => "pause",
        ObjectiveTransition::Resume => "resume",
        ObjectiveTransition::Complete => "complete",
    }
}

/// `--dcc-surface-edge`.
fn edge(cx: &App) -> Hsla {
    cx.theme().colors.border.opacity(0.8)
}

/// `color-mix(in srgb, var(--muted) 25%, var(--popover))`.
fn quiet(cx: &App) -> Hsla {
    let colors = &cx.theme().colors;
    mix_oklab(colors.muted, 0.25, colors.popover)
}

fn label(text: SharedString) -> impl IntoElement {
    div()
        .mb(px(7.0))
        .text_size(px(11.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::MEDIUM)
        .child(text)
}

/// A shadcn Textarea / Input box around one of the panel's fields.
fn field(content: impl IntoElement, min_height: Option<f32>, cx: &App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(edge(cx))
        .bg(colors.background)
        .px(px(12.0))
        .text_size(px(12.0))
        .text_color(colors.foreground)
        .cursor_text()
        .map(|field| match min_height {
            Some(height) => field.min_h(px(height)).py(px(8.0)).line_height(px(20.0)),
            None => field.h(px(32.0)).flex().items_center(),
        })
        .child(div().w_full().min_w_0().child(content))
}

fn progress(objective: &SessionObjective, cx: &App) -> AnyElement {
    let colors = &cx.theme().colors;
    let summary = summarize(objective);
    let (chip_text, chip_bg) = match objective.status {
        ObjectiveStatus::Active => (colors.accent_calm, Some(colors.accent_calm.opacity(0.09))),
        ObjectiveStatus::Paused => (colors.accent_idea, Some(colors.accent_idea.opacity(0.09))),
        ObjectiveStatus::Done => (colors.foreground, None),
    };
    let metric = |title: &str, value: String| {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t(title)),
            )
            .child(
                div()
                    .mt(px(3.0))
                    .text_size(px(16.0))
                    .line_height(px(22.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(value),
            )
    };
    let reason = objective.pause_reason.map(|reason| {
        let key = serde_json::to_value(reason)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        cx.t(&format!("composer.objective.pauseReason.{key}"))
    });
    div()
        .mb(px(18.0))
        .p(px(14.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(edge(cx))
        .bg(colors.background)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(7.0))
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(7.0))
                        .border_1()
                        .border_color(edge(cx))
                        .text_color(chip_text)
                        .when_some(chip_bg, |chip, background| chip.bg(background))
                        .child(cx.t(&format!(
                            "composer.objective.status.{}",
                            status_key(objective.status)
                        ))),
                )
                .children(reason)
                .child(div().ml_auto().child(cx.t_with(
                    "composer.objective.generation",
                    &Args::default().with("generation", objective.generation.to_string()),
                ))),
        )
        .child(
            div()
                .mt(px(14.0))
                .flex()
                .gap(px(10.0))
                .child(metric("composer.objective.design.turns", summary.turns))
                .child(metric(
                    "composer.objective.design.failures",
                    summary.failures,
                ))
                .child(metric(
                    "composer.objective.design.retries",
                    summary.retries.to_string(),
                )),
        )
        .when(summary.blocks_automatic_dispatch, |section| {
            section.child(
                div()
                    .mt(px(12.0))
                    .pt(px(10.0))
                    .border_t_1()
                    .border_color(edge(cx))
                    .text_size(px(11.0))
                    .line_height(px(17.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("composer.objective.dispatchBlocked")),
            )
        })
        .into_any_element()
}

fn footer(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let objective = composer.session.objective.as_ref();
    let busy = composer.objective.busy;
    let unavailable = busy.is_some();
    let small = |id: SharedString,
                 variant: ButtonVariant,
                 icon: Option<AnyElement>,
                 text: SharedString,
                 disabled: bool| {
        Button::new(id)
            .variant(variant)
            .size(ButtonSize::Sm)
            .disabled(disabled)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(11.0))
                    .children(icon)
                    .child(text),
            )
    };
    let saving = busy == Some(ObjectiveBusy::Save);
    let save = small(
        "objective-save".into(),
        ButtonVariant::Default,
        saving.then(|| spinner("objective-saving", 12.0).into_any_element()),
        if objective.is_some() {
            cx.t("composer.objective.save")
        } else {
            cx.t("composer.objective.create")
        },
        unavailable || !composer.objective.dirty(cx),
    )
    .on_click(cx.listener(|composer, _, _, cx| composer.save_objective(cx)));
    let transitions: Vec<AnyElement> = objective
        .map(|objective| available_transitions(objective.status))
        .unwrap_or_default()
        .iter()
        .map(|&transition| {
            let icon = if busy == Some(ObjectiveBusy::Transition(transition)) {
                spinner(
                    SharedString::from(format!("objective-{}-busy", transition_key(transition))),
                    12.0,
                )
                .into_any_element()
            } else {
                Icon::new(match transition {
                    ObjectiveTransition::Pause => "pause",
                    ObjectiveTransition::Resume => "play",
                    ObjectiveTransition::Complete => "circle-check",
                })
                .size(12.0)
                .into_any_element()
            };
            small(
                SharedString::from(format!("objective-{}", transition_key(transition))),
                ButtonVariant::Outline,
                Some(icon),
                cx.t(&format!(
                    "composer.objective.transition.{}",
                    transition_key(transition)
                )),
                unavailable,
            )
            .on_click(
                cx.listener(move |composer, _, _, cx| {
                    composer.transition_objective(transition, cx)
                }),
            )
            .into_any_element()
        })
        .collect();
    let colors = cx.theme().colors.clone();
    let clear = objective.is_some().then(|| {
        div().ml_auto().child(
            small(
                "objective-clear".into(),
                ButtonVariant::Ghost,
                Some(if busy == Some(ObjectiveBusy::Clear) {
                    spinner("objective-clearing", 12.0).into_any_element()
                } else {
                    Icon::new("trash-2").size(12.0).into_any_element()
                }),
                cx.t("composer.objective.clear"),
                unavailable,
            )
            .text_color(colors.muted_foreground)
            .hover_text_color(colors.destructive)
            .on_click(cx.listener(|composer, _, _, cx| composer.clear_objective(cx))),
        )
    });
    div()
        .flex()
        .flex_wrap()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .px(px(16.0))
        .py(px(12.0))
        .border_t_1()
        .border_color(edge(cx))
        .bg(quiet(cx))
        .child(save)
        .children(transitions)
        .children(clear)
        .into_any_element()
}

fn panel(composer: &Composer, window: &mut Window, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let state = &composer.objective;
    let calm = colors.accent_calm;
    let header = div()
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(12.0))
        .p(px(20.0))
        .bg(mix_oklab(calm, 0.07, colors.popover))
        .border_b_1()
        .border_color(edge(cx))
        .child(
            div()
                .flex()
                .size(px(40.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded(px(12.0))
                .bg(calm.opacity(0.1))
                .child(Icon::new("target").size(21.0).color(calm)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(15.0))
                        .line_height(px(22.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(cx.t("composer.objective.title")),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(11.0))
                        .line_height(px(17.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("composer.objective.design.subtitle")),
                ),
        )
        .child(
            Button::new("objective-close")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .disabled(state.busy.is_some())
                .on_click(cx.listener(|composer, _, window, cx| {
                    composer.set_objective_open(false, window, cx)
                }))
                .child(Icon::new("x").size(15.0)),
        );
    let limits = div()
        .p(px(14.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(edge(cx))
        .bg(quiet(cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(Icon::new("shield-check").size(15.0).color(calm))
                .child(cx.t("composer.objective.design.limits")),
        )
        .child(
            div()
                .mt(px(6.0))
                .mb(px(14.0))
                .text_size(px(11.0))
                .line_height(px(17.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("composer.objective.design.limitsHint")),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .min_h(px(32.0))
                                .child(label(cx.t("composer.objective.maxFailures"))),
                        )
                        .child(field(state.max_failures.clone(), None, cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .min_h(px(32.0))
                                .child(label(cx.t("composer.objective.maxTurns"))),
                        )
                        .child(field(state.max_turns.clone(), None, cx)),
                ),
        );
    // `max-height: min(760px, available)` less the header and the footer.
    let room = (f32::from(window.viewport_size().height) - 300.0).clamp(220.0, 560.0);
    let body = div()
        .id("objective-scroll")
        .flex()
        .flex_col()
        .min_h_0()
        .max_h(px(room))
        .overflow_y_scroll()
        .px(px(20.0))
        .py(px(18.0))
        .children(
            composer
                .session
                .objective
                .as_ref()
                .map(|objective| progress(objective, cx)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(
                    div()
                        .child(label(cx.t("composer.objective.intent")))
                        .child(field(state.intent.clone(), Some(90.0), cx)),
                )
                .child(
                    div()
                        .child(label(cx.t("composer.objective.doneWhen")))
                        .child(field(state.done_when.clone(), Some(72.0), cx)),
                )
                .child(limits),
        )
        .child(
            div()
                .mt(px(14.0))
                .text_size(px(10.0))
                .line_height(px(17.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("composer.objective.design.persistence")),
        );
    div()
        .debug_selector(|| "objective-panel".into())
        .flex()
        .flex_col()
        .rounded(px(16.0))
        .overflow_hidden()
        .border_1()
        .border_color(edge(cx))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .child(header)
        .child(body)
        .child(footer(composer, cx))
        .into_any_element()
}

/// The toolbar's "Objetivo" (`SessionObjectiveControl`).
pub(in crate::screens::composer) fn objective_control(
    composer: &Composer,
    window: &mut Window,
    cx: &mut Context<Composer>,
) -> Option<AnyElement> {
    composer.session.session_id.as_ref()?;
    let colors = cx.theme().colors.clone();
    let objective = composer.session.objective.as_ref();
    let tone = match objective.map(|objective| objective.status) {
        Some(ObjectiveStatus::Active) => tw::EMERALD_600.hsla(),
        Some(ObjectiveStatus::Paused) => tw::AMBER_600.hsla(),
        _ => colors.muted_foreground,
    };
    let dark = cx.theme().appearance == crate::theme::Appearance::Dark;
    let tone = match objective.map(|objective| objective.status) {
        Some(ObjectiveStatus::Active) if dark => tw::EMERALD_400.hsla(),
        Some(ObjectiveStatus::Paused) if dark => tw::AMBER_400.hsla(),
        _ => tone,
    };
    let hover = colors.accent.opacity(0.6);
    let trigger = div()
        .flex()
        .h(px(28.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .rounded(px(9.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .map(|trigger| match objective {
            Some(_) => trigger
                .bg(colors.accent.opacity(0.6))
                .text_color(colors.foreground),
            None => trigger.text_color(colors.muted_foreground.opacity(0.7)),
        })
        .child(Icon::new("target").size(13.0).stroke(1.8).color(tone))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .child(cx.t("composer.objective.compact")),
        );
    let title = cx.t("composer.objective.tooltipTitle");
    let description = cx.t("composer.objective.tooltipDescription");
    let open = composer.objective.open;
    let trigger = Tooltip::new("objective-tip", trigger, move |_, cx| {
        div()
            .flex()
            .flex_col()
            .max_w(px(320.0))
            .child(div().font_weight(FontWeight::MEDIUM).child(title.clone()))
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(cx.theme().colors.background.opacity(0.75))
                    .child(description.clone()),
            )
            .into_any_element()
    })
    .enabled(!open);
    let card = open.then(|| panel(composer, window, cx));
    let entity = cx.entity().downgrade();
    Some(
        Popover::new(
            "objective",
            open,
            trigger,
            move |_, _| card.unwrap_or_else(|| div().into_any_element()),
            move |open, window, cx| {
                let _ = entity.update(cx, |composer, cx| {
                    composer.set_objective_open(open, window, cx)
                });
            },
        )
        .side(PopoverSide::Top)
        .side_offset(10.0)
        .width(PANEL_WIDTH)
        .padding(0.0)
        .into_any_element(),
    )
}
