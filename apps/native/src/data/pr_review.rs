//! A pull request's code tab (`PullRequestCodeReview`): its files with the
//! patches, the conversations on lines, what the forge lets the person do,
//! the patch read into rows (`parseUnifiedDiff`), the comments the person
//! drafts, and the calls that send the review, reply to a conversation and
//! resolve it.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};
use super::pr_hub::{Actor, Item};

/// The desktop gives the reviewer's turn five minutes
/// (`run_ephemeral_read_only_turn`); a little more, so its own answer — or
/// its own "took too long" — arrives instead of the app giving up first.
const REVIEW_WAIT: std::time::Duration = std::time::Duration::from_secs(330);

/// `PullRequestHubFile`
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct File {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
}

/// `PullRequestHubInlineComment`
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InlineComment {
    pub id: String,
    pub thread_id: Option<String>,
    pub path: String,
    pub line: Option<u32>,
    pub side: Option<String>,
    pub body: String,
    pub author: Option<Actor>,
    pub resolved: Option<bool>,
}

/// `PullRequestHubReviewCapabilities`
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewCapabilities {
    pub inline_comments: bool,
    pub approve: bool,
    pub request_changes: bool,
    pub reply_to_threads: bool,
    pub resolve_threads: bool,
}

/// `PullRequestHubDraftComment`: a comment on a line, sent with the review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DraftComment {
    pub path: String,
    pub body: String,
    pub line: u32,
    pub side: &'static str,
}

impl DraftComment {
    pub fn key(&self) -> String {
        draft_key(&self.path, self.line, self.side)
    }
}

/// `draftKey`
pub fn draft_key(path: &str, line: u32, side: &str) -> String {
    format!("{path}:{side}:{line}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Addition,
    Deletion,
    Context,
    Hunk,
    Meta,
}

/// `UnifiedDiffLine`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchRow {
    pub kind: RowKind,
    pub content: String,
    pub old: Option<u32>,
    pub new: Option<u32>,
    /// The line a comment lands on, and its side ("left" or "right").
    pub review: Option<(u32, &'static str)>,
}

fn hunk_starts(raw: &str) -> Option<(u32, u32)> {
    // `^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@`
    let rest = raw.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let start = |range: &str| -> Option<u32> {
        let (first, count) = range.split_once(',').unwrap_or((range, "0"));
        let digits = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());
        digits(count).then_some(())?;
        digits(first).then(|| first.parse().ok()).flatten()
    };
    Some((start(old)?, start(new)?))
}

/// `parseUnifiedDiff`
pub fn parse_patch(patch: Option<&str>) -> Vec<PatchRow> {
    let Some(patch) = patch.filter(|patch| !patch.is_empty()) else {
        return Vec::new();
    };
    let (mut old, mut new) = (0u32, 0u32);
    patch
        .split('\n')
        .map(|raw| {
            let row = |kind, content: &str, numbers: (Option<u32>, Option<u32>), review| PatchRow {
                kind,
                content: content.to_string(),
                old: numbers.0,
                new: numbers.1,
                review,
            };
            if let Some((first_old, first_new)) = hunk_starts(raw) {
                old = first_old;
                new = first_new;
                return row(RowKind::Hunk, raw, (None, None), None);
            }
            if let Some(content) = raw.strip_prefix('+').filter(|_| !raw.starts_with("+++")) {
                let line = new;
                new += 1;
                return row(
                    RowKind::Addition,
                    content,
                    (None, Some(line)),
                    Some((line, "right")),
                );
            }
            if let Some(content) = raw.strip_prefix('-').filter(|_| !raw.starts_with("---")) {
                let line = old;
                old += 1;
                return row(
                    RowKind::Deletion,
                    content,
                    (Some(line), None),
                    Some((line, "left")),
                );
            }
            if let Some(content) = raw.strip_prefix(' ') {
                let (previous, current) = (old, new);
                old += 1;
                new += 1;
                return row(
                    RowKind::Context,
                    content,
                    (Some(previous), Some(current)),
                    Some((current, "right")),
                );
            }
            row(RowKind::Meta, raw, (None, None), None)
        })
        .collect()
}

