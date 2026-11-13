//! A review of a pull request the person already reviewed is a follow-up,
//! not a fresh start: the reviewer reads the threads the person opened (and
//! the replies), says which the new commits address, and comments again
//! only on what blocks. A comment on a line an earlier thread holds is
//! dropped, so a second round never repeats the first.

use std::collections::HashSet;

use super::pr_hub::Detail;
use super::pr_review::InlineComment;
use super::pr_review_agent::AgentReview;

/// What the earlier threads may take of a prompt.
const BOUND: usize = 20_000;

/// The threads `login` opened on the pull request, each with its replies.
pub fn earlier_threads<'a>(detail: &'a Detail, login: &str) -> Vec<Vec<&'a InlineComment>> {
    let mut threads: Vec<(String, Vec<&InlineComment>)> = Vec::new();
    for comment in &detail.inline_comments {
        let key = comment
            .thread_id
            .clone()
            .unwrap_or_else(|| comment.id.clone());
        match threads.iter_mut().find(|(thread, _)| *thread == key) {
            Some((_, comments)) => comments.push(comment),
            None => threads.push((key, vec![comment])),
        }
    }
    threads
        .into_iter()
        .map(|(_, comments)| comments)
        .filter(|comments| {
            comments[0]
                .author
                .as_ref()
                .is_some_and(|author| author.login.eq_ignore_ascii_case(login))
        })
        .collect()
}

/// The prompt's follow-up block, when `login` reviewed before.
pub fn section(detail: &Detail, login: &str) -> Option<String> {
    let threads = earlier_threads(detail, login);
    if threads.is_empty() {
        return None;
    }
    let mut text = String::from(
        "Your earlier review on this pull request (the threads you opened, with their replies):",
    );
    for thread in &threads {
        let root = thread[0];
        let line = root
            .line
            .map_or_else(|| "?".to_string(), |line| line.to_string());
        let state = if root.resolved == Some(true) {
            "resolved"
        } else if root.outdated {
            "open, its code changed since"
        } else if super::pr_since_review::line_unchanged(root, detail) {
            "open, its line is unchanged since"
        } else {
            "open"
        };
        text.push_str(&format!("\n\n### {}:{line} ({state})", root.path));
        for comment in thread {
            let author = comment
                .author
                .as_ref()
                .map_or("unknown", |actor| actor.login.as_str());
            text.push_str(&format!("\n@{author}: {}", comment.body.trim()));
        }
    }
    // What others wrote on the pull request after the review: an author
    // often answers every thread at once there.
    if let Some(reviewed) = super::pr_since_review::last_review(detail, login) {
        let replies: Vec<_> = detail
            .comments
            .iter()
            .filter(|comment| {
                comment
                    .created_at
                    .as_deref()
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    .is_some_and(|at| at > reviewed)
            })
            .filter(|comment| {
                !comment
                    .author
                    .as_ref()
                    .is_some_and(|actor| actor.login.eq_ignore_ascii_case(login))
            })
            .collect();
        if !replies.is_empty() {
            text.push_str("\n\nReplies on the pull request since that review:");
            for comment in replies {
                let author = comment
                    .author
                    .as_ref()
                    .map_or("unknown", |actor| actor.login.as_str());
                text.push_str(&format!("\n\n@{author}: {}", comment.body.trim()));
            }
        }
    }
    if text.chars().count() > BOUND {
        text = text.chars().take(BOUND).collect::<String>() + "\n…[truncated]";
    }
    Some(format!(
        "{text}\n\nThis is a follow-up round: the author changed the pull request since that review. Those threads are a settled conversation: do not list them one by one, and never repeat, rephrase or extend them. In the summary, only: a blocking problem from them that the current diff still has, in one line under the blocking ones; and a reply that says something is fixed when the current diff does not hold it, in one line. A minor thread left as it was is the person's decision: leave it out entirely. Add a new comment only for a blocking problem (a bug, regression, security issue or data loss) in the current diff; never style, naming, refactors or nice-to-have suggestions. When nothing blocks, the verdict is approve, the comments array is empty and the summary can be the verdict line alone."
    ))
}

