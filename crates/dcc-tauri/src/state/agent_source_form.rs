//! A source the person makes from the "Nova fonte" form on an agent's page
//! (`dcc_core::domain::agent_source`), checked as a proposed one is: a
//! connector the agent may read, one of its tools the vendor marks
//! read-only, its arguments as a JSON object. It is on from the start —
//! the person made it, there is nothing to accept.

use dcc_core::domain::agent::ResidentAgent;
use dcc_core::domain::agent_source::{
    AgentSource, DEFAULT_EVERY, EVERY_CHOICES, MAX_ARGUMENTS_CHARS,
};
use dcc_core::ports::McpRepo as _;
use dcc_infra::mcp_db::SqliteMcpRepo;

use super::*;

/// What the form sends.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInput {
    pub connector_id: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: Option<serde_json::Value>,
    /// What it watches, in the person's words; not needed for a test read.
    #[serde(default)]
    pub watch: String,
    #[serde(default)]
    pub every_minutes: Option<u32>,
    /// The project whose room its news wakes the agent in; none: Geral.
    #[serde(default)]
    pub room: Option<String>,
}

fn invalid(message: impl Into<String>) -> dcc_core::CoreError {
    dcc_core::CoreError::InvalidInput(message.into())
}

impl SessionCommandState {
    fn source_form_agent(&self, agent_id: &str) -> Result<ResidentAgent> {
        self.list_stored_agents(&Default::default())?
            .into_iter()
            .find(|agent| agent.id == agent_id)
            .ok_or_else(|| invalid("agent not found"))
    }

    /// The person's connectors the agent may read, as `(id, name)`.
    pub fn source_form_connectors(&self, agent_id: &str) -> Result<Vec<(String, String)>> {
        let agent = self.source_form_agent(agent_id)?;
        Ok(super::agent_proposals::connector_ids(&self.db_path)?
            .into_iter()
            .filter(|(id, _)| agent.tool_access.allows_connector(id, true))
            .collect())
    }

    /// The connector's tools its vendor marks read-only, as its last
    /// connection check listed them.
    pub(crate) async fn read_only_connector_tools(
        &self,
        connector_id: &str,
    ) -> Result<Vec<String>> {
        let repo = SqliteMcpRepo::open(&self.db_path)?;
        Ok(repo
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
            .unwrap_or_default())
    }

    /// `input` as a source of `agent_id`, or why it cannot be one. With
    /// `named`, what it watches must be said (a test read needs no name).
    pub async fn checked_source(
        &self,
        agent_id: &str,
        input: SourceInput,
        named: bool,
    ) -> Result<(ResidentAgent, AgentSource)> {
        let agent = self.source_form_agent(agent_id)?;
        let connector_id = input.connector_id.trim().to_string();
        let connector = super::agent_proposals::connector_ids(&self.db_path)?
            .into_iter()
            .find(|(id, _)| *id == connector_id)
            .map(|(_, name)| name)
            .ok_or_else(|| invalid("that connector is not installed"))?;
        if !agent.tool_access.allows_connector(&connector_id, true) {
            return Err(invalid(format!(
                "{connector} is off for this agent; allow reading it under Access"
            )));
        }
        let tool = input.tool.trim().to_string();
        let read_only = self.read_only_connector_tools(&connector_id).await?;
        if !read_only.iter().any(|known| *known == tool) {
            return Err(invalid(format!(
                "\"{tool}\" is not one of {connector}'s read-only tools"
            )));
        }
        let arguments = input.arguments.unwrap_or_else(|| serde_json::json!({}));
        let written =
            serde_json::to_string(&arguments).map_err(|error| invalid(error.to_string()))?;
        if !arguments.is_object() || written.chars().count() > MAX_ARGUMENTS_CHARS {
            return Err(invalid(format!(
                "arguments must be a JSON object of at most {MAX_ARGUMENTS_CHARS} characters"
            )));
        }
        let watch = input.watch.trim().to_string();
        if named && (watch.is_empty() || watch.chars().count() > 120) {
            return Err(invalid("say what to watch in 1 to 120 characters"));
        }
        let source = AgentSource {
            id: String::new(),
            agent_id: agent.id.clone(),
            connector_id,
            connector,
            watch,
            tool,
            arguments: written,
            every_minutes: input
                .every_minutes
                .filter(|every| EVERY_CHOICES.contains(every))
                .unwrap_or(DEFAULT_EVERY),
            enabled: true,
            proposed: false,
            last_checked_at: None,
            created_at: String::new(),
            room: input.room.filter(|room| !room.trim().is_empty()),
        };
        Ok((agent, source))
    }

    /// "Criar fonte": stored and on.
    pub async fn create_agent_source(
        &self,
        agent_id: &str,
        input: SourceInput,
    ) -> Result<AgentSource> {
        let (_, source) = self.checked_source(agent_id, input, true).await?;
        let mut stored = self.session_repo.propose_agent_source(&source)?;
        self.session_repo
            .set_agent_source(&stored.agent_id, &stored.id, true)?;
        stored.proposed = false;
        Ok(stored)
    }
}
