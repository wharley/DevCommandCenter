//! The task's branch against its remote, beyond the base update: the base
//! fetched ahead of a new task ("Nova tarefa"), the task's own branch
//! fetched to know what someone else pushed, and "Puxar alterações" to bring
//! it in. Built on the parent's git helpers and its sync.

use serde::Deserialize;

use super::{
    git_output_err, normalize_base_branch_for_sync, preflight_workspace_root,
    remote_branch_fetch_refspec, resolve_current_branch_name, resolve_default_branch_name,
    resolve_default_remote_name, run_git_network_output_with_workspace_auth, run_git_output,
    sync_workspace_branch, validate_branch_for_fetch, SyncIntegration, WorkspaceCommandState,
    WorkspaceGitSyncBaseOutput,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFetchBaseInput {
    pub repository_root: String,
    pub base_branch: Option<String>,
    pub forge_login: Option<String>,
}

/// Brings a project's base branch from its remote into the remote-tracking
/// ref and nothing else: no checkout, merge or local branch moves. "Nova
/// tarefa" asks for it when it opens, so the task created a moment later
/// starts from the base as the remote has it (`task_start_point`) without
/// the creation waiting on the network.
pub async fn workspace_fetch_base_with_state(
    state: &WorkspaceCommandState,
    input: WorkspaceFetchBaseInput,
) -> Result<(), String> {
    let root = input.repository_root.trim().to_string();
    if root.is_empty() {
        return Err("repository_root is empty".to_string());
    }
    let db_path = state.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let remote = resolve_default_remote_name(&root)?;
        let base_branch = input
            .base_branch
            .as_deref()
            .and_then(|branch| normalize_base_branch_for_sync(branch, &remote))
            .or_else(|| {
                resolve_default_branch_name(&root)
                    .ok()
                    .and_then(|branch| normalize_base_branch_for_sync(&branch, &remote))
            })
            .unwrap_or_else(|| "main".to_string());
        validate_branch_for_fetch(&root, &base_branch)?;
        let refspec = remote_branch_fetch_refspec(&remote, &base_branch);
        let fetch = run_git_network_output_with_workspace_auth(
            &db_path,
            &root,
            &["fetch", "--quiet", &remote, &refspec],
            input.forge_login.as_deref(),
        )?;
        if fetch.status.success() {
            Ok(())
        } else {
            Err(git_output_err("git fetch", &fetch.stderr))
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The remote and branch the checkout's own branch follows: its configured
/// upstream, else the same name on the default remote. A task still on a
/// detached commit has none.
pub(super) fn branch_upstream(root: &str) -> Result<(String, String), String> {
    let branch = resolve_current_branch_name(root)?;
    if branch == "HEAD" {
        return Err("this task has no branch of its own yet".to_string());
    }
    let config = |key: String| {
        run_git_output(root, &["config", "--get", &key])
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let remote = match config(format!("branch.{branch}.remote")).filter(|remote| remote != ".") {
        Some(remote) => remote,
        None => resolve_default_remote_name(root)?,
    };
    let upstream = config(format!("branch.{branch}.merge"))
        .and_then(|merge| merge.strip_prefix("refs/heads/").map(str::to_string))
        .unwrap_or(branch);
    Ok((remote, upstream))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGitPullInput {
    pub workspace_root: String,
    pub forge_login: Option<String>,
}

/// "Puxar alterações": what someone else pushed to this task's own branch,
/// brought in — rebased onto when the checkout is clean and the branch
/// follows its own name (its local commits on top are then the unpushed
/// ones), else merged, so commits already pushed are never rewritten;
/// conflicts go to the resolver like any base update. The task's base is not
/// touched.
pub async fn workspace_git_pull_with_state(
    state: &WorkspaceCommandState,
    input: WorkspaceGitPullInput,
) -> Result<WorkspaceGitSyncBaseOutput, String> {
    preflight_workspace_root(state, &input.workspace_root).await?;
    let root = input.workspace_root.trim().to_string();
    let lookup = root.clone();
    let (remote, branch, own) = tokio::task::spawn_blocking(move || {
        let (remote, branch) = branch_upstream(&lookup)?;
        let own = resolve_current_branch_name(&lookup)? == branch;
        Ok::<_, String>((remote, branch, own))
    })
    .await
    .map_err(|error| error.to_string())??;
    sync_workspace_branch(
        state,
        &root,
        Some(&branch),
        Some(&remote),
        false,
        pull_integration(own),
        input.forge_login.as_deref(),
        None,
        None,
    )
    .await
}

/// Rebase only onto the branch's own remote copy; anything else merges.
pub(super) fn pull_integration(follows_its_own_name: bool) -> SyncIntegration {
    if follows_its_own_name {
        SyncIntegration::PreferRebase
    } else {
        SyncIntegration::Merge
    }
}

/// The task's own branch fetched from its remote, refs only, so the header
/// can tell how many commits someone else pushed (`behind_of_remote_count`).
pub async fn workspace_fetch_upstream_with_state(
    state: &WorkspaceCommandState,
    input: WorkspaceGitPullInput,
) -> Result<(), String> {
    let root = input.workspace_root.trim().to_string();
    let db_path = state.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let (remote, branch) = branch_upstream(&root)?;
        validate_branch_for_fetch(&root, &branch)?;
        let refspec = remote_branch_fetch_refspec(&remote, &branch);
        let fetch = run_git_network_output_with_workspace_auth(
            &db_path,
            &root,
            &["fetch", "--quiet", &remote, &refspec],
            input.forge_login.as_deref(),
        )?;
        if fetch.status.success() {
            Ok(())
        } else {
            Err(git_output_err("git fetch", &fetch.stderr))
        }
    })
    .await
    .map_err(|error| error.to_string())?
}
