//! Deleting tasks and projects from the native app, through the same
//! `dcc_infra::workspace_deletion` the desktop runs. Whoever hosts the
//! backend owns the agents, terminals and git coordinator: hosting it here,
//! the host's runtime deletes; otherwise every write first takes the runtime
//! owner lock, and with the desktop open it is refused. Blocking: run off the
//! UI thread.

use std::future::Future;
use std::path::{Path, PathBuf};

use dcc_core::domain::repository::RepositoryId;
use dcc_core::domain::workspace::WorkspaceId;
use dcc_core::domain::workspace_bundle::WorkspaceBundleId;
use dcc_core::ports::WorkspaceRepo;
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use dcc_infra::runtime_owner::RuntimeOwner;
use dcc_infra::workspace_cleanup::{self, WorkspaceCleanupScanOutput};
use dcc_infra::workspace_deletion::{
    self as deletion, DeleteBundleRequest, DeleteTaskRequest, DeletionRuntime,
    UnappliedDelegationEdits, WorkspaceRemoteBranchDeletionTarget,
};
use futures::executor::block_on;

/// Why a deletion did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteError {
    /// The desktop is open and owns the checkouts.
    DesktopOpen,
    /// The task's conversations are not in ai-memory yet; only the desktop
    /// runs the memory server that saves them.
    MemoryPending,
    /// Child tasks hold edits that never reached this task (nothing removed).
    UnappliedEdits(Vec<UnappliedDelegationEdits>),
    Failed(String),
}

/// Marks the memory refusal inside the orchestration's error text.
const MEMORY_PENDING: &str = "native: ai-memory export pending";

/// The native app's side of a deletion. It runs only while holding the
/// runtime owner lock, so no other process changes these checkouts and git
/// changes run directly.
pub struct NativeRuntime {
    db_path: PathBuf,
    app_data_dir: PathBuf,
    memory_on: bool,
}

impl NativeRuntime {
    pub fn new(db_path: &Path) -> Self {
        let app_data_dir = db_path.parent().map(Path::to_path_buf).unwrap_or_default();
        let memory_on = memory_enabled(&app_data_dir);
        Self {
            db_path: db_path.to_path_buf(),
            app_data_dir,
            memory_on,
        }
    }
}

/// Mirrors when the desktop runs ai-memory: the mode saved in its settings,
/// otherwise on in release builds (`AiMemorySidecar::start`).
fn memory_enabled(app_data_dir: &Path) -> bool {
    let mode = std::fs::read_to_string(app_data_dir.join("ai-memory-settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| {
            value
                .get("mode")
                .and_then(|mode| mode.as_str())
                .map(str::to_string)
        });
    match mode.as_deref() {
        Some("disabled") => false,
        Some(_) => true,
        None => {
            !cfg!(debug_assertions)
                || std::env::var("DCC_AI_MEMORY_URL").is_ok_and(|url| !url.trim().is_empty())
        }
    }
}

impl DeletionRuntime for NativeRuntime {
    fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    /// The desktop exports the conversations now; here they must already be
    /// exported, or the deletion waits for the desktop.
    fn checkpoint_memory(
        &self,
        workspace_ids: &[WorkspaceId],
    ) -> impl Future<Output = Result<(), String>> + Send {
        let result = if self.memory_on {
            SqliteSessionRepo::open(&self.db_path)
                .and_then(|repo| repo.sessions_missing_memory_export(workspace_ids))
                .map_err(|error| error.to_string())
                .and_then(|missing| {
                    if missing.is_empty() {
                        Ok(())
                    } else {
                        Err(MEMORY_PENDING.to_string())
                    }
                })
        } else {
            Ok(())
        };
        std::future::ready(result)
    }

    fn git_mutation<T, F>(
        &self,
        root: &str,
        operation: F,
    ) -> impl Future<Output = Result<T, String>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Path) -> Result<T, String> + Send + 'static,
    {
        std::future::ready(operation(Path::new(root)))
    }

    fn git_mutation_for_workspace<T, F>(
        &self,
        _workspace_id: &WorkspaceId,
        root: &str,
        operation: F,
    ) -> impl Future<Output = Result<T, String>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Path) -> Result<T, String> + Send + 'static,
    {
        std::future::ready(operation(Path::new(root)))
    }

    fn git_pair_mutation<T, F>(
        &self,
        root: &str,
        secondary: PathBuf,
        operation: F,
    ) -> impl Future<Output = Result<T, String>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Path, &Path) -> Result<T, String> + Send + 'static,
    {
        std::future::ready(operation(Path::new(root), &secondary))
    }

    /// The native app keeps no delivery failures in memory.
    fn clear_delivery_failures(&self, _root: &str) {}
}

fn classify(error: String) -> DeleteError {
    if error.contains(MEMORY_PENDING) {
        return DeleteError::MemoryPending;
    }
    match deletion::parse_unapplied_delegation_edits(&error) {
        Some(list) => DeleteError::UnappliedEdits(list),
        None => DeleteError::Failed(error),
    }
}

