//! "Gatilhos" on an agent's page: what wakes it by itself. Each one starts
//! off; the hub's review triggers are the Reviewer's alone; the ceilings
//! (`agent_triggers`) are shown, not set; when it wakes next and the quiet
//! hours sit below (`triggers_wake`).

use chrono::Utc;
use dcc_core::domain::agent::REVIEWER_PRESET;
use gpui::{AnyElement, BorrowAppContext, Context, FontWeight, SharedString, div, prelude::*, px};

use super::AgentPage;
use crate::data::agent_triggers::{AgentTriggers, TARGET_RUNS, Trigger};
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::AgentPageData;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Switch};

impl AgentPage {
    fn trigger_row(&self, agent_id: &str, trigger: Trigger, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let on = cx.has_global::<AgentTriggers>()
            && cx.global::<AgentTriggers>().is_on(agent_id, trigger);
        let key = trigger.id();
        let agent = agent_id.to_string();
        let page = cx.entity().downgrade();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                // The text gives way and wraps; the switch keeps its place.
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t(&format!("agents.triggers.{key}.title"))),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t(&format!("agents.triggers.{key}.hint"))),
                    ),
            )
            .child(div().flex_shrink_0().child(
                Switch::new(SharedString::from(format!("agent-trigger-{key}")), on).on_toggle(
                    move |on, _, cx| {
                        if cx.has_global::<AgentTriggers>() {
                            cx.update_global::<AgentTriggers, _>(|triggers, _| {
                                triggers.set(&agent, trigger, on)
                            });
                        }
                        let _ = page.update(cx, |_, cx| cx.notify());
                    },
                ),
            ))
            .into_any_element()
    }

    pub(super) fn triggers_card(
        &self,
        data: &AgentPageData,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let reviewer = data.agent.preset.as_deref() == Some(REVIEWER_PRESET);
        let colors = cx.theme().colors.clone();
        let agent_id = data.agent.id.clone();
        let today = cx
            .has_global::<AgentTriggers>()
            .then(|| {
                cx.global::<AgentTriggers>()
                    .runs_today(&agent_id, Utc::now())
            })
            .unwrap_or_default();
        let rows: Vec<AnyElement> = Trigger::ALL
            .into_iter()
            .filter(|trigger| reviewer || !trigger.reviewer_only())
            .map(|trigger| self.trigger_row(&agent_id, trigger, cx))
            .collect();
        let wake = self.wake_rows(&agent_id, cx);
        let ceiling = self.ceiling_row(&agent_id, cx);
        let daily = cx
            .try_global::<AgentTriggers>()
            .and_then(|triggers| triggers.daily_ceiling(&agent_id));
        let args = Args::default()
            .with("target", TARGET_RUNS.to_string())
            .with("today", today.to_string());
        let limits = match daily {
            Some(daily) => cx.t_with(
                "agents.triggers.limits",
                &args.with("daily", daily.to_string()),
            ),
            None => cx.t_with("agents.triggers.limitsNoCeiling", &args),
        };
        Some(
            Self::card(cx)
                .debug_selector(|| "agent-triggers".into())
                .flex()
                .flex_col()
                .gap(px(14.0))
                .p(px(20.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(Icon::new("zap").size(16.0).color(colors.muted_foreground))
                        .child(cx.t("agents.triggers.title")),
                )
                .children(rows)
                .children(wake)
                .children(ceiling)
                .child(
                    div()
                        .debug_selector(|| "agent-triggers-limits".into())
                        .text_size(px(11.5))
                        .line_height(px(17.0))
                        .text_color(colors.muted_foreground)
                        .child(limits),
                )
                .into_any_element(),
        )
    }
}
