//! 👍 / 👎 on an agent's answer in the team room
//! (`dcc_core::domain::agent_reaction`).

use dcc_core::domain::agent_reaction::{Reaction, MAX_LINE_KEY_CHARS};

use super::*;

impl SessionCommandState {
    pub fn set_agent_reaction(
        &self,
        agent_id: &str,
        session_id: &str,
        line_key: &str,
        reaction: Option<Reaction>,
    ) -> Result<()> {
        let fits = |text: &str| !text.trim().is_empty() && text.len() <= MAX_LINE_KEY_CHARS;
        if !fits(agent_id) || !fits(session_id) || !fits(line_key) {
            return Err(dcc_core::CoreError::InvalidInput(
                "a reaction needs its agent, conversation and answer".into(),
            ));
        }
        self.session_repo
            .set_agent_reaction(agent_id, session_id, line_key, reaction)
    }
}
