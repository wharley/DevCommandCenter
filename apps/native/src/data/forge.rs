//! What the review's forge sections read from the desktop: the checkout's
//! forge context and accounts, the pull request's review state and
//! reviewers, the last delivery failure. Only the fields the native app
//! shows (the desktop's types live in `dcc-tauri`).

use serde::Deserialize;

/// `WorkspaceForgeContextOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ForgeContext {
    /// `github` | `gitlab`
    pub provider: Option<String>,
    pub host: Option<String>,
    /// `ok` | `unauthenticated` | `unavailable`
    pub remote_state: Option<String>,
    pub bound_login: Option<String>,
    pub effective_login: Option<String>,
}

impl ForgeContext {
    /// `forgeConnected`
    pub fn connected(&self) -> bool {
        self.remote_state.as_deref() == Some("ok")
    }
}

/// `ForgeCliAccountEntry`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ForgeAccount {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ForgeAccounts {
    pub accounts: Vec<ForgeAccount>,
}

/// `WorkspaceReviewer`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Reviewer {
    pub login: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    /// `pending`, `approved`, `changes_requested`, `commented`…
    pub state: String,
    pub is_team: bool,
}

/// `WorkspaceReviewStateOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewState {
    pub provider: Option<String>,
    pub number: Option<u32>,
    pub url: Option<String>,
    pub review_state: Option<String>,
    pub reviewers: Vec<Reviewer>,
    pub reviewers_available: bool,
    pub approvals_available: bool,
    pub approvals_required: Option<u32>,
    pub approvals_received: u32,
    pub approvals_left: Option<u32>,
    pub mergeable: Option<String>,
    pub has_conflicts: Option<bool>,
    pub behind_by: Option<u32>,
    pub discussions_resolved: Option<bool>,
}

/// `WorkspaceReviewerCandidate`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewerCandidate {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub suggested: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewerCandidates {
    pub candidates: Vec<ReviewerCandidate>,
}

/// `groupReviewerCandidates`: those not on the review yet, suggested first.
pub fn group_candidates<'a>(
    candidates: &'a [ReviewerCandidate],
    excluded: &[String],
) -> (Vec<&'a ReviewerCandidate>, Vec<&'a ReviewerCandidate>) {
    let excluded: Vec<String> = excluded.iter().map(|login| login.to_lowercase()).collect();
    candidates
        .iter()
        .filter(|candidate| !excluded.contains(&candidate.login.to_lowercase()))
        .partition(|candidate| candidate.suggested)
}

/// `reviewerRowAction`: withdraw a pending ask, or (GitHub) ask again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewerAction {
    Request,
    Rerequest,
    Remove,
}

pub fn reviewer_row_action(reviewer: &Reviewer, provider: Option<&str>) -> Option<ReviewerAction> {
    if reviewer.login.is_none() || reviewer.is_team {
        return None;
    }
    if reviewer.state == "pending" {
        return Some(ReviewerAction::Remove);
    }
    (provider == Some("github")).then_some(ReviewerAction::Rerequest)
}

/// `{ remote, branch }` of the push that failed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PushTarget {
    pub remote: String,
    pub branch: String,
}

/// `WorkspaceDeliveryFailureSnapshot`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeliveryFailure {
    pub attempt_token: String,
    pub workspace_root: String,
    pub branch: Option<String>,
    pub head_sha: Option<String>,
    /// `push`, `fetch`, `pull`, `pipeline`…
    pub operation: String,
    pub classification: String,
    pub remote: Option<String>,
    pub operation_target: Option<String>,
    pub push_target: Option<PushTarget>,
    pub output: String,
    pub output_truncated: bool,
    pub changed_files: Vec<String>,
    pub changed_files_truncated: bool,
    pub external_url: Option<String>,
    /// `retry`, `synchronize`, `send-to-agent`, `open-external`
    pub available_actions: Vec<String>,
    pub created_at: String,
}

impl DeliveryFailure {
    pub fn offers(&self, action: &str) -> bool {
        self.available_actions
            .iter()
            .any(|offered| offered == action)
    }

    /// The request "Enviar ao agente" puts in the composer.
    pub fn agent_prompt(&self) -> String {
        dcc_core::domain::delivery_failure_prompt::build_delivery_failure_prompt(
            &dcc_core::domain::delivery_failure_prompt::DeliveryFailurePrompt {
                operation: &self.operation,
                classification: &self.classification,
                branch: self.branch.as_deref(),
                head_sha: self.head_sha.as_deref(),
                remote: self.remote.as_deref(),
                operation_target: self.operation_target.as_deref(),
                push_target: self
                    .push_target
                    .as_ref()
                    .map(|target| (target.remote.as_str(), target.branch.as_str())),
                created_at: &self.created_at,
                attempt_token: &self.attempt_token,
                output: &self.output,
                changed_files: &self.changed_files,
                changed_files_truncated: self.changed_files_truncated,
            },
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeliveryFailureOutput {
    pub snapshot: Option<DeliveryFailure>,
}

/// `WorkspaceDeliveryRecoveryOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Recovered {
    pub snapshot: DeliveryFailure,
    pub refresh_pipeline: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reviewer(login: &str, state: &str) -> Reviewer {
        Reviewer {
            login: Some(login.into()),
            state: state.into(),
            ..Reviewer::default()
        }
    }

    #[test]
    fn a_reviewer_row_offers_what_the_forge_allows() {
        assert_eq!(
            reviewer_row_action(&reviewer("ana", "pending"), Some("gitlab")),
            Some(ReviewerAction::Remove)
        );
        assert_eq!(
            reviewer_row_action(&reviewer("ana", "approved"), Some("github")),
            Some(ReviewerAction::Rerequest)
        );
        assert_eq!(
            reviewer_row_action(&reviewer("ana", "approved"), Some("gitlab")),
            None
        );
        let team = Reviewer {
            is_team: true,
            ..reviewer("core", "pending")
        };
        assert_eq!(reviewer_row_action(&team, Some("github")), None);
    }

    #[test]
    fn candidates_leave_out_who_is_on_the_review() {
        let candidate = |login: &str, suggested: bool| ReviewerCandidate {
            login: login.into(),
            suggested,
            ..ReviewerCandidate::default()
        };
        let all = vec![
            candidate("Ana", true),
            candidate("bia", false),
            candidate("caio", true),
        ];
        let (suggested, others) = group_candidates(&all, &["ana".to_string()]);
        assert_eq!(
            suggested
                .iter()
                .map(|c| c.login.as_str())
                .collect::<Vec<_>>(),
            vec!["caio"]
        );
        assert_eq!(
            others.iter().map(|c| c.login.as_str()).collect::<Vec<_>>(),
            vec!["bia"]
        );
    }

    #[test]
    fn a_failure_reads_as_the_desktop_sends_it() {
        let failure: DeliveryFailureOutput = serde_json::from_str(
            r#"{"snapshot":{"attemptToken":"t","workspaceRoot":"/r","branch":"b","headSha":"abc","operation":"push","classification":"authentication","remote":"origin","operationTarget":null,"pushTarget":{"remote":"origin","branch":"b","url":null},"output":"denied","outputTruncated":false,"changedFiles":["a.rs"],"changedFilesTruncated":false,"externalUrl":null,"availableActions":["retry","send-to-agent"],"createdAt":"2026-10-24T00:00:00Z"}}"#,
        )
        .unwrap();
        let failure = failure.snapshot.unwrap();
        assert!(failure.offers("send-to-agent"));
        assert!(!failure.offers("synchronize"));
        assert!(failure.agent_prompt().contains("Push target: origin/b"));
    }
}
