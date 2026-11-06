//! `dcc_agent_remember`: a resident agent saves a rule or preference the
//! person taught it to its memory (`dcc_core::domain::agent_memory`). The
//! note goes with the agent's next turns, in any project, and shows on the
//! agent's page, where the person edits or deletes it. A session that runs
//! as no agent gets an error.

use dcc_core::domain::agent_memory::{MAX_NOTE_CHARS, REMEMBER_TOOL};
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RememberArgs {
    note: String,
}

pub fn is_agent_memory_tool(name: &str) -> bool {
    name == REMEMBER_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": REMEMBER_TOOL,
        "description": "Only when you run as a DCC resident agent (your instructions carry a dcc_agent_memory block): save to your memory a rule or preference the person stated or corrected you on, meant to hold in later sessions. One short sentence in the person's language. It goes with your next turns and the person can see, edit or delete it. Never save task details, code, file contents or secrets.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "note": {"type": "string", "minLength": 1, "maxLength": MAX_NOTE_CHARS}
            },
            "required": ["note"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<RememberArgs>(arguments.clone()).is_ok_and(|args| {
            !args.note.trim().is_empty() && args.note.chars().count() <= MAX_NOTE_CHARS
        })
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
    let Some(args) =
        arguments.and_then(|arguments| serde_json::from_value::<RememberArgs>(arguments).ok())
    else {
        return text("Pass the note to save.".to_string(), true);
    };
    match sessions.remember_for_session(&SessionId(session_id.to_string()), &args.note) {
        Ok((agent, note)) => text(
            format!(
                "Saved to {agent}'s memory: \"{}\". The person sees it on your page and can edit or delete it.",
                note.body
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
    fn a_note_is_one_bounded_string() {
        assert!(call_is_well_formed(&Some(
            json!({"note": "Responder em pt-BR"})
        )));
        assert!(!call_is_well_formed(&Some(json!({"note": "  "}))));
        assert!(!call_is_well_formed(&Some(
            json!({"note": "x".repeat(MAX_NOTE_CHARS + 1)})
        )));
        assert!(!call_is_well_formed(&Some(
            json!({"note": "a", "extra": 1})
        )));
        assert!(!call_is_well_formed(&None));
        assert_eq!(definition()["name"], REMEMBER_TOOL);
    }
}
