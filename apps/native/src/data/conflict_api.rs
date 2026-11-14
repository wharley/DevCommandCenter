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

    /// The resolver's agent: one read-only run in the checkout, with no
    /// task and no conversation; its final text. Waits as long as the run
    /// may take (five minutes there).
    pub fn conflict_agent(
        &self,
        root: &str,
        provider_id: &str,
        model: Option<&str>,
        prompt: &str,
        run_id: &str,
    ) -> Result<String, ApiError> {
        let runtime = crate::data::provider_runtime::for_provider(provider_id);
        let value = self.request_waiting(
            "POST",
            "/api/v1/git/conflict-agent",
            Some(&json!({
                "workspaceRoot": root,
                "providerId": provider_id,
                "model": model,
                "providerRuntime": runtime,
                "effort": "medium",
                "prompt": prompt,
                "runId": run_id,
            })),
            std::time::Duration::from_secs(330),
        )?;
        Ok(value
            .get("response")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    /// Stops that run by the name it was started with.
    pub fn cancel_conflict_agent(&self, run_id: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/git/conflict-agent/cancel",
            &json!({ "runId": run_id }),
        )
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
