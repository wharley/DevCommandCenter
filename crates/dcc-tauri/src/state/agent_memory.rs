//! A resident agent's memory (`dcc_core::domain::agent_memory`) for the
//! turns it runs, the agent page and the agent's `dcc_agent_remember`.

use dcc_core::domain::agent_memory::{
    memory_block, AgentMemoryNote, AgentMemoryVersion, NoteSource,
};

use super::*;

impl SessionCommandState {
    /// The memory block for a turn of `agent_id` in `project_id` (`None`:
    /// its own conversation); `can_remember` when the session can call DCC's
    /// tools to save a note.
    pub fn agent_memory_block(
        &self,
        agent_id: &str,
        project_id: Option<&str>,
        can_remember: bool,
    ) -> Result<Option<String>> {
        let notes = self.session_repo.agent_memory(agent_id)?;
        Ok(memory_block(&notes, project_id, can_remember))
    }

    /// The project of a checkout, for a pull request review run there.
    pub fn memory_project_of_root(&self, root: &str) -> Result<Option<String>> {
        self.session_repo.memory_project_of_root(root)
    }

    pub fn agent_memory(&self, agent_id: &str) -> Result<Vec<AgentMemoryNote>> {
        self.session_repo.agent_memory(agent_id)
    }

    pub fn agent_memory_versions(&self, agent_id: &str) -> Result<Vec<AgentMemoryVersion>> {
        self.session_repo.agent_memory_versions(agent_id)
    }

    /// A note the person writes on the agent's page, for them (`None`) or
    /// for a project.
    pub fn add_agent_memory_note(
        &self,
        agent_id: &str,
        body: &str,
        project_id: Option<&str>,
    ) -> Result<AgentMemoryNote> {
        self.session_repo
            .add_agent_memory_note(agent_id, body, NoteSource::Person, project_id)
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
    /// the agent's name and the project's, if it went to one. In a task it
    /// goes to the task's project unless `for_person`; in the agent's own
    /// conversation, always to the person. A session that runs as no agent
    /// cannot remember.
    pub fn remember_for_session(
        &self,
        session_id: &SessionId,
        body: &str,
        for_person: bool,
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
        let project = if for_person {
            None
        } else {
            self.memory_project_of_session(session_id)?
        };
        let note = self.session_repo.add_agent_memory_note(
            &agent.id,
            body,
            NoteSource::Learned,
            project.as_deref(),
        )?;
        Ok((agent.name, note))
    }
}
