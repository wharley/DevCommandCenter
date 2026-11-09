//! Routines in the team room (`dcc_core::domain::agent_routine`): a
//! routine's turn shows as one line ("Rotina · Seg 09:00 — …"), its answer
//! stays quiet when there was nothing to report; a routine an agent
//! proposed is a card with "Criar" / "Descartar"; and an agent the brake
//! stopped (`agent_brake`) is a card with "Retomar".

use chrono::Utc;
use dcc_core::domain::agent_routine::{Routine, routine_request};
use gpui::{AnyElement, BorrowAppContext, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agent_brake::UNATTENDED_LIMIT;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::AgentRecord;
use crate::data::routines::{Routines, schedule_label};
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster};

impl TeamRoom {
    /// A routine's turn, as one line.
    pub(super) fn routine_line(
        &self,
        index: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request = routine_request(text)?;
        let colors = cx.theme().colors.clone();
        Some(
            div()
                .debug_selector(move || format!("room-routine-{index}"))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(11.5))
                .text_color(colors.muted_foreground)
                .child(
                    Icon::new("alarm-clock")
                        .size(12.0)
                        .color(colors.muted_foreground),
                )
                .child(div().truncate().child(cx.t_with(
                    "agents.routines.line",
                    &Args::default().with("request", request.to_string()),
                )))
                .into_any_element(),
        )
    }

    fn answer_proposal(&mut self, routine: &Routine, accept: bool, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let (agent, id) = (routine.agent_id.clone(), routine.id.clone());
        // Gone from the room at once; the clock reads the table again.
        if cx.has_global::<Routines>() {
            cx.update_global::<Routines, _>(|routines, _| {
                if accept {
                    for found in routines.list.iter_mut().filter(|found| found.id == id) {
                        found.proposed = false;
                        found.enabled = true;
                    }
                } else {
                    routines.list.retain(|found| found.id != id);
                }
            });
        }
        cx.spawn(async move |_, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    let answered = if accept {
                        api.accept_routine(&agent, &id)
                    } else {
                        api.delete_routine(&agent, &id)
                    };
                    answered.map(|()| crate::data::routines::read())
                })
                .await;
            let _ = cx.update(|cx| match done {
                Ok(list) => cx.set_global(Routines { list }),
                Err(error) => Toaster::error(error.to_string(), cx),
            });
        })
        .detach();
        cx.notify();
    }

    fn resume(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        if cx.has_global::<AgentTriggers>() {
            cx.update_global::<AgentTriggers, _>(|triggers, _| {
                triggers.attend(agent_id, Utc::now())
            });
        }
        cx.notify();
    }

    fn card(colors: &crate::theme::Palette, amber: bool) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(if amber {
                tw::AMBER_500.hsla().opacity(0.4)
            } else {
                colors.border.opacity(0.7)
            })
            .bg(if amber {
                tw::AMBER_500.hsla().opacity(0.06)
            } else {
                colors.card
            })
    }

    /// The routines proposed and the agents the brake stopped.
    pub(super) fn routine_cards(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let proposed: Vec<Routine> = cx
            .try_global::<Routines>()
            .map(|routines| routines.proposed().cloned().collect())
            .unwrap_or_default();
        let mut rows = Vec::new();
        for (index, routine) in proposed.iter().enumerate() {
            let Some(agent) = agents.iter().find(|agent| agent.id == routine.agent_id) else {
                continue;
            };
            let (create, discard) = (routine.clone(), routine.clone());
            rows.push(
                Self::card(&colors, false)
                    .debug_selector(move || format!("room-routine-proposal-{index}"))
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("room-routine-face-{index}")),
                        &agent.avatar,
                        None,
                        22.0,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(
                                        cx.t_with(
                                            "agents.routines.proposal",
                                            &Args::default()
                                                .with("agent", agent.name.clone())
                                                .with("when", schedule_label(routine, english)),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(colors.muted_foreground)
                                    .child(routine.prompt.clone()),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("room-routine-create-{index}")))
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.answer_proposal(&create, true, cx)
                            }))
                            .child(cx.t("agents.routines.create")),
                    )
                    .child(
                        Button::new(SharedString::from(format!("room-routine-discard-{index}")))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .on_click(cx.listener(move |room, _, _, cx| {
                                room.answer_proposal(&discard, false, cx)
                            }))
                            .child(cx.t("agents.routines.discard")),
                    )
                    .into_any_element(),
            );
        }
        let braked: Vec<&AgentRecord> = cx
            .try_global::<AgentTriggers>()
            .map(|triggers| {
                agents
                    .iter()
                    .filter(|agent| triggers.brake_check(&agent.id).is_err())
                    .collect()
            })
            .unwrap_or_default();
        for (index, agent) in braked.into_iter().enumerate() {
            let id = agent.id.clone();
            rows.push(
                Self::card(&colors, true)
                    .debug_selector(move || format!("room-brake-{index}"))
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("room-brake-face-{index}")),
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
                                    "agents.routines.brake",
                                    &Args::default()
                                        .with("agent", agent.name.clone())
                                        .with("count", UNATTENDED_LIMIT.to_string()),
                                ),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("room-brake-resume-{index}")))
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .on_click(cx.listener(move |room, _, _, cx| room.resume(&id, cx)))
                            .child(cx.t("agents.routines.resume")),
                    )
                    .into_any_element(),
            );
        }
        rows
    }
}
