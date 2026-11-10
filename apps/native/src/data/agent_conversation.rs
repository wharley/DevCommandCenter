//! A resident agent's own conversation (`dcc_core::domain::agent_conversation`),
//! as its page starts it: the home folder from the desktop, a task there on
//! the first "Conversar" (out of the task list), and a conversation bound to
//! the agent. Blocking: call it off the UI thread.

use dcc_core::application::{CreateWorkspaceForRepoInput, WorkspaceIsolationMode};
use dcc_core::domain::project::ProjectId;
use serde::Deserialize;
use serde_json::json;

use crate::data::conversation::ThreadStart;
use crate::data::desktop_api::{ApiError, DesktopApi, encode};

/// The task is a resident agent's own conversation (no code, no git).
pub fn is_agent_home(project_id: Option<&str>) -> bool {
    project_id
        .and_then(dcc_core::domain::agent_conversation::home_agent_id)
        .is_some()
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHome {
    pub root_path: String,
    pub project_id: String,
}

/// What the agent runs on and how its conversation is named.
pub struct ConversationStart<'a> {
    pub agent_id: &'a str,
    pub agent_name: &'a str,
    /// Its home task from an earlier conversation, when it has one.
    pub home_task: Option<&'a str>,
    pub title: &'a str,
    pub provider: &'a str,
    pub model: Option<&'a str>,
    /// What the conversation starts with, its account included
    /// (`provider_runtime::for_agent`).
    pub runtime: Option<dcc_core::ports::provider::ProviderRuntimeConfig>,
    /// The project whose room the conversation is in; none: the Geral room.
    pub room: Option<&'a str>,
}

impl DesktopApi {
    pub fn agent_home(&self, agent_id: &str) -> Result<AgentHome, ApiError> {
        let value = self.post(
            &format!("/api/v1/agents/{}/conversation", encode(agent_id)),
            &json!({}),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// The agent's conversation: its home task (made the first time) and a
    /// new session there bound to the agent. Answers `(task, session)`.
    pub fn start_agent_conversation(
        &self,
        start: &ConversationStart<'_>,
    ) -> Result<(String, String), ApiError> {
        let home = self.agent_home(start.agent_id)?;
        let task_id = match start.home_task {
            Some(task) => task.to_string(),
            None => {
                self.create_workspace(&CreateWorkspaceForRepoInput {
                    project_id: ProjectId(home.project_id.clone()),
                    workspace_root: home.root_path.clone(),
                    base_branch: "main".into(),
                    name: Some(start.title.to_string()),
                    isolation_mode: Some(WorkspaceIsolationMode::LocalDirect),
                    context_folders: Vec::new(),
                })?
                .workspace_id
            }
        };
        let thread = ThreadStart {
            workspace_id: task_id.clone(),
            project_id: home.project_id,
            additional_workspace_ids: Vec::new(),
            name: Some(start.title.to_string()),
            auto_named: false,
        };
        let session = self.start_thread_with(
            &thread,
            start.provider,
            start.model,
            Some(start.agent_name),
            start.runtime.clone(),
        )?;
        if let Err(error) = self.bind_session_agent_in(&session, start.agent_id, start.room) {
            // Without the binding it would be a plain session with no role.
            let _ = self.close_session(&session, true);
            return Err(error);
        }
        Ok((task_id, session))
    }
}
