//! The built-in reviewer's findings (`review-findings.ts`): the
//! machine-readable `dcc-review` block its message ends with, read into
//! findings placed on lines, and what the reviewer is asked from its
//! second round on (the earlier findings listed, so the round does not
//! depend on what the provider still remembers).

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewSeverity {
    Critical,
    Major,
    Minor,
}

impl ReviewSeverity {
    /// `critical` | `major` | `minor`, as the i18n keys name them.
    pub fn id(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Major => "major",
            Self::Minor => "minor",
        }
    }
}

/// One problem the reviewer verified, anchored to the file's lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewFinding {
    pub path: String,
    pub start_line: u64,
    pub end_line: u64,
    pub severity: ReviewSeverity,
    pub title: String,
    pub detail: String,
}

const MAX_FINDINGS: usize = 50;
const MAX_TITLE_CHARS: usize = 240;
const MAX_DETAIL_CHARS: usize = 1_500;
const MAX_FOLLOW_UP_FINDINGS: usize = 20;
const FENCE: &str = "```dcc-review";

/// The block's body: after the tag's line, up to a fence on a line of its
/// own (a fence quoted inside a finding does not end it), or the end while
/// it still streams.
fn block_body(message: &str) -> Option<&str> {
    let start = message.find(FENCE)? + FENCE.len();
    let rest = &message[start..];
    let rest = match rest.find('\n') {
        Some(line_end) => &rest[line_end + 1..],
        None => "",
    };
    let mut search = 0;
    while let Some(found) = rest[search..].find("\n```") {
        let at = search + found;
        let after = &rest[at + 4..];
        let trailing = after.trim_start_matches([' ', '\t']);
        if trailing.is_empty() || trailing.starts_with('\n') {
            return Some(&rest[..at]);
        }
        search = at + 4;
    }
    Some(rest)
}

fn positive_line(value: Option<&Value>) -> Option<u64> {
    value?.as_u64().filter(|line| *line > 0)
}

/// `parseReviewFindings`: `None` without a valid block, so a review without
/// one is never mistaken for "no findings". Entries that cannot be placed
/// on a line are dropped.
pub fn parse_review_findings(message: &str) -> Option<Vec<ReviewFinding>> {
    let parsed: Value = serde_json::from_str(block_body(message)?).ok()?;
    let list = parsed.get("findings")?.as_array()?;
    Some(
        list.iter()
            .take(MAX_FINDINGS)
            .filter_map(|value| {
                let candidate = value.as_object()?;
                let path = candidate.get("path")?.as_str()?.trim();
                let path = path.strip_prefix("./").unwrap_or(path).to_string();
                let start_line = positive_line(candidate.get("line"))?;
                let title = candidate.get("title")?.as_str()?.trim();
                if path.is_empty() || title.is_empty() {
                    return None;
                }
                let end_line = positive_line(candidate.get("endLine"))
                    .filter(|end| *end >= start_line)
                    .unwrap_or(start_line);
                let severity = match candidate.get("severity").and_then(Value::as_str) {
                    Some("critical") => ReviewSeverity::Critical,
                    Some("major") => ReviewSeverity::Major,
                    _ => ReviewSeverity::Minor,
                };
                let title = if title.chars().count() > MAX_TITLE_CHARS {
                    format!(
                        "{}…",
                        title.chars().take(MAX_TITLE_CHARS).collect::<String>()
                    )
                } else {
                    title.to_string()
                };
                let detail = candidate
                    .get("detail")
                    .and_then(Value::as_str)
                    .map(|detail| detail.trim().chars().take(MAX_DETAIL_CHARS).collect())
                    .unwrap_or_default();
                Some(ReviewFinding {
                    path,
                    start_line,
                    end_line,
                    severity,
                    title,
                    detail,
                })
            })
            .collect(),
    )
}

