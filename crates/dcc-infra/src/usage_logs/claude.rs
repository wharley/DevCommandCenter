//! Claude Code's transcripts (`<config>/projects/<cwd-slug>/<session>.jsonl`,
//! subagents in `<session>/subagents/agent-<id>.jsonl`): a call is an
//! `assistant` line's usage, written once per content block with the same
//! numbers, so one `(message.id, requestId)` is one call; a turn is a
//! person's prompt (a `user` line that is not a tool result, meta or a
//! compaction summary). Only ids, times and counters are read.

use std::path::Path;

use serde::{Deserialize, Serialize};

use dcc_core::domain::usage_logs::{EventKind, Runtime, TokenBuckets, UsageEvent};

use super::sources::Resolver;

/// What a file says about all its lines, kept between appends.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClaudeFile {
    pub subagent: bool,
    /// `agent-<id>` for a subagent's file.
    #[serde(default)]
    pub agent_file: Option<String>,
    /// The subagent's type (`Explore`), from `agent-<id>.meta.json` or
    /// its first attributed call.
    #[serde(default)]
    pub agent: Option<String>,
}

impl ClaudeFile {
    pub fn for_path(path: &Path) -> Self {
        let subagent = path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|dir| dir == "subagents");
        if !subagent {
            return Self::default();
        }
        let agent_file = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
        let agent = std::fs::read_to_string(path.with_extension("meta.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<AgentMeta>(&text).ok())
            .and_then(|meta| meta.agent_type)
            .filter(|name| !name.is_empty());
        Self {
            subagent: true,
            agent_file,
            agent,
        }
    }
}

#[derive(Deserialize)]
struct AgentMeta {
    #[serde(rename = "agentType")]
    agent_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    entrypoint: Option<String>,
    is_sidechain: Option<bool>,
    request_id: Option<String>,
    uuid: Option<String>,
    is_meta: Option<bool>,
    is_compact_summary: Option<bool>,
    attribution_agent: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    output_tokens_details: Option<OutputDetails>,
}

#[derive(Deserialize)]
struct OutputDetails {
    #[serde(default)]
    thinking_tokens: u64,
}

pub(super) fn timestamp_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|at| at.timestamp_millis())
}

/// The call or turn on `line`, if any.
pub fn parse_line(
    line: &str,
    file: &mut ClaudeFile,
    resolver: &mut Resolver,
) -> Option<UsageEvent> {
    let assistant = line.contains("\"type\":\"assistant\"");
    let user = !assistant
        && line.contains("\"type\":\"user\"")
        // A tool's result (the tag can only appear unescaped as a key).
        && !line.contains("\"type\":\"tool_result\"")
        && !line.contains("<local-command-");
    if !assistant && !user {
        return None;
    }
    let parsed: Line = serde_json::from_str(line).ok()?;
    let at_ms = timestamp_ms(parsed.timestamp.as_deref()?)?;
    let session_id = parsed.session_id.clone().unwrap_or_default();
    let cwd = parsed.cwd.clone().unwrap_or_default();
    let via = resolver.claude_via(
        parsed.entrypoint.as_deref().unwrap_or(""),
        &session_id,
        &cwd,
    );
    let subagent = file.subagent || parsed.is_sidechain == Some(true);
    let session = match &file.agent_file {
        Some(agent) => format!("{session_id}/{agent}"),
        None => session_id,
    };
    let project = resolver.project(&cwd);
    match parsed.kind.as_deref()? {
        "assistant" => {
            let message = parsed.message?;
            let usage = message.usage?;
            let model = message.model.unwrap_or_default();
            if model.starts_with('<') {
                return None;
            }
            let attributed = parsed.attribution_agent.filter(|name| !name.is_empty());
            if file.subagent && file.agent.is_none() {
                file.agent = attributed.clone();
            }
            let key = match (&message.id, &parsed.request_id) {
                (None, None) => format!("c:{}", parsed.uuid?),
                (id, request) => format!(
                    "c:{}:{}",
                    id.as_deref().unwrap_or(""),
                    request.as_deref().unwrap_or("")
                ),
            };
            Some(UsageEvent {
                key,
                kind: EventKind::Call,
                at_ms,
                runtime: Runtime::Claude,
                model,
                via,
                project,
                agent: agent_name(subagent, file, attributed),
                session,
                subagent,
                tokens: TokenBuckets {
                    input: usage.input_tokens,
                    cache_read: usage.cache_read_input_tokens,
                    cache_write: usage.cache_creation_input_tokens,
                    output: usage.output_tokens,
                    reasoning: usage
                        .output_tokens_details
                        .map_or(0, |details| details.thinking_tokens),
                },
            })
        }
        "user" => {
            if parsed.is_meta == Some(true) || parsed.is_compact_summary == Some(true) {
                return None;
            }
            Some(UsageEvent {
                key: format!("ct:{}", parsed.uuid?),
                kind: EventKind::Turn,
                at_ms,
                runtime: Runtime::Claude,
                model: String::new(),
                via,
                project,
                agent: agent_name(subagent, file, None),
                session,
                subagent,
                tokens: TokenBuckets::default(),
            })
        }
        _ => None,
    }
}

fn agent_name(subagent: bool, file: &ClaudeFile, attributed: Option<String>) -> String {
    if !subagent {
        return String::new();
    }
    file.agent
        .clone()
        .or(attributed)
        .unwrap_or_else(|| "subagent".to_string())
}
