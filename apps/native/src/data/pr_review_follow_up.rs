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
    if text.chars().count() > BOUND {
        text = text.chars().take(BOUND).collect::<String>() + "\n…[truncated]";
    }
    Some(format!(
        "{text}\n\nThis is a follow-up round: the author changed the pull request since that review. Do not repeat, rephrase or extend points already raised, resolved or not. In the summary, say for each open thread whether the current diff addresses it. Add a new comment only for a blocking problem (a bug, regression, security issue or data loss) in the current diff; never style, naming, refactors or nice-to-have suggestions. An empty comments array is the expected answer when nothing blocks."
    ))
}

/// Drops comments on a line an earlier thread already holds.
pub fn drop_repeats(review: &mut AgentReview, detail: &Detail, login: &str) {
    let held: HashSet<(String, u32)> = earlier_threads(detail, login)
        .iter()
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

    #[test]
    fn a_second_round_never_comments_where_the_first_did() {
        let draft = |line: u32| DraftComment {
            path: "src/a.rs".into(),
            body: "again".into(),
            line,
            side: "right",
        };
        let mut review = AgentReview {
            summary: String::new(),
            comments: vec![draft(3), draft(5), draft(8), draft(9)],
        };
        drop_repeats(&mut review, &detail(), "wharley");
        let lines: Vec<u32> = review.comments.iter().map(|c| c.line).collect();
        assert_eq!(lines, [5, 9]);
    }
}
