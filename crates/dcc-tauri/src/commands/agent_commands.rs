//! Resident agents: person-owned identities a session can run as.

use dcc_core::domain::agent::{ResidentAgent, ResidentAgentDraft, ReviewerPresetText};
use dcc_core::domain::session::SessionId;
use dcc_infra::db::AgentSessionBinding;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::SessionCommandState;

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

/// `reviewer` carries the built-in reviewer's texts in the app language.
#[tauri::command]
pub async fn agents_overview(
    state: State<'_, SessionCommandState>,
    reviewer: ReviewerPresetText,
) -> Result<AgentsOverview, String> {
    Ok(AgentsOverview {
        agents: state
            .list_resident_agents(&reviewer)
            .map_err(|error| error.to_string())?,
        sessions: state
            .list_agent_session_bindings()
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn agents_save(
    state: State<'_, SessionCommandState>,
    input: SaveAgentInput,
) -> Result<ResidentAgent, String> {
    state
        .save_resident_agent(input.id.as_deref(), input.draft)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn agents_bind_session(
    state: State<'_, SessionCommandState>,
    input: BindSessionAgentInput,
) -> Result<(), String> {
    state
        .bind_session_agent(&SessionId(input.session_id), &input.agent_id)
        .map_err(|error| error.to_string())
}
