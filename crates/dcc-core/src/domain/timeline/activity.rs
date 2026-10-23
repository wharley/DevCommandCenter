//! How a turn's work reads (`turn-activity.logic.ts`): what kind of thing
//! each tool did, the target a row names, the prose/work segments, the
//! one-line summary of a settled turn, and the small formatters it uses.

use std::collections::{BTreeMap, HashSet};

use super::{Annotation, ToolCall};

/// What a tool call did, coarse enough to summarise a turn in one sentence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolKind {
    Command,
    Edit,
    Read,
    Search,
    Web,
    Plan,
    Agent,
    Mcp,
    Other,
}

impl ToolKind {
    /// The i18n segment (`conversation.toolCall.verbs.<kind>`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Edit => "edit",
            Self::Read => "read",
            Self::Search => "search",
            Self::Web => "web",
            Self::Plan => "plan",
            Self::Agent => "agent",
            Self::Mcp => "mcp",
            Self::Other => "other",
        }
    }
}

/// Order in which kinds appear in the one-line turn summary.
pub const SUMMARY_KIND_ORDER: [ToolKind; 9] = [
    ToolKind::Edit,
    ToolKind::Command,
    ToolKind::Read,
    ToolKind::Search,
    ToolKind::Web,
    ToolKind::Mcp,
    ToolKind::Agent,
    ToolKind::Plan,
    ToolKind::Other,
];

/// `classifyToolAction`
pub fn classify_tool_action(action: &str) -> ToolKind {
    // DCC's own delegation tools hand work to another agent: the turn's headline.
    let bare = action.rsplit("__").next().unwrap_or(action);
    if matches!(
        bare,
        "dcc_delegate_task" | "dcc_propose_task" | "dcc_task_message"
    ) {
        return ToolKind::Agent;
    }
    if action.starts_with("mcp__") {
        return ToolKind::Mcp;
    }
    let normalized: String = action
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    match normalized.as_str() {
        "bash" | "shell" | "commandexecution" | "execcommand" | "terminal" | "killshell"
        | "bashoutput" => ToolKind::Command,
        "edit" | "multiedit" | "write" | "writefile" | "applypatch" | "filechange"
        | "notebookedit" => ToolKind::Edit,
        "read" | "readfile" | "view" | "notebookread" => ToolKind::Read,
        "grep" | "glob" | "ls" | "searchfiles" | "find" | "codesearch" => ToolKind::Search,
        "websearch" | "webfetch" | "browser" => ToolKind::Web,
        "todowrite" | "updateplan" => ToolKind::Plan,
        "task" | "agent" => ToolKind::Agent,
        _ => ToolKind::Other,
    }
}

fn short_path(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if segments.len() <= 2 {
        path.to_string()
    } else {
        segments[segments.len() - 2..].join("/")
    }
}

/// `mcpToolLabel`: `mcp__github__create_issue` → `github · create_issue`.
pub fn mcp_tool_label(action: &str) -> String {
    let mut parts = action.split("__");
    parts.next();
    let server = parts.next();
    let rest: Vec<&str> = parts.collect();
    match server {
        Some(server) if !rest.is_empty() => format!("{server} · {}", rest.join("__")),
        Some(server) => server.to_string(),
        None => action.to_string(),
    }
}

fn first_line(value: &str, max: usize) -> String {
    let line = value.trim().split('\n').next().unwrap_or_default();
    if line.chars().count() > max {
        let cut: String = line.chars().take(max - 1).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    }
}