/// Drops comments on a line an earlier thread already holds.
pub fn drop_repeats(review: &mut AgentReview, detail: &Detail, login: &str) {
    // An outdated thread's line is where its code was, not a line of the
    // current diff: a new comment there is about other code.
    let held: HashSet<(String, u32)> = earlier_threads(detail, login)
        .iter()
        .filter(|thread| !thread[0].outdated)
        .filter_map(|thread| Some((thread[0].path.clone(), thread[0].line?)))
        .collect();
    review
        .comments
        .retain(|comment| !held.contains(&(comment.path.clone(), comment.line)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::pr_hub::Actor;
    use crate::data::pr_review::DraftComment;

    fn comment(id: &str, thread: &str, author: &str, line: u32, resolved: bool) -> InlineComment {
        InlineComment {
            id: id.into(),
            thread_id: Some(thread.into()),
            path: "src/a.rs".into(),
            line: Some(line),
            side: Some("right".into()),
            body: format!("body {id}"),
            author: Some(Actor {
                login: author.into(),
                ..Default::default()
            }),
            resolved: Some(resolved),
            ..Default::default()
        }
    }

    fn detail() -> Detail {
        Detail {
            inline_comments: vec![
                comment("c1", "t1", "Wharley", 3, false),
                comment("c2", "t1", "dev", 3, false),
                comment("c3", "t2", "bob", 5, false),
                comment("c4", "t3", "wharley", 8, true),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn the_follow_up_reads_only_the_persons_threads() {
        let detail = detail();
        let threads = earlier_threads(&detail, "wharley");
        let ids: Vec<Vec<&str>> = threads
            .iter()
            .map(|thread| thread.iter().map(|c| c.id.as_str()).collect())
            .collect();
        assert_eq!(ids, [vec!["c1", "c2"], vec!["c4"]]);
        let text = section(&detail, "wharley").unwrap();
        assert!(text.contains("### src/a.rs:3 (open)\n@Wharley: body c1\n@dev: body c2"));
        assert!(text.contains("### src/a.rs:8 (resolved)"));
        assert!(!text.contains("body c3"));
        assert!(text.contains("Add a new comment only for a blocking problem"));
        assert_eq!(section(&detail, "someone"), None);
    }

    /// The author answered every thread at once on the pull request: the
    /// follow-up reads that answer and which threads' code moved.
    #[test]
    fn the_follow_up_reads_the_authors_reply_and_which_code_moved() {
        use crate::data::pr_hub::{Comment, Commit, Review};
        let mut detail = detail();
        detail.inline_comments[0].created_at = Some("2026-11-01T17:51:50Z".into());
        detail.inline_comments[0].outdated = true;
        detail.reviews = vec![Review {
            author: Some(Actor {
                login: "wharley".into(),
                ..Default::default()
            }),
            state: "COMMENTED".into(),
            submitted_at: Some("2026-11-01T17:51:51Z".into()),
            ..Default::default()
        }];
        detail.commits = vec![Commit {
            oid: "7788a5a".into(),
            committed_date: Some("2026-11-01T19:57:28Z".into()),
            ..Default::default()
        }];
        detail.comments = vec![
            Comment {
                id: "old".into(),
                body: "before the review".into(),
                author: Some(Actor {
                    login: "dev".into(),
                    ..Default::default()
                }),
                created_at: Some("2026-11-01T17:00:00Z".into()),
            },
            Comment {
                id: "fix".into(),
                body: "Os cinco procedem, corrigidos.".into(),
                author: Some(Actor {
                    login: "dev".into(),
                    ..Default::default()
                }),
                created_at: Some("2026-11-01T19:57:50Z".into()),
            },
        ];
        let text = section(&detail, "wharley").unwrap();
        assert!(text.contains("### src/a.rs:3 (open, its code changed since)"));
        assert!(
            text.contains(
                "Replies on the pull request since that review:\n\n@dev: Os cinco procedem"
            )
        );
        assert!(!text.contains("before the review"));
        assert!(text.contains("when the current diff does not hold it"));
        assert!(text.contains(
            "A minor thread left as it was is the person's decision: leave it out entirely."
        ));
    }

    #[test]
    fn a_second_round_never_comments_where_the_first_did() {
        let draft = |line: u32| DraftComment {
            path: "src/a.rs".into(),
            body: "again".into(),
            line,
            side: "right",
            start_line: None,
        };
        let mut review = AgentReview {
            verdict: None,
            summary: String::new(),
            comments: vec![draft(3), draft(5), draft(8), draft(9)],
        };
        drop_repeats(&mut review, &detail(), "wharley");
        let lines: Vec<u32> = review.comments.iter().map(|c| c.line).collect();
        assert_eq!(lines, [5, 9]);

        // Line 3's thread is outdated: its code is gone, line 3 is new code.
        let mut moved = detail();
        moved.inline_comments[0].outdated = true;
        let mut review = AgentReview {
            verdict: None,
            summary: String::new(),
            comments: vec![draft(3), draft(8)],
        };
        drop_repeats(&mut review, &moved, "wharley");
        let lines: Vec<u32> = review.comments.iter().map(|c| c.line).collect();
        assert_eq!(lines, [3]);
    }
}
