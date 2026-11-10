//! What the team room's report shows of each task an agent opened
//! (`room_report`): its title, where it stands, the files and lines it
//! changed, how long it ran, and whose it is. Built from the delegations the
//! sidebar already reads; nothing new is stored and no model writes it.

use std::collections::HashMap;

use dcc_core::domain::delegation::{Delegation, DelegationKind, DelegationStatus};

use super::agents::AgentRecord;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportState {
    Working,
    /// It ended a turn and waits on the person.
    TurnEnded,
    Closed,
    Failed,
}

impl ReportState {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Working => "agents.report.state.working",
            Self::TurnEnded => "agents.report.state.turnEnded",
            Self::Closed => "agents.report.state.closed",
            Self::Failed => "agents.report.state.failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskReport {
    pub title: String,
    pub state: ReportState,
    /// The task it became, to open it.
    pub task_id: Option<String>,
    pub provider_id: String,
    /// The agent that opened it.
    pub agent_id: Option<String>,
    pub files: usize,
    pub additions: u64,
    pub deletions: u64,
    /// From its start to its last change, in seconds.
    pub seconds: Option<i64>,
    /// The session that ran it, for its turns' captures once the checkout
    /// is gone (`task_changes`).
    pub session_id: Option<String>,
}

/// `"3 file(s), +40/-12"` → (40, 12).
fn lines_changed(summary: Option<&str>) -> (u64, u64) {
    let Some((_, lines)) = summary.and_then(|summary| summary.split_once(", +")) else {
        return (0, 0);
    };
    let Some((added, removed)) = lines.split_once("/-") else {
        return (0, 0);
    };
    (
        added.trim().parse().unwrap_or(0),
        removed.trim().parse().unwrap_or(0),
    )
}

fn seconds_between(start: Option<&str>, end: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(start?).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(end).ok()?;
    Some((end - start).num_seconds().max(0))
}

/// Delegation id → report, for the tasks only.
pub fn by_delegation(
    delegations: &[Delegation],
    agents: &[AgentRecord],
) -> HashMap<String, TaskReport> {
    let owner: HashMap<&str, &str> = agents
        .iter()
        .flat_map(|agent| {
            agent
                .sessions
                .iter()
                .map(move |session| (session.session_id.as_str(), agent.id.as_str()))
        })
        .collect();
    delegations
        .iter()
        .filter(|delegation| delegation.kind == DelegationKind::Task)
        .map(|delegation| {
            let state = match delegation.status {
                DelegationStatus::Failed | DelegationStatus::Cancelled => ReportState::Failed,
                DelegationStatus::Completed | DelegationStatus::ReviewPending => {
                    ReportState::Closed
                }
                _ if delegation.reported_turn_id.is_some() => ReportState::TurnEnded,
                _ => ReportState::Working,
            };
            let (additions, deletions) = lines_changed(delegation.diff_summary.as_deref());
            let report = TaskReport {
                title: delegation
                    .title
                    .clone()
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or_else(|| delegation.id.0.clone()),
                state,
                task_id: delegation
                    .budget
                    .new_task
                    .as_ref()
                    .and_then(|target| target.workspace_id.clone()),
                provider_id: delegation.target_provider_id.0.clone(),
                agent_id: owner
                    .get(delegation.parent_session_id.0.as_str())
                    .map(|agent| agent.to_string()),
                files: delegation.touched_files.len(),
                additions,
                deletions,
                seconds: seconds_between(delegation.started_at.as_deref(), &delegation.updated_at),
                session_id: delegation
                    .child_session_id
                    .as_ref()
                    .map(|session| session.0.clone()),
            };
            (delegation.id.0.clone(), report)
        })
        .collect()
}

/// "4 min", "1 h 12 min", "< 1 min".
pub fn duration_label(seconds: i64) -> String {
    let minutes = seconds / 60;
    match minutes {
        0 => "< 1 min".to_string(),
        1..=59 => format!("{minutes} min"),
        _ if minutes % 60 == 0 => format!("{} h", minutes / 60),
        _ => format!("{} h {} min", minutes / 60, minutes % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tasks_lines_and_time_read_from_what_dcc_keeps() {
        assert_eq!(lines_changed(Some("3 file(s), +40/-12")), (40, 12));
        assert_eq!(lines_changed(None), (0, 0));
        assert_eq!(lines_changed(Some("odd")), (0, 0));
        assert_eq!(duration_label(30), "< 1 min");
        assert_eq!(duration_label(4 * 60), "4 min");
        assert_eq!(duration_label(72 * 60), "1 h 12 min");
        assert_eq!(duration_label(120 * 60), "2 h");
        let task: Delegation = serde_json::from_value(serde_json::json!({
            "id": "t-1",
            "parentSessionId": "s-home",
            "parentTurnId": "turn",
            "childSessionId": "child",
            "workspaceId": "w-home",
            "targetProviderId": "claude_code",
            "mode": "implement",
            "status": "running",
            "prompt": "",
            "contextPolicy": { "type": "minimal" },
            "budget": { "turnLimit": null, "timeoutSeconds": null, "allowFileEdits": true,
                        "newTask": { "projectId": "p", "project": "Checkout", "rootPath": "/w",
                                     "baseBranch": "main", "worktree": true, "workspaceId": "w-campo" } },
            "resultSummary": "ok",
            "touchedFiles": ["a.ts", "b.ts"],
            "diffSummary": "2 file(s), +30/-4",
            "validationSummary": null,
            "createdAt": "2026-11-09T10:00:00Z",
            "startedAt": "2026-11-09T10:00:00Z",
            "updatedAt": "2026-11-09T10:06:00Z",
            "reportedTurnId": "t",
            "kind": "task",
            "title": "Campo X"
        }))
        .unwrap();
        let reports = by_delegation(&[task], &[]);
        let report = &reports["t-1"];
        assert_eq!(report.state, ReportState::TurnEnded);
        assert_eq!(report.task_id.as_deref(), Some("w-campo"));
        assert_eq!(
            (report.files, report.additions, report.deletions),
            (2, 30, 4)
        );
        assert_eq!(report.seconds, Some(360));
    }
}
