//! The desktop's side of `dcc_infra::workspace_deletion`: the memory
//! checkpoint, the git coordinator and the delivery failures this process holds.

use std::future::Future;
use std::path::{Path, PathBuf};

use dcc_core::domain::workspace::WorkspaceId;
use dcc_infra::workspace_deletion::DeletionRuntime;

use super::workspace_commands::workspace_mutation_error;
use crate::state::WorkspaceCommandState;

impl DeletionRuntime for WorkspaceCommandState {
    fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    fn checkpoint_memory(
        &self,
        workspace_ids: &[WorkspaceId],
    ) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            self.checkpoint_ai_memory_for_workspaces(workspace_ids, true)
                .await
                .map_err(|error| error.to_string())
        }
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
        async move {
            self.run_git_workspace_mutation_blocking(root, operation)
                .await
                .map_err(workspace_mutation_error)
        }
    }

    fn git_mutation_for_workspace<T, F>(
        &self,
        workspace_id: &WorkspaceId,
        root: &str,
        operation: F,
    ) -> impl Future<Output = Result<T, String>> + Send
    where
        T: Send + 'static,
        F: FnOnce(&Path) -> Result<T, String> + Send + 'static,
    {
        async move {
            self.run_git_workspace_mutation_for_workspace_blocking(workspace_id, root, operation)
                .await
                .map_err(workspace_mutation_error)
        }
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
        async move {
            self.run_git_workspace_pair_mutation_blocking(root, secondary, operation)
                .await
                .map_err(workspace_mutation_error)
        }
    }

    fn clear_delivery_failures(&self, root: &str) {
        WorkspaceCommandState::clear_delivery_failures(self, root);
    }
}
