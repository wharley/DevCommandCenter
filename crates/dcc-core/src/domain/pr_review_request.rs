//! A pull request an agent asks DCC to review (`dcc_review_pull_request`):
//! the review the hub runs from "Gerar revisão", started from the agent's
//! own conversation instead of a task. The request waits in a small table
//! until the app takes it; the review's job then shows in the team room
//! while it runs and its drafts wait in Pull Requests. Nothing is published.

use serde::{Deserialize, Serialize};
use specta::Type;

pub const REVIEW_PR_TOOL: &str = "dcc_review_pull_request";
/// Pull requests one call may send.
pub const MAX_REVIEWS_PER_CALL: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrReviewRequest {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub url: String,
    /// `owner/name`
    pub repository: String,
    pub number: u64,
    pub created_at: String,
}

/// A pull request's link read as its host, `owner/name` and number:
/// `https://github.com/acme/app/pull/1014` (a trailing `/files` or `#…` is
/// fine). Only GitHub's form, the one the hub reviews.
pub fn parse_pull_request_url(url: &str) -> Option<(String, String, u64)> {
    let rest = url.trim().strip_prefix("https://")?;
    let rest = rest.split(['#', '?']).next()?;
    let mut parts = rest.split('/');
    let host = parts.next().filter(|host| !host.is_empty())?;
    let owner = parts.next().filter(|owner| !owner.is_empty())?;
    let name = parts.next().filter(|name| !name.is_empty())?;
    if parts.next()? != "pull" {
        return None;
    }
    let number: u64 = parts.next()?.parse().ok().filter(|number| *number > 0)?;
    Some((host.to_string(), format!("{owner}/{name}"), number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_requests_link_reads_as_its_repository_and_number() {
        assert_eq!(
            parse_pull_request_url("https://github.com/VoaHoteis/vendeagora-app/pull/1034"),
            Some((
                "github.com".to_string(),
                "VoaHoteis/vendeagora-app".to_string(),
                1034
            ))
        );
        assert_eq!(
            parse_pull_request_url("https://github.com/acme/app/pull/7/files#diff-1")
                .map(|(_, _, number)| number),
            Some(7)
        );
        for bad in [
            "github.com/acme/app/pull/7",
            "https://github.com/acme/app/issues/7",
            "https://github.com/acme/app/pull/x",
            "https://github.com/acme/pull/7",
            "https://github.com/acme/app/pull/0",
        ] {
            assert_eq!(parse_pull_request_url(bad), None, "{bad}");
        }
    }
}
