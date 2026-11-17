//! A change to what exists, proposed by an agent (`dcc_propose_change`,
//! Tucano's `propor_ajuste`): the room a source brings its news to, how
//! often it reads, whether it is on; the projects an agent works in. It
//! waits as a card with the before and the after, and nothing changes
//! until the person applies it. What lets an agent act without asking is
//! not here: that is the person's to turn on, never an agent's to propose
//! into being.

use serde::{Deserialize, Serialize};

pub const PROPOSE_CHANGE_TOOL: &str = "dcc_propose_change";
/// Changes waiting for the person at once: a migration of a few sources
/// and agents fits, a flood does not.
pub const MAX_WAITING_CHANGES: usize = 8;
pub const MAX_CHANGE_REASON_CHARS: usize = 300;

/// What changes, with what it was when proposed (the card's "before").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Change {
    /// A source of `agent_id`; only the fields that change are set, each
    /// as `(before, after)`.
    #[serde(rename_all = "camelCase")]
    Source {
        source_id: String,
        agent_id: String,
        connector: String,
        watch: String,
        /// The room's project; none: Geral.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room: Option<(Option<String>, Option<String>)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        every_minutes: Option<(u32, u32)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<(bool, bool)>,
    },
    /// The projects `agent_id` works in; empty: every project.
    #[serde(rename_all = "camelCase")]
    Projects {
        agent_id: String,
        before: Vec<String>,
        after: Vec<String>,
    },
}

impl Change {
    /// The agent the change is about.
    pub fn about(&self) -> &str {
        match self {
            Self::Source { agent_id, .. } | Self::Projects { agent_id, .. } => agent_id,
        }
    }

    /// Whether anything would be different once applied.
    pub fn changes_something(&self) -> bool {
        match self {
            Self::Source {
                room,
                every_minutes,
                enabled,
                ..
            } => {
                room.as_ref().is_some_and(|(before, after)| before != after)
                    || every_minutes.is_some_and(|(before, after)| before != after)
                    || enabled.is_some_and(|(before, after)| before != after)
            }
            Self::Projects { before, after, .. } => before != after,
        }
    }

    /// Two proposals for the same thing: the later one replaces the card.
    pub fn same_target(&self, other: &Change) -> bool {
        match (self, other) {
            (Self::Source { source_id: a, .. }, Self::Source { source_id: b, .. }) => a == b,
            (Self::Projects { agent_id: a, .. }, Self::Projects { agent_id: b, .. }) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChange {
    pub id: String,
    /// The agent that proposed it.
    pub agent_id: String,
    pub session_id: String,
    pub change: Change,
    /// Why, in one sentence; it shows on the card.
    #[serde(default)]
    pub reason: String,
    pub created_at: String,
}

/// The reason as the card shows it: one line, bounded.
pub fn clean_reason(reason: &str) -> String {
    reason
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_CHANGE_REASON_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(room: Option<(Option<&str>, Option<&str>)>, every: Option<(u32, u32)>) -> Change {
        Change::Source {
            source_id: "s".into(),
            agent_id: "sup".into(),
            connector: "Slack".into(),
            watch: "menções".into(),
            room: room
                .map(|(before, after)| (before.map(str::to_string), after.map(str::to_string))),
            every_minutes: every,
            enabled: None,
        }
    }

    #[test]
    fn a_change_says_what_it_is_about_and_whether_it_changes_anything() {
        let moved = source(Some((None, Some("app"))), None);
        assert_eq!(moved.about(), "sup");
        assert!(moved.changes_something());
        assert!(!source(Some((Some("app"), Some("app"))), Some((5, 5))).changes_something());
        assert!(!source(None, None).changes_something());
        let projects = |after: &[&str]| Change::Projects {
            agent_id: "dev".into(),
            before: Vec::new(),
            after: after.iter().map(|id| id.to_string()).collect(),
        };
        assert!(projects(&["app"]).changes_something());
        assert!(!projects(&[]).changes_something());
        // The same source twice is one card; a source and an agent are two.
        assert!(moved.same_target(&source(None, Some((1, 5)))));
        assert!(!moved.same_target(&projects(&["app"])));
        // It is stored and read back as it was proposed.
        let stored = serde_json::to_string(&moved).unwrap();
        assert!(stored.contains("\"kind\":\"source\"") && !stored.contains("everyMinutes"));
        assert_eq!(serde_json::from_str::<Change>(&stored).unwrap(), moved);
        assert_eq!(
            clean_reason("  a fonte\n está na  Geral "),
            "a fonte está na Geral"
        );
    }
}
