//! An agent's sources (Compozy's heartbeat, with any connector): what the
//! person asked it to watch — "menções a mim" in Slack, "tarefas atribuídas
//! a mim" in ClickUp, a Trello list — each one connector's read-only tool
//! and its arguments, chosen once by the agent and created by the person.
//! DCC calls that tool itself every few minutes, with no model, and wakes
//! the agent only with what is new since the last read; the first read only
//! marks what is there. An agent may watch several sources.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROPOSE_SOURCE_TOOL: &str = "dcc_propose_source";
pub const MAX_SOURCES_PER_AGENT: usize = 8;
/// How often a source may be read, in minutes.
pub const EVERY_CHOICES: [u32; 4] = [5, 15, 30, 60];
pub const DEFAULT_EVERY: u32 = 15;
/// Items remembered as seen per source.
const MAX_SEEN: usize = 300;
/// Items sent to the agent at once, and how much of each.
const MAX_NEW_ITEMS: usize = 10;
const MAX_ITEM_CHARS: usize = 600;
pub const MAX_ARGUMENTS_CHARS: usize = 4_000;
/// How a source's turn opens in the agent's conversation.
pub const SOURCE_MARK: &str = "[DCC] Source";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSource {
    pub id: String,
    pub agent_id: String,
    /// The connector's definition id and its name as the person sees it.
    pub connector_id: String,
    pub connector: String,
    /// What to watch, in the person's words.
    pub watch: String,
    /// The connector's read-only tool and its arguments (a JSON object).
    pub tool: String,
    pub arguments: String,
    pub every_minutes: u32,
    pub enabled: bool,
    /// Proposed by the agent, waiting for the person.
    pub proposed: bool,
    pub last_checked_at: Option<String>,
    pub created_at: String,
}

impl AgentSource {
    /// Whether it is time to read it again.
    pub fn due(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        if !self.enabled || self.proposed {
            return false;
        }
        let Some(last) = self
            .last_checked_at
            .as_deref()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        else {
            return true;
        };
        now - last.with_timezone(&chrono::Utc)
            >= chrono::Duration::minutes(i64::from(self.every_minutes))
    }
}

/// One thing a source returned, known by a stable id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceItem {
    pub id: String,
    pub text: String,
}

