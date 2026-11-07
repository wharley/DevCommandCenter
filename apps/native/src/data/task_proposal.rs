//! A task proposal's card answers: "Começar", with the brief as the person
//! left it, and "Descartar" (`/api/v1/task-proposals/:id/...`). Blocking:
//! call it off the UI thread.

use dcc_core::domain::attention::SessionActivityInput;
use dcc_core::domain::delegation::Delegation;
use dcc_core::domain::workspace::WorkspaceId;
use dcc_infra::db::SqliteSessionRepo;
use serde::Deserialize;
use serde_json::json;

use crate::data::delegation_record::ChildSession;
use crate::data::desktop_api::{ApiError, DesktopApi, encode};

/// Where the started task lives: the parent's task for a conversation of
/// its own, or a task of its own in another project (`dcc_open_task`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartedTask {
    pub workspace_id: String,
    pub child_session_id: Option<String>,
}

impl DesktopApi {
    pub fn start_task_proposal(
        &self,
        id: &str,
        brief: Option<&str>,
    ) -> Result<StartedTask, ApiError> {
        let value = self.post(
            &format!("/api/v1/task-proposals/{}/start", encode(id)),
            &json!({ "brief": brief }),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    pub fn dismiss_task_proposal(&self, id: &str) -> Result<(), ApiError> {
        self.post(
            &format!("/api/v1/task-proposals/{}/dismiss", encode(id)),
            &json!({}),
        )
        .map(|_| ())
    }
}

/// The conversations of the tasks an agent opened in other projects, read
/// from those tasks, so their cards here know when they work or wait.
pub fn own_task_children(
    sessions: &SqliteSessionRepo,
    delegations: &[Delegation],
) -> Vec<(String, ChildSession)> {
    delegations
        .iter()
        .filter_map(|delegation| {
            let task = delegation.budget.new_task.as_ref()?.workspace_id.clone()?;
            let child = delegation.child_session_id.as_ref()?;
            let summaries = sessions.list_workspace_sessions(&WorkspaceId(task)).ok()?;
            let summary = summaries
                .iter()
                .find(|summary| summary.session.id == *child)?;
            let activity = SessionActivityInput::from_summary(summary);
            Some((
                child.0.clone(),
                ChildSession {
                    title: summary.thread.title.clone(),
                    busy: activity.is_running(),
                    waiting: activity.waiting_reason().is_some(),
                },
            ))
        })
        .collect()
}
