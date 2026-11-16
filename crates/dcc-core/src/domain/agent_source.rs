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
pub const EVERY_CHOICES: [u32; 6] = [1, 2, 5, 15, 30, 60];
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
    /// The room its news wakes the agent in (`docs/PLANO_SALA_POR_PROJETO`):
    /// the project of the room it was asked in; none: the Geral room.
    #[serde(default)]
    pub room: Option<String>,
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

/// The ids of items cut from a document of results (`document_items`).
/// A source read before those existed remembers the whole document as one
/// item; `read_news` uses the mark to tell that apart from real news.
const DOCUMENT_ID: &str = "m:";

/// A text result that is JSON in a string (`{"results": "# Search…"}`, as
/// Slack's search answers): its list's elements when it holds one, else
/// its string values as the text.
fn unwrapped(text: &str) -> Option<Result<Vec<Value>, String>> {
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    match value {
        Value::Array(list) => Some(Ok(list)),
        Value::Object(fields) => {
            if let Some(list) = fields.values().find_map(Value::as_array) {
                return Some(Ok(list.clone()));
            }
            let texts: Vec<&str> = fields.values().filter_map(Value::as_str).collect();
            (!texts.is_empty()).then(|| Err(texts.join("\n")))
        }
        Value::String(text) => Some(Err(text)),
        _ => None,
    }
}

/// A document of results under `###` headings ("### Result 1 of 20", one
/// message each), cut into one item a result: what comes before the first
/// heading is the document's title, not an item. Each is known by its
/// permalink or timestamp when it shows one, so a result that moves down
/// the list is still the same. None when the text has no such headings.
fn document_items(text: &str) -> Option<Vec<SourceItem>> {
    let mut blocks: Vec<String> = Vec::new();
    let mut started = false;
    for line in text.lines() {
        if line.trim_start().starts_with("### ") {
            started = true;
            blocks.push(String::new());
            continue;
        }
        if !started || line.trim() == "---" {
            continue;
        }
        if let Some(block) = blocks.last_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    if !started {
        return None;
    }
    let items = blocks
        .iter()
        .map(|block| block.trim())
        .filter(|block| !block.is_empty())
        .map(|block| {
            let own = block.lines().find_map(|line| {
                ["Permalink:", "Message_ts:", "URL:"]
                    .iter()
                    .find_map(|field| line.trim().strip_prefix(field))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            });
            SourceItem {
                id: format!(
                    "{DOCUMENT_ID}{}",
                    own.map_or_else(|| stable_id(block), stable_id)
                ),
                text: clip(block),
            }
        })
        .collect();
    Some(items)
}

fn elements(list: &[Value]) -> Vec<SourceItem> {
    list.iter()
        .map(|element| {
            let text = element.to_string();
            SourceItem {
                id: element_id(element).unwrap_or_else(|| stable_id(&text)),
                text: clip(&text),
            }
        })
        .collect()
}

/// The items of an MCP `tools/call` result: the elements of the first list
/// in its structured content, or else its text — JSON in a string read as
/// what it holds, a document of results cut by its headings, anything else
/// into blocks (blank lines or list entries).
pub fn items_of(result: &Value) -> Vec<SourceItem> {
    let structured = result.get("structuredContent");
    let list = structured.and_then(|content| match content {
        Value::Array(list) => Some(list),
        Value::Object(fields) => fields.values().find_map(Value::as_array),
        _ => None,
    });
    if let Some(list) = list.filter(|list| !list.is_empty()) {
        return elements(list);
    }
    let text: String = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let text = match unwrapped(&text) {
        Some(Ok(list)) if !list.is_empty() => return elements(&list),
        Some(Err(inner)) => inner,
        _ => text,
    };
    if let Some(items) = document_items(&text) {
        return items;
    }
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
/// A source that remembered a document of results as one item (before
/// `document_items`) has none of its results in `seen`: that read only
/// marks what is there, or every old result would count as news.
pub fn read_news(seen: &[String], items: &[SourceItem]) -> (Vec<SourceItem>, Vec<String>) {
    let cut_now = |id: &String| id.starts_with(DOCUMENT_ID);
    let marks_only =
        !seen.is_empty() && items.iter().any(|item| cut_now(&item.id)) && !seen.iter().any(cut_now);
    let news: Vec<SourceItem> = items
        .iter()
        .filter(|item| !marks_only && !seen.contains(&item.id))
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
        "{SOURCE_MARK} \"{}\" ({}): {} new item(s).\n\n{}{}\n\nThis is what the person asked you to watch, not a message they wrote. Act on what needs them: look into it, open a task (it starts or waits as a card, as the person set for you) or pass it to a teammate.\n\nSomeone asking the person something, or naming them about a problem, always needs an answer: never leave it unanswered. Read the thread first. If you can answer from what you can reach (the connectors, your memory, the project's notes), propose the reply with dcc_propose_reply, in the same thread: it goes as the person set for you, by itself or as a card they send. If answering takes looking at the code or the data and you cannot from here, pass it to a teammate who can, saying exactly what to check: the result comes back to you, and then you propose the reply with what was found (what is fine, or what is wrong and that it is being fixed). If it is a problem to fix, pass it on the same way. If you can neither answer nor pass it on, tell the person here in one or two lines what was asked and what you are missing.\n\nA reply goes out in the person's name, so it carries only what was checked: what you or a teammate verified, and what was found. Never propose a reply that says you have not looked yet, that you will check, or that guesses; that is said to the person here, not to whoever asked. A late answer is better than an empty one.\n\nA pull request the person themselves opened is not yours to review, to fix or to pass on, whoever posted its link: comments on it reach its own task by themselves, and only that task changes its code. At most tell the person here that it was mentioned.\n\nAnswer exactly {} and nothing else only when no item asks anything of the person: an announcement, a reply already given, a message that is not for them.",
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
            room: None,
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

        // A document of results in a JSON string (Slack's search): one
        // item a result, known by its permalink.
        let search = |results: &[(&str, &str)]| {
            let body: String = results
                .iter()
                .enumerate()
                .map(|(index, (link, text))| {
                    format!(
                        "### Result {} of {}\nChannel: #suporte\nPermalink: {link}\nText: \n{text}\n\n---\n\n",
                        index + 1,
                        results.len()
                    )
                })
                .collect();
            let wrapped = json!({"results": format!("# Search Results for: \n\n## Messages ({} results)\n{body}", results.len())});
            json!({"content": [{"type": "text", "text": wrapped.to_string()}]})
        };
        let first = items_of(&search(&[("l/1", "@Wharley pode olhar?")]));
        assert_eq!(first.len(), 1);
        assert!(first[0].text.contains("pode olhar") && !first[0].text.contains("Search Results"));
        let (_, seen) = read_news(&[], &first);
        // Two mentions in one interval are two items, and the one already
        // seen is not news though it moved down the list.
        let two = items_of(&search(&[
            ("l/3", "@Wharley e o seller 2?"),
            ("l/2", "@Wharley arredondamento?"),
            ("l/1", "@Wharley pode olhar?"),
        ]));
        let (news, _) = read_news(&seen, &two);
        assert_eq!(news.len(), 2);
        assert!(news[0].text.contains("seller 2") && news[1].text.contains("arredondamento"));
        // A source that remembered the whole document as one item only
        // marks what is there on its next read.
        let (news, remembered) = read_news(&["62efdfb302f53a71".to_string()], &two);
        assert!(news.is_empty());
        assert_eq!(read_news(&remembered, &two).0.len(), 0);

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
