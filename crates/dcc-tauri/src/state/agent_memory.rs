//! A resident agent's memory (`dcc_core::domain::agent_memory`) for the
//! turns it runs, the agent page and the agent's `dcc_agent_remember`.

use dcc_core::domain::agent_memory::{
    memory_block, AgentMemoryNote, AgentMemoryVersion, NoteSource,
};

use super::*;

impl SessionCommandState {
    /// The memory block for a turn of `agent_id`; `can_remember` when the
    /// session can call DCC's tools to save a note.
    pub fn agent_memory_block(&self, agent_id: &str, can_remember: bool) -> Result<Option<String>> {
        let notes = self.session_repo.agent_memory(agent_id)?;
        Ok(memory_block(&notes, can_remember))
    }

    pub fn agent_memory(&self, agent_id: &str) -> Result<Vec<AgentMemoryNote>> {
        self.session_repo.agent_memory(agent_id)
    }

    pub fn agent_memory_versions(&self, agent_id: &str) -> Result<Vec<AgentMemoryVersion>> {
        self.session_repo.agent_memory_versions(agent_id)
    }

    /// A note the person writes on the agent's page.
    pub fn add_agent_memory_note(&self, agent_id: &str, body: &str) -> Result<AgentMemoryNote> {
        self.session_repo
            .add_agent_memory_note(agent_id, body, NoteSource::Person)
    }

    pub fn edit_agent_memory_note(
        &self,
        agent_id: &str,
        note_id: &str,
        body: &str,
    ) -> Result<AgentMemoryNote> {
        self.session_repo
            .edit_agent_memory_note(agent_id, note_id, body)
    }

    pub fn remove_agent_memory_note(&self, agent_id: &str, note_id: &str) -> Result<()> {
        self.session_repo
            .remove_agent_memory_note(agent_id, note_id)
    }

    pub fn restore_agent_memory_version(
        &self,
        agent_id: &str,
        version_id: i64,
    ) -> Result<Vec<AgentMemoryNote>> {
        self.session_repo
            .restore_agent_memory_version(agent_id, version_id)
    }

    /// A note the agent of `session_id` learned (`dcc_agent_remember`), with
    /// the agent's name. A session that runs as no agent cannot remember.
    pub fn remember_for_session(
        &self,
        session_id: &SessionId,
        body: &str,
    ) -> Result<(String, AgentMemoryNote)> {
        let agent = self
            .session_repo
            .load_session_agent(session_id)?
            .ok_or_else(|| {
                dcc_core::CoreError::InvalidInput(
                    "Only a resident agent's session has a memory; this one runs as no agent."
                        .to_string(),
                )
            })?;
        let note = self
            .session_repo
            .add_agent_memory_note(&agent.id, body, NoteSource::Learned)?;
        Ok((agent.name, note))
    }
}
