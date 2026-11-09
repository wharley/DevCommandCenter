//! An agent's own conversation works without the person approving each
//! read (Tucano's and Compozy's resident agents, the hub's review): there is
//! no code in its home, so what it may run on its own is what only reads —
//! a connector's tool the vendor marks `readOnlyHint` (from the last
//! connection check) is allowed there unless the person set that tool
//! otherwise. A tool that writes (sending a message, opening an issue)
//! still asks, and the agent's access (`agent_access`) still narrows what
//! it sees at all. Tasks keep the person's permission mode.

use dcc_core::domain::mcp::{McpConnectionCheck, McpToolPolicyDecision};
use dcc_core::ports::{ProviderMcpServerConfig, ProviderMcpToolPolicy};

/// Adds "allow" for each read-only tool with no policy of its own.
pub(crate) fn allow_reads(servers: &mut [ProviderMcpServerConfig], checks: &[McpConnectionCheck]) {
    for server in servers {
        let Some(check) = checks
            .iter()
            .find(|check| check.definition_id == server.definition_id)
        else {
            continue;
        };
        for tool in &check.tools {
            let reads = tool.annotations.read_only_hint == Some(true);
            let set = server
                .tool_policies
                .iter()
                .any(|policy| policy.tool_name == tool.name);
            if reads && !set {
                server.tool_policies.push(ProviderMcpToolPolicy {
                    tool_name: tool.name.clone(),
                    decision: McpToolPolicyDecision::Allow,
                });
            }
        }
        server
            .tool_policies
            .sort_unstable_by(|left, right| left.tool_name.cmp(&right.tool_name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::mcp::{
        McpConnectionState, McpDefinitionId, McpToolAnnotations, McpToolSummary,
    };
    use dcc_core::ports::ProviderMcpTransport;

    #[test]
    fn reads_go_on_their_own_and_writes_and_the_persons_choices_stay() {
        let tool = |name: &str, reads: Option<bool>| McpToolSummary {
            name: name.into(),
            annotations: McpToolAnnotations {
                read_only_hint: reads,
                ..McpToolAnnotations::default()
            },
        };
        let mut servers = vec![ProviderMcpServerConfig {
            definition_id: McpDefinitionId("slack".into()),
            server_name: "dcc-slack".into(),
            transport: ProviderMcpTransport::Http {
                url: "https://example.com/mcp".into(),
                headers: Vec::new(),
            },
            tool_policies: vec![ProviderMcpToolPolicy {
                tool_name: "slack_search_users".into(),
                decision: McpToolPolicyDecision::Deny,
            }],
        }];
        let checks = vec![McpConnectionCheck {
            definition_id: McpDefinitionId("slack".into()),
            state: McpConnectionState::Connected,
            tools: vec![
                tool("slack_read_channel", Some(true)),
                tool("slack_search_users", Some(true)),
                tool("slack_send_message", Some(false)),
                tool("slack_unknown", None),
            ],
            message: None,
            checked_at: "2026-11-09T12:00:00Z".into(),
        }];
        allow_reads(&mut servers, &checks);
        let policies: Vec<(&str, McpToolPolicyDecision)> = servers[0]
            .tool_policies
            .iter()
            .map(|policy| (policy.tool_name.as_str(), policy.decision.clone()))
            .collect();
        assert_eq!(
            policies,
            [
                ("slack_read_channel", McpToolPolicyDecision::Allow),
                ("slack_search_users", McpToolPolicyDecision::Deny),
            ]
        );
    }
}
