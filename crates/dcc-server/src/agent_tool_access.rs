//! What a resident agent may use of DCC's own tools (`agent_access`),
//! applied where the bridge serves them: `tools/list` shows only what the
//! session's agent may use, and a call to anything else is refused with a
//! message the agent reads. A session with no agent, or an agent with full
//! access, sees everything as before.

use dcc_core::domain::agent_access::{ToolAccess, ACCESS_DENIED};
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;
use serde_json::{json, Value};

/// The session's agent's access, when it narrows anything.
pub fn session_access(sessions: &SessionCommandState, session_id: &str) -> Option<ToolAccess> {
    sessions
        .session_agent(&SessionId(session_id.to_string()))
        .ok()
        .flatten()
        .map(|agent| agent.tool_access)
        .filter(|access| !access.is_full())
}

fn read_only(tool: &Value) -> bool {
    tool.pointer("/annotations/readOnlyHint")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn allowed(access: &ToolAccess, tool: &Value) -> bool {
    tool.get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| access.allows_dcc(name, read_only(tool)))
}

/// The tools the agent sees.
pub fn visible(access: Option<&ToolAccess>, tools: Vec<Value>) -> Vec<Value> {
    match access {
        Some(access) => tools
            .into_iter()
            .filter(|tool| allowed(access, tool))
            .collect(),
        None => tools,
    }
}

/// The answer to a call the agent may not make, if it may not.
pub fn refusal(access: Option<&ToolAccess>, name: &str, tools: &[Value]) -> Option<Value> {
    let access = access?;
    let tool = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))?;
    (!allowed(access, tool))
        .then(|| json!({"content": [{"type": "text", "text": ACCESS_DENIED}], "isError": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent_access::AccessLevel;

    #[test]
    fn the_list_and_the_calls_follow_the_agents_access() {
        let tools = vec![
            json!({"name": "dcc_browser_screenshot", "annotations": {"readOnlyHint": true}}),
            json!({"name": "dcc_browser_click", "annotations": {"readOnlyHint": false}}),
            json!({"name": "dcc_computer_status", "annotations": {"readOnlyHint": true}}),
            json!({"name": "dcc_agent_remember"}),
        ];
        let mut access = ToolAccess::default();
        access.groups.insert("browser".into(), AccessLevel::Read);
        access.groups.insert("computer".into(), AccessLevel::None);
        let names: Vec<String> = visible(Some(&access), tools.clone())
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["dcc_browser_screenshot", "dcc_agent_remember"]);
        assert_eq!(visible(None, tools.clone()).len(), 4);
        assert!(refusal(Some(&access), "dcc_browser_click", &tools).is_some());
        assert!(refusal(Some(&access), "dcc_browser_screenshot", &tools).is_none());
        assert!(refusal(None, "dcc_browser_click", &tools).is_none());
    }
}
