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

/// What the description may take of a prompt.
const DESCRIPTION_BOUND: usize = 8_000;

/// Files a review learns nothing from: lockfiles, snapshots, build output.
fn is_noise(path: &str) -> bool {
    const NAMES: [&str; 6] = [
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "Cargo.lock",
        "bun.lockb",
        "composer.lock",
    ];
    const ENDINGS: [&str; 4] = [".snap", ".min.js", ".map", ".lock"];
    let name = path.rsplit('/').next().unwrap_or(path);
    NAMES.contains(&name)
        || ENDINGS.iter().any(|ending| name.ends_with(ending))
        || path
            .split('/')
            .any(|part| part == "dist" || part == "build")
}

fn chars(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The patches that fit, whole, the code first; the rest named with their
/// size, so a review never judges a file cut in the middle.
fn changed_files(detail: &Detail, budget: usize) -> String {
    let mut ordered: Vec<&super::pr_review::File> = detail.files.iter().collect();
    ordered.sort_by_key(|file| is_noise(&file.path));
    let mut left = budget;
    let mut sent = Vec::new();
    let mut left_out = Vec::new();
    for file in ordered {
        let size = format!("{} (+{} −{})", file.path, file.additions, file.deletions);
        let Some(patch) = file.patch.as_deref() else {
            left_out.push(format!("{size}, no patch from the forge"));
            continue;
        };
        let text = format!("### {}\n{patch}", file.path);
        if chars(&text) <= left {
            left -= chars(&text);
            sent.push(text);
        } else {
            left_out.push(size);
        }
    }
    let mut files = sent.join("\n\n");
    if !left_out.is_empty() {
        files.push_str(&format!(
            "\n\nLeft out, too large to send (you did not see them):\n- {}",
            left_out.join("\n- ")
        ));
    }
    files
}

fn pull_request_context(item: &Item, detail: &Detail) -> String {
    let author = item
        .author
        .as_ref()
        .map_or("unknown", |actor| actor.login.as_str());
    let description = detail
        .body
        .as_deref()
        .or(item.body.as_deref())
        .unwrap_or("[none]");
    let head = [
        format!(
            "PR: {}#{} — {}",
            item.repository_name, item.number, item.title
        ),
        format!("Author: {author}"),
        format!("Branches: {} -> {}", item.head_branch, item.base_branch),
        format!(
            "Description:\n{}",
            bounded(description.to_string(), DESCRIPTION_BOUND)
        ),
    ]
    .join("\n\n");
    let budget = BOUND.saturating_sub(chars(&head) + 2_000);
    format!(
        "{head}\n\nChanged files:\n{}",
        changed_files(detail, budget)
    )
}

/// How the review decides and what its summary holds, short.
const VERDICT_RULES: &str = "Decide first, with one of these verdicts:\n- \"approve\": nothing blocks. Minor points may exist; they are optional for the author.\n- \"request_changes\": at least one blocking problem: a bug, regression, security issue or data loss on a path that will realistically run.\n- \"undecided\": only when a file you did not see could hide a blocking problem on the main path. Then say exactly which file and what to check. A file you did not see that cannot break the main path does not stop an approve.\n\nThe summary is short, written in the language of the pull request's description, and holds only:\n1. One line with the verdict in plain words.\n2. The blocking problems, one line each with where, under a label like \"Blocks:\", only when there are any.\n3. At most three optional points, one line each, under a label like \"Doesn't block:\", only when there are any.\n4. The files you did not see, in one line, only when there are any.\nTranslate the labels into the summary's language. Nothing else: do not restate what the diff does, list what you checked, praise it, or ask the person to confirm something you could not read.";

/// `buildReviewAgentPrompt`; a follow-up when the person reviewed before.
pub fn review_prompt(item: &Item, detail: &Detail, instruction: &str) -> String {
    let follow_up = item
        .forge_login
        .as_deref()
        .and_then(|login| super::pr_review_follow_up::section(detail, login))
        .map(|section| format!("\n\n{section}"))
        .unwrap_or_default();
    format!(
        "{}\n\nReview this pull request for correctness, regressions, security, data loss, concurrency, and missing tests. Focus only on actionable findings introduced by the diff. {}{follow_up}\n\n{VERDICT_RULES}\n\nReturn JSON only with this exact shape:\n{{\"verdict\":\"approve|request_changes|undecided\",\"summary\":\"editable review summary\",\"comments\":[{{\"path\":\"exact changed file path\",\"line\":123,\"side\":\"right\",\"body\":\"concise actionable finding\"}}]}}\n\nUse only lines visible in the supplied patches. If there are no findings, return an empty comments array. Do not publish or modify anything.",
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

pub(crate) fn parse_json(response: &str) -> Result<Value, String> {
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
    /// Approve, ask for changes, or not decided yet; none from an older answer.
    pub verdict: Option<super::pr_review_verdict::Verdict>,
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
        verdict: parsed
            .get("verdict")
            .and_then(Value::as_str)
            .and_then(super::pr_review_verdict::Verdict::parse),
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
    fn a_review_answer_says_its_verdict() {
        let review = parse_review(
            "{\"verdict\":\"request_changes\",\"summary\":\"Peça mudanças.\",\"comments\":[]}",
            &detail(),
        )
        .unwrap();
        assert_eq!(
            review.verdict,
            Some(super::super::pr_review_verdict::Verdict::RequestChanges)
        );
        assert_eq!(parse_review("{}", &detail()).unwrap().verdict, None);
    }

    #[test]
    fn a_patch_that_does_not_fit_is_named_never_cut() {
        use crate::data::pr_review::File;
        let file = |path: &str, size: usize| File {
            path: path.into(),
            additions: size as u64,
            deletions: 1,
            patch: Some(format!("@@ -1 +1 @@\n+{}", "x".repeat(size))),
            ..Default::default()
        };
        let detail = Detail {
            files: vec![
                file("package-lock.json", 50),
                file("src/big.rs", 400),
                file("src/a.rs", 40),
                File {
                    path: "src/huge.rs".into(),
                    additions: 9000,
                    patch: None,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let files = changed_files(&detail, 200);
        // The code first, whole; the lockfile after it, as room allows.
        assert!(files.starts_with("### src/a.rs\n"));
        assert!(files.contains("### package-lock.json"));
        assert!(!files.contains("### src/big.rs"));
        assert!(files.contains(
            "Left out, too large to send (you did not see them):\n- src/big.rs (+400 −1)"
        ));
        assert!(files.contains("- src/huge.rs (+9000 −0), no patch from the forge"));
        assert!(is_noise("web/dist/app.js") && is_noise("yarn.lock") && !is_noise("src/lib.rs"));
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
        assert!(prompt.contains("missing tests. Focus only on actionable findings introduced by the diff. foque em testes\n\nDecide first"));
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
