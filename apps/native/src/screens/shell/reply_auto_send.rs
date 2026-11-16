//! Sending a reply an agent proposed without the person pressing "Enviar",
//! where they granted it for that agent and room (`data::agent_auto_send`).
//! No clock of its own: the replies are looked at when the room's list of
//! them changes (an agent's turn ended, the sources' minute). Each goes
//! through the route "Enviar" calls, once: one that fails stays as its
//! card, with why, for the person; past the day's ceiling they wait too.

use std::collections::HashSet;

use chrono::Utc;
use dcc_core::domain::agent_reply::{AgentReply, ReplyState};
use gpui::{BorrowAppContext, Context, Subscription};

use super::Shell;
use crate::data::agent_replies::Replies;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::desktop_api::DesktopApi;
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

/// What the shell keeps for it.
pub(super) struct ReplyAutoSend {
    _replies: Subscription,
    /// The replies already tried in this run of the app.
    tried: HashSet<String>,
}

impl ReplyAutoSend {
    pub(super) fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            _replies: cx.observe_global::<Replies>(|shell, cx| shell.replies_arrived(cx)),
            tried: HashSet::new(),
        }
    }
}

/// The replies that go by themselves now, in the order they were proposed.
pub(super) fn due(
    replies: &[AgentReply],
    tried: &HashSet<String>,
    may_send: impl Fn(&AgentReply) -> bool,
) -> Vec<AgentReply> {
    replies
        .iter()
        .filter(|reply| {
            reply.state == ReplyState::Pending
                && reply.error.is_none()
                && !reply.text.trim().is_empty()
                && !tried.contains(&reply.id)
                && may_send(reply)
        })
        .cloned()
        .collect()
}

impl Shell {
    fn replies_arrived(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<AgentTriggers>() || !cx.has_global::<Replies>() {
            return;
        }
        let now = Utc::now();
        let triggers = cx.global::<AgentTriggers>();
        let waiting = due(
            &cx.global::<Replies>().list,
            &self.reply_auto_send.tried,
            |reply| triggers.may_send_reply(&reply.agent_id, reply.room.as_deref(), now),
        );
        for reply in waiting {
            // Asked again: an earlier one of this pass may have reached the
            // day's ceiling.
            if !cx.global::<AgentTriggers>().may_send_reply(
                &reply.agent_id,
                reply.room.as_deref(),
                now,
            ) {
                continue;
            }
            self.reply_auto_send.tried.insert(reply.id.clone());
            cx.update_global::<AgentTriggers, _>(|triggers, _| {
                triggers.record_reply_sent(&reply.agent_id, now)
            });
            let agent = self
                .sidebar
                .read(cx)
                .agents()
                .iter()
                .find(|agent| agent.id == reply.agent_id)
                .map(|agent| agent.name.clone())
                .unwrap_or_default();
            cx.spawn(async move |_, cx| {
                let (id, text) = (reply.id.clone(), reply.text.trim().to_string());
                let sent = cx
                    .background_executor()
                    .spawn(async move {
                        DesktopApi::load()
                            .ok_or_else(|| "DCC is not running".to_string())
                            .and_then(|api| {
                                api.send_reply(&id, &text)
                                    .map_err(|error| error.to_string())
                            })
                    })
                    .await;
                let _ = cx.update(|cx| match sent {
                    Ok(()) => {
                        cx.update_global::<Replies, _>(|replies, _| {
                            replies.list.retain(|found| found.id != reply.id)
                        });
                        let text = cx.t_with(
                            "agents.autoSend.replySent",
                            &Args::default()
                                .with("agent", agent)
                                .with("destination", reply.destination.clone()),
                        );
                        crate::ui::attention::notify_away(&text, "");
                        Toaster::success(text, cx);
                    }
                    // It stays as its card, with why.
                    Err(error) => {
                        cx.update_global::<Replies, _>(|replies, _| {
                            for found in
                                replies.list.iter_mut().filter(|found| found.id == reply.id)
                            {
                                found.error = Some(error.clone());
                            }
                        });
                        Toaster::error(error, cx);
                    }
                });
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(id: &str, agent: &str) -> AgentReply {
        AgentReply {
            id: id.into(),
            agent_id: agent.into(),
            session_id: "s".into(),
            connector_id: "c".into(),
            connector: "Slack".into(),
            tool: "send".into(),
            arguments: "{}".into(),
            text_argument: "text".into(),
            destination: "#suporte".into(),
            text: "Feito.".into(),
            state: ReplyState::Pending,
            error: None,
            created_at: "2026-11-16T10:00:00Z".into(),
            sent_at: None,
            room: None,
        }
    }

    #[test]
    fn only_a_granted_reply_not_tried_and_not_failed_goes() {
        let mut failed = reply("r-3", "sup");
        failed.error = Some("channel not found".into());
        let mut empty = reply("r-4", "sup");
        empty.text = " ".into();
        let replies = [reply("r-1", "sup"), reply("r-2", "dev"), failed, empty];
        let granted = |reply: &AgentReply| reply.agent_id == "sup";
        let ids = |tried: &HashSet<String>| -> Vec<String> {
            due(&replies, tried, granted)
                .into_iter()
                .map(|reply| reply.id)
                .collect()
        };
        assert_eq!(ids(&HashSet::new()), ["r-1"]);
        // Tried once: not again, whatever came of it.
        assert!(ids(&HashSet::from(["r-1".to_string()])).is_empty());
    }
}
