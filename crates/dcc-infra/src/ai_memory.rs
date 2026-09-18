//! Opt-in adapter for the public ai-memory HTTP/MCP surface.
//!
//! This module deliberately stays outside the provider bridges. The DCC owns
//! session identity and selects the small set of durable events exported here;
//! ai-memory remains a replaceable retrieval and wiki backend.

use std::time::Duration;

use dcc_core::domain::session::{Session, SessionEventKind, SessionEventRecord};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::form_urlencoded::Serializer;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
pub const MAX_BATCH_EVENTS: usize = 256;
const MAX_PROMPT_CHARS: usize = 16_000;
const MAX_ASSISTANT_CHARS: usize = 4_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiMemoryConfig {
    pub base_url: String,
    pub bearer_token: Option<String>,
    pub workspace: String,
    pub project: String,
    pub timeout: Duration,
}

impl AiMemoryConfig {
    pub fn new(
        base_url: impl Into<String>,
        workspace: impl Into<String>,
        project: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            bearer_token: None,
            workspace: workspace.into(),
            project: project.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn from_env(workspace: impl Into<String>, project: impl Into<String>) -> Option<Self> {
        let base_url = std::env::var("DCC_AI_MEMORY_URL").ok()?;
        if base_url.trim().is_empty() {
            return None;
        }
        let workspace = std::env::var("DCC_AI_MEMORY_WORKSPACE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| workspace.into());
        let project = std::env::var("DCC_AI_MEMORY_PROJECT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| project.into());
        let mut config = Self::new(base_url, workspace, project);
        config.bearer_token = std::env::var("DCC_AI_MEMORY_TOKEN").ok();
        Some(config)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AiMemoryHookEvent {
    pub event: String,
    pub agent: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub body: Value,
    pub ingest_key: String,
    pub source_event: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct AiMemoryBatchAck {
    pub accepted: usize,
    #[serde(default)]
    pub accepted_indices: Option<Vec<usize>>,
    #[serde(default)]
    pub failed_index: Option<usize>,
    #[serde(default)]
    pub processed: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AiMemoryHit {
    pub path: Option<String>,
    pub title: Option<String>,
    pub snippet: Option<String>,
    #[serde(default)]
    pub rank: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AiMemoryError {
    #[error("ai-memory URL is invalid: {0}")]
    InvalidUrl(String),
    #[error("ai-memory request failed: {0}")]
    Request(String),
    #[error("ai-memory returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("ai-memory returned an invalid response: {0}")]
    Response(String),
    #[error("ai-memory export exceeds the {0} event limit")]
    BatchTooLarge(usize),
}

#[derive(Clone)]
pub struct AiMemoryClient {
    config: AiMemoryConfig,
    http: reqwest::Client,
}

impl AiMemoryClient {
    pub fn new(config: AiMemoryConfig) -> Result<Self, AiMemoryError> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|error| AiMemoryError::Request(error.to_string()))?;
        Ok(Self { config, http })
    }

    pub fn config(&self) -> &AiMemoryConfig {
        &self.config
    }

    pub async fn ingest_batch(
        &self,
        events: &[AiMemoryHookEvent],
    ) -> Result<AiMemoryBatchAck, AiMemoryError> {
        if events.len() > MAX_BATCH_EVENTS {
            return Err(AiMemoryError::BatchTooLarge(MAX_BATCH_EVENTS));
        }
        if events.is_empty() {
            return Ok(AiMemoryBatchAck::default());
        }
        let endpoint = self.endpoint("/hook/batch")?;
        let items: Vec<Value> = events
            .iter()
            .map(|event| {
                let mut query = Serializer::new(String::new());
                query.append_pair("event", &event.event);
                query.append_pair("agent", &event.agent);
                query.append_pair("session_id", &event.session_id);
                query.append_pair("workspace", &self.config.workspace);
                query.append_pair("project", &self.config.project);
                query.append_pair("ingest_key", &event.ingest_key);
                if let Some(cwd) = event.cwd.as_deref() {
                    query.append_pair("cwd", cwd);
                }
                if let Some(source_event) = event.source_event.as_deref() {
                    query.append_pair("extension", "dcc");
                    query.append_pair("source_event", source_event);
                }
                json!({
                    "url": format!("{}/hook?{}", self.config.base_url, query.finish()),
                    "body": event.body,
                })
            })
            .collect();
        let response = self
            .request_builder(reqwest::Method::POST, endpoint)
            .json(&items)
            .send()
            .await
            .map_err(|error| AiMemoryError::Request(error.to_string()))?;
        self.decode_http(response).await
    }

    pub async fn query(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<AiMemoryHit>, AiMemoryError> {
        let endpoint = self.endpoint("/mcp")?;
        let args = json!({
            "query": query,
            "limit": limit.clamp(1, 50),
            "workspace": self.config.workspace,
            "project": self.config.project,
        });
        let response = self
            .request_builder(reqwest::Method::POST, endpoint)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": "memory_query", "arguments": args},
            }))
            .send()
            .await
            .map_err(|error| AiMemoryError::Request(error.to_string()))?;
        let value: Value = self.decode_http(response).await?;
        parse_query_hits(value)
    }

    fn endpoint(&self, path: &str) -> Result<String, AiMemoryError> {
        if !self.config.base_url.starts_with("http://")
            && !self.config.base_url.starts_with("https://")
        {
            return Err(AiMemoryError::InvalidUrl(self.config.base_url.clone()));
        }
        Ok(format!("{}{}", self.config.base_url, path))
    }

    fn request_builder(
        &self,
        method: reqwest::Method,
        endpoint: String,
    ) -> reqwest::RequestBuilder {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        if let Some(token) = self.config.bearer_token.as_deref() {
            if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(AUTHORIZATION, value);
            }
        }
        self.http.request(method, endpoint).headers(headers)
    }

    async fn decode_http<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, AiMemoryError> {
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| AiMemoryError::Response(error.to_string()))?;
        if !status.is_success() {
            return Err(AiMemoryError::Http {
                status: status.as_u16(),
                body: truncate(&body, 500),
            });
        }
        serde_json::from_str(&body).map_err(|error| AiMemoryError::Response(error.to_string()))
    }
}

/// Converts the DCC's durable session record into the small, replayable event
/// vocabulary accepted by ai-memory. Tool calls and reasoning are intentionally
/// omitted; they are noisy and may contain sensitive arguments.
pub fn build_hook_batch(
    session: &Session,
    events: &[SessionEventRecord],
) -> Vec<AiMemoryHookEvent> {
    let agent = if session.provider_id.trim().is_empty() {
        "dcc".to_string()
    } else {
        session.provider_id.clone()
    };
    let cwd = session.working_directory_override.clone();
    let session_id = session.id.0.clone();
    let mut result = Vec::new();
    for event in events {
        let (kind, body, source_event) = match &event.kind {
            SessionEventKind::SessionStarted { .. } => (
                "session-start",
                json!({"session_id": session_id, "cwd": cwd}),
                None,
            ),
            SessionEventKind::TurnStarted { prompt, .. } => (
                "user-prompt",
                json!({"prompt": truncate(prompt, MAX_PROMPT_CHARS)}),
                None,
            ),
            SessionEventKind::TurnAssistantMessageCompleted {
                content: Some(content),
                phase,
                ..
            } if !content.trim().is_empty()
                && *phase != dcc_core::domain::session::AssistantMessagePhase::Commentary =>
            {
                (
                    "notification",
                    json!({"message": truncate(content, MAX_ASSISTANT_CHARS)}),
                    Some("dcc-assistant-message"),
                )
            }
            SessionEventKind::SessionCompleted | SessionEventKind::SessionAborted { .. } => {
                ("session-end", json!({"session_id": session_id}), None)
            }
            _ => continue,
        };
        result.push(AiMemoryHookEvent {
            event: kind.to_string(),
            agent: agent.clone(),
            session_id: session_id.clone(),
            cwd: cwd.clone(),
            body,
            ingest_key: format!("dcc-{}-{}", session_id, event.event_id),
            source_event: source_event.map(str::to_string),
        });
    }
    result
}

fn parse_query_hits(value: Value) -> Result<Vec<AiMemoryHit>, AiMemoryError> {
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("MCP query failed");
        // A new DCC workspace has no ai-memory scope until its first close
        // export. Treat that first read as an empty index; it is not a
        // connectivity failure and should not open the query circuit.
        let lower = message.to_ascii_lowercase();
        if (lower.contains("workspace") || lower.contains("project")) && lower.contains("not found")
        {
            return Ok(Vec::new());
        }
        return Err(AiMemoryError::Response(format!("MCP error: {message}")));
    }
    let text = value
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("type") == Some(&json!("text")))
        })
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| AiMemoryError::Response("MCP response has no text content".to_string()))?;
    let payload: Value =
        serde_json::from_str(text).map_err(|error| AiMemoryError::Response(error.to_string()))?;
    let hits = payload
        .get("hits")
        .and_then(Value::as_array)
        .filter(|hits| !hits.is_empty())
        .cloned()
        .or_else(|| payload.get("raw_hits").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    serde_json::from_value(Value::Array(hits))
        .map_err(|error| AiMemoryError::Response(error.to_string()))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut output: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        output.push('…');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::{project::ProjectId, session::SessionId, workspace::WorkspaceId};

    fn session() -> Session {
        Session {
            id: SessionId("session-1".into()),
            project_id: ProjectId("project-1".into()),
            workspace_id: WorkspaceId("workspace-1".into()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".into(),
            model: Some("test-model".into()),
            provider_runtime: None,
            working_directory_override: Some("/tmp/dcc-worktree".into()),
            state: dcc_core::domain::session::SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn event(id: &str, kind: SessionEventKind) -> SessionEventRecord {
        SessionEventRecord {
            event_id: id.into(),
            session_id: SessionId("session-1".into()),
            sequence: 1,
            occurred_at: "2026-01-01T00:00:00Z".into(),
            kind,
        }
    }

    #[test]
    fn exports_only_context_bearing_events_with_stable_keys() {
        let session = session();
        let events = vec![
            event(
                "start",
                SessionEventKind::SessionStarted {
                    workspace_id: session.workspace_id.clone(),
                    project_id: session.project_id.clone(),
                    provider_id: session.provider_id.clone(),
                    model: session.model.clone(),
                    forked_from: None,
                },
            ),
            event(
                "turn",
                SessionEventKind::TurnStarted {
                    turn_id: dcc_core::domain::session::TurnId("turn-1".into()),
                    prompt: "Decide the Local/Worktree policy".into(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                },
            ),
            event(
                "tool",
                SessionEventKind::TurnDelta {
                    turn_id: dcc_core::domain::session::TurnId("turn-1".into()),
                    content: "ignored tool-like stream".into(),
                },
            ),
        ];
        let batch = build_hook_batch(&session, &events);
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].event, "session-start");
        assert_eq!(batch[1].event, "user-prompt");
        assert_eq!(batch[1].ingest_key, "dcc-session-1-turn");
    }

    #[test]
    fn assistant_completion_is_exported_as_explicit_dcc_provenance() {
        let session = session();
        let event = event(
            "assistant",
            SessionEventKind::TurnAssistantMessageCompleted {
                turn_id: dcc_core::domain::session::TurnId("turn-1".into()),
                message_id: "message-1".into(),
                phase: dcc_core::domain::session::AssistantMessagePhase::FinalAnswer,
                content: Some("A decisão validada foi usar Local.".into()),
            },
        );
        let batch = build_hook_batch(&session, &[event]);
        assert_eq!(batch[0].event, "notification");
        assert_eq!(
            batch[0].source_event.as_deref(),
            Some("dcc-assistant-message")
        );
        assert_eq!(
            batch[0].body["message"],
            "A decisão validada foi usar Local."
        );
    }

    #[test]
    fn config_rejects_non_http_endpoints_at_request_boundary() {
        let config = AiMemoryConfig::new("stdio://ai-memory", "workspace", "project");
        let client = AiMemoryClient::new(config).expect("client");
        let result = futures::executor::block_on(client.query("test", 5));
        assert!(matches!(result, Err(AiMemoryError::InvalidUrl(_))));
    }

    #[test]
    fn parses_mcp_query_text() {
        let value = json!({
            "result": {"content": [{"type": "text", "text": "{\"hits\":[{\"path\":\"decisions/a.md\",\"title\":\"A\",\"snippet\":\"Local\"}]}"}]}
        });
        let hits = parse_query_hits(value).expect("hits");
        assert_eq!(hits[0].path.as_deref(), Some("decisions/a.md"));
    }

    #[test]
    fn falls_back_to_raw_observation_hits_when_pages_are_empty() {
        let value = json!({
            "result": {"content": [{"type": "text", "text": "{\"hits\":[],\"raw_hits\":[{\"id\":\"obs-1\",\"session_id\":\"session-1\",\"kind\":\"user-prompt\",\"title\":\"Pilot\",\"snippet\":\"<mark>DCC_MEMORY_PILOT</mark>\",\"rank\":-1.2}]}"}]}
        });
        let hits = parse_query_hits(value).expect("raw hits");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title.as_deref(), Some("Pilot"));
        assert_eq!(hits[0].path, None);
    }

    #[test]
    fn missing_scope_is_an_empty_first_query() {
        let value = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32603, "message": "workspace 'new-scope' not found"}
        });
        assert!(parse_query_hits(value)
            .expect("missing scope is not fatal")
            .is_empty());
    }
}
