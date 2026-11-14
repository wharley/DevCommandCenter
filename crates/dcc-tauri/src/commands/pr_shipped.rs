//! Whether a task's merged pull request is live
//! (`dcc_infra::forge::github_shipped`): asked by the app for a task an
//! agent opened, after its pull request merged, to tell the agent the work
//! started with. GitHub only; another forge answers that it cannot tell.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::commands::forge::context as forge_context;
use crate::commands::forge_commands::ForgeCliProvider;
use crate::state::WorkspaceCommandState;
use dcc_infra::forge::github_read::RepoSlug;
use dcc_infra::forge::github_read_run::GithubTarget;
use dcc_infra::forge::github_shipped::{read_shipped, ShippedRead};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestShippedInput {
    /// The task's checkout.
    pub workspace_root: String,
    pub number: u32,
    #[serde(default)]
    pub forge_login: Option<String>,
}

/// Where the pull request stands on its way out.
pub async fn pull_request_shipped_with_state(
    state: &WorkspaceCommandState,
    input: PullRequestShippedInput,
) -> Result<ShippedRead, String> {
    let root = input.workspace_root.trim().to_string();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        &root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
    if context.provider != ForgeCliProvider::Github {
        return Err("Deployments are read from GitHub only.".to_string());
    }
    let target = GithubTarget {
        host: context.host,
        repo: RepoSlug {
            owner: context.namespace,
            name: context.repo,
        },
        login: context.effective_login,
    };
    let number = u64::from(input.number);
    tokio::task::spawn_blocking(move || read_shipped(&target, number))
        .await
        .map_err(|error| error.to_string())?
}
