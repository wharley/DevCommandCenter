//! The monthly limit in the team room (`dcc_core::domain::agent_budget`):
//! an agent past its limit is a card — "Revisor chegou ao limite do mês" —
//! with "Continuar mesmo assim" (until the month ends) and "Ajustar" (its
//! page). Read only for the agents with a limit: when the room opens and
//! when a turn was refused for it, never on the sidebar's refresh.

use chrono::Local;
use dcc_core::domain::agent_budget::{AGENT_LIMIT_ERROR, month_key, over_limit};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::editor_budget::tokens_short;
use super::room::{RoomEvent, TeamRoom};
use super::spend::read_month;
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Toaster};

/// Whether an error is the limit's.
pub(crate) fn is_limit_error(error: &str) -> bool {
    error.contains(AGENT_LIMIT_ERROR)
}

impl TeamRoom {
    /// Which agents with a limit are past it now (tests feed it themselves).
    pub(crate) fn read_limits(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        let limited: Vec<(String, u64)> = self
            .sidebar
            .read(cx)
            .agents()
            .iter()
            .filter_map(|agent| Some((agent.id.clone(), agent.monthly_token_limit?)))
            .collect();
        if limited.is_empty() {
            if !self.limits.is_empty() {
                self.limits.clear();
                cx.notify();
            }
            return;
        }
        cx.spawn(async move |room, cx| {
            let over = cx
                .background_executor()
                .spawn(async move {
                    let month = month_key(Local::now());
                    limited
                        .into_iter()
                        .filter_map(|(id, limit)| {
                            let (spend, let_through) = read_month(&id)?;
                            over_limit(&spend, Some(limit), let_through.as_deref(), &month)
                                .then_some(id)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = room.update(cx, |room, cx| {
                room.limits = over;
                cx.notify();
            });
        })
        .detach();
    }

    fn let_past_limit(&mut self, agent_id: String, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        cx.spawn(async move |room, cx| {
            let done = cx
                .background_executor()
                .spawn({
                    let agent_id = agent_id.clone();
                    async move { api.let_agent_past_limit(&agent_id) }
                })
                .await;
            let _ = room.update(cx, |room, cx| match done {
                Ok(()) => {
                    room.limits.retain(|id| *id != agent_id);
                    cx.notify();
                }
                Err(error) => Toaster::error(error.to_string(), cx),
            });
        })
        .detach();
    }

    pub(super) fn limit_rows(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        self.limits
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let agent = agents.iter().find(|agent| agent.id == *id)?;
                let limit = agent.monthly_token_limit?;
                let (open, keep) = (agent.id.clone(), agent.id.clone());
                Some(
                    div()
                        .debug_selector(move || format!("room-limit-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(tw::AMBER_500.hsla().opacity(0.4))
                        .bg(tw::AMBER_500.hsla().opacity(0.06))
                        .child(AgentAvatarView::new(
                            SharedString::from(format!("room-limit-face-{index}")),
                            &agent.avatar,
                            None,
                            22.0,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(
                                    cx.t_with(
                                        "agents.room.limit.text",
                                        &Args::default()
                                            .with("agent", agent.name.clone())
                                            .with("limit", tokens_short(limit)),
                                    ),
                                ),
                        )
                        .child(
                            Button::new(SharedString::from(format!("room-limit-continue-{index}")))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Xs)
                                .on_click(cx.listener(move |room, _, _, cx| {
                                    room.let_past_limit(keep.clone(), cx)
                                }))
                                .child(cx.t("agents.spend.continue")),
                        )
                        .child(
                            Button::new(SharedString::from(format!("room-limit-adjust-{index}")))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Xs)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(RoomEvent::OpenAgent(open.clone()))
                                }))
                                .child(cx.t("agents.room.limit.adjust")),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}
