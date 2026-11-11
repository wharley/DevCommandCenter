//! `dcc_propose_reply`: a resident agent proposes the reply it would send
//! for the person (`dcc_core::domain::agent_reply`) — the text, where it
//! goes, and the connector's tool that sends it with its arguments. It
//! waits as a card in the room; only the person's "Enviar" sends it, DCC
//! calling that tool itself. A session that runs as no agent gets an error.

use dcc_core::domain::agent_reply::{MAX_DESTINATION_CHARS, MAX_REPLY_CHARS, PROPOSE_REPLY_TOOL};
use dcc_core::domain::agent_source::MAX_ARGUMENTS_CHARS;
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::{ProposedReply, SessionCommandState};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReplyArgs {
    connector: String,
    destination: String,
    tool: String,
    arguments: Value,
    text_argument: String,
    text: String,
}

impl ReplyArgs {
    fn checked(self) -> Option<ProposedReply> {
        let fits = !self.connector.trim().is_empty()
            && !self.destination.trim().is_empty()
            && self.destination.chars().count() <= MAX_DESTINATION_CHARS
            && !self.tool.trim().is_empty()
            && !self.text_argument.trim().is_empty()
            && !self.text.trim().is_empty()
            && self.text.chars().count() <= MAX_REPLY_CHARS
            && self.arguments.is_object()
            && self.arguments.to_string().chars().count() <= MAX_ARGUMENTS_CHARS;
        fits.then_some(ProposedReply {
            connector: self.connector,
            destination: self.destination,
            tool: self.tool,
            arguments: self.arguments,
            text_argument: self.text_argument,
            text: self.text,
        })
    }
}

pub fn is_reply_tool(name: &str) -> bool {
    name == PROPOSE_REPLY_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": PROPOSE_REPLY_TOOL,
        "description": "Only when you run as a DCC resident agent: when someone asked the person something (a question in a Slack thread a source brought, a comment on a task) and you found the answer, propose the reply the person would send, in their voice and language, instead of only describing it. Pick the connector's tool that sends (e.g. slack_send_message) and its arguments that put it in the right place (the channel and the thread), naming in textArgument the argument that holds the message. It waits as a card in the room; nothing is sent until the person presses Enviar, maybe after editing it, and DCC then calls that tool itself. Never send the reply yourself with the connector's tool. One call per reply.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "connector": {"type": "string", "minLength": 1, "maxLength": 80, "description": "The connector's name as the person installed it, e.g. \"Slack\"."},
                "destination": {"type": "string", "minLength": 1, "maxLength": MAX_DESTINATION_CHARS, "description": "Where it goes, in the person's language, e.g. \"#suporte · thread do Guilherme\"."},
                "tool": {"type": "string", "minLength": 1, "maxLength": 128, "description": "The connector's tool that sends it, by its own name."},
                "arguments": {"type": "object", "description": "The tool's arguments besides the message (channel, thread...)."},
                "textArgument": {"type": "string", "minLength": 1, "maxLength": 64, "description": "The argument of the tool that holds the message, e.g. \"message\" or \"text\"."},
                "text": {"type": "string", "minLength": 1, "maxLength": MAX_REPLY_CHARS, "description": "The reply, ready to send as the person."}
            },
            "required": ["connector", "destination", "tool", "arguments", "textArgument", "text"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<ReplyArgs>(arguments.clone())
            .ok()
            .and_then(ReplyArgs::checked)
            .is_some()
    })
}

fn text(message: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": is_error})
}

pub async fn dispatch(
    sessions: &SessionCommandState,
    session_id: &str,
    arguments: Option<Value>,
) -> Value {
    let Some(proposed) = arguments
        .and_then(|arguments| serde_json::from_value::<ReplyArgs>(arguments).ok())
        .and_then(ReplyArgs::checked)
    else {
        return text(
            "Pass connector, destination, tool, arguments (an object), textArgument and text."
                .to_string(),
            true,
        );
    };
    match sessions
        .propose_reply_for_session(&SessionId(session_id.to_string()), proposed)
        .await
    {
        Ok(reply) => text(
            format!(
                "Proposed to the person: a reply to {} in {}. It waits as a card they send; say in one line that it is ready and stop. Do not send it yourself.",
                reply.destination, reply.connector
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
    fn a_reply_needs_where_how_and_what() {
        let good = Some(json!({
            "connector": "Slack",
            "destination": "#suporte · thread do Guilherme",
            "tool": "slack_send_message",
            "arguments": {"channel_id": "C1", "thread_ts": "1.2"},
            "textArgument": "message",
            "text": "Foi na loja 374."
        }));
        assert!(call_is_well_formed(&good));
        let mut no_text = good.clone().unwrap();
        no_text["text"] = json!("  ");
        let mut array = good.clone().unwrap();
        array["arguments"] = json!([1]);
        let mut extra = good.clone().unwrap();
        extra["send"] = json!(true);
        for bad in [no_text, array, extra, json!({"connector": "Slack"})] {
            assert!(!call_is_well_formed(&Some(bad)));
        }
        assert_eq!(definition()["name"], PROPOSE_REPLY_TOOL);
    }
}
