//! An agent's sources (`dcc_core::domain::agent_source`): the one it
//! proposes with `dcc_propose_source`, checked against what the connector
//! offers — a tool the vendor marks read-only, that the agent may use — and
//! the person's switches.

use dcc_core::domain::agent_source::{
    AgentSource, DEFAULT_EVERY, EVERY_CHOICES, MAX_ARGUMENTS_CHARS,
};
use dcc_core::ports::McpRepo as _;
use dcc_infra::mcp_db::SqliteMcpRepo;

use super::*;

/// What the agent writes in the tool's call.
#[derive(Clone, Debug)]
pub struct ProposedSource {
    /// The connector's name as the person installed it.
    pub connector: String,
    pub watch: String,
    pub tool: String,
    pub arguments: serde_json::Value,
    pub every_minutes: Option<u32>,
}

impl SessionCommandState {
    pub fn list_agent_sources(&self) -> Result<Vec<AgentSource>> {
        self.session_repo.list_agent_sources()
    }

    pub fn set_agent_source(&self, agent_id: &str, id: &str, enabled: bool) -> Result<bool> {
        self.session_repo.set_agent_source(agent_id, id, enabled)
    }

    pub fn delete_agent_source(&self, agent_id: &str, id: &str) -> Result<bool> {
        self.session_repo.delete_agent_source(agent_id, id)
    }

    pub fn agent_source_with_seen(&self, id: &str) -> Result<Option<(AgentSource, Vec<String>)>> {
        self.session_repo.agent_source_with_seen(id)
    }

    pub fn record_agent_source_read(&self, id: &str, seen: &[String], at: &str) -> Result<()> {
        self.session_repo.record_agent_source_read(id, seen, at)
    }

    /// The source the agent of `session_id` proposes; its name comes back.
    pub async fn propose_source_for_session(
        &self,
        session_id: &SessionId,
        proposed: ProposedSource,
    ) -> Result<(String, AgentSource)> {
        let invalid = |message: String| dcc_core::CoreError::InvalidInput(message);
        let agent = self
            .session_repo
            .load_session_agent(session_id)?
            .ok_or_else(|| invalid("this session runs as no DCC agent".into()))?;
        let installed = super::agent_proposals::connector_ids(&self.db_path)?;
        let Some((connector_id, connector)) = installed
            .iter()
            .find(|(_, name)| name.trim().eq_ignore_ascii_case(proposed.connector.trim()))
            .cloned()
        else {
            let names: Vec<&str> = installed.iter().map(|(_, name)| name.as_str()).collect();
            return Err(invalid(format!(
                "no connector named \"{}\"; the person's connectors are: {}",
                proposed.connector,
                names.join(", ")
            )));
        };
        if !agent.tool_access.allows_connector(&connector_id, true) {
            return Err(invalid(format!(
                "the person turned {connector} off for you; ask them to allow reading it under Access"
            )));
        }
        let repo = SqliteMcpRepo::open(&self.db_path)?;
        let read_only: Vec<String> = repo
            .list_mcp_connection_checks()
            .await?
            .into_iter()
            .find(|check| check.definition_id.0 == connector_id)
            .map(|check| {
                check
                    .tools
                    .into_iter()
                    .filter(|tool| tool.annotations.read_only_hint == Some(true))
                    .map(|tool| tool.name)
                    .collect()
            })
            .unwrap_or_default();
        if !read_only.iter().any(|tool| tool == proposed.tool.trim()) {
            return Err(invalid(format!(
                "\"{}\" is not one of {connector}'s read-only tools; pick one of: {}",
                proposed.tool,
                read_only.join(", ")
            )));
        }
        let arguments = serde_json::to_string(&proposed.arguments)
            .map_err(|error| invalid(error.to_string()))?;
        if !proposed.arguments.is_object() || arguments.chars().count() > MAX_ARGUMENTS_CHARS {
            return Err(invalid(format!(
                "arguments must be a JSON object of at most {MAX_ARGUMENTS_CHARS} characters"
            )));
        }
        let watch = proposed.watch.trim().to_string();
        if watch.is_empty() || watch.chars().count() > 120 {
            return Err(invalid("say what to watch in 1 to 120 characters".into()));
        }
        let source = AgentSource {
            id: String::new(),
            agent_id: agent.id.clone(),
            connector_id,
            connector,
            watch,
            tool: proposed.tool.trim().to_string(),
            arguments,
            every_minutes: proposed
                .every_minutes
                .filter(|every| EVERY_CHOICES.contains(every))
                .unwrap_or(DEFAULT_EVERY),
            enabled: true,
            proposed: true,
            last_checked_at: None,
            created_at: String::new(),
        };
        let stored = self.session_repo.propose_agent_source(&source)?;
        Ok((agent.name, stored))
    }
}
