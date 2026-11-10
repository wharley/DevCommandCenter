//! Which open MCP call a Codex approval request is for, when one server has
//! several at once (an agent searching Slack two ways in parallel). Codex's
//! request names the tool in its message (`… to run tool "<name>"`) and
//! carries the call's arguments as `_meta.tool_params`; a call that matches
//! both is the one. Calls left that are the same tool with the same
//! arguments are interchangeable, so any of them is it; anything else stays
//! ambiguous and is declined, as before.

use serde_json::Value;

/// The tool named in Codex's approval message, if it says one.
fn tool_in_message(params: &Value) -> Option<&str> {
    let message = params.get("message")?.as_str()?;
    let (_, rest) = message.split_once("run tool \"")?;
    let (tool, _) = rest.split_once('"')?;
    (!tool.is_empty()).then_some(tool)
}

/// Picks among `calls` (tool name, arguments) the one `params` asks about.
pub(crate) fn pick_call(calls: &[(&str, &Value)], params: &Value) -> Option<usize> {
    if calls.len() <= 1 {
        return (calls.len() == 1).then_some(0);
    }
    let tool = tool_in_message(params);
    let arguments = params.pointer("/_meta/tool_params");
    let matching: Vec<usize> = calls
        .iter()
        .enumerate()
        .filter(|(_, (name, args))| {
            tool.is_none_or(|tool| tool == *name) && arguments.is_none_or(|wanted| wanted == *args)
        })
        .map(|(index, _)| index)
        .collect();
    let first = *matching.first()?;
    matching
        .iter()
        .all(|index| calls[*index] == calls[first])
        .then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn two_slack_searches_at_once_are_told_apart() {
        let users = json!({ "keywords": ["Giovanni"] });
        let messages = json!({ "keywords": ["domínio"], "channel_types": "im" });
        let calls = [
            ("slack_search_users", &users),
            ("slack_search_public_and_private", &messages),
        ];
        let by_message = json!({
            "message": "Allow the slack MCP server to run tool \"slack_search_public_and_private\"?"
        });
        assert_eq!(pick_call(&calls, &by_message), Some(1));
        let by_arguments = json!({ "_meta": { "tool_params": { "keywords": ["Giovanni"] } } });
        assert_eq!(pick_call(&calls, &by_arguments), Some(0));
    }

    #[test]
    fn what_cannot_be_told_apart_stays_ambiguous() {
        let one = json!({ "q": 1 });
        let two = json!({ "q": 2 });
        let nothing = json!({ "message": "provider-controlled text" });
        assert_eq!(pick_call(&[("a", &one), ("b", &two)], &nothing), None);
        assert_eq!(pick_call(&[("a", &one), ("a", &two)], &json!({})), None);
        // The same call twice: either is it.
        assert_eq!(pick_call(&[("a", &one), ("a", &one)], &json!({})), Some(0));
        assert_eq!(pick_call(&[("a", &one)], &json!({})), Some(0));
        assert_eq!(pick_call(&[], &json!({})), None);
    }
}
