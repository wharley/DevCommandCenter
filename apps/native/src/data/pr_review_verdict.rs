//! The reviewer's verdict on a pull request, said first: approve, ask for
//! changes, or not decided yet because something it could not read may
//! hide a blocking problem. The hub picks the matching GitHub event from it
//! and the team room's card shows it; nothing is published by it.

use super::pr_review::ReviewEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Approve,
    RequestChanges,
    Undecided,
}

impl Verdict {
    /// The JSON's `verdict`, read leniently (`request-changes`, any case).
    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_")
            .as_str()
        {
            "approve" | "approved" => Some(Self::Approve),
            "request_changes" | "changes_requested" => Some(Self::RequestChanges),
            "undecided" | "comment" => Some(Self::Undecided),
            _ => None,
        }
    }

    /// The GitHub event the review is published with.
    pub fn event(self) -> ReviewEvent {
        match self {
            Self::Approve => ReviewEvent::Approve,
            Self::RequestChanges => ReviewEvent::RequestChanges,
            Self::Undecided => ReviewEvent::Comment,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Approve => "pullRequests.verdict.approve",
            Self::RequestChanges => "pullRequests.verdict.requestChanges",
            Self::Undecided => "pullRequests.verdict.undecided",
        }
    }
}

/// The verdict of a reviewer's raw answer, when it gave one.
pub fn verdict_of(response: &str) -> Option<Verdict> {
    super::pr_review_agent::parse_json(response)
        .ok()?
        .get("verdict")?
        .as_str()
        .and_then(Verdict::parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verdict_reads_from_the_answer_and_picks_the_event() {
        assert_eq!(
            verdict_of("```json\n{\"verdict\":\"request-changes\",\"summary\":\"x\"}\n```"),
            Some(Verdict::RequestChanges)
        );
        assert_eq!(
            verdict_of("{\"verdict\":\"Approve\"}"),
            Some(Verdict::Approve)
        );
        assert_eq!(verdict_of("{\"summary\":\"x\"}"), None);
        assert_eq!(verdict_of("{\"verdict\":\"maybe\"}"), None);
        assert_eq!(verdict_of("not json"), None);
        assert_eq!(Verdict::Undecided.event(), ReviewEvent::Comment);
        assert_eq!(Verdict::Approve.event(), ReviewEvent::Approve);
    }
}
