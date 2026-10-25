//! The merge conflict resolver's calls to the desktop
//! (`POST /api/v1/git/{conflict-state,accept-conflict,mark-conflict-resolved,
//! abort-merge,validation-config,complete-merge}`): git runs there, in the
//! checkout's mutation queue.

use dcc_core::domain::merge_conflict::{
    WorkspaceGitCompleteMergeOutput, WorkspaceGitConflictSide, WorkspaceGitConflictStateOutput,
    WorkspaceGitValidationConfigOutput,
};
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi};

impl DesktopApi {
    pub fn conflict_state(&self, root: &str) -> Result<WorkspaceGitConflictStateOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/git/conflict-state",
            &json!({ "workspaceRoot": root }),
        )?)
    }

    /// One side for the whole file, as git applies it (`workspaceGitAcceptConflict`).
    pub fn accept_conflict(
        &self,
        root: &str,
        path: &str,
        side: WorkspaceGitConflictSide,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/git/accept-conflict",
            &json!({ "workspaceRoot": root, "relativePath": path, "side": side }),
        )
        .map(|_| ())
    }

    /// The file staged as resolved, or removed with `delete`.
    pub fn mark_conflict_resolved(
        &self,
        root: &str,
        path: &str,
        delete: bool,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/git/mark-conflict-resolved",
            &json!({ "workspaceRoot": root, "relativePath": path, "delete": delete }),
        )
        .map(|_| ())
    }

    pub fn abort_merge(&self, root: &str) -> Result<(), ApiError> {
        self.post("/api/v1/git/abort-merge", &json!({ "workspaceRoot": root }))
            .map(|_| ())
    }

    /// The project's `before_merge` commands (`workspaceGitValidationConfig`).
    pub fn validation_config(
        &self,
        root: &str,
    ) -> Result<WorkspaceGitValidationConfigOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/git/validation-config",
            &json!({ "workspaceRoot": root }),
        )?)
    }

    /// Runs the validation, then commits and pushes the merge.
    pub fn complete_merge(
        &self,
        root: &str,
        login: Option<&str>,
        config: &WorkspaceGitValidationConfigOutput,
    ) -> Result<WorkspaceGitCompleteMergeOutput, ApiError> {
        Self::decoded(self.post(
            "/api/v1/git/complete-merge",
            &json!({
                "workspaceRoot": root,
                "forgeLogin": login,
                "validationConfigHash": config.config_hash,
                "validationCommands": config.commands,
            }),
        )?)
    }
}
