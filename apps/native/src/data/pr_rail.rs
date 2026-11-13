//! A pull request on its way to production, read in the hub's detail: the
//! rail's four steps (open, checks, review, merge), who reviewed it with
//! their latest decision, and the checks split into the ones still in its
//! way and the ones that passed.

use std::collections::BTreeMap;

use crate::data::delivery_rail::StepState;
use crate::data::pr_hub::{Actor, Check, Detail, Item, Review};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrStep {
    Opened,
    Checks,
    Review,
    Merge,
}

impl PrStep {
    /// `pullRequests.rail.*`
    pub fn key(self) -> &'static str {
        match self {
            PrStep::Opened => "opened",
            PrStep::Checks => "checks",
            PrStep::Review => "review",
            PrStep::Merge => "merge",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Own {
    Done,
    Failed,
    Open,
}

/// `approved`: someone's latest review approves it.
pub fn pr_rail(item: &Item, approved: bool) -> [(PrStep, StepState); 4] {
    let opened = if item.is_draft { Own::Open } else { Own::Done };
    let checks = match item.checks_state.as_str() {
        "failure" => Own::Failed,
        "pending" => Own::Open,
        _ => Own::Done,
    };
    let review = match item.review_decision.as_deref() {
        Some("approved") => Own::Done,
        Some("changes_requested") => Own::Failed,
        Some(_) => Own::Open,
        // No required reviews: done once approved, or when nobody was asked.
        None if approved || item.reviewers.is_empty() => Own::Done,
        None => Own::Open,
    };
    let own = [opened, checks, review, Own::Open];
    let steps = [
        PrStep::Opened,
        PrStep::Checks,
        PrStep::Review,
        PrStep::Merge,
    ];
    let mut current_taken = false;
    std::array::from_fn(|index| {
        let state = match own[index] {
            Own::Done => StepState::Done,
            Own::Failed => StepState::Failed,
            Own::Open if !current_taken => {
                current_taken = true;
                StepState::Current
            }
            Own::Open => StepState::Todo,
        };
        (steps[index], state)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    /// Asked to review, nothing sent since.
    Requested,
}

impl Verdict {
    /// `pullRequests.verdicts.*`
    pub fn key(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::ChangesRequested => "changesRequested",
            Verdict::Commented => "commented",
            Verdict::Dismissed => "dismissed",
            Verdict::Requested => "requested",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewerLine {
    pub actor: Actor,
    pub verdict: Verdict,
    pub at: Option<String>,
}

fn verdict(state: &str) -> Option<Verdict> {
    match state {
        "APPROVED" => Some(Verdict::Approved),
        "CHANGES_REQUESTED" => Some(Verdict::ChangesRequested),
        "COMMENTED" => Some(Verdict::Commented),
        "DISMISSED" => Some(Verdict::Dismissed),
        _ => None,
    }
}

/// Each reviewer once, with the review that counts: their latest decision
/// (a later comment does not undo an approval), else their latest comment;
/// then the people asked who have not answered. Decisions first.
pub fn reviewer_lines(reviews: &[Review], requested: &[Actor]) -> Vec<ReviewerLine> {
    let mut latest: BTreeMap<String, ReviewerLine> = BTreeMap::new();
    for review in reviews {
        let (Some(actor), Some(verdict)) = (review.author.as_ref(), verdict(&review.state)) else {
            continue;
        };
        let line = ReviewerLine {
            actor: actor.clone(),
            verdict,
            at: review.submitted_at.clone(),
        };
        let keep = latest.get(&actor.login).is_some_and(|known| {
            let decided = known.verdict != Verdict::Commented;
            let newer = known.at >= line.at;
            (decided && verdict == Verdict::Commented)
                || (newer && decided == (verdict != Verdict::Commented))
        });
        if !keep {
            latest.insert(actor.login.clone(), line);
        }
    }
    for actor in requested {
        // Asked again after a review: they are awaited.
        latest.insert(
            actor.login.clone(),
            ReviewerLine {
                actor: actor.clone(),
                verdict: Verdict::Requested,
                at: None,
            },
        );
    }
    let mut lines: Vec<ReviewerLine> = latest.into_values().collect();
    lines.sort_by_key(|line| match line.verdict {
        Verdict::ChangesRequested => 0,
        Verdict::Approved => 1,
        Verdict::Commented | Verdict::Dismissed => 2,
        Verdict::Requested => 3,
    });
    lines
}

/// One entry of a pull request's activity: a comment, or a review that came
/// with text (its `verdict` says which).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityEntry {
    pub id: String,
    pub author: Option<Actor>,
    pub created_at: Option<String>,
    pub body: String,
    pub verdict: Option<Verdict>,
}

/// The comments and the reviews that say something, newest first: a review
/// sent as a comment (a draft cannot be approved) still shows what it said.
pub fn activity_entries(detail: &Detail) -> Vec<ActivityEntry> {
    let comments = detail.comments.iter().map(|comment| ActivityEntry {
        id: comment.id.clone(),
        author: comment.author.clone(),
        created_at: comment.created_at.clone(),
        body: comment.body.clone(),
        verdict: None,
    });
    let reviews = detail
        .reviews
        .iter()
        .enumerate()
        .filter(|(_, review)| !review.body.trim().is_empty())
        .filter_map(|(index, review)| {
            Some(ActivityEntry {
                id: format!("review-{index}"),
                author: review.author.clone(),
                created_at: review.submitted_at.clone(),
                body: review.body.clone(),
                verdict: Some(verdict(&review.state)?),
            })
        });
    let mut entries: Vec<ActivityEntry> = comments.chain(reviews).collect();
    // ISO times sort as text; an entry without one stays last.
    entries.sort_by(|left, right| match (&left.created_at, &right.created_at) {
        (Some(left), Some(right)) => right.cmp(left),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    entries
}

/// The checks still in the way (failed first, then the rest), and the ones
/// that passed.
pub fn split_checks(checks: &[Check]) -> (Vec<&Check>, Vec<&Check>) {
    let (passed, mut open): (Vec<&Check>, Vec<&Check>) = checks
        .iter()
        .partition(|check| matches!(check.state.as_str(), "success" | "skipped"));
    open.sort_by_key(|check| check.state != "failure");
    (open, passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use StepState::{Current, Done, Failed, Todo};

    fn item(checks: &str, decision: Option<&str>) -> Item {
        Item {
            checks_state: checks.into(),
            review_decision: decision.map(str::to_string),
            ..Item::default()
        }
    }

    fn states(item: &Item, approved: bool) -> Vec<StepState> {
        pr_rail(item, approved)
            .iter()
            .map(|(_, state)| *state)
            .collect()
    }

    #[test]
    fn the_rail_stops_where_the_pull_request_is() {
        assert_eq!(
            states(&item("pending", Some("review_required")), false),
            [Done, Current, Todo, Todo]
        );
        assert_eq!(
            states(&item("failure", Some("approved")), true),
            [Done, Failed, Done, Current]
        );
        assert_eq!(
            states(&item("success", Some("changes_requested")), false),
            [Done, Done, Failed, Current]
        );
        assert_eq!(
            states(&item("success", Some("approved")), true),
            [Done, Done, Done, Current]
        );
        let mut draft = item("success", None);
        draft.is_draft = true;
        assert_eq!(states(&draft, false), [Current, Done, Done, Todo]);
    }

    fn review(login: &str, state: &str, at: &str) -> Review {
        Review {
            author: Some(Actor {
                login: login.into(),
                ..Actor::default()
            }),
            state: state.into(),
            submitted_at: Some(at.into()),
            body: String::new(),
        }
    }

    #[test]
    fn a_reviewer_shows_once_with_the_review_that_counts() {
        let reviews = [
            review("ana", "CHANGES_REQUESTED", "2026-11-01T10:00:00Z"),
            review("ana", "APPROVED", "2026-11-02T10:00:00Z"),
            review("ana", "COMMENTED", "2026-11-03T10:00:00Z"),
            review("bia", "COMMENTED", "2026-11-02T09:00:00Z"),
            review("caio", "APPROVED", "2026-11-01T09:00:00Z"),
        ];
        let asked = [
            Actor {
                login: "caio".into(),
                ..Actor::default()
            },
            Actor {
                login: "duda".into(),
                ..Actor::default()
            },
        ];
        let lines = reviewer_lines(&reviews, &asked);
        let read: Vec<(&str, Verdict)> = lines
            .iter()
            .map(|line| (line.actor.login.as_str(), line.verdict))
            .collect();
        assert_eq!(
            read,
            [
                ("ana", Verdict::Approved),
                ("bia", Verdict::Commented),
                ("caio", Verdict::Requested),
                ("duda", Verdict::Requested),
            ]
        );
    }

    #[test]
    fn a_review_with_text_joins_the_activity_newest_first() {
        use crate::data::pr_hub::Comment;
        let mut said = review("ana", "COMMENTED", "2026-11-02T10:00:00Z");
        said.body = "Aprovado: nada bloqueia.".into();
        let detail = Detail {
            comments: vec![
                Comment {
                    id: "c-1".into(),
                    body: "Primeiro".into(),
                    created_at: Some("2026-11-01T10:00:00Z".into()),
                    ..Comment::default()
                },
                Comment {
                    id: "c-2".into(),
                    body: "Depois".into(),
                    created_at: Some("2026-11-03T10:00:00Z".into()),
                    ..Comment::default()
                },
            ],
            // A review without text adds nothing to read.
            reviews: vec![said, review("bia", "APPROVED", "2026-11-02T11:00:00Z")],
            ..Detail::default()
        };
        let entries = activity_entries(&detail);
        let read: Vec<(&str, Option<Verdict>)> = entries
            .iter()
            .map(|entry| (entry.body.as_str(), entry.verdict))
            .collect();
        assert_eq!(
            read,
            [
                ("Depois", None),
                ("Aprovado: nada bloqueia.", Some(Verdict::Commented)),
                ("Primeiro", None),
            ]
        );
    }

    #[test]
    fn failed_checks_come_first_and_the_passed_ones_apart() {
        let check = |name: &str, state: &str| Check {
            name: name.into(),
            state: state.into(),
            details_url: None,
        };
        let checks = [
            check("lint", "success"),
            check("e2e", "pending"),
            check("unit", "failure"),
            check("docs", "skipped"),
        ];
        let (open, passed) = split_checks(&checks);
        let names = |list: &[&Check]| -> Vec<String> {
            list.iter().map(|check| check.name.clone()).collect()
        };
        assert_eq!(names(&open), ["unit", "e2e"]);
        assert_eq!(names(&passed), ["lint", "docs"]);
    }
}
