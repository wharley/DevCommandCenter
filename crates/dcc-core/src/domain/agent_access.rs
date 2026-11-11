//! What a resident agent may use (Tucano's per-connector access, Compozy's
//! tool allowlists): each group of DCC's own tools and each connector at
//! no access, read (only the tools marked read-only) or read and write.
//! Nothing set means read and write, as before; the agent's own tools
//! (memory, overview, opening a task, routines) are always there. DCC
//! enforces it where the tools are served — the list an agent sees and the
//! call it makes — so a change holds from the next call on.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum AccessLevel {
    None,
    Read,
    #[default]
    Write,
}

/// The groups of DCC's own tools a person can narrow, in the editor's order.
/// `tasks`: opening a task in a project (`dcc_open_task`), the Reviewer's
/// off by default — it reviews; a teammate fixes. `github`: reading pull
/// requests, checks and deploys through the person's `gh` (`dcc_github`);
/// every call only reads, so "Ler" and "Ler e escrever" both allow it.
pub const DCC_TOOL_GROUPS: [&str; 6] = [
    "browser",
    "computer",
    "device",
    "delegation",
    "tasks",
    "github",
];

/// What the error of a refused call says, so the agent stops trying.
pub const ACCESS_DENIED: &str = "The person turned this tool off for you in DCC.";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolAccess {
    /// By group of DCC's tools (`DCC_TOOL_GROUPS`); missing: read and write.
    pub groups: BTreeMap<String, AccessLevel>,
    /// By connector definition id; missing: read and write.
    pub connectors: BTreeMap<String, AccessLevel>,
    /// What the tasks it opens may do without asking (Monocode's and
    /// Compozy's per-agent mode); missing: ask, as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks_approval: Option<super::provider::ProviderApprovalPolicy>,
    /// What its own conversation may do without asking on a message the
    /// person sends; a wake (a source, a routine, a trigger) still asks.
    /// Missing: ask, as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_approval: Option<super::provider::ProviderApprovalPolicy>,
}

/// The group a DCC tool belongs to; none for the agent's own tools.
pub fn dcc_group(tool: &str) -> Option<&'static str> {
    if tool.starts_with("dcc_browser_") {
        Some("browser")
    } else if tool.starts_with("dcc_computer_") {
        Some("computer")
    } else if tool.starts_with("dcc_device_") {
        Some("device")
    } else if tool == "dcc_delegate_task"
        || tool == "dcc_propose_task"
        || tool.starts_with("dcc_task_")
    {
        Some("delegation")
    } else if tool == "dcc_open_task" {
        Some("tasks")
    } else if tool.starts_with("dcc_github") {
        Some("github")
    } else {
        None
    }
}

fn allows(level: AccessLevel, read_only: bool) -> bool {
    match level {
        AccessLevel::None => false,
        AccessLevel::Read => read_only,
        AccessLevel::Write => true,
    }
}

impl ToolAccess {
    /// A mode was set for its tasks or its conversation.
    pub fn has_modes(&self) -> bool {
        self.tasks_approval.is_some() || self.conversation_approval.is_some()
    }

    pub fn is_full(&self) -> bool {
        self.groups
            .values()
            .all(|level| *level == AccessLevel::Write)
            && self
                .connectors
                .values()
                .all(|level| *level == AccessLevel::Write)
    }

    pub fn group(&self, group: &str) -> AccessLevel {
        self.groups.get(group).copied().unwrap_or_default()
    }

    pub fn connector(&self, definition_id: &str) -> AccessLevel {
        self.connectors
            .get(definition_id)
            .copied()
            .unwrap_or_default()
    }

    /// Whether the agent may use DCC's `tool` (`read_only`: the tool's hint).
    pub fn allows_dcc(&self, tool: &str, read_only: bool) -> bool {
        dcc_group(tool).is_none_or(|group| allows(self.group(group), read_only))
    }

    /// Whether the agent may use a connector's tool.
    pub fn allows_connector(&self, definition_id: &str, read_only: bool) -> bool {
        allows(self.connector(definition_id), read_only)
    }

    /// Only what narrows anything is kept.
    pub fn normalized(mut self) -> Self {
        self.groups.retain(|group, level| {
            DCC_TOOL_GROUPS.contains(&group.as_str()) && *level != AccessLevel::Write
        });
        self.connectors
            .retain(|id, level| !id.trim().is_empty() && *level != AccessLevel::Write);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_set_is_everything_and_each_level_narrows() {
        let full = ToolAccess::default();
        assert!(full.is_full() && full.allows_dcc("dcc_browser_click", false));
        let mut access = ToolAccess::default();
        access.groups.insert("browser".into(), AccessLevel::Read);
        access.groups.insert("computer".into(), AccessLevel::None);
        access.connectors.insert("slack".into(), AccessLevel::Read);
        assert!(access.allows_dcc("dcc_browser_screenshot", true));
        assert!(!access.allows_dcc("dcc_browser_click", false));
        assert!(!access.allows_dcc("dcc_computer_status", true));
        assert!(access.allows_dcc("dcc_device_tap", false), "not narrowed");
        assert!(
            access.allows_dcc("dcc_agent_remember", false),
            "the agent's own"
        );
        assert!(access.allows_connector("slack", true));
        assert!(!access.allows_connector("slack", false));
        assert!(access.allows_connector("github", false));
        assert_eq!(dcc_group("dcc_task_wait"), Some("delegation"));
        assert_eq!(dcc_group("dcc_open_task"), Some("tasks"));
        assert_eq!(dcc_group("dcc_github"), Some("github"));
        assert!(access.allows_dcc("dcc_github", true), "missing: allowed");
        let mut github = ToolAccess::default();
        github.groups.insert("github".into(), AccessLevel::Read);
        assert!(github.allows_dcc("dcc_github", true), "read-only tool");
        github.groups.insert("github".into(), AccessLevel::None);
        assert!(!github.allows_dcc("dcc_github", true));
        let mut noisy = access.clone();
        noisy.groups.insert("device".into(), AccessLevel::Write);
        noisy.groups.insert("nope".into(), AccessLevel::None);
        assert_eq!(noisy.normalized(), access);
    }
}
