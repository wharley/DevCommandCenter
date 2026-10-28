//! The reviewer over the code tab: its panel (who reviews, on what — the
//! reviewer's own provider, or a menu of the providers that can review —,
//! Cancelar, "Gerar rascunhos", the guidance), the strip while it reads the
//! pull request, the one with what it left, and its button in the review's
//! foot.

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use crate::agent::AgentAvatarView;
use crate::data::agents::AgentState;
use crate::i18n::{Args, Translate};
use crate::screens::pr_hub::PrHub;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Menu, MenuEntry, MenuItem, Tooltip, spinner,
};

impl PrHub {
    fn reviewer_name(&self) -> Option<String> {
        self.sources
            .reviewer
            .as_ref()
            .map(|reviewer| reviewer.name.clone())
    }

    /// "Provedor · Modelo" for the reviewer's own provider; else the menu.
    fn review_runs_on(&self, busy: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let target = self.review_target();
        let label = target
            .as_ref()
            .map(|target| {
                [
                    Some(target.provider.label.clone()),
                    target.model.as_ref().map(|model| model.label.clone()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
            })
            .unwrap_or_default();
        if target.as_ref().is_some_and(|target| target.own) {
            return div()
                .rounded(px(6.0))
                .bg(colors.background.opacity(0.7))
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(label)
                .into_any_element();
        }
        let providers: Vec<_> = self.review_providers().into_iter().cloned().collect();
        let chosen = target.map(|target| {
            (
                target.provider.id.0.clone(),
                target.model.map(|model| model.id),
            )
        });
        let this = cx.entity().downgrade();
        let trigger_colors = colors.clone();
        Menu::dropdown(
            "pr-review-provider",
            move |_| {
                div()
                    .debug_selector(|| "pr-review-provider".into())
                    .flex()
                    .h(px(24.0))
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(6.0))
                    .px(px(8.0))
                    .text_size(px(11.0))
                    .text_color(trigger_colors.muted_foreground)
                    .cursor_pointer()
                    .hover(|trigger| trigger.bg(trigger_colors.accent))
                    .when(busy, |trigger| trigger.opacity(0.5))
                    .child(label.clone())
                    .child(Icon::new("chevron-down").size(12.0))
                    .into_any_element()
            },
            move |_, _| {
                providers
                    .iter()
                    .map(|provider| {
                        let entries = provider
                            .models
                            .iter()
                            .map(|model| {
                                let (provider_id, model_id) =
                                    (provider.id.0.clone(), model.id.clone());
                                let current = chosen.as_ref()
                                    == Some(&(provider_id.clone(), Some(model_id.clone())));
                                let this = this.clone();
                                let item = MenuItem::new(model.label.clone(), move |_, cx| {
                                    let pick = (provider_id.clone(), Some(model_id.clone()));
                                    let _ = this.update(cx, |hub, cx| {
                                        hub.review_pick = Some(pick);
                                        cx.notify();
                                    });
                                });
                                MenuEntry::Item(if current { item.trailing("✓") } else { item })
                            })
                            .collect();
                        MenuEntry::Submenu {
                            label: provider.label.clone().into(),
                            icon: None,
                            entries,
                        }
                    })
                    .collect()
            },
        )
        .into_any_element()
    }

    /// The panel, else the strip while it reviews, else what it left.
    pub(super) fn agent_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = cx.theme().colors.clone();
        let busy = self.agent_busy(cx);
        let reviewing = busy.as_deref() == Some("review");
        let reviewer = self.sources.reviewer.clone();
        let name = self.reviewer_name().unwrap_or_default();
        let violet = tw::VIOLET_500.hsla();
        let strip = |tint: gpui::Hsla| {
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(10.0))
                .border_b_1()
                .border_color(tint.opacity(0.2))
                .bg(tint.opacity(0.05))
                .px(px(16.0))
                .py(px(10.0))
        };
        if self.review.agent_panel_open {
            let face = match &reviewer {
                Some(agent) => AgentAvatarView::new(
                    "pr-review-panel-avatar",
                    &agent.avatar,
                    reviewing.then_some(AgentState::Working),
                    28.0,
                )
                .into_any_element(),
                None => Icon::new("sparkles")
                    .size(16.0)
                    .color(violet)
                    .into_any_element(),
            };
            let title = if reviewer.is_some() {
                cx.t_with(
                    "pullRequests.code.agentReviewTitleNamed",
                    &Args::default().with("agent", name.clone()),
                )
            } else {
                cx.t("pullRequests.code.agentReviewTitle")
            };
            let hint = if reviewing && reviewer.is_some() {
                cx.t_with(
                    "pullRequests.code.agentReviewing",
                    &Args::default().with("agent", name.clone()),
                )
            } else {
                cx.t("pullRequests.code.agentReviewHint")
            };
            let runs_on = self.review_runs_on(self.review.agent_pending.is_some(), cx);
            let can_run = self.review_target().is_some() && busy.is_none();
            return Some(
                div()
                    .debug_selector(|| "pr-review-panel".into())
                    .flex_none()
                    .border_b_1()
                    .border_color(violet.opacity(0.2))
                    .bg(violet.opacity(0.05))
                    .px(px(16.0))
                    .py(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(face)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(colors.muted_foreground)
                                            .child(hint),
                                    ),
                            )
                            .child(runs_on)
                            .child(
                                Button::new("pr-review-panel-cancel")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Xs)
                                    .text_size(12.0)
                                    .disabled(busy.is_some())
                                    .on_click(
                                        cx.listener(|hub, _, _, cx| hub.close_agent_panel(cx)),
                                    )
                                    .child(cx.t("pullRequests.code.cancel")),
                            )
                            .child(
                                Button::new("pr-review-generate")
                                    .size(ButtonSize::Xs)
                                    .text_size(12.0)
                                    .disabled(!can_run)
                                    .on_click(
                                        cx.listener(|hub, _, _, cx| hub.start_full_review(cx)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child(if reviewing {
                                                spinner("pr-review-generating", 12.0)
                                                    .into_any_element()
                                            } else {
                                                Icon::new("sparkles").size(12.0).into_any_element()
                                            })
                                            .child(cx.t("pullRequests.code.generateDrafts")),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "pr-review-instruction".into())
                            .mt(px(8.0))
                            .min_h(px(56.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.background.opacity(0.7))
                            .px(px(10.0))
                            .py(px(6.0))
                            .text_size(px(11.0))
                            .child(self.review.instruction.clone()),
                    )
                    .into_any_element(),
            );
        }
        let reviewer = reviewer?;
        if reviewing {
            return Some(
                strip(violet)
                    .debug_selector(|| "pr-review-running".into())
                    .child(AgentAvatarView::new(
                        "pr-review-running-avatar",
                        &reviewer.avatar,
                        Some(AgentState::Working),
                        28.0,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .text_color(colors.foreground)
                            .child(format!(
                                "{} {}",
                                cx.t_with(
                                    "pullRequests.code.agentReviewing",
                                    &Args::default().with("agent", name.clone()),
                                ),
                                cx.t("pullRequests.code.agentReviewingBackground"),
                            )),
                    )
                    .into_any_element(),
            );
        }
        let drafts = self.review.agent_result?;
        let emerald = tw::EMERALD_500.hsla();
        let text = if drafts > 0 {
            cx.t_with(
                "pullRequests.code.agentDrafted",
                &Args::count(drafts as i64).with("agent", name.clone()),
            )
        } else {
            cx.t_with(
                "pullRequests.code.agentFoundNothing",
                &Args::default().with("agent", name.clone()),
            )
        };
        Some(
            strip(emerald)
                .debug_selector(|| "pr-review-result".into())
                .child(AgentAvatarView::new(
                    "pr-review-result-avatar",
                    &reviewer.avatar,
                    (drafts > 0).then_some(AgentState::Done),
                    28.0,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.0))
                        .text_color(colors.foreground)
                        .child(text),
                )
                .child(
                    Button::new("pr-review-result-ok")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .text_size(12.0)
                        .on_click(cx.listener(|hub, _, _, cx| {
                            hub.review.agent_result = None;
                            cx.notify();
                        }))
                        .child(cx.t("pullRequests.code.dismissAgentResult")),
                )
                .into_any_element(),
        )
    }

    /// "Revisar com o …" in the review's foot.
    pub(super) fn agent_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let label = match self.reviewer_name() {
            Some(name) => cx.t_with(
                "pullRequests.code.reviewWithNamedAgent",
                &Args::default().with("agent", name),
            ),
            None => cx.t("pullRequests.code.reviewWithAgent"),
        };
        let off = self.review_providers().is_empty() || self.agent_busy(cx).is_some();
        let face = match &self.sources.reviewer {
            Some(agent) => {
                AgentAvatarView::new("pr-review-button-avatar", &agent.avatar, None, 16.0)
                    .into_any_element()
            }
            None => Icon::new("sparkles").size(12.0).into_any_element(),
        };
        Tooltip::new(
            "pr-review-agent-tip",
            div().debug_selector(|| "pr-review-agent".into()).child(
                Button::new("pr-review-agent")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::IconXs)
                    .disabled(off)
                    .on_click(cx.listener(|hub, _, _, cx| hub.toggle_agent_panel(cx)))
                    .child(face),
            ),
            move |_, _| div().child(label.clone()).into_any_element(),
        )
        .into_any_element()
    }
}