/// `PullRequestHubReviewEvent`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReviewEvent {
    #[default]
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewEvent {
    pub const ALL: [ReviewEvent; 3] = [Self::Comment, Self::Approve, Self::RequestChanges];

    pub fn key(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Approve => "approve",
            Self::RequestChanges => "request_changes",
        }
    }
}

/// Why an event is off: the forge can't, it is the person's own pull
/// request, or a draft.
pub fn event_blocked(
    event: ReviewEvent,
    item: &Item,
    capabilities: Option<&ReviewCapabilities>,
) -> Option<&'static str> {
    let unsupported = match event {
        ReviewEvent::Approve => !capabilities.is_some_and(|caps| caps.approve),
        ReviewEvent::RequestChanges => !capabilities.is_some_and(|caps| caps.request_changes),
        ReviewEvent::Comment => false,
    };
    let decision = event != ReviewEvent::Comment;
    if unsupported {
        Some("pullRequests.code.unsupportedReviewAction")
    } else if item.created_by_viewer && decision {
        Some("pullRequests.code.ownPullRequestRestriction")
    } else if item.is_draft && decision {
        Some("pullRequests.code.draftRestriction")
    } else {
        None
    }
}

/// `canSubmit`: approving alone is enough; otherwise a comment or a body,
/// and asking for changes says why.
pub fn can_submit(event: ReviewEvent, drafts: usize, body: &str, pending: bool) -> bool {
    let has_body = !body.trim().is_empty();
    !pending
        && (event == ReviewEvent::Approve || drafts > 0 || has_body)
        && (event != ReviewEvent::RequestChanges || has_body)
}

/// `PullRequestHubSubmitReviewOutput`
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SubmitResult {
    pub submitted: bool,
    pub submitted_comment_count: u32,
    pub body_submitted: bool,
    pub decision_submitted: bool,
    pub warning: Option<String>,
}

/// `RunPullRequestReviewAgentInput`
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentRun {
    pub working_directory: String,
    pub provider_id: String,
    pub model: Option<String>,
    pub provider_runtime: Option<dcc_core::ports::provider::ProviderRuntimeConfig>,
    pub prompt: String,
    /// The reviewer itself: its method and extra instructions lead the prompt.
    pub agent_id: Option<String>,
    pub effort: Option<String>,
}

