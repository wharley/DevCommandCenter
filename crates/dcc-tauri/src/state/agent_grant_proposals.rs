//! What lets an agent act without asking, proposed by the Guide with
//! `dcc_propose_grant` (`dcc_core::domain::agent_grant_proposal`). Only the
//! Guide proposes it, never for itself; it waits as a card and the app
//! writes it on the person's click — nothing here turns anything on.

use std::collections::BTreeMap;

use dcc_core::domain::agent::REVIEWER_PRESET;
use dcc_core::domain::agent_change::{clean_reason, AgentChange};
use dcc_core::domain::agent_grant_proposal::{grants_change, WantedGrants};
use dcc_core::domain::agent_guide::GUIDE_PRESET;
use dcc_core::domain::agent_trigger::Trigger;

use super::*;

/// The desktop's switches and grants, beside the database.
const TRIGGERS_FILE: &str = "agent-triggers.json";

/// What the Guide writes in the tool's call, by the names it reads.
#[derive(Clone, Debug, Default)]
pub struct ProposedGrants {
    pub agent: String,
    pub review: Option<String>,
    pub review_projects: Option<Vec<String>>,
    pub reply: Option<bool>,
    /// Project names, or "Geral".
    pub reply_rooms: Option<Vec<String>>,
    pub deliver: Option<bool>,
    pub deliver_projects: Option<Vec<String>>,
    pub fix_in_task: Option<bool>,
    /// By trigger id (`Trigger::id`).
    pub triggers: BTreeMap<String, bool>,
    pub reason: String,
}

fn invalid(message: String) -> dcc_core::CoreError {
    dcc_core::CoreError::InvalidInput(message)
}

impl SessionCommandState {
    fn project_ids(&self, names: &Option<Vec<String>>) -> Result<Option<Vec<String>>> {
        names
            .as_ref()
            .map(|names| {
                names
                    .iter()
                    .map(|name| self.project_of_name(name))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()
    }

    /// The grant the Guide (the agent of `session_id`) proposes.
    pub fn propose_grants_for_session(
        &self,
        session_id: &SessionId,
        proposed: ProposedGrants,
    ) -> Result<AgentChange> {
        let guide = self
            .session_repo
            .load_session_agent(session_id)?
            .ok_or_else(|| invalid("this session runs as no DCC agent".into()))?;
        if guide.preset.as_deref() != Some(GUIDE_PRESET) {
            return Err(invalid(
                "only the Guide proposes what an agent does without asking; tell the person it is on the agent's page, under \"Enviar sem perguntar\" and \"Gatilhos\"".into(),
            ));
        }
        let agent = self.agent_of_name(&proposed.agent)?;
        if agent.id == guide.id || agent.preset.as_deref() == Some(GUIDE_PRESET) {
            return Err(invalid(
                "the Guide proposes nothing for itself: it sends nothing and nothing wakes it"
                    .into(),
            ));
        }
        let mut triggers = Vec::new();
        for (id, on) in &proposed.triggers {
            let trigger = serde_json::from_value::<Trigger>(serde_json::Value::String(id.clone()))
                .map_err(|_| {
                    invalid(format!(
                        "no trigger \"{id}\"; the triggers are: {}",
                        Trigger::ALL.map(Trigger::id).join(", ")
                    ))
                })?;
            triggers.push((trigger, *on));
        }
        let reply_rooms = proposed
            .reply_rooms
            .as_ref()
            .map(|rooms| {
                rooms
                    .iter()
                    .map(|room| {
                        if super::agent_changes::is_general(room) {
                            Ok(String::new())
                        } else {
                            self.project_of_name(room)
                        }
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?;
        let wanted = WantedGrants {
            review: proposed.review.map(|review| review.trim().to_lowercase()),
            review_projects: self.project_ids(&proposed.review_projects)?,
            reply: proposed.reply,
            reply_rooms,
            deliver: proposed.deliver,
            deliver_projects: self.project_ids(&proposed.deliver_projects)?,
            fix_in_task: proposed.fix_in_task,
            triggers,
        };
        let file = self
            .db_path
            .parent()
            .and_then(|dir| std::fs::read_to_string(dir.join(TRIGGERS_FILE)).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::Value::Null);
        let change = grants_change(
            &agent.id,
            agent.preset.as_deref() == Some(REVIEWER_PRESET),
            wanted,
            &file,
        )
        .map_err(invalid)?;
        self.session_repo.add_agent_change(
            &guide.id,
            &session_id.0,
            &change,
            &clean_reason(&proposed.reason),
        )
    }
}
