//! Replies an agent proposes (`dcc_core::domain::agent_reply`), checked
//! against what the connector offers — a tool the vendor does not mark
//! read-only (it sends), on a connector the agent may write to — and the
//! person's "Descartar".

use dcc_core::domain::agent_reply::{
    sendable, AgentReply, ReplyState, MAX_DESTINATION_CHARS, MAX_REPLY_CHARS,
};
use dcc_core::ports::McpRepo as _;
use dcc_infra::mcp_db::SqliteMcpRepo;

use super::*;

/// What the agent writes in the tool's call.
#[derive(Clone, Debug)]
pub struct ProposedReply {
    pub connector: String,
    pub destination: String,
    pub tool: String,
    pub arguments: serde_json::Value,
    pub text_argument: String,
    pub text: String,
}

impl SessionCommandState {
    pub fn list_pending_agent_replies(&self) -> Result<Vec<AgentReply>> {
        self.session_repo.list_pending_agent_replies()
    }

    pub fn agent_reply(&self, id: &str) -> Result<Option<AgentReply>> {
        self.session_repo.agent_reply(id)
    }

    pub fn discard_agent_reply(&self, id: &str) -> Result<bool> {
        self.session_repo.discard_agent_reply(id)
    }

    pub fn mark_agent_reply_sent(&self, id: &str, text: &str, at: &str) -> Result<bool> {
        self.session_repo.mark_agent_reply_sent(id, text, at)
    }

    pub fn record_agent_reply_error(&self, id: &str, text: &str, error: &str) -> Result<()> {
        self.session_repo.record_agent_reply_error(id, text, error)
    }

    /// The reply the agent of `session_id` proposes.
    pub async fn propose_reply_for_session(
        &self,
        session_id: &SessionId,
        proposed: ProposedReply,
    ) -> Result<AgentReply> {
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
        if !agent.tool_access.allows_connector(&connector_id, false) {
            return Err(invalid(format!(
                "the person does not let you write to {connector}; describe the reply in your answer instead"
            )));
        }
        let repo = SqliteMcpRepo::open(&self.db_path)?;
        let senders: Vec<String> = repo
            .list_mcp_connection_checks()
            .await?
            .into_iter()
            .find(|check| check.definition_id.0 == connector_id)
            .map(|check| {
                check
                    .tools
                    .into_iter()
                    .filter(|tool| tool.annotations.read_only_hint != Some(true))
                    .map(|tool| tool.name)
                    .collect()
            })
            .unwrap_or_default();
        let tool = proposed.tool.trim().to_string();
        if !senders.iter().any(|name| *name == tool) {
            return Err(invalid(format!(
                "\"{}\" is not one of {connector}'s tools that send; pick one of: {}",
                proposed.tool,
                senders.join(", ")
            )));
        }
        let text_argument = proposed.text_argument.trim().to_string();
        let mut arguments = proposed.arguments;
        let object = arguments
            .as_object_mut()
            .filter(|_| !text_argument.is_empty())
            .ok_or_else(|| {
                invalid("arguments must be a JSON object and textArgument named".into())
            })?;
        // The text lives on its own; the person may change it.
        object.remove(&text_argument);
        let arguments =
            serde_json::to_string(&arguments).map_err(|error| invalid(error.to_string()))?;
        let text = sendable(&proposed.text)
            .ok_or_else(|| {
                invalid(format!(
                    "write the reply in 1 to {MAX_REPLY_CHARS} characters"
                ))
            })?
            .to_string();
        let destination = proposed.destination.trim().to_string();
        if destination.is_empty() || destination.chars().count() > MAX_DESTINATION_CHARS {
            return Err(invalid(format!(
                "say where it goes in 1 to {MAX_DESTINATION_CHARS} characters"
            )));
        }
        let reply = AgentReply {
            id: String::new(),
            agent_id: agent.id,
            session_id: session_id.0.clone(),
            connector_id,
            connector,
            tool,
            arguments,
            text_argument,
            destination,
            text,
            state: ReplyState::Pending,
            error: None,
            created_at: String::new(),
            sent_at: None,
            room: self.session_room(session_id)?,
        };
        self.session_repo.propose_agent_reply(&reply)
    }
}