/// `buildFollowUpKickoff`: the intro, then the earlier round's findings
/// (at most twenty) with their severity's word.
pub fn build_follow_up_kickoff(
    previous: &[ReviewFinding],
    intro: &str,
    previous_label: &str,
    severity: impl Fn(ReviewSeverity) -> String,
) -> String {
    if previous.is_empty() {
        return intro.to_string();
    }
    let mut lines = vec![
        intro.to_string(),
        String::new(),
        format!("{previous_label}:"),
    ];
    lines.extend(
        previous
            .iter()
            .take(MAX_FOLLOW_UP_FINDINGS)
            .enumerate()
            .map(|(index, finding)| {
                format!(
                    "{}. [{}] `{}:{}` {}",
                    index + 1,
                    severity(finding.severity),
                    finding.path,
                    finding.start_line,
                    finding.title
                )
            }),
    );
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(json: &str) -> String {
        format!("Found two problems.\n\n```dcc-review\n{json}\n```")
    }

    #[test]
    fn reads_the_block_and_normalizes_each_finding() {
        let findings = parse_review_findings(&block(
            r#"{"findings":[{"path":"./src/a.ts","line":10,"endLine":12,"severity":"major","title":" Off by one ","detail":" The loop skips the last item. "},{"path":"src/b.ts","line":3,"endLine":1,"severity":"blocker","title":"Null deref"}]}"#,
        ))
        .unwrap();
        assert_eq!(
            findings,
            vec![
                ReviewFinding {
                    path: "src/a.ts".into(),
                    start_line: 10,
                    end_line: 12,
                    severity: ReviewSeverity::Major,
                    title: "Off by one".into(),
                    detail: "The loop skips the last item.".into(),
                },
                ReviewFinding {
                    path: "src/b.ts".into(),
                    start_line: 3,
                    end_line: 3,
                    severity: ReviewSeverity::Minor,
                    title: "Null deref".into(),
                    detail: String::new(),
                },
            ]
        );
    }

    #[test]
    fn drops_entries_without_a_line() {
        let findings = parse_review_findings(&block(
            r#"{"findings":[{"path":"src/a.ts","line":0,"title":"zero"},{"path":"src/a.ts","line":1.5,"title":"fraction"},{"path":"","line":1,"title":"no path"},{"path":"src/a.ts","line":1,"title":" "},"text",{"path":"src/a.ts","line":4,"title":"kept"}]}"#,
        ))
        .unwrap();
        let titles: Vec<&str> = findings
            .iter()
            .map(|finding| finding.title.as_str())
            .collect();
        assert_eq!(titles, ["kept"]);
    }

    #[test]
    fn a_clean_review_differs_from_none() {
        assert_eq!(
            parse_review_findings(&block(r#"{"findings":[]}"#)),
            Some(Vec::new())
        );
        assert_eq!(parse_review_findings("No issues found."), None);
        assert_eq!(parse_review_findings(&block("{not json")), None);
        assert_eq!(
            parse_review_findings(&block(r#"{"findings":"none"}"#)),
            None
        );
    }

    #[test]
    fn a_quoted_fence_does_not_end_the_block() {
        let message = block(
            r#"{"findings":[{"path":"src/a.ts","line":2,"title":"Guard missing","detail":"Add ```if (!b) throw``` first."}]}"#,
        );
        let findings = parse_review_findings(&message).unwrap();
        assert_eq!(findings[0].detail, "Add ```if (!b) throw``` first.");
    }

    #[test]
    fn the_follow_up_lists_the_earlier_findings() {
        let finding = |path: &str, line: u64, severity, title: &str| ReviewFinding {
            path: path.into(),
            start_line: line,
            end_line: line,
            severity,
            title: title.into(),
            detail: String::new(),
        };
        let words = |severity: ReviewSeverity| severity.id().to_uppercase();
        let earlier = [
            finding("src/a.ts", 10, ReviewSeverity::Major, "Off by one"),
            finding(
                "src/b.ts",
                3,
                ReviewSeverity::Minor,
                "Cache branch has no test",
            ),
        ];
        assert_eq!(
            build_follow_up_kickoff(
                &earlier,
                "Review again.",
                "Findings of the earlier round",
                words
            ),
            [
                "Review again.",
                "",
                "Findings of the earlier round:",
                "1. [MAJOR] `src/a.ts:10` Off by one",
                "2. [MINOR] `src/b.ts:3` Cache branch has no test",
            ]
            .join("\n")
        );
        assert_eq!(
            build_follow_up_kickoff(&[], "Review again.", "x", words),
            "Review again."
        );
        let many: Vec<ReviewFinding> = (1..=30)
            .map(|line| finding("src/a.ts", line, ReviewSeverity::Minor, "t"))
            .collect();
        assert_eq!(
            build_follow_up_kickoff(&many, "R", "P", words)
                .lines()
                .count(),
            23
        );
    }
}
