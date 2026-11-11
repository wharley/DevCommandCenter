//! "Enviar" on a reply an agent proposed (`dcc_core::domain::agent_reply`):
//! DCC calls the connector's tool itself, with no model, with the text as
//! the person left it, under the agent's own access to that connector. A
//! failure keeps the reply waiting with why, to be sent again.

use dcc_core::domain::agent_access::AccessLevel;
use dcc_core::domain::agent_reply::{arguments_with_text, sendable, ReplyState};
use dcc_core::domain::mcp::McpDefinitionId;

use crate::connector_gateway::ConnectorGateway;
use crate::state::SessionCommandState;

/// Sends reply `id` with `text`; the error is the connector's own words.
pub async fn send_reply(
    state: &SessionCommandState,
    gateway: &ConnectorGateway,
    id: &str,
    text: &str,
) -> Result<(), String> {
    let error_text = |error: dcc_core::CoreError| error.to_string();
    let reply = state
        .agent_reply(id)
        .map_err(error_text)?
        .ok_or_else(|| "reply not found".to_string())?;
    if reply.state != ReplyState::Pending {
        return Err("this reply was already sent or discarded".to_string());
    }
    let text = sendable(text).ok_or_else(|| "the reply is empty or too long".to_string())?;
    let agent = state
        .list_stored_agents(&Default::default())
        .map_err(error_text)?
        .into_iter()
        .find(|agent| agent.id == reply.agent_id)
        .ok_or_else(|| "the reply's agent is gone".to_string())?;
    let access = agent.tool_access.connector(&reply.connector_id);
    if access != AccessLevel::Write {
        return Err(format!(
            "{} is not allowed to write to {}",
            agent.name, reply.connector
        ));
    }
    let arguments = arguments_with_text(&reply.arguments, &reply.text_argument, text)?;
    let sent = crate::source_read::call_once(
        gateway,
        &McpDefinitionId(reply.connector_id.clone()),
        &reply.tool,
        arguments,
        access,
    )
    .await;
    match sent {
        Ok(_) => {
            let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            state
                .mark_agent_reply_sent(id, text, &at)
                .map_err(error_text)?;
            Ok(())
        }
        Err(error) => {
            let _ = state.record_agent_reply_error(id, text, &error);
            Err(error)
        }
    }
}