/// `toolTarget`: the command line, the file or the pattern a row names.
pub fn tool_target(call: &ToolCall) -> Option<String> {
    let detail = call.detail.as_ref();
    let command = detail
        .and_then(|detail| detail.command.as_deref())
        .or(call.command.as_deref());
    let file = detail
        .and_then(|detail| detail.file.as_deref())
        .or(call.file.as_deref());
    let kind = classify_tool_action(&call.action);
    match (kind, command, file) {
        (ToolKind::Command, Some(command), _) => Some(first_line(command, 120)),
        (ToolKind::Edit | ToolKind::Read, _, Some(file)) => Some(short_path(file)),
        (_, Some(command), _) => Some(first_line(command, 120)),
        (_, _, Some(file)) => Some(short_path(file)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

/// `diffStats`
pub fn diff_stats(diff: Option<&str>) -> Option<DiffStats> {
    let diff = diff.filter(|diff| !diff.is_empty())?;
    let mut stats = DiffStats {
        additions: 0,
        deletions: 0,
    };
    for line in diff.split('\n') {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            stats.additions += 1;
        } else if line.starts_with('-') {
            stats.deletions += 1;
        }
    }
    (stats.additions > 0 || stats.deletions > 0).then_some(stats)
}

/// Commentary, reasoning and tool calls are the activity; the rest are cards.
pub fn is_activity(annotation: &Annotation) -> bool {
    matches!(
        annotation,
        Annotation::Commentary { .. } | Annotation::Reasoning { .. } | Annotation::ToolCall(_)
    )
}

/// A turn in execution order: the prose between bursts of work, and each
/// burst (tools and thinking) as one run.
#[derive(Debug, PartialEq, Eq)]
pub enum TurnSegment<'a> {
    Prose(&'a Annotation),
    Work(Vec<&'a Annotation>),
}

/// `segmentTurn` over the activity annotations.
pub fn segment_turn(annotations: &[Annotation]) -> Vec<TurnSegment<'_>> {
    let mut segments: Vec<TurnSegment> = Vec::new();
    for annotation in annotations
        .iter()
        .filter(|annotation| is_activity(annotation))
    {
        if let Annotation::Commentary { content, .. } = annotation {
            if !content.trim().is_empty() {
                segments.push(TurnSegment::Prose(annotation));
            }
            continue;
        }
        match segments.last_mut() {
            Some(TurnSegment::Work(items)) => items.push(annotation),
            _ => segments.push(TurnSegment::Work(vec![annotation])),
        }
    }
    segments
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnWorkSummary {
    pub counts: BTreeMap<ToolKind, usize>,
    pub edited_files: usize,
    pub failures: usize,
    pub thoughts: usize,
    pub tools: usize,
}

/// `summarizeTurnWork`
pub fn summarize_turn_work(annotations: &[Annotation]) -> TurnWorkSummary {
    let mut summary = TurnWorkSummary::default();
    let mut edited: HashSet<String> = HashSet::new();
    for annotation in annotations {
        match annotation {
            Annotation::Reasoning { .. } => summary.thoughts += 1,
            Annotation::ToolCall(call) => {
                summary.tools += 1;
                if call.failure.is_some() {
                    summary.failures += 1;
                }
                let kind = classify_tool_action(&call.action);
                *summary.counts.entry(kind).or_default() += 1;
                if kind == ToolKind::Edit {
                    let file = call
                        .detail
                        .as_ref()
                        .and_then(|detail| detail.file.clone())
                        .or_else(|| call.file.clone());
                    edited.insert(file.unwrap_or_else(|| call.id.clone()));
                }
            }
            _ => {}
        }
    }
    summary.edited_files = edited.len();
    summary
}

/// `formatElapsed`: `42s`, `3m 05s`, `1h 07m`.
pub fn format_elapsed(ms: i64) -> String {
    let total = ((ms.max(0) as f64) / 1000.0).round() as i64;
    if total < 60 {
        return format!("{total}s");
    }
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

/// `turnDurationMs`
pub fn turn_duration_ms(started_at: Option<&str>, ended_at: Option<&str>) -> Option<i64> {
    let parse = |value: &str| chrono::DateTime::parse_from_rfc3339(value).ok();
    let start = parse(started_at?)?;
    let end = parse(ended_at?)?;
    let ms = (end - start).num_milliseconds();
    (ms >= 0).then_some(ms)
}

/// `formatTokenCount`: `950`, `4.2k`, `38k`, `1.2M`.
pub fn format_token_count(value: u64) -> String {
    if value < 1000 {
        value.to_string()
    } else if value < 1_000_000 {
        let k = value as f64 / 1000.0;
        if value < 10_000 {
            format!("{k:.1}k")
        } else {
            format!("{k:.0}k")
        }
    } else {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    }
}

/// `stripAnsi`: the escape sequences CLIs leave in captured output.
pub fn strip_ansi(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                // Parameters and intermediates, then one final byte @–~.
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\u{7}' {
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Add,
    Del,
    Hunk,
    Meta,
    Context,
}

/// `classifyDiffLine`
pub fn classify_diff_line(line: &str) -> DiffLineKind {
    if line.starts_with("+++") || line.starts_with("---") {
        DiffLineKind::Meta
    } else if line.starts_with("@@") {
        DiffLineKind::Hunk
    } else if line.starts_with('+') {
        DiffLineKind::Add
    } else if line.starts_with('-') {
        DiffLineKind::Del
    } else {
        DiffLineKind::Context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(action: &str, command: Option<&str>, file: Option<&str>) -> ToolCall {
        ToolCall {
            id: action.to_string(),
            action: action.to_string(),
            command: command.map(str::to_string),
            file: file.map(str::to_string),
            content: String::new(),
            detail: None,
            streaming: false,
            created_at: None,
            failure: None,
        }
    }

    #[test]
    fn tools_classify_like_the_desktop() {
        assert_eq!(classify_tool_action("Bash"), ToolKind::Command);
        assert_eq!(classify_tool_action("apply_patch"), ToolKind::Edit);
        assert_eq!(
            classify_tool_action("mcp__dcc__dcc_delegate_task"),
            ToolKind::Agent
        );
        assert_eq!(
            classify_tool_action("mcp__github__create_issue"),
            ToolKind::Mcp
        );
        assert_eq!(classify_tool_action("Whatever"), ToolKind::Other);
        assert_eq!(
            mcp_tool_label("mcp__github__create_issue"),
            "github · create_issue"
        );
    }

    #[test]
    fn targets_and_summaries() {
        assert_eq!(
            tool_target(&call("Edit", None, Some("/repo/src/app/main.rs"))).as_deref(),
            Some("app/main.rs")
        );
        assert_eq!(
            tool_target(&call("Bash", Some("cargo test\nmore"), None)).as_deref(),
            Some("cargo test")
        );
        let annotations = vec![
            Annotation::Reasoning {
                id: "r".into(),
                label: None,
                content: "x".into(),
                streaming: false,
                created_at: None,
            },
            Annotation::ToolCall(call("Edit", None, Some("a.rs"))),
            Annotation::ToolCall(call("Write", None, Some("a.rs"))),
            Annotation::ToolCall(call("Bash", Some("ls"), None)),
        ];
        let summary = summarize_turn_work(&annotations);
        assert_eq!(summary.edited_files, 1);
        assert_eq!(summary.counts[&ToolKind::Edit], 2);
        assert_eq!(summary.thoughts, 1);
        assert_eq!(segment_turn(&annotations).len(), 1, "one burst of work");
    }

    #[test]
    fn formatters() {
        assert_eq!(format_elapsed(42_000), "42s");
        assert_eq!(format_elapsed(185_000), "3m 05s");
        assert_eq!(format_elapsed(4_020_000), "1h 07m");
        assert_eq!(format_token_count(4_200), "4.2k");
        assert_eq!(format_token_count(38_000), "38k");
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
        assert_eq!(
            diff_stats(Some("--- a\n+++ b\n+x\n-y\n+z")),
            Some(DiffStats {
                additions: 2,
                deletions: 1
            })
        );
        assert_eq!(
            turn_duration_ms(Some("2026-05-01T12:00:00Z"), Some("2026-05-01T12:00:05Z")),
            Some(5000)
        );
    }
}
