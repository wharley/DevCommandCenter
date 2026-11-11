//! A reply an agent proposed (`dcc_core::domain::agent_reply`) as a card in
//! the room it was asked in: the agent's face, "Resposta pronta" with where
//! it goes, the text, and "Enviar" / "Editar" / "Descartar". Nothing leaves
//! until "Enviar"; then the desktop calls the connector's tool with the text
//! as the person left it. A failure keeps the card with why.

use std::collections::HashSet;

use dcc_core::domain::agent_reply::{AgentReply, MAX_REPLY_CHARS};
use gpui::{
    AnyElement, BorrowAppContext, Context, Entity, FontWeight, SharedString, Window, div,
    prelude::*, px,
};

use super::room::TeamRoom;
use crate::agent::AgentAvatarView;
use crate::data::agent_replies::Replies;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::text_area::{TextArea, TextAreaEvent};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster};

/// The room's replies being edited or sent.
#[derive(Default)]
pub(crate) struct RoomReplies {
    /// "Editar" open on a reply, with its field.
    pub editing: Option<(String, Entity<TextArea>, gpui::Subscription)>,
    pub sending: HashSet<String>,
}

impl TeamRoom {
    fn reply_text(&self, reply: &AgentReply, cx: &gpui::App) -> String {
        match &self.replies.editing {
            Some((id, area, _)) if *id == reply.id => area.read(cx).text().to_string(),
            _ => reply.text.clone(),
        }
    }

    pub(crate) fn edit_reply(
        &mut self,
        reply: &AgentReply,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let area = cx.new(|cx| {
            let mut area = TextArea::new(window, cx).with_max_height(220.0);
            area.set_text(&reply.text, cx);
            area
        });
        let id = reply.id.clone();
        let events = cx.subscribe(
            &area,
            move |room, _, event: &TextAreaEvent, cx| match event {
                TextAreaEvent::Cancelled => {
                    room.replies.editing = None;
                    cx.notify();
                }
                _ => cx.notify(),
            },
        );
        let handle = gpui::Focusable::focus_handle(area.read(cx), cx);
        window.focus(&handle, cx);
        self.replies.editing = Some((id, area, events));
        cx.notify();
    }

    fn leave_reply(&mut self, id: &str, cx: &mut Context<Self>) {
        if cx.has_global::<Replies>() {
            cx.update_global::<Replies, _>(|replies, _| {
                replies.list.retain(|reply| reply.id != id)
            });
        }
        if self
            .replies
            .editing
            .as_ref()
            .is_some_and(|(open, ..)| open == id)
        {
            self.replies.editing = None;
        }
    }

