//! The agent's own conversation, in its home (`data::agent_conversation`):
//! the latest one, and opening it whole, with its tools ("Conversa
//! completa" on the "Conversa" tab). The room makes the home, its task and
//! the conversation the first time.

use dcc_core::domain::agent_conversation::home_project_id;
use gpui::Context;

use super::{AgentPage, AgentPageEvent};
use crate::data::agents::{AgentRecord, AgentSession};

/// The agent's latest conversation in `room` (none: the Geral room).
pub(crate) fn room_session<'a>(
    agent: &'a AgentRecord,
    room: Option<&str>,
) -> Option<&'a AgentSession> {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .filter(|session| session.project_id == home && session.room.as_deref() == room)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
}

impl AgentPage {
    /// The agent's own conversation, whole, with its tools.
    pub(crate) fn open_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return;
        };
        let room = self.agent_room(cx);
        if let Some(session) = room_session(&data.agent, room.as_deref()) {
            cx.emit(AgentPageEvent::OpenSession {
                task_id: session.workspace_id.clone(),
                session_id: session.session_id.clone(),
            });
        }
    }
}