impl DesktopApi {
    /// `pullRequestHubSubmitReview`
    pub fn pr_hub_submit_review(
        &self,
        item: &Item,
        body: Option<&str>,
        event: ReviewEvent,
        comments: &[DraftComment],
    ) -> Result<SubmitResult, ApiError> {
        let value = self.post(
            "/api/v1/pull-request-hub/submit-review",
            &json!({
                "repositoryRoot": item.repository_root,
                "number": item.number,
                "body": body,
                "event": event.key(),
                "comments": comments,
                "forgeLogin": item.forge_login,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }

    /// `pullRequestHubReplyThread`: on the conversation's first comment.
    pub fn pr_hub_reply_thread(
        &self,
        item: &Item,
        comment_id: &str,
        thread_id: &str,
        body: &str,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/pull-request-hub/reply-thread",
            &json!({
                "repositoryRoot": item.repository_root,
                "number": item.number,
                "commentId": comment_id,
                "threadId": thread_id,
                "body": body,
                "forgeLogin": item.forge_login,
            }),
        )
        .map(|_: Value| ())
    }

    /// `runPullRequestReviewAgent`: a read-only run in the repository; its answer.
    /// `runPullRequestReviewAgent`: as long as the reviewer's turn takes (a
    /// whole pull request can take minutes).
    pub fn pr_review_agent(&self, run: &AgentRun) -> Result<String, ApiError> {
        let value = self.request_waiting(
            "POST",
            "/api/v1/pull-request-hub/review-agent",
            Some(&json!({
                "workingDirectory": run.working_directory,
                "providerId": run.provider_id,
                "model": run.model,
                "providerRuntime": run.provider_runtime,
                "prompt": run.prompt,
                "agentId": run.agent_id,
                "effort": run.effort,
            })),
            REVIEW_WAIT,
        )?;
        Ok(value
            .get("response")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    /// `pullRequestHubResolveThread`
    pub fn pr_hub_resolve_thread(
        &self,
        item: &Item,
        thread_id: &str,
        resolved: bool,
    ) -> Result<(), ApiError> {
        self.post(
            "/api/v1/pull-request-hub/resolve-thread",
            &json!({
                "repositoryRoot": item.repository_root,
                "number": item.number,
                "threadId": thread_id,
                "resolved": resolved,
                "forgeLogin": item.forge_login,
            }),
        )
        .map(|_: Value| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_reads_like_parse_unified_diff() {
        let rows = parse_patch(Some(
            "@@ -10,3 +10,4 @@ fn main\n keep\n-old\n+new\n+more\n\\ No newline",
        ));
        let kinds: Vec<_> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(
            kinds,
            [
                RowKind::Hunk,
                RowKind::Context,
                RowKind::Deletion,
                RowKind::Addition,
                RowKind::Addition,
                RowKind::Meta
            ]
        );
        assert_eq!(
            (rows[1].old, rows[1].new, rows[1].review),
            (Some(10), Some(10), Some((10, "right")))
        );
        assert_eq!(
            (rows[2].old, rows[2].review),
            (Some(11), Some((11, "left")))
        );
        assert_eq!((rows[3].new, rows[3].content.as_str()), (Some(11), "new"));
        assert_eq!(rows[4].review, Some((12, "right")));
        assert!(parse_patch(None).is_empty());
        assert!(parse_patch(Some("")).is_empty());
        // `+++`/`---` headers are meta, and a one-line hunk has no count.
        let rows = parse_patch(Some("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b"));
        assert_eq!(rows[0].kind, RowKind::Meta);
        assert_eq!(rows[1].kind, RowKind::Meta);
        assert_eq!(rows[3].old, Some(1));
        assert_eq!(rows[4].new, Some(1));
    }

    #[test]
    fn the_review_says_what_it_can_send() {
        let item = Item::default();
        let caps = ReviewCapabilities {
            approve: true,
            request_changes: true,
            ..Default::default()
        };
        assert_eq!(
            event_blocked(ReviewEvent::Approve, &item, Some(&caps)),
            None
        );
        assert_eq!(
            event_blocked(ReviewEvent::Approve, &item, None),
            Some("pullRequests.code.unsupportedReviewAction")
        );
        let mine = Item {
            created_by_viewer: true,
            ..Default::default()
        };
        assert_eq!(
            event_blocked(ReviewEvent::RequestChanges, &mine, Some(&caps)),
            Some("pullRequests.code.ownPullRequestRestriction")
        );
        assert_eq!(event_blocked(ReviewEvent::Comment, &mine, None), None);
        let draft = Item {
            is_draft: true,
            ..Default::default()
        };
        assert_eq!(
            event_blocked(ReviewEvent::Approve, &draft, Some(&caps)),
            Some("pullRequests.code.draftRestriction")
        );
        assert!(can_submit(ReviewEvent::Approve, 0, "", false));
        assert!(!can_submit(ReviewEvent::Comment, 0, "  ", false));
        assert!(can_submit(ReviewEvent::Comment, 1, "", false));
        assert!(!can_submit(ReviewEvent::RequestChanges, 2, "", false));
        assert!(can_submit(ReviewEvent::RequestChanges, 0, "why", false));
        assert!(!can_submit(ReviewEvent::Approve, 0, "", true));
        assert_eq!(draft_key("a.rs", 3, "left"), "a.rs:left:3");
    }
}
