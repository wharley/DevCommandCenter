//! `dcc_propose_source`: a resident agent proposes something to watch for
//! the person (`dcc_core::domain::agent_source`) — a connector, in the
//! person's words what to watch, the connector's read-only tool and its
//! arguments. It waits as a card the person creates; then DCC reads it
//! every few minutes with no model and wakes the agent only with news. A
//! session that runs as no agent gets an error.

use dcc_core::domain::agent_source::{EVERY_CHOICES, MAX_ARGUMENTS_CHARS, PROPOSE_SOURCE_TOOL};
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::{ProposedSource, SessionCommandState};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SourceArgs {
    connector: String,
    watch: String,
    tool: String,
    #[serde(default)]
    arguments: Option<Value>,
    #[serde(default)]
    every_minutes: Option<u32>,
}

impl SourceArgs {
    fn checked(self) -> Option<ProposedSource> {
        let arguments = self.arguments.unwrap_or_else(|| json!({}));
        let fits = !self.connector.trim().is_empty()
            && !self.watch.trim().is_empty()
            && !self.tool.trim().is_empty()
            && arguments.is_object()
            && arguments.to_string().chars().count() <= MAX_ARGUMENTS_CHARS;
        fits.then_some(ProposedSource {
            connector: self.connector,
            watch: self.watch,
            tool: self.tool,
            arguments,
            every_minutes: self.every_minutes,
        })
    }
}

pub fn is_source_tool(name: &str) -> bool {
    name == PROPOSE_SOURCE_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": PROPOSE_SOURCE_TOOL,
        "description": "Only when you run as a DCC resident agent and the person asks you to watch something in one of their connectors (\"my mentions in Slack\", \"tasks assigned to me in ClickUp\", \"new cards in the Trello list Suporte\"): propose a source. Pick the connector's read-only tool and the arguments that return just that — DCC calls it itself every few minutes with no model and wakes you only with what is new. It waits as a card the person creates. One call per thing to watch.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "connector": {"type": "string", "minLength": 1, "maxLength": 80, "description": "The connector's name as the person installed it, e.g. \"Slack\"."},
                "watch": {"type": "string", "minLength": 1, "maxLength": 120, "description": "What it watches, in the person's language, e.g. \"menções a mim\"."},
                "tool": {"type": "string", "minLength": 1, "maxLength": 128, "description": "One of the connector's read-only tools, by its own name (e.g. slack_search_public)."},
                "arguments": {"type": "object", "description": "The tool's arguments that return just what to watch."},
                "everyMinutes": {"type": "integer", "enum": EVERY_CHOICES, "default": 15}
            },
            "required": ["connector", "watch", "tool"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<SourceArgs>(arguments.clone())
            .ok()
            .and_then(SourceArgs::checked)
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
        .and_then(|arguments| serde_json::from_value::<SourceArgs>(arguments).ok())
        .and_then(SourceArgs::checked)
    else {
        return text(
            "Pass connector, watch and tool; arguments (an object) and everyMinutes are optional."
                .to_string(),
            true,
        );
    };
    match sessions
        .propose_source_for_session(&SessionId(session_id.to_string()), proposed)
        .await
    {
        Ok((_, source)) => text(
            format!(
                "Proposed to the person: watching \"{}\" in {} every {} minutes. It waits as a card they create; tell them it is there and stop.",
                source.watch, source.connector, source.every_minutes
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
    fn a_source_needs_a_connector_what_to_watch_and_a_tool() {
        let good = Some(
            json!({"connector": "Slack", "watch": "menções a mim", "tool": "slack_search_public", "arguments": {"query": "@me"}}),
        );
        assert!(call_is_well_formed(&good));
        for bad in [
            json!({"connector": "Slack", "watch": "x"}),
            json!({"connector": " ", "watch": "x", "tool": "t"}),
            json!({"connector": "Slack", "watch": "x", "tool": "t", "arguments": [1]}),
            json!({"connector": "Slack", "watch": "x", "tool": "t", "extra": 1}),
        ] {
            assert!(!call_is_well_formed(&Some(bad.clone())), "{bad}");
        }
        assert_eq!(definition()["name"], PROPOSE_SOURCE_TOOL);
    }
}
