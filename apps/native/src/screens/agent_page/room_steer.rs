//! "Enviar agora" on a message that waits for the agent to finish
//! (`room_queue`): instead of waiting, it goes into the turn at work as
//! guidance — the agent takes it at its next step — as the task's composer
//! does with "orientar". Offered only where the agent's provider takes
//! guidance mid-turn. A turn already wrapping up refuses it: the message
//! then stays queued and goes next, and the person is told.

use dcc_core::ports::STEER_WINDOW_CLOSED;
use gpui::{AnyElement, Context, SharedString, prelude::*};

use super::room::{Pending, TeamRoom};
use super::room_queue::Queued;
use crate::data::desktop_api::ApiError;
use crate::i18n::Translate;
use crate::ui::{Button, ButtonSize, ButtonVariant, Toaster};

impl TeamRoom {
    /// Whether `agent_id`'s provider takes guidance while a turn runs.
    pub(crate) fn steers(&self, agent_id: &str, cx: &gpui::App) -> bool {
        let Some(agent) = super::agent_card::agent_of(&self.sidebar, agent_id, cx) else {
            return false;
        };
        let Ok((provider, _, _)) = self.agent_turn(&agent, cx) else {
            return false;
        };
        self.composer
            .read(cx)
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.iter().find(|known| known.id.0 == provider))
            .is_some_and(|provider| provider.capabilities.supports_steering)
    }

    /// Sends the queued message `id` into the turn at work.
    pub(crate) fn steer_queued(
        &mut self,
        session_id: String,
        id: String,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_executor()
                .spawn({
                    let id = id.clone();
                    async move {
                        match api.steer_turn(&session_id, &text) {
                            // Taken by the turn: it leaves the queue.
                            Ok(()) => api.remove_queued_turn(&session_id, &id).map(|_| true),
                            // Too late for this turn: it stays next in line.
                            Err(ApiError::Refused { code, .. }) if code == STEER_WINDOW_CLOSED => {
                                Ok(false)
                            }
                            Err(error) => Err(error),
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                match sent {
                    Ok(true) => room.pending.retain(|pending| {
                        pending
                            .queued
                            .as_ref()
                            .is_none_or(|queued| queued.id.as_deref() != Some(id.as_str()))
                    }),
                    Ok(false) => Toaster::info(cx.t("composer.followUp.steerQueued"), cx),
                    Err(error) => Toaster::error(
                        format!("{} {error}", cx.t("composer.followUp.steerFailed")),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Enviar agora", beside "Tirar da fila".
    pub(super) fn steer_button(
        &self,
        index: usize,
        sent: &Pending,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Queued {
            session_id,
            id: Some(id),
        } = sent.queued.clone()?
        else {
            return None;
        };
        if !self.steers(&sent.agent_id, cx) {
            return None;
        }
        let text = sent.text.clone();
        Some(
            Button::new(SharedString::from(format!("room-steer-{index}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .on_click(cx.listener(move |room, _, _, cx| {
                    room.steer_queued(session_id.clone(), id.clone(), text.clone(), cx)
                }))
                .child(cx.t("agents.room.steerNow"))
                .into_any_element(),
        )
    }
}
