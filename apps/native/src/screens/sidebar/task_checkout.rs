//! What the room's change card reads of a task from the sidebar: its
//! checkout, whether it is open or runs a turn, and the git and pull
//! request state the rail already keeps (`git_watch`) — so the card itself
//! never runs git for the status.

use super::Sidebar;
use crate::data::git_watch::GitSnapshot;
use crate::data::tasks::TaskStatus;

pub(crate) struct TaskCheckout<'a> {
    /// The worktree, or the project root for a task without one.
    pub root: &'a str,
    pub branch: &'a str,
    pub status: TaskStatus,
    /// An agent turn is open in it (running or waiting on an approval).
    pub turn_open: bool,
    /// None until the rail read it, and for a closed task, which it skips.
    pub snapshot: Option<&'a GitSnapshot>,
}

impl Sidebar {
    /// None for a task the sidebar does not list (deleted).
    pub(crate) fn task_checkout(&self, id: &str) -> Option<TaskCheckout<'_>> {
        let task = self.state.task(id)?;
        Some(TaskCheckout {
            root: task.worktree_path.as_deref().unwrap_or(&task.root_path),
            branch: &task.branch,
            status: task.status,
            turn_open: self
                .state
                .data
                .activities
                .get(id)
                .is_some_and(|activity| activity.is_turn_open()),
            snapshot: self.state.git.get(id),
        })
    }

    #[cfg(test)]
    pub(crate) fn set_git_for_tests(
        &mut self,
        id: &str,
        snapshot: GitSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        self.state.git.insert(id.to_string(), snapshot);
        cx.notify();
    }
}
