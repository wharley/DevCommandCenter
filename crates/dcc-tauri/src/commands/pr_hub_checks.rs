//! A check of a pull request, looked at from the hub: what failed in its
//! GitHub Actions job (the failing step's excerpt the task's delivery
//! already reads), and running that job again. Both are asked for one
//! check, by its link, when the person clicks; nothing is read ahead.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::commands::forge::context as forge_context;
use crate::commands::forge_commands::ForgeCliProvider;
use crate::state::WorkspaceCommandState;
use dcc_infra::forge::{github_actions_job, github_checks};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubCheckInput {
    pub repository_root: String,
    pub forge_login: Option<String>,
    /// The check's link, as the detail gave it.
    pub details_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubCheckLog {
    pub excerpt: String,
}

const NOT_A_JOB: &str = "This check is not a GitHub Actions job: open it on its own page.";

fn github_context(
    state: &WorkspaceCommandState,
    input: &PullRequestHubCheckInput,
) -> Result<(forge_context::ResolvedWorkspaceForgeContext, u64), String> {
    let root = input.repository_root.trim();
    if root.is_empty() {
        return Err("repository_root is empty".to_string());
    }
    let (_, job) =
        github_actions_job::run_and_job(&input.details_url).ok_or_else(|| NOT_A_JOB.to_string())?;
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
    if context.provider != ForgeCliProvider::Github {
        return Err(NOT_A_JOB.to_string());
    }
    Ok((context, job))
}

/// The failing step of the check's job (the HTTP API's caller).
pub async fn pull_request_hub_check_log_with_state(
    state: &WorkspaceCommandState,
    input: PullRequestHubCheckInput,
) -> Result<PullRequestHubCheckLog, String> {
    let (context, job) = github_context(state, &input)?;
    let log = crate::commands::forge::github::failed_job_log(
        input.repository_root.trim(),
        &context.host,
        job,
        context.effective_login.as_deref(),
    )?;
    Ok(PullRequestHubCheckLog {
        excerpt: github_checks::failing_step_excerpt(&log),
    })
}

/// Runs the check's job again (the HTTP API's caller).
pub async fn pull_request_hub_check_rerun_with_state(
    state: &WorkspaceCommandState,
    input: PullRequestHubCheckInput,
) -> Result<(), String> {
    let (context, job) = github_context(state, &input)?;
    github_actions_job::rerun_job(
        input.repository_root.trim(),
        &context.host,
        &context.namespace,
        &context.repo,
        job,
        context.effective_login.as_deref(),
    )
}
