//! A reply an agent proposes for the person (the "agent drafts, the person
//! sends" of support copilots): what it found to answer someone — a
//! question in a Slack thread a source brought, a comment on a task — as
//! the text, where it goes, and the connector's tool that sends it with its
//! arguments, chosen by the agent. It waits as a card in the room; nothing
//! leaves until the person presses "Enviar", and then DCC calls that tool
//! itself, with no model, with the text as the person left it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROPOSE_REPLY_TOOL: &str = "dcc_propose_reply";
/// The access group the person switches it with (`agent_access`).
pub const REPLY_GROUP: &str = "replies";
pub const MAX_REPLY_CHARS: usize = 4_000;
pub const MAX_DESTINATION_CHARS: usize = 120;
/// Replies waiting at once per agent; an older one is set aside.
pub const MAX_PENDING_PER_AGENT: usize = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplyState {
    #[default]
    Pending,
    Sent,
    Discarded,
}

impl ReplyState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Discarded => "discarded",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "sent" => Self::Sent,
            "discarded" => Self::Discarded,
            _ => Self::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReply {
    pub id: String,
    pub agent_id: String,
    /// The agent's conversation it was proposed in.
    pub session_id: String,
    /// The connector's definition id and its name as the person sees it.
    pub connector_id: String,
    pub connector: String,
    /// The connector's tool that sends it and its other arguments (a JSON
    /// object), the text going in `text_argument`.
    pub tool: String,
    pub arguments: String,
    pub text_argument: String,
    /// Where it goes, in the person's words: "#suporte · thread do Guilherme".
    pub destination: String,
    pub text: String,
    pub state: ReplyState,
    /// Why the last "Enviar" failed; it stays to be sent again.
    pub error: Option<String>,
    pub created_at: String,
    pub sent_at: Option<String>,
    /// The room it shows in: its project's, none for Geral.
    #[serde(default)]
    pub room: Option<String>,
}

/// The tool's arguments with the person's text in place.
pub fn arguments_with_text(arguments: &str, key: &str, text: &str) -> Result<Value, String> {
    let mut value: Value =
        serde_json::from_str(arguments).map_err(|error| format!("arguments: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "arguments must be a JSON object".to_string())?;
    object.insert(key.to_string(), Value::String(text.to_string()));
    Ok(value)
}

/// The text the person may send: trimmed, not empty, within the limit.
pub fn sendable(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty() && text.chars().count() <= MAX_REPLY_CHARS).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_text_goes_in_its_argument_and_the_rest_stays() {
        let sent = arguments_with_text(
            r#"{"channel_id":"C1","thread_ts":"1.2","message":"old"}"#,
            "message",
            "Foi na loja 374.",
        )
        .unwrap();
        assert_eq!(
            sent,
            json!({"channel_id": "C1", "thread_ts": "1.2", "message": "Foi na loja 374."})
        );
        assert!(arguments_with_text("[1]", "message", "x").is_err());
    }

    #[test]
    fn only_a_real_text_is_sendable() {
        assert_eq!(sendable("  oi \n"), Some("oi"));
        assert_eq!(sendable("   "), None);
        assert_eq!(sendable(&"a".repeat(MAX_REPLY_CHARS + 1)), None);
        assert_eq!(ReplyState::parse("sent"), ReplyState::Sent);
        assert_eq!(ReplyState::parse("?"), ReplyState::Pending);
    }
}
