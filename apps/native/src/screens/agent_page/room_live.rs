//! The room's live lines, the way Slack shows who is typing: one shape for
//! everything at work now (`live_row`: the agent's face, a trigger's chip,
//! what it does, a spinner, a click opening it) and "Revisor está
//! trabalhando…" / "precisa de você" for an agent's own conversation.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::agent::{AgentAvatarView, Mood, MoodAvatarView};
use crate::data::agents::{AgentRecord, AgentState};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::spinner;

impl TeamRoom {
    /// One thing at work now: `key` names it for tests.
    pub(super) fn live_row(
        &self,
        key: String,
        agent: &AgentRecord,
        chip: Option<SharedString>,
        text: SharedString,
        open: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let chip = chip.map(|chip| {
            div()
                .flex_none()
                .px(px(6.0))
                .rounded(px(4.0))
                .bg(colors.accent)
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(chip)
        });
        let selector = key.clone();
        div()
            .id(SharedString::from(key.clone()))
            .debug_selector(move || selector.clone())
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.card)
            .cursor_pointer()
            .hover(|row| row.bg(colors.accent.opacity(0.4)))
            .on_click(cx.listener(move |room, _, _, cx| open(room, cx)))
            .child(AgentAvatarView::new(
                SharedString::from(format!("{key}-face")),
                &agent.avatar,
                None,
                22.0,
            ))
            .children(chip)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .text_color(colors.foreground)
                    .child(text),
            )
            .child(spinner(SharedString::from(format!("{key}-spin")), 14.0))
            .into_any_element()
    }

    /// "Revisor está trabalhando…" / "Revisor precisa de você".
    pub(super) fn busy_line(
        &self,
        agent: &AgentRecord,
        state: AgentState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let needs_you = state == AgentState::NeedsYou;
        let text = cx.t_with(
            if needs_you {
                "agents.room.needsYou"
            } else {
                "agents.room.working"
            },
            &Args::default().with("agent", agent.name.clone()),
        );
        let agent_id = agent.id.clone();
        let id = agent.id.clone();
        div()
            .debug_selector(move || format!("room-busy-{id}"))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(MoodAvatarView::new(
                SharedString::from(format!("room-busy-face-{}", agent.id)),
                &agent.avatar,
                if needs_you {
                    Mood::NeedsYou
                } else {
                    Mood::Working
                },
                28.0,
            ))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(text),
            )
            .when(needs_you, |row| {
                row.child(
                    div()
                        .id(SharedString::from(format!("room-busy-open-{agent_id}")))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |room, _, _, cx| {
                                room.open_conversation(&agent_id, cx)
                            }),
                        )
                        .child(cx.t("agents.room.open")),
                )
            })
            .into_any_element()
    }

    /// "Revisor chegou ao teto de 5 gatilhos hoje…": a trigger held for the
    /// day is said where the person looks, not only on the agent's page.
    pub(super) fn held_rows(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(triggers) = cx.try_global::<crate::data::agent_triggers::AgentTriggers>() else {
            return Vec::new();
        };
        let now = chrono::Utc::now();
        let held: Vec<AgentRecord> = agents
            .iter()
            .filter(|agent| {
                self.shows(&agent.id)
                    && triggers.any_on(&agent.id)
                    && triggers.at_ceiling(&agent.id, now)
            })
            .cloned()
            .collect();
        let colors = cx.theme().colors.clone();
        held.into_iter()
            .map(|agent| {
                let id = agent.id.clone();
                div()
                    .debug_selector(move || format!("room-held-{id}"))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("room-held-face-{}", agent.id)),
                        &agent.avatar,
                        None,
                        20.0,
                    ))
                    .child(
                        cx.t_with(
                            "agents.room.ceiling",
                            &Args::default().with("agent", agent.name.clone()).with(
                                "count",
                                triggers
                                    .daily_ceiling(&agent.id)
                                    .unwrap_or_default()
                                    .to_string(),
                            ),
                        ),
                    )
                    .into_any_element()
            })
            .collect()
    }
}
