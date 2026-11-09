//! The agent's own conversation, in its home (`data::agent_conversation`):
//! the latest one, and opening it whole, with its tools ("Conversa
//! completa" on the "Conversa" tab). The room makes the home, its task and
//! the conversation the first time.

use dcc_core::domain::agent_conversation::home_project_id;
use gpui::Context;

use super::{AgentPage, AgentPageEvent};
use crate::data::agents::{AgentRecord, AgentSession};

/// The agent's latest conversation in its home, if it has one.
pub(crate) fn home_session(agent: &AgentRecord) -> Option<&AgentSession> {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .filter(|session| session.project_id == home)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
}

impl AgentPage {
    /// The agent's own conversation, whole, with its tools.
    pub(crate) fn open_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return;
        };
        if let Some(session) = home_session(&data.agent) {
            cx.emit(AgentPageEvent::OpenSession {
                task_id: session.workspace_id.clone(),
                session_id: session.session_id.clone(),
            });
        }
    }
}
