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
/// ai-memory keeps at most 2 KB of a notification body, cutting the tail.
/// Assistant answers put their conclusion last, so the DCC fits the message
/// under that cap itself and keeps the opening and the ending.
const MAX_ASSISTANT_BYTES: usize = 1_900;
const ASSISTANT_HEAD_BYTES: usize = 400;

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

    /// Builds the automatic DCC connection while keeping the configured
    /// workspace as the shared namespace and isolating each DCC project.
    ///
    /// The settings UI stores a human-readable project prefix. Automatic
    /// checkpoints and retrieval add the stable DCC project id so two
    /// repositories cannot accidentally share one memory namespace.
    pub fn from_env_for_project(project_id: impl AsRef<str>) -> Option<Self> {
        let base_url = std::env::var("DCC_AI_MEMORY_URL").ok()?;
        if base_url.trim().is_empty() {
            return None;
        }
        let workspace = std::env::var("DCC_AI_MEMORY_WORKSPACE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "dcc-workspace".to_string());
        let project_prefix = std::env::var("DCC_AI_MEMORY_PROJECT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "dcc-project".to_string());
        let project = scoped_project_name(&project_prefix, project_id.as_ref());
        let mut config = Self::new(base_url, workspace, project);
        config.bearer_token = std::env::var("DCC_AI_MEMORY_TOKEN").ok();
        Some(config)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn scoped_project_name(prefix: &str, project_id: &str) -> String {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}::{project_id}")
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
    /// Per-event outcomes (ai-memory 2.5+). An acknowledged event can still be
    /// a terminal drop (`dropped_policy`, `dropped_invalid`, ...), which a
    /// retry will never store. Older servers omit the field.
    #[serde(default)]
    pub results: Option<Vec<AiMemoryEventOutcome>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AiMemoryEventOutcome {
    pub index: usize,
    pub outcome: String,
}

impl AiMemoryBatchAck {
    /// Acknowledged events that ai-memory discarded instead of storing.
    pub fn dropped(&self) -> Vec<&AiMemoryEventOutcome> {
        self.results
            .iter()
            .flatten()
            .filter(|result| result.outcome.starts_with("dropped_"))
            .collect()
    }

    /// Events ai-memory acknowledged and kept, excluding terminal drops.
    pub fn kept(&self) -> usize {
        self.accepted.saturating_sub(self.dropped().len())
    }

    /// A short description of the drops, such as `dropped_policy×2`.
    pub fn dropped_summary(&self) -> Option<String> {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for result in self.dropped() {
            match counts
                .iter_mut()
                .find(|(outcome, _)| *outcome == result.outcome)
            {
                Some((_, count)) => *count += 1,
                None => counts.push((result.outcome.as_str(), 1)),
            }
        }
        (!counts.is_empty()).then(|| {
            counts
                .iter()
                .map(|(outcome, count)| format!("{outcome}×{count}"))
                .collect::<Vec<_>>()
                .join(", ")
        })
    }
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

impl AiMemoryHit {
    /// Stable identity of a recovered source, shared with the UI's curation
    /// actions: path, title and the first 160 characters of the snippet.
    pub fn source_key(&self) -> String {
        let snippet: String = self
            .snippet
            .as_deref()
            .unwrap_or_default()
            .chars()
            .take(160)
            .collect();
        format!(
            "{}|{}|{}",
            self.path.as_deref().unwrap_or_default(),
            self.title.as_deref().unwrap_or_default(),
            snippet
        )
    }
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
        parse_query_hits(value).map(dedupe_hits)
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
    let mut exported_prompts = std::collections::HashSet::new();
    for event in events {
        let (kind, body, source_event) = match &event.kind {
            // Resending the same message (after an error or an abort) starts a
            // new turn with identical text; memory only needs it once.
            SessionEventKind::TurnStarted { prompt, .. }
                if !exported_prompts.insert(prompt.trim().to_string()) =>
            {
                continue
            }
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
                    json!({"message": fit_head_and_tail(content, MAX_ASSISTANT_BYTES, ASSISTANT_HEAD_BYTES)}),
                    Some("dcc-assistant-message"),
                )
            }
            SessionEventKind::SessionCompleted | SessionEventKind::SessionAborted { .. } => {
                ("session-end", json!({"session_id": session_id}), None)
            }
            _ => continue,
        };
        let body = with_occurred_at(body, &event.occurred_at);
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
        // A new DCC workspace has no ai-memory scope until its first
        // checkpoint export. Treat that first read as an empty index; it is not a
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

/// Collapses hits that carry the same text. The same prompt sent twice in a
/// session is two observations in ai-memory; showing or injecting both only
/// spends a slot of the bounded context on a repeat. The first (best ranked)
/// hit wins.
pub fn dedupe_hits(hits: Vec<AiMemoryHit>) -> Vec<AiMemoryHit> {
    let mut seen = std::collections::HashSet::new();
    hits.into_iter()
        .filter(|hit| {
            seen.insert((
                comparable_text(hit.title.as_deref()),
                comparable_text(hit.snippet.as_deref()),
            ))
        })
        .collect()
}

/// Recovered text without ai-memory's `<mark>` highlights, case or spacing,
/// which differ between two hits for the same text depending on the query.
fn comparable_text(value: Option<&str>) -> String {
    let value = value.unwrap_or_default();
    let mut plain = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => plain.push(character),
        }
    }
    plain
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Stamps the DCC's own event time so a session exported later keeps its real
/// chronology. ai-memory falls back to "now" for a missing or invalid value.
fn with_occurred_at(mut body: Value, occurred_at: &str) -> Value {
    if !occurred_at.trim().is_empty() {
        if let Some(object) = body.as_object_mut() {
            object.insert("occurred_at".to_string(), json!(occurred_at));
        }
    }
    body
}

/// Keeps `value` within `max_bytes` UTF-8 bytes by dropping its middle:
/// about `head_bytes` from the start, the rest from the end.
fn fit_head_and_tail(value: &str, max_bytes: usize, head_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    const MARKER: &str = "\n…\n";
    let budget = max_bytes.saturating_sub(MARKER.len());
    let head_bytes = head_bytes.min(budget);
    let mut head_end = head_bytes;
    while !value.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = value.len() - (budget - head_end);
    while !value.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    format!("{}{MARKER}{}", &value[..head_end], &value[tail_start..])
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
                    approval_policy: None,
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
    fn a_resent_prompt_is_exported_once() {
        let session = session();
        let turn = |id: &str, prompt: &str| {
            event(
                id,
                SessionEventKind::TurnStarted {
                    turn_id: dcc_core::domain::session::TurnId(id.into()),
                    prompt: prompt.into(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: None,
                },
            )
        };
        let batch = build_hook_batch(
            &session,
            &[
                turn("t1", "Revise o fluxo de convite"),
                turn("t2", "Revise o fluxo de convite "),
                turn("t3", "Agora o CRM"),
            ],
        );
        let keys: Vec<_> = batch.iter().map(|event| event.ingest_key.as_str()).collect();
        assert_eq!(keys, ["dcc-session-1-t1", "dcc-session-1-t3"]);
    }

    #[test]
    fn hits_with_the_same_text_are_shown_once() {
        let hit = |title: &str, snippet: &str, rank: f64| AiMemoryHit {
            path: None,
            title: Some(title.into()),
            snippet: Some(snippet.into()),
            rank: Some(rank),
            created_at: None,
            session_id: None,
            kind: Some("user-prompt".into()),
        };
        let hits = dedupe_hits(vec![
            hit("Tem uma situacao do hosp", "…o <mark>convite</mark> para usuarios", -2.0),
            hit("Tem uma situacao do hosp", "…o convite  para <mark>usuarios</mark>", -1.0),
            hit("Outra coisa", "…o convite para usuarios", -0.5),
        ]);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].rank, Some(-2.0));
    }

    #[test]
    fn exported_events_carry_the_dcc_event_time() {
        let session = session();
        let batch = build_hook_batch(
            &session,
            &[event("end", SessionEventKind::SessionCompleted)],
        );
        assert_eq!(batch[0].body["occurred_at"], "2026-01-01T00:00:00Z");
    }

    #[test]
    fn long_assistant_answers_keep_their_conclusion_under_the_server_cap() {
        let session = session();
        let content = format!(
            "Contexto ação {}Conclusão: usar Worktree.",
            "é".repeat(3_000)
        );
        let event = event(
            "assistant",
            SessionEventKind::TurnAssistantMessageCompleted {
                turn_id: dcc_core::domain::session::TurnId("turn-1".into()),
                message_id: "message-1".into(),
                phase: dcc_core::domain::session::AssistantMessagePhase::FinalAnswer,
                content: Some(content),
            },
        );
        let batch = build_hook_batch(&session, &[event]);
        let message = batch[0].body["message"].as_str().expect("message");
        assert!(message.len() <= MAX_ASSISTANT_BYTES);
        assert!(message.starts_with("Contexto ação"));
        assert!(message.ends_with("Conclusão: usar Worktree."));
    }

    #[test]
    fn batch_ack_separates_terminal_drops_from_stored_events() {
        let ack: AiMemoryBatchAck = serde_json::from_value(json!({
            "accepted": 4,
            "results": [
                {"index": 0, "outcome": "stored"},
                {"index": 1, "outcome": "dropped_policy"},
                {"index": 2, "outcome": "replayed"},
                {"index": 3, "outcome": "dropped_policy"}
            ]
        }))
        .expect("ack");
        assert_eq!(ack.kept(), 2);
        assert_eq!(ack.dropped_summary().as_deref(), Some("dropped_policy×2"));

        let legacy: AiMemoryBatchAck =
            serde_json::from_value(json!({"accepted": 3})).expect("legacy ack");
        assert_eq!(legacy.kept(), 3);
        assert_eq!(legacy.dropped_summary(), None);
    }

    #[test]
    fn config_rejects_non_http_endpoints_at_request_boundary() {
        let config = AiMemoryConfig::new("stdio://ai-memory", "workspace", "project");
        let client = AiMemoryClient::new(config).expect("client");
        let result = futures::executor::block_on(client.query("test", 5));
        assert!(matches!(result, Err(AiMemoryError::InvalidUrl(_))));
    }

    #[test]
    fn scopes_automatic_project_names_without_cross_project_collisions() {
        assert_eq!(
            scoped_project_name("dcc-project", "repo-a"),
            "dcc-project::repo-a"
        );
        assert_eq!(scoped_project_name("team", " repo-b "), "team::repo-b");
        assert_eq!(scoped_project_name("dcc-project", ""), "dcc-project");
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
