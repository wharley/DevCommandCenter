//! "Sempre permitir" on an approval card: a connector's tool the vendor
//! marks read-only is allowed from then on (its tool policy, the same one
//! Settings › Connectors sets). A tool that writes is allowed only this
//! once — writing always asks. The tool comes as the provider names it,
//! `mcp__<server>__<tool>`, the server being the connector's projected
//! name (`connector_server_name`).

use std::collections::HashSet;
use std::path::Path;

use dcc_core::domain::mcp::McpToolPolicyDecision;
use dcc_core::ports::McpRepo;
use dcc_infra::mcp_db::SqliteMcpRepo;

use super::mcp_commands::{set_tool_policy_in, SetMcpToolPolicyInput};

/// `mcp__dcc-slack__list_channels` → (`dcc-slack`, `list_channels`).
pub fn connector_tool(tool_name: &str) -> Option<(&str, &str)> {
    let (server, tool) = tool_name.strip_prefix("mcp__")?.split_once("__")?;
    (server.starts_with("dcc-") && !tool.is_empty()).then_some((server, tool))
}

/// Whether the tool was read-only and is now always allowed.
pub async fn remember_read_only_allow(db_path: &Path, tool_name: &str) -> Result<bool, String> {
    let Some((server, tool)) = connector_tool(tool_name) else {
        return Ok(false);
    };
    let repo = SqliteMcpRepo::open(db_path).map_err(|error| error.to_string())?;
    let definitions = repo
        .list_mcp_definitions()
        .await
        .map_err(|error| error.to_string())?;
    let none = HashSet::new();
    let Some(definition) = definitions.iter().find(|definition| {
        crate::state::connector_server_name(&definition.display_name, &none) == server
    }) else {
        return Ok(false);
    };
    let read_only = repo
        .list_mcp_connection_checks()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|check| check.definition_id == definition.id)
        .and_then(|check| check.tools.into_iter().find(|found| found.name == tool))
        .is_some_and(|found| found.annotations.read_only_hint == Some(true));
    if !read_only {
        return Ok(false);
    }
    set_tool_policy_in(
        &repo,
        SetMcpToolPolicyInput {
            definition_id: definition.id.clone(),
            tool_name: tool.to_string(),
            decision: McpToolPolicyDecision::Allow,
        },
    )
    .await
    .map(|_| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connectors_tool_is_read_from_the_providers_name() {
        assert_eq!(
            connector_tool("mcp__dcc-slack__list_channels"),
            Some(("dcc-slack", "list_channels"))
        );
        assert_eq!(
            connector_tool("mcp__dcc-browser-webview__dcc_browser_click"),
            Some(("dcc-browser-webview", "dcc_browser_click"))
        );
        assert_eq!(connector_tool("Bash"), None);
        assert_eq!(connector_tool("mcp__other__x"), None);
    }
}
