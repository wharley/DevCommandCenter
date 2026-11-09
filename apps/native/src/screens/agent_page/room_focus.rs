//! The team room on one agent: the "Conversa" tab of the agent's page. The
//! same room — its lines, its cards, the field — showing only that agent:
//! what the person sent it and what it answered, its own results and
//! reviews, and the field sends straight to it with no routing. The room's
//! own cards (the recap, the bulletin, the space, the check-in) stay in the
//! room.

use gpui::Context;

use super::room::TeamRoom;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};

impl TeamRoom {
    /// The agent the room shows alone, or the whole team (`None`).
    pub(crate) fn set_focus(&mut self, focus: Option<String>, cx: &mut Context<Self>) {
        if self.focus == focus {
            return;
        }
        self.focus = focus;
        self.mention = None;
        // The new conversation opens at its bottom.
        self.shown = usize::MAX;
        let placeholder = match self.focused(cx) {
            Some(agent) => cx.t_with(
                "agents.room.placeholderAgent",
                &Args::default().with("agent", agent.name),
            ),
            None => cx.t("agents.room.placeholder"),
        };
        self.input
            .update(cx, |area, cx| area.set_placeholder(placeholder, cx));
        cx.notify();
    }

    /// The agent the room shows alone.
    pub(crate) fn focused(&self, cx: &gpui::App) -> Option<AgentRecord> {
        let id = self.focus.as_deref()?;
        self.sidebar
            .read(cx)
            .agents()
            .iter()
            .find(|agent| agent.id == id)
            .cloned()
    }

    /// Whether what `agent_id` did shows now.
    pub(crate) fn shows(&self, agent_id: &str) -> bool {
        self.focus.as_deref().is_none_or(|focus| focus == agent_id)
    }
}
