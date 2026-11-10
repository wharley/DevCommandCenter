//! The tasks agents opened that are running now, from what the sidebar
//! already read: the team room shows each under its room's conversation and
//! the agent stays "Trabalhando" until they end, so a task that is still
//! working is not mistaken for a finished one.

use dcc_core::domain::agent_conversation::home_agent_id;

use super::Sidebar;
use crate::data::agents::AgentState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningTask {
    pub task_id: String,
    pub name: String,
    pub agent_id: String,
    /// The agent's conversation that opened it.
    pub session_id: String,
    /// The task's project, for its room.
    pub project_id: String,
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
            .map(|(task, opened)| {
                let summary = data.tasks.iter().find(|summary| summary.id == *task);
                (task, opened, summary)
            })
            .map(|(task, opened, summary)| RunningTask {
                task_id: task.clone(),
                name: summary
                    .map(|summary| summary.name.clone())
                    .unwrap_or_default(),
                project_id: summary
                    .map(|summary| summary.project_id.clone())
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

/// An agent at work in a task's conversation right now, however it got
/// there (a trigger, a call from the task's header, a pass).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAtWork {
    pub agent_id: String,
    pub task_id: String,
    pub task_name: String,
    pub session_id: String,
    pub project_id: String,
    pub started_at: Option<String>,
}

impl Sidebar {
    pub fn agents_at_work(&self) -> Vec<AgentAtWork> {
        let data = &self.state.data;
        data.agents
            .iter()
            .flat_map(|agent| agent.sessions.iter().map(move |session| (agent, session)))
            .filter(|(_, session)| {
                session.state == AgentState::Working && home_agent_id(&session.project_id).is_none()
            })
            .filter_map(|(agent, session)| {
                let task = data
                    .tasks
                    .iter()
                    .find(|task| task.id == session.workspace_id)?;
                Some(AgentAtWork {
                    agent_id: agent.id.clone(),
                    task_id: task.id.clone(),
                    task_name: task.name.clone(),
                    session_id: session.session_id.clone(),
                    project_id: task.project_id.clone(),
                    started_at: data
                        .activities
                        .get(&task.id)
                        .and_then(|activity| activity.activity.started_at.clone()),
                })
            })
            .collect()
    }
}
