//! Where a task an agent opened stands and how much it changed, for the
//! report under the answer that tells of it: read from its delegation, so
//! the desktop's room and the phone's say the same.

use crate::domain::delegation::{Delegation, DelegationKind, DelegationStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportState {
    Working,
    /// It ended a turn and waits on the person.
    TurnEnded,
    Closed,
    Failed,
}

impl ReportState {
    /// Its name on the wire (`turnEnded`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::TurnEnded => "turnEnded",
            Self::Closed => "closed",
            Self::Failed => "failed",
        }
    }
}

/// Whether `delegation` is a task an agent opened (not a review or a
/// question).
pub fn is_task(delegation: &Delegation) -> bool {
    delegation.kind == DelegationKind::Task
}

pub fn report_state(delegation: &Delegation) -> ReportState {
    match delegation.status {
        DelegationStatus::Failed | DelegationStatus::Cancelled => ReportState::Failed,
        DelegationStatus::Completed | DelegationStatus::ReviewPending => ReportState::Closed,
        _ if delegation.reported_turn_id.is_some() => ReportState::TurnEnded,
        _ => ReportState::Working,
    }
}

/// `"3 file(s), +40/-12"` → (40, 12).
pub fn lines_changed(summary: Option<&str>) -> (u64, u64) {
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

/// The task it became, to open it.
pub fn report_task_id(delegation: &Delegation) -> Option<String> {
    delegation
        .budget
        .new_task
        .as_ref()
        .and_then(|target| target.workspace_id.clone())
}

/// Its title, else its id.
pub fn report_title(delegation: &Delegation) -> String {
    delegation
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| delegation.id.0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tasks_state_and_lines_read_from_its_delegation() {
        assert_eq!(lines_changed(Some("3 file(s), +40/-12")), (40, 12));
        assert_eq!(lines_changed(None), (0, 0));
        assert_eq!(lines_changed(Some("odd")), (0, 0));
        let mut task: Delegation = serde_json::from_value(serde_json::json!({
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
            "touchedFiles": [],
            "diffSummary": null,
            "validationSummary": null,
            "createdAt": "2026-11-09T10:00:00Z",
            "updatedAt": "2026-11-09T10:06:00Z",
            "kind": "task",
            "title": " "
        }))
        .unwrap();
        assert!(is_task(&task));
        assert_eq!(report_state(&task), ReportState::Working);
        assert_eq!(report_task_id(&task).as_deref(), Some("w-campo"));
        assert_eq!(report_title(&task), "t-1");
        task.reported_turn_id = Some(crate::domain::session::TurnId("t".into()));
        assert_eq!(report_state(&task).as_str(), "turnEnded");
        task.status = DelegationStatus::Cancelled;
        assert_eq!(report_state(&task), ReportState::Failed);
    }
}
