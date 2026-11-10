//! Which agent each task belongs to, for its mascot on the sidebar's row
//! (Monocode shows whose session it is): the agent that opened it from its
//! own conversation (`dcc_open_task`, a delegation whose new task was
//! started), or the agent working in one of the task's conversations now.
//! Built from what the sidebar already reads; nothing new is stored.

use std::collections::HashMap;

use dcc_core::domain::agent_conversation::home_agent_id;
use dcc_core::domain::delegation::Delegation;

use super::agents::{AgentRecord, AgentState};

/// A task an agent opened from one of its conversations, and from which:
/// the team room shows it running in that conversation's room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedTask {
    pub agent_id: String,
    pub session_id: String,
    /// The task's own conversation, where its turns run.
    pub task_session_id: Option<String>,
}

/// Task id → who opened it and from which conversation.
pub fn opened_tasks(
    agents: &[AgentRecord],
    delegations: &[Delegation],
) -> HashMap<String, OpenedTask> {
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
        .filter_map(|delegation| {
            let task = delegation.budget.new_task.as_ref()?.workspace_id.clone()?;
            let parent = delegation.parent_session_id.0.as_str();
            let agent = owner.get(parent)?;
            Some((
                task,
                OpenedTask {
                    agent_id: agent.to_string(),
                    session_id: parent.to_string(),
                    task_session_id: delegation.child_session_id.as_ref().map(|id| id.0.clone()),
                },
            ))
        })
        .collect()
}

/// Task id → agent id.
pub fn by_task(agents: &[AgentRecord], delegations: &[Delegation]) -> HashMap<String, String> {
    let owner: HashMap<&str, &str> = agents
        .iter()
        .flat_map(|agent| {
            agent
                .sessions
                .iter()
                .map(move |session| (session.session_id.as_str(), agent.id.as_str()))
        })
        .collect();
    let mut tasks = HashMap::new();
    for delegation in delegations {
        let started = delegation
            .budget
            .new_task
            .as_ref()
            .and_then(|target| target.workspace_id.as_deref());
        let opener = owner.get(delegation.parent_session_id.0.as_str());
        if let (Some(task), Some(agent)) = (started, opener) {
            tasks.insert(task.to_string(), agent.to_string());
        }
    }
    // An agent called into a task's conversation: the one at work shows,
    // else it shows where no one opened the task.
    for agent in agents {
        for session in &agent.sessions {
            if home_agent_id(&session.project_id).is_some() {
                continue;
            }
            if session.state == AgentState::Working || !tasks.contains_key(&session.workspace_id) {
                tasks.insert(session.workspace_id.clone(), agent.id.clone());
            }
        }
    }
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::agents::AgentSession;

    fn session(id: &str, task: &str, project: &str, state: AgentState) -> AgentSession {
        AgentSession {
            session_id: id.into(),
            workspace_id: task.into(),
            state,
            completed_at: None,
            project_id: project.into(),
            title: None,
            updated_at: "2026-11-09T10:00:00Z".into(),
            room: None,
        }
    }

    fn agent(id: &str, sessions: Vec<AgentSession>) -> AgentRecord {
        AgentRecord {
            id: id.into(),
            name: id.into(),
            preset: None,
            avatar: dcc_core::domain::agent_proposal::mascot_for(id),
            provider_id: None,
            model: None,
            effort: None,
            kickoff_prompt: String::new(),
            role: String::new(),
            offer_prompt: String::new(),
            extra_instructions: String::new(),
            projects: Vec::new(),
            monthly_token_limit: None,
            tool_access: Default::default(),
            auto_start_tasks: false,
            may_call: Vec::new(),
            account_id: None,
            sessions,
        }
    }

    fn opened(parent: &str, task: Option<&str>) -> Delegation {
        serde_json::from_value(serde_json::json!({
            "id": format!("d-{parent}"),
            "parentSessionId": parent,
            "parentTurnId": null,
            "childSessionId": "child",
            "workspaceId": "w-home",
            "targetProviderId": "claude_code",
            "mode": "implement",
            "status": "running",
            "prompt": "",
            "contextPolicy": { "type": "minimal" },
            "budget": {
                "turnLimit": null, "timeoutSeconds": null, "allowFileEdits": true,
                "newTask": {
                    "projectId": "p", "project": "Checkout", "rootPath": "/w", "baseBranch": "main",
                    "worktree": true, "workspaceId": task
                }
            },
            "resultSummary": null,
            "diffSummary": null,
            "validationSummary": null,
            "createdAt": "2026-11-09T10:00:00Z",
            "updatedAt": "2026-11-09T10:00:00Z",
            "kind": "task"
        }))
        .expect("delegation")
    }

    #[test]
    fn a_task_shows_the_agent_that_opened_it_or_works_in_it() {
        let agents = vec![
            agent(
                "suporte",
                vec![session(
                    "s-home",
                    "w-home",
                    "dcc-agent-suporte",
                    AgentState::Idle,
                )],
            ),
            agent(
                "rev",
                vec![session("s-rev", "w-shopify", "p", AgentState::Working)],
            ),
        ];
        let tasks = by_task(
            &agents,
            &[opened("s-home", Some("w-campo")), opened("s-home", None)],
        );
        assert_eq!(tasks.get("w-campo").map(String::as_str), Some("suporte"));
        // At work in a task right now.
        assert_eq!(tasks.get("w-shopify").map(String::as_str), Some("rev"));
        // A proposal not started yet has no task, and a home is no task.
        assert_eq!(tasks.len(), 2);
        // Who opened which, and from which conversation.
        let by_opener = opened_tasks(&agents, &[opened("s-home", Some("w-campo"))]);
        assert_eq!(
            by_opener.get("w-campo"),
            Some(&OpenedTask {
                agent_id: "suporte".into(),
                session_id: "s-home".into(),
                // The task's own conversation, where its turns run.
                task_session_id: Some("child".into()),
            })
        );
    }
}
