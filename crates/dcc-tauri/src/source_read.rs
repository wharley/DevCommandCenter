//! Reading an agent's source with no model (`dcc_core::domain::agent_source`):
//! a short MCP session through the connector gateway — initialize, call the
//! source's read-only tool with its arguments, close — like the connection
//! check, with the agent's own access applied. What is new since the last
//! read comes back; the first read only remembers what is there.

use dcc_core::domain::agent_source::{items_of, read_news, SourceItem};
use dcc_core::domain::mcp::McpDefinitionId;
use serde_json::{json, Value};

use crate::connector_gateway::{ConnectorCall, ConnectorGateway, ConnectorResponse};
use crate::state::SessionCommandState;

const PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRead {
    /// The first read: what is there is only remembered.
    pub first: bool,
    pub news: Vec<SourceItem>,
}

fn json_body(response: ConnectorResponse) -> Result<(Option<String>, Value), String> {
    match response {
        ConnectorResponse::Json { session_id, body } => Ok((session_id, body)),
        ConnectorResponse::Accepted { session_id } => Ok((session_id, Value::Null)),
        ConnectorResponse::SessionExpired => Err("the connector ended the session".to_string()),
    }
}

fn rpc_error(body: &Value) -> Option<String> {
    body.get("error")
        .map(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the connector refused the call")
                .to_string()
        })
        .or_else(|| {
            (body.pointer("/result/isError") == Some(&Value::Bool(true))).then(|| {
                body.pointer("/result/content/0/text")
                    .and_then(Value::as_str)
                    .unwrap_or("the tool failed")
                    .to_string()
            })
        })
}

/// One call of `tool` on the connector, in a session of its own.
async fn call_once(
    gateway: &ConnectorGateway,
    connector: &McpDefinitionId,
    tool: &str,
    arguments: Value,
    access: dcc_core::domain::agent_access::AccessLevel,
) -> Result<Value, String> {
    let call = |session_id: Option<String>, protocol: Option<String>, body: Value| ConnectorCall {
        session_id,
        protocol_version: protocol,
        body,
    };
    let initialize = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "dcc-agent-source", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    let (session, body) = json_body(
        gateway
            .handle_with_access(connector, call(None, None, initialize), access)
            .await,
    )?;
    if let Some(error) = rpc_error(&body) {
        return Err(error);
    }
    let protocol = body
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION)
        .to_string();
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let _ = gateway
        .handle_with_access(
            connector,
            call(session.clone(), Some(protocol.clone()), initialized),
            access,
        )
        .await;
    let request = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    });
    let answer = json_body(
        gateway
            .handle_with_access(
                connector,
                call(session.clone(), Some(protocol), request),
                access,
            )
            .await,
    );
    if let Some(session) = &session {
        gateway.close(connector, session).await;
    }
    let (_, body) = answer?;
    if let Some(error) = rpc_error(&body) {
        return Err(error);
    }
    Ok(body.get("result").cloned().unwrap_or(Value::Null))
}

/// Reads source `id` now and remembers what it showed.
pub async fn read_source(
    state: &SessionCommandState,
    gateway: &ConnectorGateway,
    id: &str,
) -> Result<SourceRead, String> {
    let text = |error: dcc_core::CoreError| error.to_string();
    let (source, seen) = state
        .agent_source_with_seen(id)
        .map_err(text)?
        .ok_or_else(|| "source not found".to_string())?;
    if source.proposed || !source.enabled {
        return Err("this source is not on".to_string());
    }
    let agent = state
        .list_stored_agents(&Default::default())
        .map_err(text)?
        .into_iter()
        .find(|agent| agent.id == source.agent_id)
        .ok_or_else(|| "the source's agent is gone".to_string())?;
    let access = agent.tool_access.connector(&source.connector_id);
    let arguments: Value =
        serde_json::from_str(&source.arguments).map_err(|error| error.to_string())?;
    let result = call_once(
        gateway,
        &McpDefinitionId(source.connector_id.clone()),
        &source.tool,
        arguments,
        access,
    )
    .await?;
    let items = items_of(&result);
    let (news, remembered) = read_news(&seen, &items);
    let first = source.last_checked_at.is_none();
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    state
        .record_agent_source_read(id, &remembered, &at)
        .map_err(text)?;
    Ok(SourceRead {
        first,
        news: if first { Vec::new() } else { news },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_call_reads_as_its_message() {
        assert_eq!(
            rpc_error(&json!({"error": {"message": "nope"}})).as_deref(),
            Some("nope")
        );
        assert_eq!(
            rpc_error(&json!({"result": {"isError": true, "content": [{"text": "bad query"}]}}))
                .as_deref(),
            Some("bad query")
        );
        assert_eq!(rpc_error(&json!({"result": {"content": []}})), None);
    }
}
