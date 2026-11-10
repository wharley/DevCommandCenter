//! The tasks agents opened that are running now, from what the sidebar
//! already read: the team room shows each under its room's conversation and
//! the agent stays "Trabalhando" until they end, so a task that is still
//! working is not mistaken for a finished one.

use super::Sidebar;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningTask {
    pub task_id: String,
    pub name: String,
    pub agent_id: String,
    /// The agent's conversation that opened it.
    pub session_id: String,
    /// When its running turn started.
    pub started_at: Option<String>,
}

impl Sidebar {
    pub fn running_agent_tasks(&self) -> Vec<RunningTask> {
        let running = self.state.running_ids();
        let data = &self.state.data;
        let mut tasks: Vec<RunningTask> = data
            .opened_tasks
            .iter()
            .filter(|(task, _)| running.contains(*task))
            .map(|(task, opened)| RunningTask {
                task_id: task.clone(),
                name: data
                    .tasks
                    .iter()
                    .find(|summary| summary.id == *task)
                    .map(|summary| summary.name.clone())
                    .unwrap_or_default(),
                agent_id: opened.agent_id.clone(),
                session_id: opened.session_id.clone(),
                started_at: data
                    .activities
                    .get(task)
                    .and_then(|activity| activity.activity.started_at.clone()),
            })
            .collect();
        tasks.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        tasks
    }
}
