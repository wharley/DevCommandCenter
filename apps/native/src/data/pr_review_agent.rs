//! What the reviewer is asked about a pull request and how its answer is
//! read (`pull-request-agent.ts`): the whole review (a summary and comments
//! on lines the patches show), one line's comment, a reply to a
//! conversation; the answer is JSON, maybe inside a fence.

use std::collections::HashSet;

use serde_json::Value;

use super::pr_hub::{Detail, Item};
use super::pr_review::{DraftComment, InlineComment, PatchRow, parse_patch};

/// What a prompt carries at most, as `bounded(…, 95_000)`.
const BOUND: usize = 95_000;

/// `fencedJson`: the inside of the first ```json fence, else the whole text.
fn fenced_json(value: &str) -> &str {
    let fenced = value.find("```").and_then(|start| {
        let rest = &value[start + 3..];
        let rest = if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("json") {
            &rest[4..]
        } else {
            rest
        };
        rest.find("```").map(|end| &rest[..end])
    });
    fenced.unwrap_or(value).trim()
}

/// `bounded`: cut past `max` characters, saying so.
fn bounded(value: String, max: usize) -> String {
    let utf16: usize = value.chars().map(char::len_utf16).sum();
    if utf16 <= max {
        return value;
    }
    let mut taken = 0;
    let cut: String = value
        .chars()
        .take_while(|c| {
            taken += c.len_utf16();
            taken <= max
        })
        .collect();
    format!("{cut}\n…[truncated]")
}

fn instruction_text(instruction: &str) -> &str {
    instruction.trim()
}

