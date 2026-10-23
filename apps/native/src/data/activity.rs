//! What a task's agent is doing, from its sessions. The rule itself is
//! `dcc_core::domain::attention::derive_agent_activity` (shared with the
//! desktop and the phone); this adds what only the rail reads: when the
//! person last acted, the provider and which conversations wait or run.

use std::collections::HashSet;

use dcc_core::domain::attention::{
    AgentActivity, AgentActivityState, AttentionReason, SessionActivityInput, derive_agent_activity,
};
use dcc_core::domain::session::WorkspaceSessionSummary;

#[derive(Clone, Debug, PartialEq)]
pub struct TaskActivity {
    pub activity: AgentActivity,
    /// The newest turn start: the rail orders tasks by what the person did.
    pub last_interaction_at: Option<String>,
    /// Conversations blocked on the person, the one to open first.
    pub waiting_session_ids: Vec<String>,
    pub running_session_ids: Vec<String>,
}

impl TaskActivity {
    pub fn is_turn_open(&self) -> bool {
        matches!(
            self.activity.state,
            AgentActivityState::Active | AgentActivityState::Waiting(_)
        )
    }

    pub fn is_running(&self) -> bool {
        self.activity.state == AgentActivityState::Active
    }

    pub fn waiting_for_permission(&self) -> bool {
        self.activity.state == AgentActivityState::Waiting(AttentionReason::Permission)
    }

    /// `workspaceActivityTimestamp`: the start while open, else the end.
    pub fn timestamp(&self) -> Option<&str> {
        if self.is_turn_open() {
            self.activity.started_at.as_deref()
        } else {
            self.activity.completed_at.as_deref()
        }
    }
}

/// Summaries come newest session first, as `list_workspace_sessions` returns them.
pub fn task_activity(summaries: &[WorkspaceSessionSummary]) -> Option<TaskActivity> {
    let inputs: Vec<SessionActivityInput> = summaries
        .iter()
        .map(SessionActivityInput::from_summary)
        .collect();
    let activity = derive_agent_activity(&inputs)?;
    let running: Vec<(usize, &SessionActivityInput)> = inputs
        .iter()
        .enumerate()
        .filter(|(_, input)| input.is_running())
        .collect();
    // In session order, so the one the rule picked comes first.
    let waiting: Vec<String> = running
        .iter()
        .filter(|(_, input)| input.waiting_reason().is_some())
        .map(|(index, _)| summaries[*index].session.id.0.clone())
        .collect();
    Some(TaskActivity {
        activity,
        last_interaction_at: last_interaction_at(summaries),
        waiting_session_ids: waiting,
        running_session_ids: running
            .iter()
            .map(|(index, _)| summaries[*index].session.id.0.clone())
            .collect(),
    })
}

/// `lastInteractionAtFromSessions`
pub fn last_interaction_at(summaries: &[WorkspaceSessionSummary]) -> Option<String> {
    summaries
        .iter()
        .filter_map(|summary| {
            let started = summary.last_turn_started_at.as_deref()?;
            Some((crate::data::time::parse_ms(started)?, started))
        })
        .max_by_key(|(ms, _)| *ms)
        .map(|(_, started)| started.to_string())
}

/// `deriveProviderIdFromSessions`: the task's own conversation, never a
/// child it delegated to; the running one wins, then the newest.
pub fn provider_id(
    summaries: &[WorkspaceSessionSummary],
    delegated_session_ids: &HashSet<String>,
) -> Option<String> {
    let own: Vec<&WorkspaceSessionSummary> = summaries
        .iter()
        .filter(|summary| !delegated_session_ids.contains(&summary.session.id.0))
        .collect();
    let candidates: Vec<&WorkspaceSessionSummary> = if own.is_empty() {
        summaries.iter().collect()
    } else {
        own
    };
    let summary = candidates
        .iter()
        .find(|summary| SessionActivityInput::from_summary(summary).is_running())
        .or_else(|| candidates.first())?;
    let provider = summary.session.provider_id.trim();
    (!provider.is_empty()).then(|| provider.to_string())
}
