//! A resident agent's own conversation (`dcc_core::domain::agent_conversation`)
//! for the turns it runs and its DCC tools.

use dcc_core::domain::agent_conversation::conversation_block;
use dcc_core::domain::team_room::handoff_block;

use super::*;

impl SessionCommandState {
    /// What a turn of `agent_id` carries besides its role: its memory and,
    /// in its own conversation, where it is and what it may do there.
    pub fn agent_turn_context(
        &self,
        session_id: &SessionId,
        agent_id: &str,
    ) -> Result<Option<String>> {
        let home = self
            .session_repo
            .is_agent_home_session(session_id, agent_id)?;
        let project = if home {
            None
        } else {
            self.memory_project_of_session(session_id)?
        };
        let memory = self.agent_memory_block(agent_id, project.as_deref(), true)?;
        if !home {
            return Ok(memory);
        }
        // Its teammates, and how it suggests one of them takes over (or,
        // where the person allowed it, passes to one itself).
        let agents = self
            .session_repo
            .list_stored_resident_agents(&Default::default())?;
        let may_call: Vec<String> = agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .map(|agent| agent.may_call.clone())
            .unwrap_or_default();
        let teammates: Vec<&dcc_core::domain::agent::ResidentAgent> =
            agents.iter().filter(|agent| agent.id != agent_id).collect();
        let names: Vec<&str> = teammates.iter().map(|agent| agent.name.as_str()).collect();
        let direct: Vec<&str> = teammates
            .iter()
            .filter(|agent| {
                may_call.contains(&agent.id)
                    && agent.preset.as_deref() != Some(dcc_core::domain::agent::REVIEWER_PRESET)
            })
            .map(|agent| agent.name.as_str())
            .collect();
        let conversation = match handoff_block(&names, &direct) {
            Some(team) => format!("{}\n\n{team}", conversation_block()),
            None => conversation_block(),
        };
        // In a project's room: which project it is.
        let conversation = match self.room_block(session_id)? {
            Some(room) => format!("{conversation}\n\n{room}"),
            None => conversation,
        };
        Ok(Some(match memory {
            Some(memory) => format!("{conversation}\n\n{memory}"),
            None => conversation,
        }))
    }

    /// The live agents as stored, without seeding the built-in ones.
    pub fn list_stored_agents(
        &self,
        reviewer: &dcc_core::domain::agent::ReviewerPresetText,
    ) -> Result<Vec<dcc_core::domain::agent::ResidentAgent>> {
        self.session_repo.list_stored_resident_agents(reviewer)
    }

    /// Every session bound to a live agent, most recently updated first.
    pub fn agent_session_bindings(&self) -> Result<Vec<dcc_infra::db::AgentSessionBinding>> {
        self.session_repo.list_agent_session_bindings()
    }

    /// The agent `session_id` runs as, if any.
    pub fn session_agent(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<dcc_core::domain::agent::ResidentAgent>> {
        self.session_repo.load_session_agent(session_id)
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
