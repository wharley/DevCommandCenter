//! Agents an agent proposed (`dcc_propose_agent`,
//! `dcc_core::domain::agent_proposal`): the sidebar's read brings the
//! waiting ones; "Criar" makes the agent in the desktop and "Descartar"
//! answers the proposal.

use dcc_core::domain::agent_proposal::AgentProposal;
use serde_json::{Value, json};

use super::agents::AgentRecord;
use super::desktop_api::{ApiError, DesktopApi, encode};

impl DesktopApi {
    /// "Criar": the new agent's id.
    pub fn accept_agent_proposal(&self, proposal: &AgentProposal) -> Result<String, ApiError> {
        let value = self.post(
            &format!(
                "/api/v1/agents/{}/proposals/{}/accept",
                encode(&proposal.agent_id),
                encode(&proposal.id)
            ),
            &json!({}),
        )?;
        value
            .get("agentId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError::Failed("the new agent has no id".to_string()))
    }

    pub fn dismiss_agent_proposal(&self, proposal: &AgentProposal) -> Result<(), ApiError> {
        self.post(
            &format!(
                "/api/v1/agents/{}/proposals/{}/dismiss",
                encode(&proposal.agent_id),
                encode(&proposal.id)
            ),
            &json!({}),
        )
        .map(|_| ())
    }
}

/// The proposal as a new agent for the editor ("Editar antes").
pub fn draft_record(proposal: &AgentProposal) -> AgentRecord {
    let draft = proposal.draft.clone();
    AgentRecord {
        id: String::new(),
        name: draft.name,
        preset: None,
        avatar: draft.avatar,
        provider_id: draft.provider_id,
        model: draft.model,
        effort: draft.effort,
        kickoff_prompt: draft.kickoff_prompt,
        role: draft.role,
        offer_prompt: draft.offer_prompt,
        extra_instructions: draft.extra_instructions,
        projects: draft.projects,
        monthly_token_limit: draft.monthly_token_limit,
        tool_access: draft.tool_access,
        auto_start_tasks: draft.auto_start_tasks,
        may_call: draft.may_call,
        account_id: draft.account_id,
        sessions: Vec::new(),
    }
}