    pub(crate) fn send_reply(&mut self, reply: &AgentReply, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let text = self.reply_text(reply, cx).trim().to_string();
        if text.is_empty() || self.replies.sending.contains(&reply.id) {
            return;
        }
        let id = reply.id.clone();
        self.replies.sending.insert(id.clone());
        cx.notify();
        cx.spawn(async move |room, cx| {
            let (sent_id, sent_text) = (id.clone(), text.clone());
            let done = cx
                .background_executor()
                .spawn(async move { api.send_reply(&sent_id, &sent_text) })
                .await;
            let _ = room.update(cx, |room, cx| {
                room.replies.sending.remove(&id);
                match done {
                    Ok(()) => {
                        room.leave_reply(&id, cx);
                        Toaster::success(cx.t("agents.replies.sent"), cx);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        if cx.has_global::<Replies>() {
                            cx.update_global::<Replies, _>(|replies, _| {
                                for found in replies.list.iter_mut().filter(|found| found.id == id)
                                {
                                    found.error = Some(message.clone());
                                    found.text = text.clone();
                                }
                            });
                        }
                        Toaster::error(message, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn discard_reply(&mut self, reply: &AgentReply, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        // Gone from the room at once; the next read agrees.
        self.leave_reply(&reply.id, cx);
        cx.notify();
        let id = reply.id.clone();
        cx.spawn(async move |room, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { api.discard_reply(&id) })
                .await;
            if let Err(error) = done {
                let _ = room.update(cx, |_, cx| Toaster::error(error.to_string(), cx));
            }
        })
        .detach();
    }

    /// The replies agents proposed in this room, a card each.
    pub(super) fn reply_cards(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let waiting: Vec<AgentReply> = cx
            .try_global::<Replies>()
            .map(|replies| replies.in_room(&self.room).cloned().collect())
            .unwrap_or_default();
        let mut rows = Vec::new();
        for (index, reply) in waiting.iter().enumerate() {
            let Some(agent) = agents.iter().find(|agent| agent.id == reply.agent_id) else {
                continue;
            };
            rows.push(self.reply_card(index, reply, agent, cx));
        }
        rows
    }

    fn reply_card(
        &self,
        index: usize,
        reply: &AgentReply,
        agent: &AgentRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let amber = tw::AMBER_500.hsla();
        let editing = self
            .replies
            .editing
            .as_ref()
            .filter(|(id, ..)| *id == reply.id)
            .map(|(_, area, _)| area.clone());
        let sending = self.replies.sending.contains(&reply.id);
        let too_long = editing
            .as_ref()
            .is_some_and(|area| area.read(cx).text().chars().count() > MAX_REPLY_CHARS);
        let (send, edit, discard) = (reply.clone(), reply.clone(), reply.clone());
        let body = match &editing {
            Some(area) => div()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(colors.ring.opacity(0.6))
                .bg(colors.background)
                .text_size(px(13.0))
                .child(area.clone())
                .into_any_element(),
            None => div()
                .debug_selector(move || format!("room-reply-text-{index}"))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .bg(colors.accent.opacity(0.5))
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(colors.foreground)
                .child(reply.text.clone())
                .into_any_element(),
        };
        div()
            .debug_selector(move || format!("room-reply-{index}"))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(amber.opacity(0.45))
            .bg(colors.card)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(AgentAvatarView::new(
                        SharedString::from(format!("room-reply-face-{index}")),
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
                                    .child(cx.t_with(
                                        "agents.replies.title",
                                        &Args::default().with("agent", agent.name.clone()),
                                    )),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(colors.muted_foreground)
                                    .child(format!("{} · {}", reply.connector, reply.destination)),
                            ),
                    )
                    .child(div().flex_none().size(px(7.0)).rounded_full().bg(amber)),
            )
            .child(body)
            .children(reply.error.as_ref().map(|error| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(11.5))
                    .text_color(tw::RED_500.hsla())
                    .child(
                        Icon::new("circle-alert")
                            .size(12.0)
                            .color(tw::RED_500.hsla()),
                    )
                    .child(div().truncate().child(cx.t_with(
                        "agents.replies.failed",
                        &Args::default().with("error", error.clone()),
                    )))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        Button::new(SharedString::from(format!("room-reply-discard-{index}")))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .disabled(sending)
                            .on_click(
                                cx.listener(move |room, _, _, cx| room.discard_reply(&discard, cx)),
                            )
                            .child(cx.t("agents.replies.discard")),
                    )
                    .child(div().flex_1())
                    .when(editing.is_none(), |row| {
                        row.child(
                            Button::new(SharedString::from(format!("room-reply-edit-{index}")))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Xs)
                                .disabled(sending)
                                .on_click(cx.listener(move |room, _, window, cx| {
                                    room.edit_reply(&edit, window, cx)
                                }))
                                .child(cx.t("agents.replies.edit")),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!("room-reply-send-{index}")))
                            .variant(ButtonVariant::Default)
                            .size(ButtonSize::Xs)
                            .disabled(sending || too_long)
                            .on_click(cx.listener(move |room, _, _, cx| room.send_reply(&send, cx)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(Icon::new("send").size(12.0))
                                    .child(cx.t_with(
                                        if sending {
                                            "agents.replies.sending"
                                        } else {
                                            "agents.replies.send"
                                        },
                                        &Args::default().with("connector", reply.connector.clone()),
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }
}
