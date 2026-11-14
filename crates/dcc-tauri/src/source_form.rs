//! What the "Nova fonte" form asks of a connector, with no model: its
//! read-only tools as the vendor describes them now (name, description and
//! the arguments' schema, which the stored connection check does not keep),
//! and a test read of the tool with the arguments typed, before anything is
//! stored. Both go through the gateway with the agent's own access, as a
//! source's reads do (`source_read`).

use dcc_core::domain::agent_source::items_of;
use dcc_core::domain::mcp::McpDefinitionId;
use serde_json::{json, Value};

use crate::connector_gateway::ConnectorGateway;
use crate::source_read::request_once;
use crate::state::{SessionCommandState, SourceInput};

/// Pages of `tools/list` asked at most.
const MAX_PAGES: usize = 5;
/// Items of a test read shown to the person.
const SHOWN: usize = 5;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceTest {
    /// How many items the read brought.
    pub count: usize,
    /// The first ones, as the agent would get them.
    pub items: Vec<String>,
}

/// The read-only tools among one `tools/list` page.
fn read_only(result: &Value) -> Vec<FormTool> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|tool| tool.pointer("/annotations/readOnlyHint") == Some(&Value::Bool(true)))
        .filter_map(|tool| {
            Some(FormTool {
                name: tool.get("name")?.as_str()?.to_string(),
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                input_schema: tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            })
        })
        .collect()
}

/// The connector's read-only tools, asked of it now.
pub async fn tools(
    state: &SessionCommandState,
    gateway: &ConnectorGateway,
    agent_id: &str,
    connector_id: &str,
) -> Result<Vec<FormTool>, String> {
    let allowed = state
        .source_form_connectors(agent_id)
        .map_err(|error| error.to_string())?;
    if !allowed.iter().any(|(id, _)| id == connector_id) {
        return Err("this agent may not read that connector".to_string());
    }
    let agent = state
        .list_stored_agents(&Default::default())
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| "agent not found".to_string())?;
    let access = agent.tool_access.connector(connector_id);
    let connector = McpDefinitionId(connector_id.to_string());
    let mut found = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let params = match &cursor {
            Some(cursor) => json!({ "cursor": cursor }),
            None => json!({}),
        };
        let result = request_once(gateway, &connector, "tools/list", params, access).await?;
        found.extend(read_only(&result));
        cursor = result
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    Ok(found)
}

/// Reads the tool once with the arguments typed; nothing is stored.
pub async fn test(
    state: &SessionCommandState,
    gateway: &ConnectorGateway,
    agent_id: &str,
    input: SourceInput,
) -> Result<SourceTest, String> {
    let (agent, source) = state
        .checked_source(agent_id, input, false)
        .await
        .map_err(|error| error.to_string())?;
    let arguments: Value =
        serde_json::from_str(&source.arguments).map_err(|error| error.to_string())?;
    let result = request_once(
        gateway,
        &McpDefinitionId(source.connector_id.clone()),
        "tools/call",
        json!({ "name": source.tool, "arguments": arguments }),
        agent.tool_access.connector(&source.connector_id),
    )
    .await?;
    let items = items_of(&result);
    Ok(SourceTest {
        count: items.len(),
        items: items
            .into_iter()
            .take(SHOWN)
            .map(|item| item.text)
            .collect(),
    })
}

/// The fill button's answer: the tool and arguments the agent picked.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSuggestion {
    pub tool: String,
    pub arguments: Value,
    pub note: String,
    /// The connector's read-only tools, as asked for the pick.
    pub tools: Vec<FormTool>,
}

/// Asks the agent which tool and arguments return what the person wrote
/// in `watch` (`agent_source_suggest`): one read-only run on the agent's
/// own provider and model, with no conversation; nothing is stored.
pub async fn suggest(
    state: &SessionCommandState,
    gateway: &ConnectorGateway,
    agent_id: &str,
    connector_id: &str,
    watch: &str,
) -> Result<SourceSuggestion, String> {
    use dcc_core::domain::agent_source_suggest::{parse_suggestion, suggest_prompt, SuggestTool};
    let watch = watch.trim();
    if watch.is_empty() || watch.chars().count() > 120 {
        return Err("say what to watch in 1 to 120 characters".to_string());
    }
    let connector = state
        .source_form_connectors(agent_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|(id, _)| id == connector_id)
        .map(|(_, name)| name)
        .ok_or_else(|| "this agent may not read that connector".to_string())?;
    let agent = state
        .list_stored_agents(&Default::default())
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| "agent not found".to_string())?;
    let provider = agent
        .provider_id
        .clone()
        .filter(|provider| !provider.trim().is_empty())
        .ok_or_else(|| "this agent has no provider to ask".to_string())?;
    let found = tools(state, gateway, agent_id, connector_id).await?;
    if found.is_empty() {
        return Err(format!("{connector} offers no read-only tool"));
    }
    let listed: Vec<SuggestTool<'_>> = found
        .iter()
        .map(|tool| SuggestTool {
            name: &tool.name,
            description: &tool.description,
            input_schema: &tool.input_schema,
        })
        .collect();
    let prompt = suggest_prompt(&connector, watch, &listed);
    // The run reads nothing: an empty folder of its own is where it runs.
    let folder = std::env::temp_dir().join("dcc-source-suggest");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let answer = state
        .run_ephemeral_read_only_turn_until(
            folder.to_string_lossy().into_owned(),
            provider,
            agent.model.clone(),
            None,
            agent.effort.clone(),
            prompt,
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    let names: Vec<&str> = found.iter().map(|tool| tool.name.as_str()).collect();
    let pick = parse_suggestion(&answer, &names)?;
    Ok(SourceSuggestion {
        tool: pick.tool,
        arguments: pick.arguments,
        note: pick.note,
        tools: found,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_tools_marked_read_only_are_offered() {
        let page = json!({"tools": [
            {"name": "search", "description": "Finds messages", "inputSchema": {"type": "object"},
             "annotations": {"readOnlyHint": true}},
            {"name": "send", "annotations": {"readOnlyHint": false}},
            {"name": "unmarked"},
            {"name": "list", "annotations": {"readOnlyHint": true}}
        ]});
        let tools = read_only(&page);
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
        assert_eq!(names, ["search", "list"]);
        assert_eq!(tools[0].description, "Finds messages");
        assert_eq!(tools[1].input_schema, json!({}));
        assert!(read_only(&json!({})).is_empty());
    }
}
