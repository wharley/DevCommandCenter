//! `dcc_help`: DCC's own help, as the "?" panel shows it, for an agent to
//! read before it explains how something in the app works — so it names the
//! screens and buttons the app has instead of making them up. The text is
//! the desktop's (`help.topics` of its catalogs, compiled in); with no
//! topic it lists them. Static text: no session, no database, no model.

use std::sync::OnceLock;

use dcc_core::domain::agent_conversation::HELP_TOOL;
use serde::Deserialize;
use serde_json::{json, Value};

const PT_BR: &str = include_str!("../../../apps/native/locales/pt-BR/common.json");
const EN: &str = include_str!("../../../apps/native/locales/en/common.json");

const LANGUAGES: [&str; 2] = ["pt-BR", "en"];

/// `help.topics` of each catalog, parsed once; the rest is dropped.
fn topics(language: &str) -> &'static Value {
    static TOPICS: OnceLock<[Value; 2]> = OnceLock::new();
    let all = TOPICS.get_or_init(|| {
        [PT_BR, EN].map(|source| {
            serde_json::from_str::<Value>(source)
                .ok()
                .map(|mut catalog| catalog["help"]["topics"].take())
                .unwrap_or(Value::Null)
        })
    });
    &all[usize::from(language == "en")]
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelpArgs {
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

pub fn is_help_tool(name: &str) -> bool {
    name == HELP_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": HELP_TOOL,
        "description": "DCC's own help, the text of its \"?\" panel. With no topic it lists the topics (id — name: what it covers); with a topic it returns that page: what it is, when to use it, the steps and a tip. Read the right page before explaining how something in DCC works or where a setting is, and explain only what it says: names in [brackets] are the screens and buttons exactly as the app shows them, so quote them as they are. When no page covers the question, say you do not know rather than describing a screen from memory. Pass language \"en\" when the person uses DCC in English.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "topic": {"type": "string", "minLength": 1, "maxLength": 64, "description": "A topic's id from the list, e.g. agentSources."},
                "language": {"type": "string", "enum": LANGUAGES, "default": "pt-BR"}
            }
        },
        "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

pub fn parse(arguments: Option<Value>) -> Option<HelpArgs> {
    let args = match arguments {
        None | Some(Value::Null) => HelpArgs {
            topic: None,
            language: None,
        },
        Some(arguments) => serde_json::from_value::<HelpArgs>(arguments).ok()?,
    };
    let known = args
        .language
        .as_deref()
        .is_none_or(|language| LANGUAGES.contains(&language));
    let short = args
        .topic
        .as_deref()
        .is_none_or(|topic| !topic.trim().is_empty() && topic.len() <= 64);
    (known && short).then_some(args)
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    parse(arguments.clone()).is_some()
}

fn text(message: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": is_error})
}

/// The topics, one a line: `id — name: what it covers`.
fn index(topics: &Value) -> String {
    topics
        .as_object()
        .map(|topics| {
            topics
                .iter()
                .map(|(id, topic)| {
                    format!(
                        "{id} — {}: {}",
                        topic["label"].as_str().unwrap_or(id),
                        topic["summary"].as_str().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// One topic as a page, or none when there is no such topic.
fn page(topics: &Value, id: &str) -> Option<String> {
    let topic = topics.get(id)?.as_object()?;
    let field = |name: &str| topic.get(name).and_then(Value::as_str).unwrap_or_default();
    let mut page = format!(
        "# {}\n{}\n\n{}\n\n{}",
        field("label"),
        field("summary"),
        field("whatIs"),
        field("whenToUse")
    );
    let steps = topic.get("steps").and_then(Value::as_array);
    for (number, step) in steps.into_iter().flatten().enumerate() {
        if let Some(step) = step.as_str() {
            page.push_str(&format!("\n{}. {step}", number + 1));
        }
    }
    if !field("tip").is_empty() {
        page.push_str(&format!("\n\n{}", field("tip")));
    }
    Some(page)
}

/// The index, a page, or the list again when the topic is not one.
pub fn read_help(args: &HelpArgs) -> (String, bool) {
    let topics = topics(args.language.as_deref().unwrap_or("pt-BR"));
    match args.topic.as_deref().map(str::trim) {
        None => (index(topics), false),
        Some(id) => match page(topics, id) {
            Some(page) => (page, false),
            None => (
                format!("no help topic \"{id}\"; the topics are:\n{}", index(topics)),
                true,
            ),
        },
    }
}

pub fn dispatch(arguments: Option<Value>) -> Value {
    match parse(arguments) {
        Some(args) => {
            let (message, is_error) = read_help(&args);
            text(message, is_error)
        }
        None => text("dcc_help takes a topic id and a language".to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_help_lists_its_topics_and_reads_a_page_in_each_language() {
        assert!(call_is_well_formed(&None));
        assert!(!call_is_well_formed(&Some(json!({"topic": " "}))));
        assert!(!call_is_well_formed(&Some(json!({"language": "fr"}))));
        assert!(!call_is_well_formed(&Some(json!({"page": 2}))));

        let (list, error) = read_help(&parse(None).unwrap());
        assert!(!error);
        for id in [
            "agents",
            "agentRooms",
            "agentSources",
            "agentAutoSend",
            "mcp",
        ] {
            assert!(list.contains(&format!("{id} — ")), "{id}");
        }
        let read = |args: Value| read_help(&parse(Some(args)).unwrap());
        let (page, error) = read(json!({"topic": "agentSources"}));
        assert!(!error);
        // The labels stay as the app shows them, steps are numbered.
        assert!(page.contains("[Avisar na sala]") && page.contains("\n1. "));
        let (english, _) = read(json!({"topic": "agentSources", "language": "en"}));
        assert!(english.contains("[Notify in room]"));
        // No such topic: it says so and lists the ones there are.
        let (missing, error) = read(json!({"topic": "nope"}));
        assert!(error && missing.contains("agentRooms — "));
    }
}
