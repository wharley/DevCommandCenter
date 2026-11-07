//! How a trigger asks the Reviewer something in a task: in its one
//! conversation there (`reviewer-convergence`), started and bound to it the
//! first time, the turn read-only on the Reviewer's provider and model.
//! Blocking: call `ask` off the UI thread.

use crate::data::agents::AgentRecord;
use crate::data::conversation::ThreadStart;
use crate::data::desktop_api::{DesktopApi, TurnOptions};
use crate::data::pr_review::AgentRun;

/// The Reviewer's conversation in `task`, when it has one (one per task).
pub(super) fn conversation_in(reviewer: &AgentRecord, task: &str) -> Option<String> {
    reviewer
        .sessions
        .iter()
        .filter(|session| session.workspace_id == task)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
        .map(|session| session.session_id.clone())
}

/// Everything `ask` needs, taken on the UI thread.
pub(super) struct TaskAsk {
    pub agent_id: String,
    pub agent_name: String,
    /// Its conversation in the task, when it has one.
    pub existing: Option<String>,
    pub start: ThreadStart,
    pub run: AgentRun,
}

impl TaskAsk {
    pub(super) fn new(reviewer: &AgentRecord, task: &str, project: &str, run: AgentRun) -> Self {
        Self {
            agent_id: reviewer.id.clone(),
            agent_name: reviewer.name.clone(),
            existing: conversation_in(reviewer, task),
            start: ThreadStart {
                workspace_id: task.to_string(),
                project_id: project.to_string(),
                additional_workspace_ids: Vec::new(),
                name: None,
                auto_named: false,
            },
            run,
        }
    }

    fn options(&self) -> TurnOptions {
        TurnOptions {
            provider_id: Some(self.run.provider_id.clone()),
            model: self.run.model.clone(),
            effort: self.run.effort.clone().or_else(|| Some("medium".into())),
            approval_policy: Some("read_only".into()),
            plan_mode: false,
            fast_mode: false,
            tool_instructions: None,
            provider_runtime: self.run.provider_runtime.clone(),
        }
    }

    /// Sends `prompt` as a read-only turn of the Reviewer's conversation.
    pub(super) fn ask(&self, api: &DesktopApi, prompt: &str) -> Result<(), String> {
        let session = match &self.existing {
            Some(session) => session.clone(),
            None => {
                let session = api
                    .start_thread(
                        &self.start,
                        &self.run.provider_id,
                        self.run.model.as_deref(),
                        Some(&self.agent_name),
                    )
                    .map_err(|error| error.to_string())?;
                if let Err(error) = api.bind_session_agent(&session, &self.agent_id) {
                    // Unbound it is a plain conversation with no role.
                    let _ = api.close_session(&session, true);
                    return Err(error.to_string());
                }
                session
            }
        };
        api.send_turn(&session, prompt, None, &self.options())
            .map_err(|error| error.to_string())
    }
}
