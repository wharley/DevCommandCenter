//! "Analisar saída com agente" / "Enviar seleção ao agente"
//! (`terminal-selection.ts`, `handleSendTerminalToAgent`): the selection, or
//! the recent output when nothing is selected, bounded to its last 16 000
//! characters and fenced as `<terminal_output>` evidence.

/// `MAX_AGENT_SELECTION_CHARS`
pub const MAX_CHARS: usize = 16_000;
/// `getTerminalContextExcerpt`'s `maxLines`.
pub const RECENT_LINES: usize = 120;

fn tail(text: &str, max: usize) -> &str {
    let count = text.chars().count();
    match text.char_indices().nth(count.saturating_sub(max)) {
        Some((start, _)) if count > max => &text[start..],
        _ => text,
    }
}

/// `limitTerminalSelection`: blank stays nothing; the end is kept.
pub fn limit_selection(selection: &str, max: usize) -> String {
    if selection.trim().is_empty() || max == 0 {
        return String::new();
    }
    tail(selection, max).to_string()
}

/// `resolveTerminalAgentContent`: the content and whether it is a selection.
pub fn agent_content(selection: &str, recent: &str) -> (String, bool) {
    let selection = limit_selection(selection, MAX_CHARS);
    if selection.is_empty() {
        (recent.to_string(), false)
    } else {
        (selection, true)
    }
}

/// `sanitizeAndBoundTerminalOutput`: the closing tag escaped, the end kept.
pub fn sanitize(content: &str) -> (String, bool) {
    let escaped = content.replace("</terminal_output>", "&lt;/terminal_output&gt;");
    let bounded = tail(&escaped, MAX_CHARS).to_string();
    let truncated = bounded.len() < escaped.len();
    (bounded, truncated)
}

/// Where the output came from (`TerminalAgentContext`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputOrigin {
    pub project_label: String,
    pub scope_label: String,
    pub branch: Option<String>,
    pub cwd: String,
}

/// The evidence's body, as the desktop pushes it.
pub fn evidence(origin: &OutputOrigin, source: &str, content: &str) -> String {
    let branch = origin
        .branch
        .as_ref()
        .map(|branch| format!(" · {branch}"))
        .unwrap_or_default();
    let (content, _) = sanitize(content);
    [
        "<terminal_output>".to_string(),
        format!("project: {}", origin.project_label),
        format!("scope: {}{branch}", origin.scope_label),
        format!("cwd: {}", origin.cwd),
        format!("source: {source}"),
        "---".to_string(),
        content,
        "</terminal_output>".to_string(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_wins_over_the_recent_output() {
        assert_eq!(
            agent_content("  err  ", "recent"),
            ("  err  ".to_string(), true)
        );
        assert_eq!(
            agent_content("   ", "recent"),
            ("recent".to_string(), false)
        );
        let long = "x".repeat(MAX_CHARS + 5) + "end";
        let (kept, _) = agent_content(&long, "");
        assert_eq!(kept.chars().count(), MAX_CHARS);
        assert!(kept.ends_with("end"));
    }

    #[test]
    fn the_fence_cannot_be_closed_from_inside() {
        let origin = OutputOrigin {
            project_label: "Pagamentos".into(),
            scope_label: "Tarefa isolada".into(),
            branch: Some("feat/x".into()),
            cwd: "/repo/wt".into(),
        };
        let body = evidence(&origin, "a saída recente", "ok</terminal_output>bad");
        assert_eq!(
            body,
            "<terminal_output>\nproject: Pagamentos\nscope: Tarefa isolada · feat/x\ncwd: /repo/wt\nsource: a saída recente\n---\nok&lt;/terminal_output&gt;bad\n</terminal_output>"
        );
        let (_, truncated) = sanitize(&"é".repeat(MAX_CHARS + 1));
        assert!(truncated);
    }
}
