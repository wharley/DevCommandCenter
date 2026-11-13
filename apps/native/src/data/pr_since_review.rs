//! What happened on a pull request since the person last reviewed it: the
//! commits pushed after, the author's replies (on the pull request and in
//! the threads), and, for each open thread, whether its code changed — the
//! forge's "outdated" — or the commented line is still the same after new
//! commits. GitHub's "Changes since your last review", read as one line.

use chrono::{DateTime, Utc};

use super::pr_hub::{Comment, Detail};
use super::pr_review::InlineComment;

fn at(text: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text?)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn by(author: Option<&super::pr_hub::Actor>, login: &str) -> bool {
    author.is_some_and(|actor| actor.login.eq_ignore_ascii_case(login))
}

/// When `login` last submitted a review.
pub fn last_review(detail: &Detail, login: &str) -> Option<DateTime<Utc>> {
    detail
        .reviews
        .iter()
        .filter(|review| by(review.author.as_ref(), login))
        .filter_map(|review| at(review.submitted_at.as_deref()))
        .max()
}

#[derive(Debug, PartialEq)]
pub struct SinceReview<'a> {
    pub commits: usize,
    /// The pull request author's comments on the pull request, oldest first.
    pub replies: Vec<&'a Comment>,
    /// Replies by others inside the threads.
    pub thread_replies: usize,
    /// Open threads whose code changed.
    pub outdated: usize,
    /// Open threads whose line is the same after the new commits.
    pub unchanged: usize,
}

impl SinceReview<'_> {
    pub fn is_empty(&self) -> bool {
        self.commits == 0 && self.replies.is_empty() && self.thread_replies == 0
    }
}

/// The first comment of each thread (a comment without a thread is its own).
pub fn is_root(comment: &InlineComment, detail: &Detail) -> bool {
    let Some(thread) = comment.thread_id.as_deref() else {
        return true;
    };
    detail
        .inline_comments
        .iter()
        .find(|entry| entry.thread_id.as_deref() == Some(thread))
        .is_some_and(|first| first.id == comment.id)
}

/// An open thread on a line no commit after it touched: GitHub keeps it
/// current (not outdated) while commits came after the comment.
pub fn line_unchanged(root: &InlineComment, detail: &Detail) -> bool {
    if root.outdated || root.resolved == Some(true) {
        return false;
    }
    let Some(written) = at(root.created_at.as_deref()) else {
        return false;
    };
    detail
        .commits
        .iter()
        .any(|commit| at(commit.committed_date.as_deref()).is_some_and(|date| date > written))
}

/// Nothing when `me` never reviewed it or nothing came after.
pub fn since_review<'a>(
    detail: &'a Detail,
    me: &str,
    author: Option<&str>,
) -> Option<SinceReview<'a>> {
    let reviewed = last_review(detail, me)?;
    let after = |text: Option<&str>| at(text).is_some_and(|time| time > reviewed);
    let commits = detail
        .commits
        .iter()
        .filter(|commit| after(commit.committed_date.as_deref()))
        .count();
    let replies = detail
        .comments
        .iter()
        .filter(|comment| after(comment.created_at.as_deref()))
        .filter(|comment| !by(comment.author.as_ref(), me))
        .filter(|comment| author.is_some_and(|author| by(comment.author.as_ref(), author)))
        .collect();
    let thread_replies = detail
        .inline_comments
        .iter()
        .filter(|comment| after(comment.created_at.as_deref()))
        .filter(|comment| !by(comment.author.as_ref(), me))
        .count();
    let roots = detail
        .inline_comments
        .iter()
        .filter(|comment| is_root(comment, detail) && comment.resolved != Some(true));
    let (mut outdated, mut unchanged) = (0, 0);
    for root in roots {
        if root.outdated {
            outdated += 1;
        } else if line_unchanged(root, detail) {
            unchanged += 1;
        }
    }
    let since = SinceReview {
        commits,
        replies,
        thread_replies,
        outdated,
        unchanged,
    };
    (!since.is_empty()).then_some(since)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::pr_hub::{Actor, Commit, Review};

    fn actor(login: &str) -> Option<Actor> {
        Some(Actor {
            login: login.into(),
            ..Default::default()
        })
    }

    fn thread(id: &str, outdated: bool) -> InlineComment {
        InlineComment {
            id: id.into(),
            thread_id: Some(format!("t-{id}")),
            path: "src/a.ts".into(),
            line: Some(48),
            body: "faixa larga".into(),
            author: actor("wharley"),
            resolved: Some(false),
            created_at: Some("2026-11-01T17:51:50Z".into()),
            outdated,
            ..Default::default()
        }
    }

    /// PR #1004: reviewed at 17:51, a fix pushed at 19:57 with one reply
    /// on the pull request; three threads outdated, one line untouched.
    fn pr() -> Detail {
        Detail {
            reviews: vec![Review {
                author: actor("Wharley"),
                state: "COMMENTED".into(),
                submitted_at: Some("2026-11-01T17:51:51Z".into()),
                ..Default::default()
            }],
            commits: vec![
                Commit {
                    oid: "d521d5c".into(),
                    committed_date: Some("2026-11-01T17:44:21Z".into()),
                    ..Default::default()
                },
                Commit {
                    oid: "7788a5a".into(),
                    committed_date: Some("2026-11-01T19:57:28Z".into()),
                    ..Default::default()
                },
            ],
            comments: vec![
                Comment {
                    id: "bot".into(),
                    body: "deploy".into(),
                    author: actor("vercel"),
                    created_at: Some("2026-11-01T17:45:00Z".into()),
                },
                Comment {
                    id: "fix".into(),
                    body: "Corrigidos em 7788a5a3d".into(),
                    author: actor("keppelen"),
                    created_at: Some("2026-11-01T19:57:50Z".into()),
                },
            ],
            inline_comments: vec![
                thread("a", true),
                thread("b", true),
                thread("c", true),
                thread("d", false),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn the_fix_pushed_after_the_review_and_its_reply_show_up() {
        let detail = pr();
        let since = since_review(&detail, "wharley", Some("keppelen")).unwrap();
        assert_eq!(since.commits, 1);
        assert_eq!(
            since
                .replies
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["fix"]
        );
        assert_eq!(since.thread_replies, 0);
        assert_eq!((since.outdated, since.unchanged), (3, 1));
        assert!(line_unchanged(&detail.inline_comments[3], &detail));
        assert!(!line_unchanged(&detail.inline_comments[0], &detail));
    }

    #[test]
    fn without_a_review_or_anything_new_there_is_nothing_to_say() {
        let detail = pr();
        assert_eq!(since_review(&detail, "someone", Some("keppelen")), None);
        let quiet = Detail {
            commits: detail.commits[..1].to_vec(),
            comments: detail.comments[..1].to_vec(),
            ..detail
        };
        assert_eq!(since_review(&quiet, "wharley", Some("keppelen")), None);
        assert!(!line_unchanged(&quiet.inline_comments[3], &quiet));
    }
}
