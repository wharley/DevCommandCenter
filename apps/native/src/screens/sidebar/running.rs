//! The turns running now, for the agents' check-in (`data::check_ins`):
//! each task's running conversations with when the turn started, read from
//! what the sidebar already loaded (no read of its own).

use super::Sidebar;
use crate::data::check_ins::Running;

impl Sidebar {
    /// Running turns, their last sign of life still to read.
    pub fn running_turns(&self) -> Vec<Running> {
        let data = &self.state.data;
        data.tasks
            .iter()
            .filter_map(|task| {
                let activity = data.activities.get(&task.id)?;
                activity.is_running().then_some((task, activity))
            })
            .flat_map(|(task, activity)| {
                let project = data
                    .projects
                    .iter()
                    .find(|project| project.project_id == task.project_id)
                    .map(|project| project.display_name.clone())
                    .unwrap_or_default();
                let started = activity.activity.started_at.clone().unwrap_or_default();
                activity
                    .running_session_ids
                    .iter()
                    .map(move |session| Running {
                        workspace_id: task.id.clone(),
                        session_id: session.clone(),
                        title: task.name.clone(),
                        project: project.clone(),
                        project_id: task.project_id.clone(),
                        started_at: started.clone(),
                        last_event_at: None,
                    })
            })
            .collect()
    }
}
