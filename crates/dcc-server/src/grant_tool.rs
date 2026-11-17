//! `dcc_propose_grant`: the Guide proposes what an agent does without
//! asking — sending its reviews, its replies, delivering its tasks, fixing
//! review comments in the task, the triggers that wake it — as a card the
//! person applies (`dcc_core::domain::agent_grant_proposal`). Only the
//! Guide, never for itself; the server stores the proposal and turns
//! nothing on: the app writes it on the person's own click, one card at a
//! time. Any other agent gets an error that says where the person does it.

use std::collections::BTreeMap;

use dcc_core::domain::agent_change::{MAX_CHANGE_REASON_CHARS, PROPOSE_GRANT_TOOL, REVIEW_SENDS};
use dcc_core::domain::agent_trigger::Trigger;
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::{ProposedGrants, SessionCommandState};
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_NAME_CHARS: usize = 160;
const MAX_PLACES: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct GrantArgs {
    agent: String,
    #[serde(default)]
    review: Option<String>,
    #[serde(default)]
    review_projects: Option<Vec<String>>,
    #[serde(default)]
    reply: Option<bool>,
    #[serde(default)]
    reply_rooms: Option<Vec<String>>,
    #[serde(default)]
    deliver: Option<bool>,
    #[serde(default)]
    deliver_projects: Option<Vec<String>>,
    #[serde(default)]
    fix_in_task: Option<bool>,
    #[serde(default)]
    triggers: BTreeMap<String, bool>,
    #[serde(default)]
    reason: String,
}

fn trigger_ids() -> Vec<&'static str> {
    Trigger::ALL.map(Trigger::id).to_vec()
}

impl GrantArgs {
    fn checked(self) -> Option<ProposedGrants> {
        let short = |text: &str| {
            let count = text.trim().chars().count();
            count > 0 && count <= MAX_NAME_CHARS
        };
        let places = |list: &Option<Vec<String>>| {
            list.as_ref().is_none_or(|list| {
                list.len() <= MAX_PLACES && list.iter().all(|name| name.len() <= MAX_NAME_CHARS)
            })
        };
        let something = self.review.is_some()
            || self.review_projects.is_some()
            || self.reply.is_some()
            || self.reply_rooms.is_some()
            || self.deliver.is_some()
            || self.deliver_projects.is_some()
            || self.fix_in_task.is_some()
            || !self.triggers.is_empty();
        let ids = trigger_ids();
        let fits = short(&self.agent)
            && something
            && self
                .review
                .as_deref()
                .is_none_or(|review| REVIEW_SENDS.contains(&review))
            && places(&self.review_projects)
            && places(&self.reply_rooms)
            && places(&self.deliver_projects)
            && self.triggers.keys().all(|id| ids.contains(&id.as_str()))
            && self.reason.chars().count() <= MAX_CHANGE_REASON_CHARS * 2;
        fits.then_some(ProposedGrants {
            agent: self.agent,
            review: self.review,
            review_projects: self.review_projects,
            reply: self.reply,
            reply_rooms: self.reply_rooms,
            deliver: self.deliver,
            deliver_projects: self.deliver_projects,
            fix_in_task: self.fix_in_task,
            triggers: self.triggers,
            reason: self.reason,
        })
    }
}

pub fn is_grant_tool(name: &str) -> bool {
    name == PROPOSE_GRANT_TOOL
}

pub fn definition() -> Value {
    let names = json!({"type": "array", "maxItems": MAX_PLACES, "items": {"type": "string", "maxLength": MAX_NAME_CHARS}});
    let switches: serde_json::Map<String, Value> = trigger_ids()
        .into_iter()
        .map(|id| (id.to_string(), json!({"type": "boolean"})))
        .collect();
    json!({
        "name": PROPOSE_GRANT_TOOL,
        "description": "Only the Guide, and never for itself: propose what another agent does WITHOUT asking the person, when the person asks for it. It waits as a card that shows each thing with its before and after and says the agent will act by itself; only the person's click on that card turns it on — you turn nothing on, and neither does any agent. Send only what changes. review (the Reviewer only): ask, comment (its comments go) or decide (it also approves or requests changes), with reviewProjects where it holds. reply: its proposed replies go by themselves, in replyRooms (project names, or \"Geral\"). deliver: a task it opened is committed, pushed and gets its pull request, in deliverProjects. fixInTask (the Reviewer only): review comments on the person's own pull request go to the task's agent; it needs the reviewReceived trigger. triggers: what wakes the agent (prUpdated and reviewRequested are the Reviewer's). Read dcc_team_state first; a grant with nowhere to hold is refused, so name the projects or rooms. The merge is never automatic and is not here. Say in reason what the agent will do once applied, in the person's language.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "agent": {"type": "string", "minLength": 1, "maxLength": MAX_NAME_CHARS, "description": "The agent's name, as dcc_team_state shows it. Not you."},
                "review": {"type": "string", "enum": REVIEW_SENDS},
                "reviewProjects": names.clone(),
                "reply": {"type": "boolean"},
                "replyRooms": names.clone(),
                "deliver": {"type": "boolean"},
                "deliverProjects": names,
                "fixInTask": {"type": "boolean"},
                "triggers": {"type": "object", "additionalProperties": false, "properties": switches},
                "reason": {"type": "string", "maxLength": MAX_CHANGE_REASON_CHARS, "description": "What the agent will do by itself once applied, in one sentence. It shows on the card."}
            },
            "required": ["agent"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<GrantArgs>(arguments.clone())
            .ok()
            .and_then(GrantArgs::checked)
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
        .and_then(|arguments| serde_json::from_value::<GrantArgs>(arguments).ok())
        .and_then(GrantArgs::checked)
    else {
        return text(
            "Pass agent and what changes: review, reply, deliver, fixInTask, their places, or triggers.".to_string(),
            true,
        );
    };
    match sessions.propose_grants_for_session(&SessionId(session_id.to_string()), proposed) {
        Ok(_) => text(
            "Proposed to the person as a card. Nothing is on: only their click on it turns it on. Tell them the card is there, what the agent will do by itself once they apply it, and stop.".to_string(),
            false,
        ),
        Err(error) => text(error.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_names_an_agent_and_something_that_changes() {
        for good in [
            json!({"agent": "Dev", "deliver": true, "deliverProjects": ["Checkout"], "reason": "entrega sozinho"}),
            json!({"agent": "Revisor", "review": "decide", "reviewProjects": ["Checkout"]}),
            json!({"agent": "Suporte", "reply": true, "replyRooms": ["Geral", "Checkout"]}),
            json!({"agent": "Revisor", "fixInTask": true, "triggers": {"reviewReceived": true}}),
        ] {
            assert!(call_is_well_formed(&Some(good.clone())), "{good}");
        }
        for bad in [
            json!({"agent": "Dev"}),
            json!({"agent": "Revisor", "review": "always"}),
            json!({"agent": "Dev", "triggers": {"routine": true}}),
            json!({"agent": "Dev", "merge": true}),
            json!({"agent": "Dev", "deliver": true, "autoStart": true}),
            json!({"agent": " ", "deliver": true}),
        ] {
            assert!(!call_is_well_formed(&Some(bad.clone())), "{bad}");
        }
        let tool = definition();
        assert_eq!(tool["name"], PROPOSE_GRANT_TOOL);
        // The switches are the six the person has, no more.
        assert_eq!(
            tool["inputSchema"]["properties"]["triggers"]["properties"]
                .as_object()
                .unwrap()
                .len(),
            Trigger::ALL.len()
        );
    }
}
