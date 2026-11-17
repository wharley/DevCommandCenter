//! Changes an agent proposes with `dcc_propose_change`
//! (`dcc_core::domain::agent_change`): the agent, the source and the room
//! come by the names the agent reads in `dcc_team_state` and are kept by
//! id, with what each was when proposed; "Aplicar" writes it and
//! "Descartar" answers it.

use dcc_core::domain::agent::{ResidentAgent, ReviewerPresetText};
use dcc_core::domain::agent_change::{clean_reason, AgentChange, Change};
use dcc_core::domain::agent_source::{AgentSource, EVERY_CHOICES};

use super::*;

/// What the agent writes in the tool's call.
#[derive(Clone, Debug, Default)]
pub struct ProposedChange {
    /// The agent the change is about, by name.
    pub agent: String,
    /// One of its sources, by what it watches; none: its projects change.
    pub source: Option<String>,
    /// The source's new room: a project's name, or "Geral".
    pub room: Option<String>,
    pub every_minutes: Option<u32>,
    pub on: Option<bool>,
    /// The agent's new projects by name; "all" or none: every project.
    pub projects: Option<Vec<String>>,
    pub reason: String,
}

fn invalid(message: String) -> dcc_core::CoreError {
    dcc_core::CoreError::InvalidInput(message)
}

/// Geral, as an agent may write it.
fn is_general(room: &str) -> bool {
    ["geral", "general", ""].contains(&room.trim().to_lowercase().as_str())
}

