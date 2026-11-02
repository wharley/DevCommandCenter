//! Resident agents: person-owned identities a session can run as.

use dcc_core::domain::agent::{ResidentAgent, ResidentAgentDraft};
use dcc_infra::db::AgentSessionBinding;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsOverview {
    pub agents: Vec<ResidentAgent>,
    pub sessions: Vec<AgentSessionBinding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAgentInput {
    #[serde(default)]
    pub id: Option<String>,
    pub draft: ResidentAgentDraft,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindSessionAgentInput {
    pub session_id: String,
    pub agent_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecapInput {
    pub from: String,
    pub to: String,
}
