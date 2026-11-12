//! The provider's own goal (`/goal`: Claude, Codex, Cursor, Grok) behind
//! "Objetivo": "Iniciar objetivo" sends the intent and "pronto quando" as
//! `/goal`, which then works on its own until the condition holds. While
//! that goal lives the panel shows it, with "Continuar" and "Limpar" as the
//! strip above the composer has them.

use dcc_core::domain::goal::{GoalStatus, SessionGoal};
use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*, px};

use super::panel::{edge, field, label, quiet};
use crate::data::goal::{GoalAction, present};
use crate::i18n::Translate;
use crate::screens::composer::Composer;
use crate::screens::composer::strips::StripAction;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster};

/// The `/goal` turn for an intent and its "pronto quando" (`prefix` is that
/// label in the person's language); none without an intent.
pub(crate) fn goal_prompt(intent: &str, done_when: &str, prefix: &str) -> Option<String> {
    let flat = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let intent = flat(intent);
    if intent.is_empty() {
        return None;
    }
    let done_when = flat(done_when);
    Some(if done_when.is_empty() {
        format!("/goal {intent}")
    } else {
        let stop = if intent.ends_with(['.', '!', '?']) {
            ""
        } else {
            "."
        };
        format!("/goal {intent}{stop} {prefix}: {done_when}")
    })
}

/// The provider's goal that still runs or waits (a finished one is the
/// strip's to close).
pub(crate) fn live_goal(composer: &Composer) -> Option<&SessionGoal> {
    composer
        .session
        .goal
        .as_ref()
        .filter(|goal| goal.status != GoalStatus::Complete)
}

impl Composer {
    /// "Objetivo" shows only where the provider has its own goal.
    pub(crate) fn offers_goal(&self) -> bool {
        self.provider()
            .is_some_and(|provider| provider.capabilities.supports_native_goal)
    }

    /// "Iniciar objetivo": the fields as the provider's `/goal`.
    pub(crate) fn start_native_goal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let intent = self.objective.intent.read(cx).text().to_string();
        let done_when = self.objective.done_when.read(cx).text().to_string();
        let prefix = cx.t("composer.objective.doneWhen");
        let Some(prompt) = goal_prompt(&intent, &done_when, &prefix) else {
            let reason = cx.t("composer.objective.errors.intent_required");
            Toaster::error(reason, cx);
            return;
        };
        self.send_text(&prompt, cx);
        self.objective.clear(cx);
        self.set_objective_open(false, window, cx);
    }
}

/// The panel's body: the provider's live goal, or the two fields.
pub(super) fn body(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let Some(goal) = live_goal(composer) else {
        let state = &composer.objective;
        return div()
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
            .child(
                div()
                    .text_size(px(10.0))
                    .line_height(px(17.0))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("composer.objective.native.hint")),
            )
            .into_any_element();
    };
    let presented = present(goal, composer.session.running);
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(14.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(edge(cx))
        .bg(quiet(cx))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .child(cx.t(presented.title_key)),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(18.0))
                .child(goal.objective.clone()),
        )
        .children(presented.last_check.map(|reason| {
            div()
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(colors.muted_foreground)
                .child(cx.t_with(
                    "goal.lastCheck",
                    &crate::i18n::Args::default().with("reason", reason),
                ))
        }))
        .into_any_element()
}

/// The panel's actions: "Iniciar objetivo", or the live goal's own.
pub(super) fn footer(composer: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let small = |id: &'static str, variant: ButtonVariant, icon: &'static str, text, disabled| {
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
                    .child(Icon::new(icon).size(12.0))
                    .child(text),
            )
    };
    let busy = composer.strip_busy.is_some() || composer.busy.is_some();
    let actions: Vec<AnyElement> = match live_goal(composer) {
        None => {
            let empty = composer.objective.intent.read(cx).text().trim().is_empty();
            vec![
                small(
                    "objective-start",
                    ButtonVariant::Default,
                    "target",
                    cx.t("composer.objective.native.start"),
                    busy || empty,
                )
                .on_click(
                    cx.listener(|composer, _, window, cx| composer.start_native_goal(window, cx)),
                )
                .into_any_element(),
            ]
        }
        Some(goal) => {
            let presented = present(goal, composer.session.running);
            let mut actions = Vec::new();
            if presented.action == GoalAction::ResumeAndClear {
                actions.push(
                    small(
                        "objective-goal-resume",
                        ButtonVariant::Outline,
                        "play",
                        cx.t("goal.resume"),
                        busy,
                    )
                    .on_click(cx.listener(|composer, _, _, cx| {
                        composer.goal_action(StripAction::GoalResume, cx)
                    }))
                    .into_any_element(),
                );
            }
            actions.push(
                div()
                    .ml_auto()
                    .child(
                        small(
                            "objective-goal-clear",
                            ButtonVariant::Ghost,
                            "trash-2",
                            cx.t("goal.clear"),
                            busy,
                        )
                        .on_click(cx.listener(
                            |composer, _, window, cx| {
                                composer.goal_action(StripAction::GoalClear, cx);
                                composer.set_objective_open(false, window, cx);
                            },
                        )),
                    )
                    .into_any_element(),
            );
            actions
        }
    };
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
        .children(actions)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_goal_turn_joins_the_intent_and_what_done_means_on_one_line() {
        assert_eq!(goal_prompt("  ", "x", "Pronto quando"), None);
        assert_eq!(
            goal_prompt("Feito!", "ok", "Pronto quando").as_deref(),
            Some("/goal Feito! Pronto quando: ok")
        );
        assert_eq!(
            goal_prompt("Corrigir o upload", "", "Pronto quando").as_deref(),
            Some("/goal Corrigir o upload")
        );
        assert_eq!(
            goal_prompt(
                "Corrigir\no upload",
                " testes passam\n e lint limpo ",
                "Pronto quando"
            )
            .as_deref(),
            Some("/goal Corrigir o upload. Pronto quando: testes passam e lint limpo")
        );
    }
}
