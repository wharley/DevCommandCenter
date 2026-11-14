//! The "Nova fonte" form's data (`dcc_tauri::source_form`): the connectors
//! an agent may read, a connector's read-only tools with the arguments each
//! takes, the agent's pick for what the person wrote, a test read and the
//! source created — all asked of the desktop, which owns the connectors.
//! The tool's arguments are a JSON schema; `fields` reads it into the
//! inputs the form draws, and `arguments` reads what was typed back into
//! the JSON object the tool takes.

use std::time::Duration;

use dcc_core::domain::agent_source::AgentSource;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::data::desktop_api::{ApiError, DesktopApi, encode};

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FormTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// What a test read brought.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceTest {
    pub count: usize,
    pub items: Vec<String>,
}

/// The agent's pick for what the person wrote.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceSuggestion {
    pub tool: String,
    pub arguments: Value,
    pub note: String,
    pub tools: Vec<FormTool>,
}

/// What an argument holds, as the form asks for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number,
    /// A switch.
    Flag,
    /// A list or an object: typed as JSON.
    Json,
}

/// One argument of a tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub required: bool,
    /// The vendor's description, with the allowed values when it lists them.
    pub hint: String,
}

fn kind_of(property: &Value) -> FieldKind {
    let named = match property.get("type") {
        Some(Value::String(name)) => Some(name.as_str()),
        // `["string", "null"]`: the one that is not null.
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .find(|name| *name != "null"),
        _ => None,
    };
    match named {
        Some("string") => FieldKind::Text,
        Some("integer" | "number") => FieldKind::Number,
        Some("boolean") => FieldKind::Flag,
        _ => FieldKind::Json,
    }
}

/// The tool's arguments as fields: the required ones first, each group in
/// the schema's order.
pub fn fields(schema: &Value) -> Vec<Field> {
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut found: Vec<Field> = schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, property)| {
            let mut hint = property
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let choices: Vec<String> = property
                .get("enum")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|choice| match choice {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect();
            if !choices.is_empty() {
                if !hint.is_empty() {
                    hint.push(' ');
                }
                hint.push_str(&format!("({})", choices.join(", ")));
            }
            Field {
                name: name.clone(),
                kind: kind_of(property),
                required: required.contains(&name.as_str()),
                hint,
            }
        })
        .collect();
    found.sort_by_key(|field| !field.required);
    found
}

/// What the person put in a field: its text, or a switch's state (none:
/// never touched).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Typed {
    pub text: String,
    pub flag: Option<bool>,
}

/// Why the arguments cannot be sent: the field, and what is wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BadField {
    Missing(String),
    NotANumber(String),
    NotJson(String),
}

/// The JSON object the tool takes: each field as its kind reads it, the
/// empty optional ones left out, and `extra` (arguments the agent picked
/// that the schema does not list) kept as they came.
pub fn arguments(
    fields: &[Field],
    typed: &[Typed],
    extra: &Map<String, Value>,
) -> Result<Value, BadField> {
    let mut object = extra.clone();
    for (field, typed) in fields.iter().zip(typed) {
        let text = typed.text.trim();
        let value = match field.kind {
            FieldKind::Flag => typed.flag.map(Value::Bool),
            _ if text.is_empty() => None,
            FieldKind::Text => Some(Value::String(text.to_string())),
            FieldKind::Number => Some(
                text.parse::<i64>()
                    .map(Value::from)
                    .or_else(|_| text.parse::<f64>().map(Value::from))
                    .map_err(|_| BadField::NotANumber(field.name.clone()))?,
            ),
            FieldKind::Json => Some(
                serde_json::from_str(text).map_err(|_| BadField::NotJson(field.name.clone()))?,
            ),
        };
        match value {
            Some(value) => {
                object.insert(field.name.clone(), value);
            }
            None if field.required => return Err(BadField::Missing(field.name.clone())),
            None => {
                object.remove(&field.name);
            }
        }
    }
    Ok(Value::Object(object))
}

/// `arguments` (the agent's pick) as what each field shows, and the ones no
/// field holds.
pub fn typed_from(fields: &[Field], arguments: &Value) -> (Vec<Typed>, Map<String, Value>) {
    let mut rest = arguments.as_object().cloned().unwrap_or_default();
    let typed = fields
        .iter()
        .map(|field| match rest.remove(&field.name) {
            None | Some(Value::Null) => Typed::default(),
            Some(Value::Bool(on)) if field.kind == FieldKind::Flag => Typed {
                text: String::new(),
                flag: Some(on),
            },
            Some(Value::String(text)) if field.kind != FieldKind::Json => {
                Typed { text, flag: None }
            }
            Some(other) => Typed {
                text: other.to_string(),
                flag: None,
            },
        })
        .collect();
    (typed, rest)
}

