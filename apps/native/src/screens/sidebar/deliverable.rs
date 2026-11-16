//! The tasks an agent opened whose work is ready to be sent, as the
//! sidebar's rows read it: for the shell to commit, push and open the pull
//! request where the person granted that (`shell::deliver_auto`).

use dcc_core::domain::task_report::ReportState;

use super::Sidebar;
use crate::data::git_watch::GitSnapshot;

/// A task of an agent's with local changes and no turn at work.
#[derive(Clone, Debug, PartialEq)]
pub struct DeliverableTask {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    /// The agent's conversation the task was opened from.
    pub session_id: String,
    pub project_id: String,
    /// Its pull request is open already: the changes are pushed to it.
    pub has_request: bool,
    /// When its own conversation's last turn ended.
    pub ended_at: String,
    /// One per state of its changes, so each is sent once.
    pub key: String,
}

/// Whether the checkout can be sent as it stands, and whether its pull
/// request exists: local changes, a remote, no merge or conflict under
/// way, and a pull request that is open or not there yet.
pub(crate) fn sendable(snapshot: &GitSnapshot) -> Option<bool> {
    let git = snapshot.git.as_ref()?;
    let dirty = !git.staged.is_empty() || !git.unstaged.is_empty();
    if !dirty || !git.has_remote || git.merge_in_progress || git.conflict_count > 0 {
        return None;
    }
    let request = snapshot.pr.as_ref().filter(|pr| pr.number.is_some());
    match request.and_then(|pr| pr.state.as_deref()) {
        None => Some(request.is_some()),
        Some(state) if state.eq_ignore_ascii_case("open") => Some(true),
        // Merged or closed: what changed since is the person's to decide.
        Some(_) => None,
    }
}

impl Sidebar {
    /// The task's open pull request as the rail last read it: its number
    /// and its link.
    pub fn request_of(&self, task_id: &str) -> Option<(u32, Option<String>)> {
        let pr = self.state.git.get(task_id)?.pr.as_ref()?;
        Some((pr.number?, pr.url.clone()))
    }

    /// Every task an agent opened that ended a turn with changes to send.
    /// Only a task in its own worktree: one in the project's checkout
    /// would be committed on whatever branch is checked out there.
    pub fn deliverable_tasks(&self) -> Vec<DeliverableTask> {
        let data = &self.state.data;
        data.tasks
            .iter()
            .filter_map(|task| {
                let opened = data.opened_tasks.get(&task.id)?;
                task.worktree_path.as_ref()?;
                (task.member_workspace_ids.len() <= 1).then_some(())?;
                let activity = data.activities.get(&task.id)?;
                if activity.is_turn_open() {
                    return None;
                }
                let ended_at = activity
                    .session_completed_at(opened.task_session_id.as_deref()?)?
                    .to_string();
                data.task_reports.values().find(|report| {
                    report.task_id.as_deref() == Some(task.id.as_str())
                        && report.state == ReportState::TurnEnded
                })?;
                let snapshot = self.state.git.get(&task.id)?;
                let has_request = sendable(snapshot)?;
                let git = snapshot.git.as_ref()?;
                Some(DeliverableTask {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    agent_id: opened.agent_id.clone(),
                    session_id: opened.session_id.clone(),
                    project_id: task.project_id.clone(),
                    has_request,
                    ended_at,
                    key: format!(
                        "{}:{}:{}:{}",
                        task.id,
                        git.staged_fingerprint,
                        git.staged.len(),
                        git.unstaged.len()
                    ),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::workspace_git::{
        WorkspaceGitChangeEntry, WorkspaceGitStatusOutput, WorkspacePrStatusOutput,
    };

    use super::*;

    fn snapshot(dirty: bool, pr: Option<&str>) -> GitSnapshot {
        let mut git: WorkspaceGitStatusOutput = serde_json::from_value(serde_json::json!({
            "staged": [], "unstaged": [], "stagedFingerprint": "f", "currentBranch": "dcc/x",
            "aheadOfRemoteCount": 0, "behindOfRemoteCount": 0, "conflictCount": 0,
            "mergeInProgress": false, "hasRemote": true
        }))
        .unwrap();
        if dirty {
            git.unstaged.push(WorkspaceGitChangeEntry {
                path: "a.rs".into(),
                name: "a.rs".into(),
                absolute_path: "/w/a.rs".into(),
                status: "M".into(),
                insertions: 1,
                deletions: 0,
            });
        }
        let pr = pr.map(|state| {
            serde_json::from_value::<WorkspacePrStatusOutput>(serde_json::json!({
                "provider": "github", "host": null, "number": 7, "title": null, "url": null,
                "headBranch": null, "baseBranch": null, "state": state, "isDraft": false,
                "mergeable": null, "mergeStateStatus": null
            }))
            .unwrap()
        });
        GitSnapshot {
            git: Some(git),
            pr,
            ..GitSnapshot::default()
        }
    }

    #[test]
    fn only_local_changes_with_no_settled_pull_request_are_sent() {
        assert_eq!(sendable(&snapshot(false, None)), None, "nothing changed");
        assert_eq!(
            sendable(&snapshot(true, None)),
            Some(false),
            "no pull request yet"
        );
        assert_eq!(sendable(&snapshot(true, Some("OPEN"))), Some(true));
        assert_eq!(sendable(&snapshot(true, Some("MERGED"))), None);
        let mut merging = snapshot(true, None);
        merging.git.as_mut().unwrap().merge_in_progress = true;
        assert_eq!(sendable(&merging), None);
        let mut local = snapshot(true, None);
        local.git.as_mut().unwrap().has_remote = false;
        assert_eq!(sendable(&local), None);
    }
}
