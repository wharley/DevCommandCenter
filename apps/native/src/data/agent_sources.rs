//! The agents' sources as the app knows them (`dcc_core::domain::agent_source`):
//! read from DCC's database once a minute by their clock (`shell::sources`)
//! into a global the room and the agent's page read; switching, deleting
//! and reading go through the desktop, which owns the table and the
//! connectors.

use dcc_core::domain::agent_source::{AgentSource, SourceItem};
use gpui::Global;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::data::desktop_api::{ApiError, DesktopApi, encode};

#[derive(Default)]
pub struct Sources {
    pub list: Vec<AgentSource>,
}

impl Global for Sources {}

impl Sources {
    pub fn of<'a>(&'a self, agent_id: &'a str) -> impl Iterator<Item = &'a AgentSource> {
        self.list
            .iter()
            .filter(move |source| source.agent_id == agent_id && !source.proposed)
    }

    /// The sources agents proposed, waiting for the person.
    pub fn proposed(&self) -> impl Iterator<Item = &AgentSource> {
        self.list.iter().filter(|source| source.proposed)
    }
}

/// Every live agent's sources from DCC's database.
pub fn read() -> Vec<AgentSource> {
    let db = crate::data::loader::database_path();
    dcc_infra::db::SqliteSessionRepo::open_read_only(&db)
        .and_then(|repo| repo.list_agent_sources())
        .unwrap_or_default()
}

/// What a read brought: on the first, nothing (it only remembers).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceRead {
    pub first: bool,
    pub news: Vec<SourceItem>,
}

fn source_path(source: &AgentSource, action: &str) -> String {
    format!(
        "/api/v1/agents/{}/sources/{}/{action}",
        encode(&source.agent_id),
        encode(&source.id)
    )
}

impl DesktopApi {
    /// "Criar" on a proposal, or the switch on the agent's page.
    pub fn switch_source(&self, source: &AgentSource, enabled: bool) -> Result<(), ApiError> {
        self.post(
            &source_path(source, "switch"),
            &json!({ "enabled": enabled }),
        )
        .map(|_| ())
    }

    pub fn delete_source(&self, source: &AgentSource) -> Result<(), ApiError> {
        self.post(&source_path(source, "delete"), &json!({}))
            .map(|_| ())
    }

    /// Reads the source now (the connector answers in its own time).
    pub fn read_source(&self, source: &AgentSource) -> Result<SourceRead, ApiError> {
        let value: Value = self.request_waiting(
            "POST",
            &source_path(source, "read"),
            Some(&json!({})),
            std::time::Duration::from_secs(60),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }
}
