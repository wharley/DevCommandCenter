//! Writing while the agent works (Tucano's way): a message to a conversation
//! with a turn open, or one waiting on the person, is queued on the desktop
//! (`DesktopApi::queue_turn_for_id`) instead of sent, and goes when that turn
//! ends. Its bubble shows at once with "Na fila · sai quando o Dev terminar"
//! and "Tirar da fila", until its conversation shows the turn started.

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::{Pending, TeamRoom};
use crate::data::agents::AgentState;
use crate::i18n::{Args, Translate};
use crate::ui::{Button, ButtonSize, ButtonVariant, Toaster};

/// A message queued on its conversation; `id` once the desktop answered.
#[derive(Clone, Debug, PartialEq)]
pub struct Queued {
    pub session_id: String,
    pub id: Option<String>,
}

/// Whether a message waits for the conversation's turn: one runs (as the
/// sidebar or the events see it), it waits on the person, or a message of
/// the person's is queued there already (they go in order).
pub(crate) fn must_wait(state: Option<AgentState>, live: bool, queued_ahead: bool) -> bool {
    live || queued_ahead || matches!(state, Some(AgentState::Working | AgentState::NeedsYou))
}

/// Whether the queued turn `id` was taken out of its queue (here or from
/// the task's own composer).
pub(crate) fn taken_out(events: &[SessionEventRecord], id: &str) -> bool {
    events.iter().any(|event| {
        matches!(
            &event.kind,
            SessionEventKind::QueuedTurnRemoved { queued_turn_id } if queued_turn_id == id
        )
    })
}

impl TeamRoom {
    /// Whether a message to `agent_id`'s conversation `session_id` is queued
    /// instead of sent now.
    pub(crate) fn queues_for(&self, agent_id: &str, session_id: &str, cx: &gpui::App) -> bool {
        let state = self
            .sidebar
            .read(cx)
            .agents()
            .iter()
            .filter(|agent| agent.id == agent_id)
            .flat_map(|agent| agent.sessions.iter())
            .find(|session| session.session_id == session_id)
            .map(|session| session.state);
        let live = self
            .feeds
            .iter()
            .any(|feed| feed.session_id == session_id && feed.now.live().is_some());
        let ahead = self.pending.iter().any(|pending| {
            pending
                .queued
                .as_ref()
                .is_some_and(|queued| queued.session_id == session_id)
        });
        must_wait(state, live, ahead)
    }

    /// The desktop queued `text`: its bubble can be taken out now.
    pub(crate) fn mark_queued(&mut self, text: &str, id: String) {
        if let Some(queued) = self
            .pending
            .iter_mut()
            .filter(|pending| pending.text == text)
            .filter_map(|pending| pending.queued.as_mut())
            .find(|queued| queued.id.is_none())
        {
            queued.id = Some(id);
        }
    }

    /// Drops the queued messages taken out of their queue elsewhere.
    pub(crate) fn drop_unqueued(&mut self) {
        let feeds = &self.feeds;
        self.pending.retain(|pending| {
            let Some(Queued {
                session_id,
                id: Some(id),
            }) = &pending.queued
            else {
                return true;
            };
            !feeds
                .iter()
                .filter(|feed| feed.session_id == *session_id)
                .any(|feed| taken_out(&feed.events, id))
        });
    }

    /// "Tirar da fila": the desktop drops the queued turn, the bubble goes.
    pub(crate) fn unqueue(&mut self, session_id: String, id: String, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let removed = cx
                .background_executor()
                .spawn({
                    let id = id.clone();
                    async move { api.remove_queued_turn(&session_id, &id) }
                })
                .await;
            let _ = this.update(cx, |room, cx| {
                match removed {
                    Ok(_) => room.pending.retain(|pending| {
                        pending
                            .queued
                            .as_ref()
                            .is_none_or(|queued| queued.id.as_deref() != Some(id.as_str()))
                    }),
                    Err(error) => Toaster::error(
                        cx.t_with(
                            "agents.room.unqueueFailed",
                            &Args::default().with("error", error.to_string()),
                        ),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Under a message on its way: "enviando…", or that it waits for
    /// `name` to finish.
    pub(crate) fn pending_note(&self, sent: &Pending, name: &str, cx: &gpui::App) -> SharedString {
        match sent.queued {
            Some(_) => cx.t_with(
                "agents.room.queued",
                &Args::default().with("agent", name.to_string()),
            ),
            None => cx.t("agents.room.sending"),
        }
    }

    /// A message on its way, with "Tirar da fila" once it is queued.
    pub(crate) fn with_unqueue(
        &self,
        index: usize,
        sent: &Pending,
        bubble: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(Queued {
            session_id,
            id: Some(id),
        }) = sent.queued.clone()
        else {
            return bubble;
        };
        div()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.0))
            .child(bubble)
            .child(
                Button::new(SharedString::from(format!("room-unqueue-{index}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Xs)
                    .on_click(cx.listener(move |room, _, _, cx| {
                        room.unqueue(session_id.clone(), id.clone(), cx)
                    }))
                    .child(cx.t("agents.room.unqueue")),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::session::SessionId;

    #[test]
    fn a_message_waits_while_its_conversation_is_busy() {
        assert!(!must_wait(None, false, false), "no conversation yet");
        assert!(!must_wait(Some(AgentState::Idle), false, false));
        assert!(!must_wait(Some(AgentState::Done), false, false));
        assert!(must_wait(Some(AgentState::Working), false, false));
        assert!(must_wait(Some(AgentState::NeedsYou), false, false));
        assert!(
            must_wait(Some(AgentState::Idle), true, false),
            "events first"
        );
        assert!(must_wait(Some(AgentState::Idle), false, true), "in order");
    }

    #[test]
    fn a_turn_taken_out_elsewhere_is_seen() {
        let event = SessionEventRecord {
            event_id: "e-1".into(),
            session_id: SessionId("s".into()),
            sequence: 1,
            occurred_at: "2026-11-11T10:00:00Z".into(),
            kind: SessionEventKind::QueuedTurnRemoved {
                queued_turn_id: "q-1".into(),
            },
        };
        assert!(taken_out(std::slice::from_ref(&event), "q-1"));
        assert!(!taken_out(&[event], "q-2"));
    }
}
