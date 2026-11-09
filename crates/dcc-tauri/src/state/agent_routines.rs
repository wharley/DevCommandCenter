//! A resident agent's routines (`dcc_core::domain::agent_routine`): what the
//! agent's page saves and the app's scheduler marks, and the proposal an
//! agent makes with `dcc_propose_routine`, which waits for the person.

use dcc_core::domain::agent_routine::{Routine, RoutineDraft};

use super::*;

impl SessionCommandState {
    pub fn list_agent_routines(&self) -> Result<Vec<Routine>> {
        self.session_repo.list_agent_routines()
    }

    pub fn save_agent_routine(
        &self,
        agent_id: &str,
        id: Option<&str>,
        draft: RoutineDraft,
        enabled: bool,
    ) -> Result<Routine> {
        self.session_repo
            .save_agent_routine(agent_id, id, draft, enabled, false)
    }

    pub fn delete_agent_routine(&self, agent_id: &str, id: &str) -> Result<bool> {
        self.session_repo.delete_agent_routine(agent_id, id)
    }

    pub fn accept_agent_routine(&self, agent_id: &str, id: &str) -> Result<bool> {
        self.session_repo.accept_agent_routine(agent_id, id)
    }

    pub fn mark_agent_routine_ran(&self, agent_id: &str, id: &str, at: &str) -> Result<bool> {
        self.session_repo.mark_agent_routine_ran(agent_id, id, at)
    }

    /// The routine the agent of `session_id` proposes, off until the person
    /// creates it; the agent's name comes back for the answer.
    pub fn propose_routine_for_session(
        &self,
        session_id: &SessionId,
        draft: RoutineDraft,
    ) -> Result<(String, Routine)> {
        let agent = self
            .session_repo
            .load_session_agent(session_id)?
            .ok_or_else(|| {
                dcc_core::CoreError::InvalidInput("this session runs as no DCC agent".into())
            })?;
        let routine = self
            .session_repo
            .save_agent_routine(&agent.id, None, draft, false, true)?;
        Ok((agent.name, routine))
    }
}