fn form_path(agent_id: &str, action: &str) -> String {
    format!("/api/v1/agents/{}/source-form/{action}", encode(agent_id))
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// The connectors the agent may read, as `(id, name)`.
    pub fn source_connectors(&self, agent_id: &str) -> Result<Vec<(String, String)>, ApiError> {
        let value = self.get(&form_path(agent_id, "connectors"))?;
        Ok(value
            .get("connectors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|connector| {
                Some((
                    connector.get("id")?.as_str()?.to_string(),
                    connector.get("name")?.as_str()?.to_string(),
                ))
            })
            .collect())
    }

    /// The connector's read-only tools, asked of it now.
    pub fn source_tools(
        &self,
        agent_id: &str,
        connector_id: &str,
    ) -> Result<Vec<FormTool>, ApiError> {
        let value: Value = self.request_waiting(
            "POST",
            &form_path(agent_id, "tools"),
            Some(&json!({ "connectorId": connector_id })),
            Duration::from_secs(45),
        )?;
        decode(value.get("tools").cloned().unwrap_or_else(|| json!([])))
    }

    /// The agent's pick for `watch` (one model run).
    pub fn source_suggestion(
        &self,
        agent_id: &str,
        connector_id: &str,
        watch: &str,
    ) -> Result<SourceSuggestion, ApiError> {
        decode(self.request_waiting(
            "POST",
            &form_path(agent_id, "suggest"),
            Some(&json!({ "connectorId": connector_id, "watch": watch })),
            Duration::from_secs(180),
        )?)
    }

    /// One read with what the form holds; nothing is stored.
    pub fn source_test(
        &self,
        agent_id: &str,
        connector_id: &str,
        tool: &str,
        arguments: &Value,
    ) -> Result<SourceTest, ApiError> {
        decode(self.request_waiting(
            "POST",
            &form_path(agent_id, "test"),
            Some(&json!({ "connectorId": connector_id, "tool": tool, "arguments": arguments })),
            Duration::from_secs(60),
        )?)
    }

    /// "Salvar" on a source being edited.
    pub fn update_source(
        &self,
        agent_id: &str,
        source_id: &str,
        source: &Value,
    ) -> Result<(), ApiError> {
        self.post(
            &format!(
                "/api/v1/agents/{}/sources/{}/update",
                encode(agent_id),
                encode(source_id)
            ),
            source,
        )
        .map(|_| ())
    }

    /// "Criar fonte": stored and on.
    pub fn create_source(&self, agent_id: &str, source: &Value) -> Result<AgentSource, ApiError> {
        decode(self.post(
            &format!("/api/v1/agents/{}/sources", encode(agent_id)),
            source,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "description": "How many."},
                "query": {"type": "string", "description": "Search text."},
                "sort": {"type": ["string", "null"], "enum": ["timestamp", "score"]},
                "include_bots": {"type": "boolean"},
                "channels": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["query"]
        })
    }

    #[test]
    fn a_schema_reads_as_fields_with_the_required_first() {
        let fields = fields(&search_schema());
        assert_eq!(fields[0].name, "query");
        assert!(fields[0].required);
        let kind = |name: &str| fields.iter().find(|field| field.name == name).unwrap().kind;
        assert_eq!(kind("query"), FieldKind::Text);
        assert_eq!(kind("limit"), FieldKind::Number);
        assert_eq!(kind("sort"), FieldKind::Text);
        assert_eq!(kind("include_bots"), FieldKind::Flag);
        assert_eq!(kind("channels"), FieldKind::Json);
        let sort = fields.iter().find(|field| field.name == "sort").unwrap();
        assert_eq!(sort.hint, "(timestamp, score)");
        // No schema, no fields.
        assert!(super::fields(&json!({})).is_empty());
    }

    #[test]
    fn what_was_typed_becomes_the_tools_arguments() {
        let fields = fields(&search_schema());
        let (mut typed, extra) = typed_from(
            &fields,
            &json!({"query": "to:me", "limit": 20, "channels": ["C1"], "cursor": "x"}),
        );
        assert_eq!(extra.get("cursor"), Some(&json!("x")), "kept as it came");
        let at = |name: &str| fields.iter().position(|field| field.name == name).unwrap();
        assert_eq!(typed[at("query")].text, "to:me");
        assert_eq!(typed[at("limit")].text, "20");
        assert_eq!(typed[at("channels")].text, "[\"C1\"]");
        assert_eq!(
            arguments(&fields, &typed, &extra).unwrap(),
            json!({"query": "to:me", "limit": 20, "channels": ["C1"], "cursor": "x"})
        );
        // A switch touched, an optional field cleared.
        typed[at("include_bots")].flag = Some(false);
        typed[at("limit")].text = "  ".into();
        assert_eq!(
            arguments(&fields, &typed, &Map::new()).unwrap(),
            json!({"query": "to:me", "channels": ["C1"], "include_bots": false})
        );
        // What cannot be sent says which field.
        typed[at("limit")].text = "vinte".into();
        assert_eq!(
            arguments(&fields, &typed, &Map::new()),
            Err(BadField::NotANumber("limit".into()))
        );
        typed[at("limit")].text.clear();
        typed[at("channels")].text = "C1".into();
        assert_eq!(
            arguments(&fields, &typed, &Map::new()),
            Err(BadField::NotJson("channels".into()))
        );
        typed[at("channels")].text.clear();
        typed[at("query")].text.clear();
        assert_eq!(
            arguments(&fields, &typed, &Map::new()),
            Err(BadField::Missing("query".into()))
        );
    }
}
