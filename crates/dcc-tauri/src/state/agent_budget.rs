//! A resident agent's monthly limit (`dcc_core::domain::agent_budget`),
//! checked before a turn of a conversation bound to it: one read of the
//! month's usage, only for an agent with a limit that was not let through
//! this month.

use chrono::Local;
use dcc_core::domain::agent_budget::{month_key, month_start, over_limit, AgentSpend};

use super::*;

impl SessionCommandState {
    /// The agent `session_id` runs as, when it is past its limit now.
    pub fn agent_over_limit(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<(dcc_core::domain::agent::ResidentAgent, AgentSpend)>> {
        let Some(agent) = self.session_repo.load_session_agent(session_id)? else {
            return Ok(None);
        };
        if agent.monthly_token_limit.is_none() {
            return Ok(None);
        }
        let now = Local::now();
        let month = month_key(now);
        if agent.limit_override_month.as_deref() == Some(month.as_str()) {
            return Ok(None);
        }
        let spend = self
            .session_repo
            .agent_spend_since(&agent.id, &month_start(now))?;
        Ok(over_limit(
            &spend,
            agent.monthly_token_limit,
            agent.limit_override_month.as_deref(),
            &month,
        )
        .then_some((agent, spend)))
    }

    /// "Continuar mesmo assim": past its limit for the rest of this month.
    pub fn let_agent_past_limit(&self, agent_id: &str) -> Result<bool> {
        self.session_repo
            .let_agent_past_limit(agent_id, &month_key(Local::now()))
    }
}
