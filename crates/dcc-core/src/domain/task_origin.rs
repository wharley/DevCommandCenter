//! Where a task an agent opened came from, read back from what DCC already
//! records — no id of its own: the task is the identifier. A task opened
//! with `dcc_open_task` has a delegation record naming the agent's
//! conversation and the turn that asked; that turn's prompt says what
//! started it: the person's message in the room, a teammate that passed it
//! on (`↪ Suporte → Dev: …`, followed back to the teammate's own turn), a
//! source's news, a routine, or another event DCC woke the agent with. The
//! steps come out oldest first, ending at the agent that opened the task.

use super::agent_routine::routine_request;
use super::agent_source::source_request;
use super::team_room::{handoff_heading, HANDOFF_MARK};

/// How far a pass is followed back.
pub const MAX_PASSES: usize = 5;
const EXCERPT_CHARS: usize = 160;

/// What started a turn, as its prompt reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Started {
    /// A source's news: the first item, and its link when the connector
    /// gave one.
    Source {
        connector: String,
        watch: String,
        excerpt: String,
        link: Option<String>,
    },
    Routine {
        request: String,
    },
    /// A teammate passed it on.
    Passed {
        from: String,
        to: String,
        request: String,
    },
    /// Something else DCC woke the agent with (a trigger, a status).
    Event {
        line: String,
    },
    /// The person wrote it.
    Person {
        excerpt: String,
    },
}

/// One step of a task's origin, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginStep {
    pub started: Started,
    /// The agent whose turn it was, and where.
    pub agent_id: String,
    pub agent: String,
    pub session_id: String,
    /// The project whose room the conversation is in; none: Geral.
    pub room: Option<String>,
    pub at: String,
}

/// A task's origin: the turns that led to it, and the agent that opened it
/// (the last step's).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskOrigin {
    pub steps: Vec<OriginStep>,
    /// When the task was opened.
    pub opened_at: String,
}

fn excerpt(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= EXCERPT_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(EXCERPT_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// The value of `"field":"…"` in an item written as JSON, cut or not.
fn json_text(item: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\":\"");
    let rest = &item[item.find(&key)? + key.len()..];
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                'n' | 't' | 'r' => value.push(' '),
                '/' => value.push('/'),
                other => value.push(other),
            },
            other => value.push(other),
        }
    }
    // Cut before its closing quote: what there is of it.
    (!value.is_empty()).then_some(value)
}

/// The first item of a source's prompt: its words and its link.
fn first_item(prompt: &str) -> (String, Option<String>) {
    let mut item = String::new();
    for line in prompt.lines().skip(1) {
        if let Some(first) = line.strip_prefix("- ") {
            if !item.is_empty() {
                break;
            }
            item.push_str(first);
        } else if !item.is_empty() {
            match line.strip_prefix("  ") {
                Some(more) => {
                    item.push(' ');
                    item.push_str(more);
                }
                None => break,
            }
        }
    }
    let link = ["permalink", "url", "html_url", "link"]
        .iter()
        .find_map(|field| json_text(&item, field))
        .filter(|link| link.starts_with("https://") || link.starts_with("http://"));
    let words = ["text", "title", "name", "subject", "body"]
        .iter()
        .find_map(|field| json_text(&item, field))
        .unwrap_or_else(|| item.clone());
    (excerpt(&words), link)
}

/// What started the turn `prompt` opened.
pub fn started(prompt: &str) -> Started {
    if let Some((watch, connector)) = source_request(prompt) {
        let (excerpt, link) = first_item(prompt);
        return Started::Source {
            connector: connector.to_string(),
            watch: watch.to_string(),
            excerpt,
            link,
        };
    }
    if let Some(request) = routine_request(prompt) {
        return Started::Routine {
            request: excerpt(request),
        };
    }
    if let Some(heading) = handoff_heading(prompt) {
        let rest = heading.trim_start_matches(HANDOFF_MARK);
        if let Some((from, rest)) = rest.split_once(" → ") {
            let (to, request) = rest.split_once(": ").unwrap_or((rest, ""));
            return Started::Passed {
                from: from.trim().to_string(),
                to: to.trim().to_string(),
                request: excerpt(request),
            };
        }
    }
    if prompt.starts_with("[DCC]") {
        return Started::Event {
            line: excerpt(prompt.lines().next().unwrap_or_default()),
        };
    }
    Started::Person {
        excerpt: excerpt(prompt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent_source::{source_prompt, AgentSource, SourceItem};
    use crate::domain::team_room::handoff_prompt;

    fn slack() -> AgentSource {
        AgentSource {
            id: "s".into(),
            agent_id: "sup".into(),
            connector_id: "slack".into(),
            connector: "Slack".into(),
            watch: "menções a mim".into(),
            tool: "slack_search_public".into(),
            arguments: "{}".into(),
            every_minutes: 15,
            enabled: true,
            proposed: false,
            last_checked_at: None,
            created_at: String::new(),
            room: None,
        }
    }

    #[test]
    fn a_sources_turn_reads_as_its_first_item_with_the_link() {
        let item = serde_json::json!({
            "ts": "1731",
            "text": "@wharley o CEP não\ncalcula frete",
            "permalink": "https://acme.slack.com/archives/C1/p1731"
        });
        let prompt = source_prompt(
            &slack(),
            &[
                SourceItem {
                    id: "1".into(),
                    text: item.to_string(),
                },
                SourceItem {
                    id: "2".into(),
                    text: "{\"text\":\"outra\"}".into(),
                },
            ],
        );
        assert_eq!(
            started(&prompt),
            Started::Source {
                connector: "Slack".into(),
                watch: "menções a mim".into(),
                excerpt: "@wharley o CEP não calcula frete".into(),
                link: Some("https://acme.slack.com/archives/C1/p1731".into()),
            }
        );
        // An item that is plain text has no link; its words are the excerpt.
        let plain = source_prompt(
            &slack(),
            &[SourceItem {
                id: "1".into(),
                text: "Giovanni: o deploy sai hoje?".into(),
            }],
        );
        match started(&plain) {
            Started::Source { excerpt, link, .. } => {
                assert_eq!(excerpt, "Giovanni: o deploy sai hoje?");
                assert_eq!(link, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_pass_a_routine_an_event_and_the_person_are_told_apart() {
        let passed = handoff_prompt("Suporte", "Dev", "corrige o CEP", "O campo não valida.");
        assert_eq!(
            started(&passed),
            Started::Passed {
                from: "Suporte".into(),
                to: "Dev".into(),
                request: "corrige o CEP".into(),
            }
        );
        assert_eq!(
            started("[DCC] Routine (09:00): resumo dos PRs\n\nThis is a routine…"),
            Started::Routine {
                request: "resumo dos PRs".into()
            }
        );
        assert_eq!(
            started("[DCC] Task \"x\" ended a turn\nmore"),
            Started::Event {
                line: "[DCC] Task \"x\" ended a turn".into()
            }
        );
        assert_eq!(
            started("corrige o   login\npor favor"),
            Started::Person {
                excerpt: "corrige o login por favor".into()
            }
        );
        // A long message is cut.
        match started(&"a".repeat(400)) {
            Started::Person { excerpt } => assert_eq!(excerpt.chars().count(), EXCERPT_CHARS + 1),
            other => panic!("{other:?}"),
        }
    }
}
