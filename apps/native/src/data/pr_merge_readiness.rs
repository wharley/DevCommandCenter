//! Whether a pull request can be merged now, and what it waits on when it
//! cannot: GitHub's own verdict (`mergeStateStatus`) read with the review
//! decision and the checks, so the hub says why before the forge refuses.
//! A pull request that waits can still be set to merge when it is ready,
//! where the repository allows GitHub's auto-merge.

use serde_json::json;

use super::desktop_api::{ApiError, DesktopApi};
use super::pr_hub::{Item, MergeCapabilities};

/// What stands between a pull request and its merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    ChecksRunning,
    ChecksFailing,
    ReviewRequired,
    ChangesRequested,
    /// Its branch is behind the base and the repository asks it up to date.
    Behind,
    /// A rule of the repository GitHub does not name.
    Blocked,
}

impl Wait {
    pub fn key(self) -> &'static str {
        match self {
            Self::ChecksRunning => "pullRequests.merge.wait.checksRunning",
            Self::ChecksFailing => "pullRequests.merge.wait.checksFailing",
            Self::ReviewRequired => "pullRequests.merge.wait.reviewRequired",
            Self::ChangesRequested => "pullRequests.merge.wait.changesRequested",
            Self::Behind => "pullRequests.merge.wait.behind",
            Self::Blocked => "pullRequests.merge.wait.blocked",
        }
    }
}

/// `None` when GitHub would merge it now (or does not say).
pub fn waiting_on(item: &Item, merge: &MergeCapabilities) -> Option<Wait> {
    match merge.merge_state_status.as_deref()? {
        "BEHIND" => Some(Wait::Behind),
        "BLOCKED" => Some(match item.review_decision.as_deref() {
            Some("CHANGES_REQUESTED") => Wait::ChangesRequested,
            _ if item.checks_state == "failure" => Wait::ChecksFailing,
            _ if item.checks_state == "pending" => Wait::ChecksRunning,
            Some("REVIEW_REQUIRED") => Wait::ReviewRequired,
            _ => Wait::Blocked,
        }),
        // CLEAN, HAS_HOOKS, UNSTABLE (only optional checks fail), UNKNOWN.
        _ => None,
    }
}

/// What the merge dialog offers for a pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergePath {
    /// Merge it now.
    Now,
    /// It waits on something: GitHub merges it when that clears.
    WhenReady(Wait),
    /// It waits and the repository has no auto-merge: nothing to do here.
    Held(Wait),
}

pub fn merge_path(item: &Item, merge: &MergeCapabilities) -> MergePath {
    match waiting_on(item, merge) {
        None => MergePath::Now,
        Some(wait) if merge.allow_auto_merge => MergePath::WhenReady(wait),
        Some(wait) => MergePath::Held(wait),
    }
}

/// What the person asks of a pull request besides its merge
/// (`PullRequestHubAction`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrAction {
    DisableAutoMerge,
    /// Merge the base into its branch, while its head is the commit seen.
    UpdateBranch {
        head_sha: String,
    },
    Close {
        comment: String,
        delete_branch: bool,
    },
    Reopen,
    /// The branch of one already closed or merged.
    DeleteBranch,
}

impl PrAction {
    fn body(&self) -> serde_json::Value {
        match self {
            Self::DisableAutoMerge => json!({ "kind": "disableAutoMerge" }),
            Self::UpdateBranch { head_sha } => {
                json!({ "kind": "updateBranch", "expectedHeadSha": head_sha })
            }
            Self::Close {
                comment,
                delete_branch,
            } => {
                json!({ "kind": "close", "comment": comment, "deleteBranch": delete_branch })
            }
            Self::Reopen => json!({ "kind": "reopen" }),
            Self::DeleteBranch => json!({ "kind": "deleteBranch" }),
        }
    }
}

impl DesktopApi {
    /// Runs it on the forge; `true` when a branch was deleted with it.
    pub fn pr_hub_action(&self, item: &Item, action: &PrAction) -> Result<bool, ApiError> {
        let value = self.post(
            "/api/v1/pull-request-hub/action",
            &json!({
                "repositoryRoot": item.repository_root,
                "number": item.number,
                "forgeLogin": item.forge_login,
                "action": action.body(),
            }),
        )?;
        Ok(value
            .get("branchDeleted")
            .and_then(serde_json::Value::as_bool)
            == Some(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(review: Option<&str>, checks: &str) -> Item {
        Item {
            review_decision: review.map(str::to_string),
            checks_state: checks.into(),
            ..Item::default()
        }
    }

    fn merge(status: Option<&str>, auto: bool) -> MergeCapabilities {
        MergeCapabilities {
            merge_state_status: status.map(str::to_string),
            allow_auto_merge: auto,
            ..MergeCapabilities::default()
        }
    }

    #[test]
    fn a_pull_request_says_what_it_waits_on() {
        let blocked = merge(Some("BLOCKED"), false);
        let cases = [
            (
                item(Some("CHANGES_REQUESTED"), "failure"),
                Wait::ChangesRequested,
            ),
            (item(Some("APPROVED"), "failure"), Wait::ChecksFailing),
            (
                item(Some("REVIEW_REQUIRED"), "pending"),
                Wait::ChecksRunning,
            ),
            (
                item(Some("REVIEW_REQUIRED"), "success"),
                Wait::ReviewRequired,
            ),
            (item(Some("APPROVED"), "success"), Wait::Blocked),
        ];
        for (item, wait) in cases {
            assert_eq!(waiting_on(&item, &blocked), Some(wait));
        }
        assert_eq!(
            waiting_on(&item(None, "success"), &merge(Some("BEHIND"), false)),
            Some(Wait::Behind)
        );
        // Ready, only optional checks failing, or GitHub does not say: no wait.
        for status in [Some("CLEAN"), Some("UNSTABLE"), Some("UNKNOWN"), None] {
            assert_eq!(
                waiting_on(&item(None, "failure"), &merge(status, false)),
                None
            );
        }
    }

    #[test]
    fn an_action_is_sent_as_the_desktop_reads_it() {
        assert_eq!(
            PrAction::UpdateBranch {
                head_sha: "abc".into()
            }
            .body(),
            json!({ "kind": "updateBranch", "expectedHeadSha": "abc" })
        );
        assert_eq!(
            PrAction::Close {
                comment: "dup".into(),
                delete_branch: true
            }
            .body(),
            json!({ "kind": "close", "comment": "dup", "deleteBranch": true })
        );
        assert_eq!(PrAction::Reopen.body(), json!({ "kind": "reopen" }));
    }

    #[test]
    fn waiting_merges_when_ready_only_where_the_repository_allows() {
        let waits = item(Some("REVIEW_REQUIRED"), "success");
        assert_eq!(
            merge_path(&waits, &merge(Some("BLOCKED"), true)),
            MergePath::WhenReady(Wait::ReviewRequired)
        );
        assert_eq!(
            merge_path(&waits, &merge(Some("BLOCKED"), false)),
            MergePath::Held(Wait::ReviewRequired)
        );
        assert_eq!(
            merge_path(&waits, &merge(Some("CLEAN"), true)),
            MergePath::Now
        );
    }
}
