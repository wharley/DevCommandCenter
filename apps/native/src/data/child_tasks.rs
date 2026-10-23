//! Tasks an agent proposed or started from a task (`dcc_propose_task`), as
//! the sidebar nests them under it (`sidebar-child-tasks.logic.ts`).

use std::collections::HashMap;

use dcc_core::domain::delegation::{Delegation, DelegationKind, DelegationStatus};

/// `working` has a turn in progress; `idle` is open but stopped between
/// turns; `waiting` stopped on the person (approval, question).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildTaskState {
    Proposed,
    Working,
    Idle,
    Waiting,
    Review,
}

impl ChildTaskState {
    /// The task waits on the person: a proposal, an approval, edits to review.
    pub fn waits_on_person(self) -> bool {
        matches!(self, Self::Proposed | Self::Waiting | Self::Review)
    }

    /// `sidebar.childTask.*`
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Proposed => "sidebar.childTask.proposed",
            Self::Working => "sidebar.childTask.working",
            Self::Idle => "sidebar.childTask.idle",
            Self::Waiting => "sidebar.childTask.waiting",
            Self::Review => "sidebar.childTask.review",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChildTask {
    pub id: String,
    pub title: String,
    pub state: ChildTaskState,
    /// The conversation a click opens: the task's own, or the parent's while
    /// it is only a proposal.
    pub session_id: String,
    /// The provider the task runs on, which can differ from its parent's.
    pub provider_id: Option<String>,
}

/// `childTasksByWorkspace`: the tasks still in play under each task, oldest
/// first. Closed, failed and dismissed ones leave; plain delegations never enter.
pub fn by_workspace(delegations: &[Delegation]) -> HashMap<String, Vec<ChildTask>> {
    let mut ordered: Vec<&Delegation> = delegations.iter().collect();
    ordered.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    let mut tasks: HashMap<String, Vec<ChildTask>> = HashMap::new();
    for delegation in ordered {
        let state = match delegation.status {
            DelegationStatus::Draft => ChildTaskState::Proposed,
            DelegationStatus::Queued | DelegationStatus::Running => ChildTaskState::Idle,
            DelegationStatus::ReviewPending => ChildTaskState::Review,
            _ => continue,
        };
        if delegation.kind != DelegationKind::Task {
            continue;
        }
        let title = delegation
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
            .or_else(|| {
                delegation
                    .instruction
                    .as_deref()
                    .map(str::trim)
                    .and_then(|text| text.lines().next())
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| delegation.id.0.clone());
        let provider = delegation.target_provider_id.0.trim();
        tasks
            .entry(delegation.workspace_id.0.clone())
            .or_default()
            .push(ChildTask {
                id: delegation.id.0.clone(),
                title,
                state,
                session_id: delegation
                    .child_session_id
                    .as_ref()
                    .unwrap_or(&delegation.parent_session_id)
                    .0
                    .clone(),
                provider_id: (!provider.is_empty()).then(|| provider.to_string()),
            });
    }
    tasks
}

/// `withLiveTaskState`: what an open task does now, from its conversation.
pub fn live_state(task: &ChildTask, running: &[String], waiting: &[String]) -> ChildTaskState {
    if task.state != ChildTaskState::Idle {
        return task.state;
    }
    if waiting.contains(&task.session_id) {
        ChildTaskState::Waiting
    } else if running.contains(&task.session_id) {
        ChildTaskState::Working
    } else {
        ChildTaskState::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delegation(
        id: &str,
        kind: DelegationKind,
        status: DelegationStatus,
        created_at: &str,
    ) -> Delegation {
        let mut value = serde_json::json!({
            "id": id,
            "parentSessionId": "parent",
            "parentTurnId": null,
            "childSessionId": null,
            "workspaceId": "w",
            "targetProviderId": "codex",
            "mode": "implement",
            "status": "draft",
            "prompt": "",
            "contextPolicy": { "type": "minimal" },
            "budget": { "turnLimit": null, "timeoutSeconds": null, "allowFileEdits": false },
            "resultSummary": null,
            "diffSummary": null,
            "validationSummary": null,
            "createdAt": created_at,
            "updatedAt": created_at,
            "title": format!("Fase {id}"),
        });
        value["status"] = serde_json::to_value(status).unwrap();
        value["kind"] = serde_json::to_value(kind).unwrap();
        serde_json::from_value(value).expect("delegation")
    }

    #[test]
    fn only_live_tasks_nest_oldest_first() {
        let list = [
            delegation(
                "b",
                DelegationKind::Task,
                DelegationStatus::Running,
                "2026-10-23T11:00:00Z",
            ),
            delegation(
                "a",
                DelegationKind::Task,
                DelegationStatus::Draft,
                "2026-10-23T10:00:00Z",
            ),
            delegation(
                "c",
                DelegationKind::Task,
                DelegationStatus::Completed,
                "2026-10-23T12:00:00Z",
            ),
            delegation(
                "d",
                DelegationKind::Delegation,
                DelegationStatus::Running,
                "2026-10-23T09:00:00Z",
            ),
        ];
        let tasks = &by_workspace(&list)["w"];
        let ids: Vec<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(tasks[0].state, ChildTaskState::Proposed);
        assert_eq!(tasks[0].session_id, "parent", "a proposal opens the parent");
        assert_eq!(tasks[1].provider_id.as_deref(), Some("codex"));
        assert_eq!(
            live_state(&tasks[1], &["parent".into()], &[]),
            ChildTaskState::Working
        );
        assert_eq!(
            live_state(&tasks[1], &["parent".into()], &["parent".into()]),
            ChildTaskState::Waiting
        );
        assert_eq!(
            live_state(&tasks[0], &["parent".into()], &[]),
            ChildTaskState::Proposed
        );
    }
}