fn stable_id(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

fn clip(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_ITEM_CHARS {
        return text.to_string();
    }
    let cut: String = text.chars().take(MAX_ITEM_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// The id an element of structured content carries, when it has one.
fn element_id(element: &Value) -> Option<String> {
    ["id", "ts", "url", "permalink", "key"]
        .iter()
        .find_map(|field| match element.get(field) {
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            Some(Value::Number(value)) => Some(value.to_string()),
            _ => None,
        })
}

/// The items of an MCP `tools/call` result: the elements of the first list
/// in its structured content, or else its text cut into blocks (blank lines
/// or list entries).
pub fn items_of(result: &Value) -> Vec<SourceItem> {
    let structured = result.get("structuredContent");
    let list = structured.and_then(|content| match content {
        Value::Array(list) => Some(list),
        Value::Object(fields) => fields.values().find_map(Value::as_array),
        _ => None,
    });
    if let Some(list) = list.filter(|list| !list.is_empty()) {
        return list
            .iter()
            .map(|element| {
                let text = element.to_string();
                SourceItem {
                    id: element_id(element).unwrap_or_else(|| stable_id(&text)),
                    text: clip(&text),
                }
            })
            .collect();
    }
    let text: String = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let mut items: Vec<SourceItem> = Vec::new();
    let mut block = String::new();
    let mut flush = |block: &mut String| {
        let trimmed = block.trim();
        if !trimmed.is_empty() {
            items.push(SourceItem {
                id: stable_id(trimmed),
                text: clip(trimmed),
            });
        }
        block.clear();
    };
    for line in text.lines() {
        let entry = line.trim_start().starts_with(['-', '*', '•'])
            || line
                .trim_start()
                .split_once(". ")
                .is_some_and(|(number, _)| number.chars().all(|c| c.is_ascii_digit()));
        if line.trim().is_empty() || entry {
            flush(&mut block);
        }
        if !line.trim().is_empty() {
            block.push_str(line);
            block.push('\n');
        }
    }
    flush(&mut block);
    items
}

/// The items not in `seen`, and `seen` with all of `items` remembered.
pub fn read_news(seen: &[String], items: &[SourceItem]) -> (Vec<SourceItem>, Vec<String>) {
    let news: Vec<SourceItem> = items
        .iter()
        .filter(|item| !seen.contains(&item.id))
        .cloned()
        .collect();
    let mut remembered: Vec<String> = items.iter().map(|item| item.id.clone()).collect();
    let older: Vec<String> = seen
        .iter()
        .filter(|id| !remembered.contains(id))
        .cloned()
        .collect();
    remembered.extend(older);
    remembered.truncate(MAX_SEEN);
    (news, remembered)
}

/// What the agent gets when its source has news.
pub fn source_prompt(source: &AgentSource, news: &[SourceItem]) -> String {
    let listed: Vec<String> = news
        .iter()
        .take(MAX_NEW_ITEMS)
        .map(|item| format!("- {}", item.text.replace('\n', "\n  ")))
        .collect();
    let more = news.len().saturating_sub(MAX_NEW_ITEMS);
    format!(
        "{SOURCE_MARK} \"{}\" ({}): {} new item(s).\n\n{}{}\n\nThis is what the person asked you to watch, not a message they wrote. Act on what needs them: look into it, open a task (it starts or waits as a card, as the person set for you) or pass it to a teammate. If nothing here needs the person, answer exactly {} and nothing else.",
        source.watch,
        source.connector,
        news.len(),
        listed.join("\n"),
        if more > 0 {
            format!("\n…and {more} more.")
        } else {
            String::new()
        },
        super::agent_routine::QUIET_ANSWER
    )
}

/// The source's watch and connector, when `text` is a source's turn.
pub fn source_request(text: &str) -> Option<(&str, &str)> {
    let rest = text
        .strip_prefix(SOURCE_MARK)?
        .trim_start()
        .strip_prefix('"')?;
    let (watch, rest) = rest.split_once("\" (")?;
    let (connector, _) = rest.split_once("):")?;
    Some((watch, connector))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source() -> AgentSource {
        AgentSource {
            id: "src".into(),
            agent_id: "sup".into(),
            connector_id: "slack".into(),
            connector: "Slack".into(),
            watch: "menções a mim".into(),
            tool: "slack_search_public".into(),
            arguments: "{}".into(),
            every_minutes: 15,
            enabled: true,
            proposed: false,
            last_checked_at: Some("2026-11-09T10:00:00Z".into()),
            created_at: "2026-11-09T09:00:00Z".into(),
        }
    }

    #[test]
    fn a_source_is_read_when_due_and_only_news_wakes_the_agent() {
        let at = |text: &str| chrono::DateTime::parse_from_rfc3339(text).unwrap().to_utc();
        assert!(!source().due(at("2026-11-09T10:10:00Z")));
        assert!(source().due(at("2026-11-09T10:15:00Z")));
        let mut proposed = source();
        proposed.proposed = true;
        assert!(!proposed.due(at("2026-11-09T12:00:00Z")));

        // Text results are cut into entries.
        let text = json!({"content": [{"type": "text", "text": "# Results\n\n- Giovanni: @Wharley o campo X?\n- Ana: deploy ok\n"}]});
        let items = items_of(&text);
        assert_eq!(items.len(), 3);
        let (news, seen) = read_news(&[], &items);
        assert_eq!(news.len(), 3);
        let again = json!({"content": [{"type": "text", "text": "# Results\n\n- Giovanni: @Wharley o campo X?\n- Ana: deploy ok\n- Bia: @Wharley pode olhar?\n"}]});
        let (news, _) = read_news(&seen, &items_of(&again));
        assert_eq!(news.len(), 1);
        assert!(news[0].text.contains("Bia"));

        // Structured results keep their own ids.
        let structured = json!({"structuredContent": {"tasks": [{"id": "t1", "name": "Ver campo"}, {"id": "t2", "name": "Deploy"}]}});
        let items = items_of(&structured);
        assert_eq!(
            items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["t1", "t2"]
        );

        let prompt = source_prompt(&source(), &news);
        assert_eq!(source_request(&prompt), Some(("menções a mim", "Slack")));
        assert!(prompt.contains("NOTHING_TO_REPORT"));
        assert_eq!(source_request("oi"), None);
    }
}
