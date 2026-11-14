//! The "Nova fonte" form's fill button: the person says what to watch in
//! their own words ("menções a mim") and the agent picks, among the
//! connector's read-only tools, the one that returns just that and its
//! arguments. One read-only run with no conversation; its answer is JSON
//! the form puts in its fields, for the person to test and change.

use serde_json::Value;

/// A tool as the prompt lists it.
pub struct SuggestTool<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub input_schema: &'a Value,
}

/// How much of a tool's description and schema the prompt carries.
const MAX_DESCRIPTION: usize = 600;
const MAX_SCHEMA: usize = 3_000;
/// Tools listed at most.
const MAX_TOOLS: usize = 40;

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}…")
}

/// What the agent is asked.
pub fn suggest_prompt(connector: &str, watch: &str, tools: &[SuggestTool<'_>]) -> String {
    let listed: Vec<String> = tools
        .iter()
        .take(MAX_TOOLS)
        .map(|tool| {
            format!(
                "- {}\n  description: {}\n  inputSchema: {}",
                tool.name,
                clip(&tool.description.replace('\n', " "), MAX_DESCRIPTION),
                clip(&tool.input_schema.to_string(), MAX_SCHEMA)
            )
        })
        .collect();
    format!(
        "The person wants DCC to watch something in their {connector} connector: \"{watch}\".\n\nDCC will call ONE of the connector's read-only tools every few minutes, with fixed arguments, and compare the items it returns with the ones seen before. Pick the tool and the arguments that return just what the person described, newest first when the tool allows it. Use only arguments in the tool's inputSchema. \"Me\" is the person whose account the connector is signed in with: use the connector's own way of saying that (a search operator, a \"me\" value) when it has one, and leave an argument out rather than invent an id you do not know.\n\nThe connector's read-only tools:\n{}\n\nDo not call any tool and do not read files. Answer with one JSON object and nothing else:\n{{\"tool\": \"<one of the names above>\", \"arguments\": {{…}}, \"note\": \"<one short sentence, in the person's language, on what it returns or what they should check>\"}}",
        listed.join("\n")
    )
}

/// The agent's pick.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub tool: String,
    pub arguments: Value,
    pub note: String,
}

/// The pick in the agent's answer, if it is one of `tools` with arguments
/// as an object. Text around the JSON (a code fence, a sentence) is
/// ignored.
pub fn parse_suggestion(answer: &str, tools: &[&str]) -> Result<Suggestion, String> {
    let start = answer
        .find('{')
        .ok_or("the agent did not answer a suggestion")?;
    let end = answer
        .rfind('}')
        .ok_or("the agent did not answer a suggestion")?;
    if end < start {
        return Err("the agent did not answer a suggestion".to_string());
    }
    let value: Value = serde_json::from_str(&answer[start..=end])
        .map_err(|_| "the agent's suggestion could not be read".to_string())?;
    let tool = value
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if !tools.contains(&tool) {
        return Err(format!(
            "the agent picked \"{tool}\", which is not one of the connector's read-only tools"
        ));
    }
    let arguments = match value.get("arguments") {
        Some(arguments) if arguments.is_object() => arguments.clone(),
        None | Some(Value::Null) => Value::Object(Default::default()),
        Some(_) => return Err("the agent's arguments are not an object".to_string()),
    };
    Ok(Suggestion {
        tool: tool.to_string(),
        arguments,
        note: value
            .get("note")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_prompt_lists_the_tools_and_what_to_watch() {
        let schema = json!({"type": "object", "properties": {"query": {"type": "string"}}});
        let prompt = suggest_prompt(
            "Slack",
            "menções a mim",
            &[SuggestTool {
                name: "slack_search_public",
                description: "Finds messages",
                input_schema: &schema,
            }],
        );
        assert!(prompt.contains("\"menções a mim\""));
        assert!(prompt.contains("- slack_search_public"));
        assert!(prompt.contains("\"query\""));
    }

    #[test]
    fn the_pick_is_read_out_of_the_answer_and_checked() {
        let tools = ["slack_search_public", "slack_read_channel"];
        let answer = "```json\n{\"tool\": \"slack_search_public\", \"arguments\": {\"query\": \"to:me\"}, \"note\": \"Mensagens que mencionam você.\"}\n```";
        let pick = parse_suggestion(answer, &tools).unwrap();
        assert_eq!(pick.tool, "slack_search_public");
        assert_eq!(pick.arguments, json!({"query": "to:me"}));
        assert_eq!(pick.note, "Mensagens que mencionam você.");
        // No arguments is an empty object.
        let bare = parse_suggestion("{\"tool\": \"slack_read_channel\"}", &tools).unwrap();
        assert_eq!(bare.arguments, json!({}));
        // A tool that is not offered, arguments that are a list, no JSON.
        assert!(parse_suggestion("{\"tool\": \"slack_send\"}", &tools).is_err());
        assert!(parse_suggestion(
            "{\"tool\": \"slack_read_channel\", \"arguments\": [1]}",
            &tools
        )
        .is_err());
        assert!(parse_suggestion("I could not decide.", &tools).is_err());
    }
}
