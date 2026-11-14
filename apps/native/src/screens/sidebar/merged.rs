//! The tasks whose pull request merged, as the sidebar's rows read it: for
//! the shell to tell the agent the work started with (`shell::shipped`).

use super::Sidebar;
use crate::data::recap::PrState;
use crate::data::tasks::TaskSummary;

/// A task with a merged pull request.
#[derive(Clone, Debug, PartialEq)]
pub struct MergedTask {
    pub id: String,
    pub name: String,
    /// Where its branch is checked out.
    pub checkout: String,
    pub number: u32,
}

impl Sidebar {
    /// Every task whose own pull request merged.
    pub fn merged_tasks(&self) -> Vec<MergedTask> {
        self.state
            .data
            .tasks
            .iter()
            .filter_map(|task: &TaskSummary| {
                let meta = self.state.row_git(task).meta?;
                (meta.pr_state == Some(PrState::Merged)).then_some(())?;
                Some(MergedTask {
                    id: task.id.clone(),
                    name: task.name.clone(),
                    checkout: task
                        .worktree_path
                        .clone()
                        .unwrap_or_else(|| task.root_path.clone()),
                    number: meta.pr_number?,
                })
            })
            .collect()
    }
}