/// Runs `write` holding the runtime owner lock, released when it returns.
fn exclusive<T>(
    db_path: &Path,
    write: impl FnOnce(&NativeRuntime) -> Result<T, String>,
) -> Result<T, DeleteError> {
    let runtime = NativeRuntime::new(db_path);
    let owner = RuntimeOwner::try_acquire(&runtime.app_data_dir)
        .map_err(|error| DeleteError::Failed(error.to_string()))?;
    let Some(_owner) = owner else {
        return Err(DeleteError::DesktopOpen);
    };
    write(&runtime).map_err(classify)
}

/// How the person confirmed deleting a task.
#[derive(Clone, Debug, Default)]
pub struct DeleteTask {
    pub task_id: String,
    pub bundle_id: Option<String>,
    pub delete_remote_branches: bool,
    pub remote_targets: Vec<WorkspaceRemoteBranchDeletionTarget>,
    /// Asked again after the dialog listed the child tasks' edits.
    pub discard_child_edits: bool,
}

pub fn delete_task(db_path: &Path, task: &DeleteTask) -> Result<(), DeleteError> {
    // Hosting the backend, this process owns the checkouts: the host's own
    // runtime deletes, as the desktop's commands do.
    if let Some(host) = crate::data::host::current() {
        return crate::data::host::block_on(delete_task_with(&host.workspace, task))
            .unwrap_or_else(|| Err("the backend is not running".into()))
            .map_err(classify);
    }
    exclusive(db_path, |runtime| block_on(delete_task_with(runtime, task)))
}

async fn delete_task_with<R: DeletionRuntime>(
    runtime: &R,
    task: &DeleteTask,
) -> Result<(), String> {
    match &task.bundle_id {
        Some(bundle_id) => {
            deletion::delete_workspace_bundle(
                runtime,
                DeleteBundleRequest {
                    bundle_id: bundle_id.clone(),
                    delete_remote_branches: task.delete_remote_branches,
                    expected_remote_targets: task.remote_targets.clone(),
                },
            )
            .await
        }
        None => {
            deletion::delete_workspace(
                runtime,
                DeleteTaskRequest {
                    workspace_id: task.task_id.clone(),
                    delete_remote_branch: task.delete_remote_branches,
                    expected_remote_target: task.remote_targets.first().cloned(),
                    discard_delegation_changes: task.discard_child_edits,
                },
            )
            .await
        }
    }
}

/// Removes a project from DCC with its tasks (`delete_repository`); the
/// folder on disk stays.
pub fn remove_project(db_path: &Path, repository_id: &str) -> Result<(), DeleteError> {
    if let Some(host) = crate::data::host::current() {
        return crate::data::host::block_on(remove_project_with(
            &host.workspace,
            db_path,
            repository_id,
        ))
        .unwrap_or_else(|| Err("the backend is not running".into()))
        .map_err(classify);
    }
    exclusive(db_path, |runtime| {
        block_on(remove_project_with(runtime, db_path, repository_id))
    })
}

async fn remove_project_with<R: DeletionRuntime>(
    runtime: &R,
    db_path: &Path,
    repository_id: &str,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(db_path).map_err(|error| error.to_string())?;
    let session_repo = SqliteSessionRepo::open(db_path).map_err(|error| error.to_string())?;
    deletion::delete_repository_with_workspaces(
        runtime,
        &repo,
        &session_repo,
        &RepositoryId(repository_id.to_string()),
        db_path,
    )
    .await
    .map(drop)
}

/// The remote branches the deletion dialog offers to remove with the task:
/// a multi-project task's are those of the worktrees it created.
pub fn remote_targets(
    db_path: &Path,
    task_id: &str,
    bundle_id: Option<&str>,
) -> Vec<WorkspaceRemoteBranchDeletionTarget> {
    let Ok(repo) = SqliteWorkspaceRepo::open(db_path) else {
        return Vec::new();
    };
    let workspaces = match bundle_id {
        Some(bundle_id) => block_on(deletion::bundle_created_workspaces(
            &repo,
            &WorkspaceBundleId(bundle_id.to_string()),
        ))
        .unwrap_or_default()
        .into_iter()
        .filter(|workspace| workspace.worktree_path.is_some())
        .collect(),
        None => block_on(repo.get_workspace(&WorkspaceId(task_id.to_string())))
            .ok()
            .flatten()
            .into_iter()
            .collect::<Vec<_>>(),
    };
    deletion::remote_deletion_targets(&workspaces)
}

/// Size and safety of completed tasks' worktrees (read only).
pub fn cleanup_scan(
    db_path: &Path,
    task_ids: Vec<String>,
) -> Result<WorkspaceCleanupScanOutput, String> {
    let targets = block_on(workspace_cleanup::cleanup_targets(db_path, task_ids))?;
    workspace_cleanup::measure_cleanup(targets)
}
