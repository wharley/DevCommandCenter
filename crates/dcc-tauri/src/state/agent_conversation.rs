//! A resident agent's own conversation (`dcc_core::domain::agent_conversation`)
//! for the turns it runs and its DCC tools.

use dcc_core::domain::agent_conversation::conversation_block;

use super::*;

impl SessionCommandState {
    /// What a turn of `agent_id` carries besides its role: its memory and,
    /// in its own conversation, where it is and what it may do there.
    pub fn agent_turn_context(
        &self,
        session_id: &SessionId,
        agent_id: &str,
    ) -> Result<Option<String>> {
        let memory = self.agent_memory_block(agent_id, true)?;
        if !self
            .session_repo
            .is_agent_home_session(session_id, agent_id)?
        {
            return Ok(memory);
        }
        let conversation = conversation_block();
        Ok(Some(match memory {
            Some(memory) => format!("{conversation}\n\n{memory}"),
            None => conversation,
        }))
    }

    /// Whether `session_id` is the conversation of the agent it runs as.
    pub fn is_agent_conversation(&self, session_id: &SessionId) -> Result<bool> {
        match self.session_repo.load_session_agent(session_id)? {
            Some(agent) => self
                .session_repo
                .is_agent_home_session(session_id, &agent.id),
            None => Ok(false),
        }
    }

    /// DCC's database, for read-only snapshots outside this crate.
    pub fn database_file(&self) -> &std::path::Path {
        self.db_path()
    }
}
