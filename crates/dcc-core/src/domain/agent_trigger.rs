//! What wakes a resident agent by itself, and each time it did (a run).
//! The desktop keeps the switches and the runs in its own file
//! (`agent-triggers.json`); the team room's cards, on the desktop and on
//! the phone, read the runs to say what woke an agent ("Gatilho · CI falhou").

use serde::{Deserialize, Serialize};

/// Something that wakes an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    /// A pull request the person reviewed got commits or replies.
    PrUpdated,
    /// The checks of a task's pull request failed.
    CiFailed,
    /// Someone reviewed or commented on a task's pull request.
    ReviewReceived,
    /// A pull request asks for the person's review (the check-in).
    ReviewRequested,
    /// An issue was assigned to the person (the check-in).
    IssueAssigned,
    /// A task's turn is open with no sign of life (the check-in).
    TaskStalled,
    /// A routine's time came; not a switch, and not counted in the daily
    /// ceiling, only in the brake.
    Routine,
    /// A source the agent watches brought news; like a routine, counted
    /// only in the brake.
    Source,
}

impl Trigger {
    pub const ALL: [Trigger; 6] = [
        Trigger::PrUpdated,
        Trigger::ReviewRequested,
        Trigger::CiFailed,
        Trigger::ReviewReceived,
        Trigger::IssueAssigned,
        Trigger::TaskStalled,
    ];

    /// `agents.triggers.<id>.*`, `agents.room.card.trigger.<id>`.
    pub fn id(self) -> &'static str {
        match self {
            Self::PrUpdated => "prUpdated",
            Self::CiFailed => "ciFailed",
            Self::ReviewReceived => "reviewReceived",
            Self::ReviewRequested => "reviewRequested",
            Self::IssueAssigned => "issueAssigned",
            Self::TaskStalled => "taskStalled",
            Self::Routine => "routine",
            Self::Source => "source",
        }
    }

    /// The pull request hub's review is the Reviewer's own job; the others
    /// wake any agent in its conversation.
    pub fn reviewer_only(self) -> bool {
        matches!(self, Self::PrUpdated | Self::ReviewRequested)
    }
}

/// A trigger that woke an agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRun {
    pub agent_id: String,
    pub trigger: Trigger,
    /// The pull request's URL or the task's id.
    pub target: String,
    pub at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_reads_back_as_the_desktop_writes_it() {
        let raw =
            r#"{"agentId":"rev","trigger":"ciFailed","target":"w-1","at":"2026-11-07T10:20:00Z"}"#;
        let run: TriggerRun = serde_json::from_str(raw).unwrap();
        assert_eq!(run.trigger, Trigger::CiFailed);
        assert_eq!(serde_json::to_string(&run).unwrap(), raw);
        assert_eq!(Trigger::TaskStalled.id(), "taskStalled");
    }
}
