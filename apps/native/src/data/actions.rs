//! The task writes the sidebar makes, through the same `dcc_infra`
//! functions the desktop's commands call. Blocking: run off the UI thread.

use std::path::Path;

use chrono::Utc;
use dcc_core::domain::workspace_bundle::{WorkspaceBundleId, WorkspaceBundleState};
use dcc_core::ports::WorkspaceBundleRepo;
use dcc_infra::db::SqliteWorkspaceRepo;
use dcc_infra::workspace_actions as actions;
use futures::executor::block_on;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskAction {
    Rename(String),
    SetPinned(bool),
    /// An RFC 3339 instant, or `None` to wake the task.
    Snooze(Option<String>),
    /// "Em espera".
    Archive,
    Complete,
    Restore,
}

impl TaskAction {
    /// The i18n key of the toast the desktop shows when this fails (Tauri
    /// rejects with a plain string, so the desktop never shows the backend's
    /// text). Moving between sections has none: the backend text is shown.
    pub fn error_key(&self) -> Option<&'static str> {
        match self {
            Self::Rename(_) => Some("sidebar.renameWorkspaceError"),
            Self::SetPinned(_) => Some("sidebar.pinWorkspaceError"),
            Self::Snooze(_) => Some("sidebar.snooze.error"),
            Self::Archive | Self::Complete | Self::Restore => None,
        }
    }
}

/// Runs `action` on a task; a multi-project task changes state as a whole.
/// Unlike the desktop, completing does not checkpoint the task's memory:
/// that runs in the desktop process (its outbox catches up).
pub fn run(
    db_path: &Path,
    task_id: &str,
    bundle_id: Option<&str>,
    action: &TaskAction,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    let id = task_id.to_string();
    let bundle_state = match action {
        TaskAction::Archive => Some(WorkspaceBundleState::Archived),
        TaskAction::Complete => Some(WorkspaceBundleState::Completed),
        TaskAction::Restore => Some(WorkspaceBundleState::Ready),
        _ => None,
    };
    if let (Some(bundle_id), Some(state)) = (bundle_id, bundle_state) {
        let bundle_id = WorkspaceBundleId(bundle_id.to_string());
        return block_on(repo.set_workspace_bundle_state(
            &bundle_id,
            state,
            Utc::now().to_rfc3339(),
        ))
        .map_err(|error| error.to_string())?
        .map(|_| ())
        .ok_or_else(|| format!("workspace bundle not found: {}", bundle_id.0));
    }
    block_on(async {
        match action {
            TaskAction::Rename(name) => actions::rename_workspace(&repo, id, name).await.map(drop),
            TaskAction::SetPinned(pinned) => actions::set_workspace_pinned(&repo, id, *pinned)
                .await
                .map(drop),
            TaskAction::Snooze(until) => actions::set_workspace_snooze(&repo, id, until.as_deref())
                .await
                .map(drop),
            TaskAction::Archive => actions::archive_workspace(&repo, id).await.map(drop),
            TaskAction::Complete => actions::complete_workspace(&repo, id).await.map(drop),
            TaskAction::Restore => actions::restore_workspace(&repo, id).await.map(drop),
        }
    })
}
