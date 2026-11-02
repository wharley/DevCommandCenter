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
