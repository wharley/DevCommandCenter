//! `dcc_propose_change`: a resident agent proposes a change to what exists
//! (`dcc_core::domain::agent_change`) — a source's room, interval or
//! switch, the projects an agent works in — instead of listing steps for
//! the person. It waits as a card with the before and the after; nothing
//! changes until the person applies it. A session that runs as no agent
//! gets an error.

use dcc_core::domain::agent_change::{Change, MAX_CHANGE_REASON_CHARS, PROPOSE_CHANGE_TOOL};
use dcc_core::domain::agent_source::EVERY_CHOICES;
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::{ProposedChange, SessionCommandState};
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_NAME_CHARS: usize = 160;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ChangeArgs {
    agent: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    room: Option<String>,
    #[serde(default)]
    every_minutes: Option<u32>,
    #[serde(default)]
    on: Option<bool>,
    #[serde(default)]
    projects: Option<Vec<String>>,
    #[serde(default)]
    reason: String,
}

impl ChangeArgs {
    fn checked(self) -> Option<ProposedChange> {
        let short = |text: &str| {
            let count = text.trim().chars().count();
            count > 0 && count <= MAX_NAME_CHARS
        };
        let of_source = self.room.is_some() || self.every_minutes.is_some() || self.on.is_some();
        // A source's change names the source and something of it; the
        // projects' change names only the projects.
        let shaped = match (&self.source, &self.projects) {
            (Some(source), None) => short(source) && of_source,
            (None, Some(projects)) => {
                !of_source && projects.len() <= 20 && projects.iter().all(|name| short(name))
            }
            _ => false,
        };
        let fits = short(&self.agent)
            && shaped
            && self
                .room
                .as_deref()
                .is_none_or(|room| room.len() <= MAX_NAME_CHARS)
            && self
                .every_minutes
                .is_none_or(|every| EVERY_CHOICES.contains(&every))
            && self.reason.chars().count() <= MAX_CHANGE_REASON_CHARS * 2;
        fits.then_some(ProposedChange {
            agent: self.agent,
            source: self.source,
            room: self.room,
            every_minutes: self.every_minutes,
            on: self.on,
            projects: self.projects,
            reason: self.reason,
        })
    }
}

pub fn is_change_tool(name: &str) -> bool {
    name == PROPOSE_CHANGE_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": PROPOSE_CHANGE_TOOL,
        "description": "Only when you run as a DCC resident agent: propose a change to how the team is set up, instead of listing steps for the person. Two kinds. A source of an agent (agent + source, by what it watches as dcc_team_state shows it): the room it brings its news to (room: a project's name, or \"Geral\"), how often it reads (everyMinutes), or turning it on or off (on) — any of them together. Or the projects an agent works in (agent + projects: project names, or [\"all\"]). It waits as a card with the before and the after that the person applies or discards; nothing changes before that. Read dcc_team_state first and propose only what differs from it. Several changes asked together go in the same reply, one call each. It does not reach what lets an agent act without asking (\"Enviar sem perguntar\", starting tasks by itself, who it may call, triggers): those are only the person's, on the agent's page.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "agent": {"type": "string", "minLength": 1, "maxLength": MAX_NAME_CHARS, "description": "The agent's name, as dcc_team_state shows it."},
                "source": {"type": "string", "minLength": 1, "maxLength": MAX_NAME_CHARS, "description": "What the source watches, as dcc_team_state shows it. Leave out to change the agent's projects."},
                "room": {"type": "string", "maxLength": MAX_NAME_CHARS, "description": "The source's new room: a project's name, or \"Geral\"."},
                "everyMinutes": {"type": "integer", "enum": EVERY_CHOICES},
                "on": {"type": "boolean", "description": "Whether the source reads."},
                "projects": {"type": "array", "maxItems": 20, "items": {"type": "string", "maxLength": MAX_NAME_CHARS}, "description": "The agent's projects by name; [\"all\"]: every project."},
                "reason": {"type": "string", "maxLength": MAX_CHANGE_REASON_CHARS, "description": "Why, in one sentence, in the person's language. It shows on the card."}
            },
            "required": ["agent"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<ChangeArgs>(arguments.clone())
            .ok()
            .and_then(ChangeArgs::checked)
            .is_some()
    })
}

fn text(message: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": is_error})
}

pub fn dispatch(
    sessions: &SessionCommandState,
    session_id: &str,
    arguments: Option<Value>,
) -> Value {
    let Some(proposed) = arguments
        .and_then(|arguments| serde_json::from_value::<ChangeArgs>(arguments).ok())
        .and_then(ChangeArgs::checked)
    else {
        return text(
            "Pass agent and either source with room, everyMinutes or on, or projects.".to_string(),
            true,
        );
    };
    match sessions.propose_change_for_session(&SessionId(session_id.to_string()), proposed) {
        Ok((_, kept)) => {
            let what = match &kept.change {
                Change::Source { watch, .. } => format!("the source \"{watch}\""),
                Change::Projects { .. } => "the agent's projects".to_string(),
            };
            text(
                format!("Proposed to the person: a change to {what}. It waits as a card they apply or discard; nothing changed yet. Tell them it is there and stop."),
                false,
            )
        }
        Err(error) => text(error.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_names_an_agent_and_one_thing_of_it() {
        for good in [
            json!({"agent": "Suporte", "source": "menções a mim", "room": "Checkout", "reason": "está na Geral"}),
            json!({"agent": "Suporte", "source": "menções a mim", "everyMinutes": 5, "on": true}),
            json!({"agent": "Dev", "projects": ["Checkout", "zapp"]}),
            json!({"agent": "Dev", "projects": ["all"]}),
        ] {
            assert!(call_is_well_formed(&Some(good.clone())), "{good}");
        }
        for bad in [
            json!({"agent": "Suporte"}),
            json!({"agent": "Suporte", "source": "menções"}),
            json!({"agent": "Suporte", "source": "menções", "everyMinutes": 7}),
            json!({"agent": "Dev", "projects": ["Checkout"], "room": "Checkout"}),
            json!({"agent": "Dev", "source": "x", "room": "Geral", "projects": []}),
            json!({"agent": "Dev", "projects": ["Checkout"], "autoStart": true}),
            json!({"agent": " ", "projects": ["Checkout"]}),
        ] {
            assert!(!call_is_well_formed(&Some(bad.clone())), "{bad}");
        }
        assert_eq!(definition()["name"], PROPOSE_CHANGE_TOOL);
    }
}