/// The source of `sources` the agent means by `watch`: its exact words, or
/// the only one that has them.
fn pick_source<'a>(
    sources: &'a [AgentSource],
    watch: &str,
) -> std::result::Result<&'a AgentSource, String> {
    let wanted = watch.trim().to_lowercase();
    let exact: Vec<&AgentSource> = sources
        .iter()
        .filter(|source| source.watch.trim().to_lowercase() == wanted)
        .collect();
    let found = if exact.is_empty() {
        sources
            .iter()
            .filter(|source| source.watch.to_lowercase().contains(&wanted))
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [one] => Ok(one),
        _ => Err(format!(
            "which source? this agent's sources watch: {}",
            sources
                .iter()
                .map(|source| format!("\"{}\"", source.watch))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

impl SessionCommandState {
    fn agent_named(&self, name: &str) -> Result<ResidentAgent> {
        let agents = self
            .session_repo
            .list_stored_resident_agents(&ReviewerPresetText::default())?;
        let wanted = name.trim().trim_start_matches('@');
        agents
            .iter()
            .find(|agent| agent.name.trim().eq_ignore_ascii_case(wanted))
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "no agent named \"{wanted}\"; the team is: {}",
                    agents
                        .iter()
                        .map(|agent| agent.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    }

    fn project_named(&self, name: &str) -> Result<String> {
        crate::agent_task::find_project(&self.db_path, name, false, None)
            .map(|target| target.project_id)
            .map_err(invalid)
    }

    /// The change as it is kept: ids, with what each field is now.
    fn resolve_change(&self, proposed: &ProposedChange) -> Result<Change> {
        let agent = self.agent_named(&proposed.agent)?;
        let Some(watch) = proposed.source.as_deref() else {
            let Some(names) = &proposed.projects else {
                return Err(invalid(
                    "say what changes: a source of the agent (source) or its projects".into(),
                ));
            };
            let all = names.is_empty()
                || names
                    .iter()
                    .all(|name| ["all", "todos"].contains(&name.trim().to_lowercase().as_str()));
            let mut after = Vec::new();
            if !all {
                for name in names {
                    let id = self.project_named(name)?;
                    if !after.contains(&id) {
                        after.push(id);
                    }
                }
            }
            return Ok(Change::Projects {
                agent_id: agent.id,
                before: agent.projects,
                after,
            });
        };
        let sources: Vec<AgentSource> = self
            .session_repo
            .list_agent_sources()?
            .into_iter()
            .filter(|source| source.agent_id == agent.id && !source.proposed)
            .collect();
        if sources.is_empty() {
            return Err(invalid(format!("{} watches no source", agent.name)));
        }
        let source = pick_source(&sources, watch).map_err(invalid)?;
        let room = match proposed.room.as_deref() {
            None => None,
            Some(room) if is_general(room) => Some(None),
            Some(room) => Some(Some(self.project_named(room)?)),
        };
        if let Some(every) = proposed.every_minutes {
            if !EVERY_CHOICES.contains(&every) {
                return Err(invalid(format!(
                    "a source reads every {} minutes",
                    EVERY_CHOICES.map(|choice| choice.to_string()).join(", ")
                )));
            }
        }
        let before_room = source.room.clone().filter(|room| !room.is_empty());
        Ok(Change::Source {
            source_id: source.id.clone(),
            agent_id: agent.id,
            connector: source.connector.clone(),
            watch: source.watch.clone(),
            room: room.map(|after| (before_room, after)),
            every_minutes: proposed
                .every_minutes
                .map(|after| (source.every_minutes, after)),
            enabled: proposed.on.map(|after| (source.enabled, after)),
        })
    }

    /// The change the agent of `session_id` proposes; its name comes back.
    pub fn propose_change_for_session(
        &self,
        session_id: &SessionId,
        proposed: ProposedChange,
    ) -> Result<(String, AgentChange)> {
        let agent = self
            .session_repo
            .load_session_agent(session_id)?
            .ok_or_else(|| invalid("this session runs as no DCC agent".into()))?;
        let change = self.resolve_change(&proposed)?;
        if !change.changes_something() {
            return Err(invalid(
                "it is already set like that; nothing to change".into(),
            ));
        }
        let kept = self.session_repo.add_agent_change(
            &agent.id,
            &session_id.0,
            &change,
            &clean_reason(&proposed.reason),
        )?;
        Ok((agent.name, kept))
    }

    pub fn waiting_agent_changes(&self) -> Result<Vec<AgentChange>> {
        self.session_repo.waiting_agent_changes()
    }

    /// "Aplicar": written and answered; none if it was already answered. A
    /// source or an agent that is gone answers it and says so.
    pub fn accept_agent_change(&self, id: &str) -> Result<Option<AgentChange>> {
        let Some(waiting) = self
            .session_repo
            .waiting_agent_changes()?
            .into_iter()
            .find(|change| change.id == id)
        else {
            return Ok(None);
        };
        let written = self.session_repo.apply_agent_change(&waiting.change)?;
        self.session_repo.answer_agent_change(id)?;
        if !written {
            return Err(invalid(
                "what this change was about no longer exists".into(),
            ));
        }
        Ok(Some(waiting))
    }

    /// "Descartar".
    pub fn dismiss_agent_change(&self, id: &str) -> Result<bool> {
        Ok(self.session_repo.answer_agent_change(id)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(watch: &str) -> AgentSource {
        AgentSource {
            id: watch.to_string(),
            agent_id: "a".into(),
            connector_id: "c".into(),
            connector: "Slack".into(),
            watch: watch.to_string(),
            tool: "t".into(),
            arguments: "{}".into(),
            every_minutes: 15,
            enabled: true,
            proposed: false,
            last_checked_at: None,
            created_at: String::new(),
            room: None,
        }
    }

    #[test]
    fn a_source_is_picked_by_its_words_or_asked_again() {
        let sources = [
            source("links de PR no #tech"),
            source("links de PR nas minhas DMs"),
            source("Menções a mim"),
        ];
        assert_eq!(
            pick_source(&sources, "menções a mim").unwrap().id,
            "Menções a mim"
        );
        assert_eq!(
            pick_source(&sources, "#tech").unwrap().id,
            "links de PR no #tech"
        );
        // Two have these words: it is told which there are.
        let asked = pick_source(&sources, "links de PR").unwrap_err();
        assert!(asked.contains("\"Menções a mim\""));
        assert!(pick_source(&sources, "jira").is_err());
        assert!(is_general("Geral") && is_general(" general ") && !is_general("Checkout"));
    }
}
