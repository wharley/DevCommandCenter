//! `help-topics.ts`: the topics in the desktop's order, each with its icon
//! and the shortcut its card shows; the copy comes from `help.topics.<id>`.

use std::ops::Range;

use gpui::SharedString;

use crate::i18n::I18n;
use crate::ui::command_key;

/// `HELP_TOPIC_IDS`, in order.
pub const TOPICS: [&str; 34] = [
    "workspaces",
    "composer",
    "plan",
    "objective",
    "goal",
    "inspector",
    "review",
    "prComments",
    "agents",
    "researcher",
    "reporter",
    "terminal",
    "browser",
    "delegate",
    "handoff",
    "providerAccounts",
    "memory",
    "skills",
    "delivery",
    "undo",
    "mobile",
    "keepAwake",
    "usageLimit",
    "pullRequests",
    "spec",
    "multiProject",
    "mergeConflict",
    "mcp",
    "codeRabbit",
    "automation",
    "usage",
    "search",
    "editor",
    "account",
];

/// `DEFAULT_HELP_TOPIC`
pub const DEFAULT_TOPIC: &str = "workspaces";

/// `HELP_TOPIC_ICONS`, as Lucide names.
pub fn icon(topic: &str) -> &'static str {
    match topic {
        "workspaces" => "folder-git-2",
        "composer" => "message-square-text",
        "plan" => "list-checks",
        "inspector" => "panel-right",
        "review" => "file-diff",
        "prComments" => "messages-square",
        "agents" => "badge-check",
        "researcher" => "lightbulb",
        "reporter" => "newspaper",
        "terminal" => "square-terminal",
        "browser" => "globe",
        "delegate" => "send",
        "handoff" => "arrow-left-right",
        "providerAccounts" => "users-round",
        "memory" => "database",
        "skills" => "sparkles",
        "delivery" => "git-pull-request-arrow",
        "undo" => "undo-2",
        "mobile" => "smartphone",
        "keepAwake" => "coffee",
        "usageLimit" => "hourglass",
        "pullRequests" => "git-pull-request",
        "spec" => "file-text",
        "objective" => "target",
        "goal" => "goal",
        "multiProject" => "layers",
        "mergeConflict" => "git-merge",
        "mcp" => "cable",
        "codeRabbit" => "bot",
        "automation" => "workflow",
        "usage" => "chart-column",
        "search" => "search",
        "editor" => "app-window",
        _ => "circle-user-round",
    }
}

/// `resolveHelpTopicShortcut`: the surface's own shortcut, when it has one.
pub fn shortcut(topic: &str) -> Option<Vec<&'static str>> {
    let command = command_key();
    Some(match topic {
        "workspaces" => vec![command, "K"],
        "composer" => vec![command, "Shift", "L"],
        "inspector" => vec![command, "Shift", "G"],
        "review" => vec![command, "Shift", "E"],
        "terminal" => vec![command, "J"],
        "search" => vec![command, "P"],
        "editor" => vec![command, "O"],
        _ => return None,
    })
}

/// One topic's text, read once when the dialog opens.
#[derive(Clone, Debug, PartialEq)]
pub struct TopicCopy {
    pub id: &'static str,
    pub label: SharedString,
    pub summary: SharedString,
    pub what_is: SharedString,
    pub when_to_use: SharedString,
    pub steps: Vec<SharedString>,
    pub prompt: Option<SharedString>,
    pub tip: Option<SharedString>,
    keywords: SharedString,
}

impl TopicCopy {
    pub fn read(id: &'static str, i18n: &I18n) -> Self {
        let key = |field: &str| format!("help.topics.{id}.{field}");
        // `t(key, { defaultValue: "" })`: missing reads as nothing.
        let optional = |field: &str| {
            let key = key(field);
            i18n.has(&key, &Default::default())
                .then(|| i18n.t(&key))
                .filter(|text| !text.is_empty())
        };
        Self {
            id,
            label: i18n.t(&key("label")),
            summary: i18n.t(&key("summary")),
            what_is: i18n.t(&key("whatIs")),
            when_to_use: i18n.t(&key("whenToUse")),
            steps: i18n.t_list(&key("steps")),
            prompt: optional("prompt"),
            tip: optional("tip"),
            keywords: optional("keywords").unwrap_or_default(),
        }
    }

    /// The palette's keywords for this topic: summary and keywords.
    pub fn palette_keywords(&self) -> String {
        format!("{} {}", self.summary, self.keywords)
    }

    /// What the search looks in: label, summary, keywords and "what is".
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {} {}",
            self.label, self.summary, self.keywords, self.what_is
        );
        crate::data::text_match::matches_all_words(&haystack, query)
    }
}

/// `HelpRichText`: `[Rótulo]` fragments become chips. The text without the
/// brackets, and where each chip sits in it.
pub fn chips(text: &str) -> (String, Vec<Range<usize>>) {
    let mut out = String::with_capacity(text.len());
    let mut ranges = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(length) = rest[open + 1..].find(']') else {
            break;
        };
        // `[^\]]+`: an empty pair stays as it is.
        if length == 0 {
            out.push_str(&rest[..open + 2]);
            rest = &rest[open + 2..];
            continue;
        }
        out.push_str(&rest[..open]);
        let start = out.len();
        out.push_str(&rest[open + 1..open + 1 + length]);
        ranges.push(start..out.len());
        rest = &rest[open + 2 + length..];
    }
    out.push_str(rest);
    (out, ranges)
}
