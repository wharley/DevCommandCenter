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

/// Whether the task's own conversation (the one the agent opened) runs; a
/// task whose conversation is not known yet counts as its running state.
fn own_turn_runs(
    opened: &crate::data::task_agents::OpenedTask,
    activity: Option<&crate::data::activity::TaskActivity>,
) -> bool {
    match opened.task_session_id.as_deref() {
        Some(own) => activity.is_some_and(|activity| activity.session_open(own)),
        None => true,
    }
}

impl Sidebar {
    pub fn running_agent_tasks(&self) -> Vec<RunningTask> {
        let running = self.state.running_ids();
        let data = &self.state.data;
        let mut tasks: Vec<RunningTask> = data
            .opened_tasks
            .iter()
            .filter(|(task, _)| running.contains(*task))
            // Its own conversation runs, not only another agent called into
            // the task: that one shows in its own name (`agents_at_work`).
            .filter(|(task, opened)| own_turn_runs(opened, data.activities.get(*task)))
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

#[cfg(test)]
mod own_turn_tests {
    use super::own_turn_runs;
    use crate::data::activity::TaskActivity;
    use crate::data::task_agents::OpenedTask;
    use dcc_core::domain::attention::{AgentActivity, AgentActivityState};

    #[test]
    fn a_task_runs_as_its_opener_only_while_its_own_conversation_runs() {
        let opened = OpenedTask {
            agent_id: "dev".into(),
            session_id: "s-geral".into(),
            task_session_id: Some("s-task".into()),
        };
        let running = |ids: &[&str]| TaskActivity {
            activity: AgentActivity {
                state: AgentActivityState::Active,
                started_at: None,
                completed_at: None,
                waiting_since: None,
                usage_limit: None,
            },
            last_interaction_at: None,
            waiting_session_ids: Vec::new(),
            running_session_ids: ids.iter().map(|id| id.to_string()).collect(),
            completed_by_session: Vec::new(),
        };
        assert!(own_turn_runs(&opened, Some(&running(&["s-task"]))));
        // The Reviewer answering a review in the Dev's task: not the Dev.
        assert!(!own_turn_runs(&opened, Some(&running(&["s-revisor"]))));
        assert!(!own_turn_runs(&opened, None));
        let unknown = OpenedTask {
            task_session_id: None,
            ..opened
        };
        assert!(own_turn_runs(&unknown, Some(&running(&["s-revisor"]))));
    }
}
