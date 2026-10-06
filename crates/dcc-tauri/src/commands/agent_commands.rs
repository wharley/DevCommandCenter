//! Resident agents: person-owned identities a session can run as.

use dcc_core::domain::agent::{
    ChroniclerPresetText, ResearcherPresetText, ResidentAgent, ResidentAgentDraft,
    ReviewerPresetText,
};
use dcc_core::domain::session::SessionId;
use dcc_infra::db::{ActivityRecap, AgentSessionBinding};
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

/// `reviewer`, `researcher` and `chronicler` carry the built-in agents' texts
/// in the app language. An app that does not send one gets the English text.
#[tauri::command]
pub async fn agents_overview(
    state: State<'_, SessionCommandState>,
    reviewer: ReviewerPresetText,
    researcher: Option<ResearcherPresetText>,
    chronicler: Option<ChroniclerPresetText>,
) -> Result<AgentsOverview, String> {
    Ok(AgentsOverview {
        agents: state
            .list_resident_agents(
                &reviewer,
                &researcher.unwrap_or_default(),
                &chronicler.unwrap_or_default(),
            )
            .map_err(|error| error.to_string())?,
        sessions: state
            .list_agent_session_bindings()
            .map_err(|error| error.to_string())?,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecapInput {
    pub from: String,
    pub to: String,
}

/// Instants are compared as text in the database, so both ends are brought
/// to the form DCC stores: UTC with milliseconds and a `Z`.
fn stored_instant(value: &str) -> Result<String, String> {
    chrono::DateTime::parse_from_rfc3339(value.trim())
        .map(|at| {
            at.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .map_err(|error| format!("invalid recap instant {value:?}: {error}"))
}

/// What moved in the person's tasks during the chronicler's recap window.
#[tauri::command]
pub async fn agents_activity_recap(
    state: State<'_, SessionCommandState>,
    input: ActivityRecapInput,
) -> Result<ActivityRecap, String> {
    let from = stored_instant(&input.from)?;
    let to = stored_instant(&input.to)?;
    state
        .activity_recap(&from, &to)
        .map_err(|error| error.to_string())
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

#[cfg(test)]
mod tests {
    use super::stored_instant;

    #[test]
    fn recap_instants_take_the_stored_form() {
        assert_eq!(
            stored_instant("2026-10-06T04:00:00-03:00").unwrap(),
            "2026-10-06T07:00:00.000Z"
        );
        assert_eq!(
            stored_instant(" 2026-10-06T07:00:00.5Z ").unwrap(),
            "2026-10-06T07:00:00.500Z"
        );
        assert!(stored_instant("yesterday").is_err());
    }
}
