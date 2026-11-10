//! "Teto por dia" under the agent's triggers (`data::agent_ceiling`): how
//! many times a day triggers may wake it — 5 (the default), 10, 20 or no
//! ceiling. The person's messages, routines and sources never count.

use gpui::{AnyElement, BorrowAppContext, Context, SharedString, div, prelude::*, px};

use super::AgentPage;
use crate::data::agent_ceiling::CEILING_CHOICES;
use crate::data::agent_triggers::AgentTriggers;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::ToggleGroup;

impl AgentPage {
    pub(super) fn ceiling_row(&self, agent_id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let current = cx.try_global::<AgentTriggers>()?.daily_ceiling(agent_id);
        let colors = cx.theme().colors.clone();
        let labels: Vec<SharedString> = CEILING_CHOICES
            .iter()
            .map(|choice| match choice {
                Some(count) => count.to_string().into(),
                None => cx.t("agents.triggers.ceiling.none"),
            })
            .collect();
        let picked = CEILING_CHOICES.iter().position(|choice| *choice == current);
        let page = cx.entity().downgrade();
        let agent = agent_id.to_string();
        let group = ToggleGroup::new("agent-triggers-ceiling", labels, picked).on_select(
            move |index, _, cx| {
                if cx.has_global::<AgentTriggers>() {
                    cx.update_global::<AgentTriggers, _>(|triggers, _| {
                        triggers.set_daily_ceiling(&agent, CEILING_CHOICES[index])
                    });
                }
                let _ = page.update(cx, |_, cx| cx.notify());
            },
        );
        Some(
            div()
                .debug_selector(|| "agent-triggers-ceiling".into())
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.0))
                .text_size(px(12.0))
                .text_color(colors.muted_foreground)
                .child(cx.t("agents.triggers.ceiling.title"))
                .child(group)
                .into_any_element(),
        )
    }
}
