//! Agents an agent proposes with `dcc_propose_agent`
//! (`dcc_core::domain::agent_proposal`): the projects and connectors come
//! by the names the agent knows and are kept by id; "Criar" makes the agent
//! from the stored draft and "Descartar" (or creating it after editing)
//! answers the proposal.

use std::collections::BTreeMap;

use dcc_core::domain::agent::{AgentAvatar, ResidentAgent};
use dcc_core::domain::agent_access::{AccessLevel, ToolAccess};
use dcc_core::domain::agent_proposal::{proposal_draft, AgentProposal};
use rusqlite::{Connection, OpenFlags};

use super::*;

/// What the agent writes in the tool's call.
#[derive(Clone, Debug, Default)]
pub struct ProposedAgent {
    pub name: String,
    pub role: String,
    pub first_request: String,
    pub avatar: Option<AgentAvatar>,
    /// Project names as `dcc_overview` shows them.
    pub projects: Vec<String>,
    /// By group of DCC's tools (`DCC_TOOL_GROUPS`).
    pub groups: BTreeMap<String, AccessLevel>,
    /// By connector name, as the person installed it.
    pub connectors: BTreeMap<String, AccessLevel>,
}

/// The connectors' ids by their names, any case.
pub(super) fn connector_ids(db_path: &Path) -> Result<Vec<(String, String)>> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
    let mut statement = conn
        .prepare("SELECT id, display_name FROM dcc_mcp_definitions")
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .and_then(|rows| rows.collect::<rusqlite::Result<Vec<(String, String)>>>())
        .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
    Ok(rows)
}

impl SessionCommandState {
    /// The proposal of the agent of `session_id`; its name comes back.
    pub fn propose_agent_for_session(
        &self,
        session_id: &SessionId,
        proposed: ProposedAgent,
    ) -> Result<(String, AgentProposal)> {
        let invalid = |message: String| dcc_core::CoreError::InvalidInput(message);
        let agent = self
            .session_repo
            .load_session_agent(session_id)?
            .ok_or_else(|| invalid("this session runs as no DCC agent".into()))?;
        let projects = proposed
            .projects
            .iter()
            .map(|name| {
                crate::agent_task::find_project(&self.db_path, name, false, None)
                    .map(|target| target.project_id)
                    .map_err(invalid)
            })
            .collect::<Result<Vec<_>>>()?;
        let installed = if proposed.connectors.is_empty() {
            Vec::new()
        } else {
            connector_ids(&self.db_path)?
        };
        let mut connectors = BTreeMap::new();
        for (name, level) in proposed.connectors {
            let Some((id, _)) = installed
                .iter()
                .find(|(_, display)| display.trim().eq_ignore_ascii_case(name.trim()))
            else {
                let names: Vec<&str> = installed.iter().map(|(_, name)| name.as_str()).collect();
                return Err(invalid(format!(
                    "no connector named \"{name}\"; the person's connectors are: {}",
                    names.join(", ")
                )));
            };
            connectors.insert(id.clone(), level);
        }
        let draft = proposal_draft(
            &proposed.name,
            &proposed.role,
            &proposed.first_request,
            proposed.avatar,
            projects,
            ToolAccess {
                groups: proposed.groups,
                connectors,
                tasks_approval: None,
            },
        )
        .map_err(invalid)?;
        let proposal = self
            .session_repo
            .add_agent_proposal(&agent.id, &session_id.0, &draft)?;
        Ok((agent.name, proposal))
    }

    /// "Criar": the agent, made from the proposal; none if it was answered.
    pub fn accept_agent_proposal(&self, id: &str) -> Result<Option<ResidentAgent>> {
        let Some(proposal) = self
            .session_repo
            .waiting_agent_proposals()?
            .into_iter()
            .find(|proposal| proposal.id == id)
        else {
            return Ok(None);
        };
        let agent = self
            .session_repo
            .save_resident_agent(None, proposal.draft)?;
        self.session_repo.answer_agent_proposal(id)?;
        Ok(Some(agent))
    }

    /// "Descartar", or the person created it after editing it.
    pub fn dismiss_agent_proposal(&self, id: &str) -> Result<bool> {
        Ok(self.session_repo.answer_agent_proposal(id)?.is_some())
    }
}
