//! Rooms (`docs/PLANO_SALA_POR_PROJETO`): which room an agent's conversation
//! is in, what a turn there hears about its project, and the teammate's
//! conversation in the same room for a pass.

use dcc_core::domain::agent_conversation::home_project_id;

use super::*;

impl SessionCommandState {
    pub fn set_session_room(&self, session_id: &SessionId, project_id: &str) -> Result<()> {
        self.session_repo.set_session_room(session_id, project_id)
    }

    pub fn session_room(&self, session_id: &SessionId) -> Result<Option<String>> {
        self.session_repo.session_room(session_id)
    }

    /// What a turn in a project's room hears: which project it is, so work
    /// asked there goes to it without the person naming it.
    pub(super) fn room_block(&self, session_id: &SessionId) -> Result<Option<String>> {
        let Some(project) = self.session_room(session_id)? else {
            return Ok(None);
        };
        let (name, path) = self
            .session_repo
            .project_name_and_path(&project)
            .unwrap_or_else(|| (project.clone(), String::new()));
        let folder = if path.is_empty() {
            String::new()
        } else {
            format!(" Its folder is {path}.")
        };
        Ok(Some(format!(
            "<dcc_room project=\"{name}\">\nThis conversation is the team room of the project \"{name}\".{folder} What the person asks here is about it unless they name another project: open its tasks in \"{name}\" and, for where things stand, look at its tasks first.\n</dcc_room>"
        )))
    }

    /// `agent_id`'s conversation in the room of `room` (none: the Geral
    /// room), the most recent one.
    pub fn agent_room_session(&self, agent_id: &str, room: Option<&str>) -> Result<Option<String>> {
        let home = home_project_id(agent_id);
        Ok(self
            .list_agent_session_bindings()?
            .into_iter()
            .filter(|binding| {
                binding.agent_id == agent_id
                    && binding.project_id == home
                    && binding.room.as_deref() == room
            })
            .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
            .map(|binding| binding.session_id))
    }
}
