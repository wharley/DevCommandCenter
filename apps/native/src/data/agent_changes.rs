//! Changes an agent proposed to what exists (`dcc_propose_change`,
//! `dcc_core::domain::agent_change`): the sidebar's read brings the waiting
//! ones; "Aplicar" writes the change in the desktop and "Descartar"
//! answers it.

use dcc_core::domain::agent_change::AgentChange;
use serde_json::json;

use super::desktop_api::{ApiError, DesktopApi, encode};

impl DesktopApi {
    /// "Aplicar".
    pub fn accept_agent_change(&self, change: &AgentChange) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/agent-changes/{}/accept", encode(&change.id)),
            &json!({}),
        )
        .map(|_| ())
    }

    /// "Descartar".
    pub fn dismiss_agent_change(&self, change: &AgentChange) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/agent-changes/{}/dismiss", encode(&change.id)),
            &json!({}),
        )
        .map(|_| ())
    }
}
