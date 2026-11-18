//! DCC's own tools that this engine's host chooses not to offer at all
//! (`DCC_HIDE_TOOLS`: comma-separated tool names, or prefixes ending in `_`,
//! such as `dcc_propose_,dcc_team_state`). A product built on `dcc serve`
//! that has no use for some of them (another product's agents, say) keeps
//! them out of every session: they are left out of `tools/list`, and a call
//! made anyway is refused. Unset, every tool is offered, as before.

use std::sync::OnceLock;

fn patterns() -> &'static [String] {
    static PATTERNS: OnceLock<Vec<String>> = OnceLock::new();
    PATTERNS.get_or_init(|| parse(&std::env::var("DCC_HIDE_TOOLS").unwrap_or_default()))
}

fn parse(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(str::to_string)
        .collect()
}

fn matches(patterns: &[String], tool: &str) -> bool {
    patterns
        .iter()
        .any(|pattern| match pattern.strip_suffix('_') {
            Some(_) => tool.starts_with(pattern.as_str()),
            None => tool == pattern,
        })
}

/// Whether this engine keeps `tool` out of every session.
pub(crate) fn hidden(tool: &str) -> bool {
    matches(patterns(), tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_prefixes_hide_only_what_they_name() {
        let hide = parse(" dcc_propose_, dcc_team_state ,,");
        assert!(matches(&hide, "dcc_propose_task"));
        assert!(matches(&hide, "dcc_team_state"));
        assert!(!matches(&hide, "dcc_team_state_more"), "a name is exact");
        assert!(!matches(&hide, "dcc_browser_click"));
        assert!(
            !matches(&parse(""), "dcc_propose_task"),
            "unset: nothing hidden"
        );
    }
}