fn pull_request_context(item: &Item, detail: &Detail) -> String {
    let files = detail
        .files
        .iter()
        .map(|file| {
            format!(
                "### {}\n{}",
                file.path,
                file.patch.as_deref().unwrap_or("[patch unavailable]")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let author = item
        .author
        .as_ref()
        .map_or("unknown", |actor| actor.login.as_str());
    let description = detail
        .body
        .as_deref()
        .or(item.body.as_deref())
        .unwrap_or("[none]");
    bounded(
        [
            format!(
                "PR: {}#{} — {}",
                item.repository_name, item.number, item.title
            ),
            format!("Author: {author}"),
            format!("Branches: {} -> {}", item.head_branch, item.base_branch),
            format!("Description:\n{description}"),
            format!("Changed files:\n{files}"),
        ]
        .join("\n\n"),
        BOUND,
    )
}

/// `buildReviewAgentPrompt`; a follow-up when the person reviewed before.
pub fn review_prompt(item: &Item, detail: &Detail, instruction: &str) -> String {
    let follow_up = item
        .forge_login
        .as_deref()
        .and_then(|login| super::pr_review_follow_up::section(detail, login))
        .map(|section| format!("\n\n{section}"))
        .unwrap_or_default();
    format!(
        "{}\n\nReview this pull request for correctness, regressions, security, data loss, concurrency, and missing tests. Focus only on actionable findings introduced by the diff. {}{follow_up}\n\nReturn JSON only with this exact shape:\n{{\"summary\":\"editable review summary\",\"comments\":[{{\"path\":\"exact changed file path\",\"line\":123,\"side\":\"right\",\"body\":\"concise actionable finding\"}}]}}\n\nUse only lines visible in the supplied patches. If there are no findings, return an empty comments array. Do not publish or modify anything.",
        pull_request_context(item, detail),
        instruction_text(instruction),
    )
}

/// `buildInlineAgentPrompt`
pub fn inline_prompt(
    item: &Item,
    path: &str,
    row: &PatchRow,
    patch: Option<&str>,
    instruction: &str,
) -> String {
    let (line, side) = row.review.unwrap_or((0, "right"));
    let context = bounded(
        format!(
            "PR: {}#{} — {}\nFile: {path}\nSelected {side} line {line}: {}\nPatch:\n{}",
            item.repository_name,
            item.number,
            item.title,
            row.content,
            patch.unwrap_or("[unavailable]"),
        ),
        BOUND,
    );
    format!(
        "{context}\n\nDraft one precise inline review comment for the selected line. {}\nReturn JSON only: {{\"reply\":\"editable comment text\"}}. Do not publish or modify anything.",
        instruction_text(instruction),
    )
}

/// `buildThreadReplyAgentPrompt`
pub fn thread_reply_prompt(
    item: &Item,
    comment: &InlineComment,
    patch: Option<&str>,
    instruction: &str,
) -> String {
    let line = comment
        .line
        .map_or_else(|| "unknown".to_string(), |line| line.to_string());
    let author = comment
        .author
        .as_ref()
        .map_or("unknown", |actor| actor.login.as_str());
    let context = bounded(
        format!(
            "PR: {}#{} — {}\nThread: {}:{line}\nComment by @{author}:\n{}\nPatch:\n{}",
            item.repository_name,
            item.number,
            item.title,
            comment.path,
            comment.body,
            patch.unwrap_or("[unavailable]"),
        ),
        BOUND,
    );
    format!(
        "{context}\n\nDraft a helpful direct reply to this review thread. {}\nReturn JSON only: {{\"reply\":\"editable reply text\"}}. Do not publish or resolve anything.",
        instruction_text(instruction),
    )
}

fn parse_json(response: &str) -> Result<Value, String> {
    serde_json::from_str(fenced_json(response)).map_err(|error| error.to_string())
}

/// `parseAgentReply`
pub fn parse_reply(response: &str) -> Result<String, String> {
    let parsed = parse_json(response)?;
    parsed
        .get("reply")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reply| !reply.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "The agent did not return a usable draft.".to_string())
}

/// `PullRequestAgentReview`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentReview {
    pub summary: String,
    pub comments: Vec<DraftComment>,
}

/// `parseAgentReview`: only comments on lines the patches show stay.
pub fn parse_review(response: &str, detail: &Detail) -> Result<AgentReview, String> {
    let parsed = parse_json(response)?;
    let reviewable: HashSet<String> = detail
        .files
        .iter()
        .flat_map(|file| {
            parse_patch(file.patch.as_deref())
                .into_iter()
                .filter_map(|row| row.review)
                .map(|(line, side)| super::pr_review::draft_key(&file.path, line, side))
                .collect::<Vec<_>>()
        })
        .collect();
    let comments = parsed
        .get("comments")
        .and_then(Value::as_array)
        .map(|comments| {
            comments
                .iter()
                .filter_map(|value| {
                    let candidate = value.as_object()?;
                    let path = candidate.get("path").and_then(Value::as_str).unwrap_or("");
                    // `typeof line === "number"`: a whole, positive line or nothing.
                    let line = candidate
                        .get("line")
                        .and_then(Value::as_f64)
                        .filter(|line| {
                            line.fract() == 0.0 && *line >= 0.0 && *line <= u32::MAX as f64
                        })
                        .map_or(0, |line| line as u32);
                    let side = if candidate.get("side").and_then(Value::as_str) == Some("left") {
                        "left"
                    } else {
                        "right"
                    };
                    let body = candidate
                        .get("body")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim();
                    let key = super::pr_review::draft_key(path, line, side);
                    (!body.is_empty() && reviewable.contains(&key)).then(|| DraftComment {
                        path: path.to_string(),
                        body: body.to_string(),
                        line,
                        side,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(AgentReview {
        summary: parsed
            .get("summary")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string(),
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::pr_review::File;

    fn detail() -> Detail {
        Detail {
            body: Some("Fixes login".into()),
            files: vec![File {
                path: "src/a.rs".into(),
                patch: Some("@@ -1,2 +1,2 @@\n keep\n-old\n+new".into()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn the_review_keeps_only_lines_the_patch_shows() {
        let response = "Here:\n```json\n{\"summary\":\" Looks fine \",\"comments\":[\
            {\"path\":\"src/a.rs\",\"line\":2,\"side\":\"right\",\"body\":\" Check new \"},\
            {\"path\":\"src/a.rs\",\"line\":2,\"side\":\"left\",\"body\":\"old one\"},\
            {\"path\":\"src/a.rs\",\"line\":9,\"side\":\"right\",\"body\":\"not shown\"},\
            {\"path\":\"src/b.rs\",\"line\":1,\"body\":\"other file\"},\
            {\"path\":\"src/a.rs\",\"line\":1,\"body\":\"  \"}, 3]}\n```";
        let review = parse_review(response, &detail()).unwrap();
        assert_eq!(review.summary, "Looks fine");
        let found: Vec<_> = review
            .comments
            .iter()
            .map(|comment| (comment.line, comment.side, comment.body.as_str()))
            .collect();
        assert_eq!(found, [(2, "right", "Check new"), (2, "left", "old one")]);
        assert!(parse_review("not json", &detail()).is_err());
        assert_eq!(
            parse_review("{}", &detail()).unwrap(),
            AgentReview::default()
        );
    }

    #[test]
    fn a_reply_needs_text() {
        assert_eq!(
            parse_reply("```\n{\"reply\":\" Sim \"}\n```").unwrap(),
            "Sim"
        );
        assert!(parse_reply("{\"reply\":\"  \"}").is_err());
        assert!(parse_reply("{\"reply\":3}").is_err());
    }

    #[test]
    fn the_prompts_carry_the_pull_request() {
        let item = Item {
            repository_name: "acme/app".into(),
            number: 7,
            title: "Fix".into(),
            head_branch: "feat".into(),
            base_branch: "main".into(),
            ..Default::default()
        };
        let prompt = review_prompt(&item, &detail(), "  foque em testes ");
        assert!(
            prompt.starts_with("PR: acme/app#7 — Fix\n\nAuthor: unknown\n\nBranches: feat -> main")
        );
        assert!(prompt.contains("### src/a.rs\n@@ -1,2 +1,2 @@"));
        assert!(prompt.contains("missing tests. Focus only on actionable findings introduced by the diff. foque em testes\n\nReturn JSON"));
        let row = &parse_patch(detail().files[0].patch.as_deref())[2];
        let inline = inline_prompt(&item, "src/a.rs", row, None, "");
        assert!(inline.contains("Selected left line 2: old\nPatch:\n[unavailable]"));
        assert!(inline.ends_with("Do not publish or modify anything."));
        let long = bounded("é".repeat(10), 4);
        assert_eq!(long, "éééé\n…[truncated]");
    }

    #[test]
    fn a_second_review_follows_up_on_the_persons_threads() {
        let mut detail = detail();
        detail.inline_comments = vec![InlineComment {
            id: "c1".into(),
            path: "src/a.rs".into(),
            line: Some(2),
            body: "Falta teste".into(),
            author: Some(crate::data::pr_hub::Actor {
                login: "wharley".into(),
                ..Default::default()
            }),
            ..Default::default()
        }];
        let mut item = Item::default();
        assert!(!review_prompt(&item, &detail, "").contains("follow-up round"));
        item.forge_login = Some("wharley".into());
        let prompt = review_prompt(&item, &detail, "");
        assert!(prompt.contains("### src/a.rs:2 (open)\n@wharley: Falta teste"));
        assert!(prompt.contains("follow-up round"));
        assert!(prompt.ends_with("Do not publish or modify anything."));
    }
}
