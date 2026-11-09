//! What a resident agent may use of a connector (`agent_access`), applied
//! at DCC's gateway: with no access the agent sees no tool and every call
//! is refused; with read access it sees and calls only the tools the
//! vendor marks read-only (the hint DCC keeps from the last connection
//! check). Plain functions over the JSON-RPC bodies, so they are tested
//! without a vendor.

use dcc_core::domain::agent_access::{AccessLevel, ACCESS_DENIED};
use serde_json::{json, Value};

fn is_read_only(tool: &Value) -> bool {
    tool.pointer("/annotations/readOnlyHint")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn method(message: &Value) -> Option<&str> {
    message.get("method").and_then(Value::as_str)
}

/// Why `body` may not go to the vendor, if it may not: a tool call the
/// access does not allow. `read_only` says whether a tool is read-only.
pub fn refusal(
    access: AccessLevel,
    body: &Value,
    read_only: impl Fn(&str) -> bool,
) -> Option<&'static str> {
    if access == AccessLevel::Write {
        return None;
    }
    let messages: Vec<&Value> = match body {
        Value::Array(items) => items.iter().collect(),
        message => vec![message],
    };
    let refused = messages.iter().any(|message| {
        method(message) == Some("tools/call")
            && (access == AccessLevel::None
                || !message
                    .pointer("/params/name")
                    .and_then(Value::as_str)
                    .is_some_and(&read_only))
    });
    refused.then_some(ACCESS_DENIED)
}

/// The answer to a `tools/list` without asking the vendor: none when the
/// agent has no access to the connector.
pub fn empty_list(access: AccessLevel, body: &Value) -> Option<Value> {
    (access == AccessLevel::None && method(body) == Some("tools/list")).then(|| {
        json!({"jsonrpc": "2.0", "id": body.get("id").cloned().unwrap_or(Value::Null), "result": {"tools": []}})
    })
}

/// A vendor's `tools/list` answer cut to what the access allows.
pub fn filter_list(access: AccessLevel, request: &Value, mut answer: Value) -> Value {
    if access != AccessLevel::Read || method(request) != Some("tools/list") {
        return answer;
    }
    if let Some(tools) = answer
        .pointer_mut("/result/tools")
        .and_then(Value::as_array_mut)
    {
        tools.retain(is_read_only);
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str) -> Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name}})
    }

    #[test]
    fn read_access_keeps_only_read_only_tools_and_none_keeps_nothing() {
        let read_only = |name: &str| name == "list_channels";
        assert_eq!(refusal(AccessLevel::Write, &call("post"), read_only), None);
        assert_eq!(
            refusal(AccessLevel::Read, &call("list_channels"), read_only),
            None
        );
        assert_eq!(
            refusal(AccessLevel::Read, &call("post"), read_only),
            Some(ACCESS_DENIED)
        );
        assert_eq!(
            refusal(AccessLevel::None, &call("list_channels"), read_only),
            Some(ACCESS_DENIED)
        );
        let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        assert!(refusal(AccessLevel::None, &list, read_only).is_none());
        assert_eq!(
            empty_list(AccessLevel::None, &list).unwrap()["result"]["tools"],
            json!([])
        );
        assert!(empty_list(AccessLevel::Read, &list).is_none());
        let answer = json!({"result": {"tools": [
            {"name": "list_channels", "annotations": {"readOnlyHint": true}},
            {"name": "post"}
        ]}});
        let filtered = filter_list(AccessLevel::Read, &list, answer.clone());
        assert_eq!(filtered["result"]["tools"].as_array().unwrap().len(), 1);
        assert_eq!(
            filter_list(AccessLevel::Write, &list, answer.clone()),
            answer
        );
    }
}
