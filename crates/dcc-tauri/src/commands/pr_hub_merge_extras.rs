//! Around the hub's merge of a GitHub pull request: whether the repository
//! lets GitHub merge it when it becomes ready, whether that is already on,
//! whether its branch can be deleted afterwards (its own repository's, never
//! a fork's); and the actions beside the merge: turning "merge when ready"
//! off, updating the branch with the base, closing, reopening, and deleting
//! the branch of a closed one.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::commands::forge::context as forge_context;
use crate::commands::forge_commands::ForgeCliProvider;
use crate::state::WorkspaceCommandState;

/// What the pull request and its repository say about the merge's options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeExtras {
    /// The repository allows GitHub's auto-merge.
    pub allow_auto_merge: bool,
    /// GitHub will merge it once its requirements are met.
    pub auto_merge_enabled: bool,
    /// The head branch is in this repository and GitHub does not already
    /// delete it on merge.
    pub can_delete_branch: bool,
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn merge_extras(pull_request: &Value, repository: &Value) -> MergeExtras {
    MergeExtras {
        allow_auto_merge: flag(repository, "allow_auto_merge"),
        auto_merge_enabled: pull_request
            .get("autoMergeRequest")
            .is_some_and(|request| !request.is_null()),
        can_delete_branch: !flag(pull_request, "isCrossRepository")
            && !flag(repository, "delete_branch_on_merge")
            && head_branch(pull_request).is_some(),
    }
}

pub fn head_branch(pull_request: &Value) -> Option<&str> {
    pull_request
        .get("headRefName")
        .and_then(Value::as_str)
        .filter(|branch| !branch.trim().is_empty())
}

/// What the person asked of a pull request, besides its merge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PullRequestHubAction {
    /// Turn "merge when ready" off.
    DisableAutoMerge,
    /// Merge the base into its branch, while its head is the commit seen.
    UpdateBranch {
        expected_head_sha: String,
    },
    /// Close it without merging.
    Close {
        comment: Option<String>,
        #[serde(default)]
        delete_branch: bool,
    },
    Reopen,
    /// Delete the branch of one already closed or merged.
    DeleteBranch,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubActionInput {
    pub repository_root: String,
    pub number: u32,
    pub forge_login: Option<String>,
    pub action: PullRequestHubAction,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubActionOutput {
    /// The branch was deleted (asked with the close, or by itself).
    pub branch_deleted: bool,
}

/// One of those actions on a GitHub pull request (the HTTP API's caller).
/// What GitHub refuses comes back as its own message.
pub async fn pull_request_hub_action_with_state(
    state: &WorkspaceCommandState,
    input: PullRequestHubActionInput,
) -> Result<PullRequestHubActionOutput, String> {
    use dcc_infra::forge::github_merge as github;
    let root = input.repository_root.trim();
    if root.is_empty() {
        return Err("repository_root is empty".to_string());
    }
    let context = forge_context::resolve_workspace_forge_context(
        &state.db_path,
        root,
        input.forge_login.as_deref(),
    )?
    .ok_or_else(|| "No GitHub or GitLab remote was found.".to_string())?;
    if context.provider != ForgeCliProvider::Github {
        return Err("This is supported only for GitHub pull requests.".to_string());
    }
    let (host, namespace, repo) = (&context.host, &context.namespace, &context.repo);
    let login = context.effective_login.as_deref();
    // Its own repository's branch, never a fork's; read at the time of asking.
    let delete_branch = || -> Result<(), String> {
        let pull_request = crate::commands::forge::github::pull_request_detail_json(
            root,
            host,
            input.number,
            login,
        )?;
        if pull_request.get("state").and_then(Value::as_str) == Some("OPEN") {
            return Err("The branch of an open pull request is not deleted.".to_string());
        }
        if flag(&pull_request, "isCrossRepository") {
            return Err("The branch is in a fork and is not deleted from here.".to_string());
        }
        let branch = head_branch(&pull_request)
            .ok_or_else(|| "GitHub did not return the pull request's branch.".to_string())?;
        github::delete_head_branch(root, host, namespace, repo, branch, login)
    };
    let mut output = PullRequestHubActionOutput::default();
    match &input.action {
        PullRequestHubAction::DisableAutoMerge => {
            github::disable_auto_merge(root, host, namespace, repo, input.number, login)?
        }
        PullRequestHubAction::UpdateBranch { expected_head_sha } => {
            let expected = expected_head_sha.trim();
            if expected.is_empty() || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(
                    "The pull request's commit is unavailable. Refresh and try again.".to_string(),
                );
            }
            github::update_branch(root, host, namespace, repo, input.number, expected, login)?
        }
        PullRequestHubAction::Close {
            comment,
            delete_branch: with_branch,
        } => {
            github::close_pull_request(
                root,
                host,
                namespace,
                repo,
                input.number,
                comment.as_deref(),
                login,
            )?;
            // The close stands even when the branch could not go.
            output.branch_deleted = *with_branch && delete_branch().is_ok();
        }
        PullRequestHubAction::Reopen => {
            github::reopen_pull_request(root, host, namespace, repo, input.number, login)?
        }
        PullRequestHubAction::DeleteBranch => {
            delete_branch()?;
            output.branch_deleted = true;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_forks_branch_is_never_deleted_and_auto_merge_follows_the_repository() {
        let own = json!({ "headRefName": "feat/pay", "isCrossRepository": false, "autoMergeRequest": null });
        let repo = json!({ "allow_auto_merge": true, "delete_branch_on_merge": false });
        assert_eq!(
            merge_extras(&own, &repo),
            MergeExtras {
                allow_auto_merge: true,
                auto_merge_enabled: false,
                can_delete_branch: true
            }
        );
        let fork = json!({ "headRefName": "feat/pay", "isCrossRepository": true });
        assert!(!merge_extras(&fork, &repo).can_delete_branch);
        // GitHub already deletes it: nothing to offer.
        let cleans = json!({ "allow_auto_merge": false, "delete_branch_on_merge": true });
        let extras = merge_extras(&own, &cleans);
        assert!(!extras.can_delete_branch && !extras.allow_auto_merge);
        let queued = json!({ "headRefName": "x", "autoMergeRequest": { "mergeMethod": "SQUASH" } });
        assert!(merge_extras(&queued, &repo).auto_merge_enabled);
    }

    #[test]
    fn an_action_reads_as_the_app_sends_it() {
        let input: PullRequestHubActionInput = serde_json::from_value(json!({
            "repositoryRoot": "/repo", "number": 7, "forgeLogin": null,
            "action": { "kind": "close", "comment": "dup", "deleteBranch": true }
        }))
        .unwrap();
        assert_eq!(
            input.action,
            PullRequestHubAction::Close {
                comment: Some("dup".into()),
                delete_branch: true
            }
        );
        let update: PullRequestHubAction =
            serde_json::from_value(json!({ "kind": "updateBranch", "expectedHeadSha": "abc" }))
                .unwrap();
        assert_eq!(
            update,
            PullRequestHubAction::UpdateBranch {
                expected_head_sha: "abc".into()
            }
        );
        let reopen: PullRequestHubAction =
            serde_json::from_value(json!({ "kind": "reopen" })).unwrap();
        assert_eq!(reopen, PullRequestHubAction::Reopen);
    }
}
