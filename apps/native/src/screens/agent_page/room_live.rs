//! The room's live lines, the way Slack shows who is typing: "Revisor está
//! trabalhando…" / "precisa de você" for an agent's own conversation (a task
//! at work has its own row, `room_task_live`). What moves here moves on the
//! shared clock (`ui::pulse`).

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::agent::{AgentAvatarView, Mood, MoodAvatarView};
use crate::data::agents::{AgentRecord, AgentState};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;

impl TeamRoom {
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
            .child(
                MoodAvatarView::new(
                    SharedString::from(format!("room-busy-face-{}", agent.id)),
                    &agent.avatar,
                    if needs_you {
                        Mood::NeedsYou
                    } else {
                        Mood::Working
                    },
                    28.0,
                )
                .on_clock(),
            )
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
