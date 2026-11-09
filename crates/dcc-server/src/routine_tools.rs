//! `dcc_propose_routine`: a resident agent proposes a routine to the person
//! (`dcc_core::domain::agent_routine`) — days, a time and a request. It
//! waits as a card in the team room and on the agent's page and runs only
//! after the person creates it. A session that runs as no agent gets an
//! error.

use dcc_core::domain::agent_routine::{
    RoutineDraft, MAX_ROUTINE_PROMPT_CHARS, PROPOSE_ROUTINE_TOOL,
};
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeArgs {
    days: Vec<u8>,
    time: String,
    request: String,
}

impl ProposeArgs {
    fn draft(self) -> Option<RoutineDraft> {
        let (hour, minute) = self.time.trim().split_once(':')?;
        RoutineDraft {
            days: self.days,
            hour: hour.parse().ok()?,
            minute: minute.parse().ok()?,
            prompt: self.request,
        }
        .normalized()
        .ok()
    }
}

pub fn is_routine_tool(name: &str) -> bool {
    name == PROPOSE_ROUTINE_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": PROPOSE_ROUTINE_TOOL,
        "description": "Only when you run as a DCC resident agent and the person asked for something recurring (\"every Monday…\", \"each morning…\"): propose a routine — the days of the week, a local time and the request you will get then. It waits as a card the person creates or discards; nothing runs before that. At most one per turn; never propose one the person did not ask for.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "days": {"type": "array", "minItems": 1, "maxItems": 7, "items": {"type": "integer", "minimum": 0, "maximum": 6}, "description": "0 = Monday … 6 = Sunday."},
                "time": {"type": "string", "pattern": "^[0-2]?[0-9]:[0-5][0-9]$", "description": "Local time, HH:MM."},
                "request": {"type": "string", "minLength": 1, "maxLength": MAX_ROUTINE_PROMPT_CHARS, "description": "What you will be asked at that time, in the person's language."}
            },
            "required": ["days", "time", "request"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<ProposeArgs>(arguments.clone())
            .ok()
            .and_then(ProposeArgs::draft)
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
    let Some(draft) = arguments
        .and_then(|arguments| serde_json::from_value::<ProposeArgs>(arguments).ok())
        .and_then(ProposeArgs::draft)
    else {
        return text(
            "Pass the days (0 = Monday … 6 = Sunday), the time as HH:MM and the request."
                .to_string(),
            true,
        );
    };
    match sessions.propose_routine_for_session(&SessionId(session_id.to_string()), draft) {
        Ok((agent, routine)) => text(
            format!(
                "Proposed to the person: a routine of {agent} at {:02}:{:02}. It waits as a card they create or discard; tell them it is there and stop.",
                routine.hour, routine.minute
            ),
            false,
        ),
        Err(error) => text(error.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_needs_days_a_time_and_a_request() {
        let good = Some(json!({"days": [0], "time": "09:30", "request": "Resuma a semana"}));
        assert!(call_is_well_formed(&good));
        for bad in [
            json!({"days": [], "time": "09:30", "request": "x"}),
            json!({"days": [0], "time": "9h", "request": "x"}),
            json!({"days": [0], "time": "25:00", "request": "x"}),
            json!({"days": [0], "time": "09:00", "request": " "}),
            json!({"days": [0], "time": "09:00", "request": "x", "extra": 1}),
        ] {
            assert!(!call_is_well_formed(&Some(bad.clone())), "{bad}");
        }
        assert_eq!(definition()["name"], PROPOSE_ROUTINE_TOOL);
    }
}
