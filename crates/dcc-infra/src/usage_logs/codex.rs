//! Codex's rollouts (`<codex home>/sessions/YYYY/MM/DD/rollout-*.jsonl`):
//! the first `session_meta` names the session, its folder, how it was
//! started (`originator`, `source`) and whether a subagent runs it;
//! `turn_context` carries the model; each `token_count` event carries the
//! call's own usage (`last_token_usage`) and the session's running total,
//! repeated unchanged when nothing new was billed — those repeats are
//! skipped. Codex's input includes its cached (and written) part. A turn
//! is `task_started` (or a `turn_context` with its turn id).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use dcc_core::domain::usage_logs::{EventKind, Runtime, TokenBuckets, UsageEvent, Via};

use super::claude::timestamp_ms;
use super::sources::{codex_via, Resolver};

/// What earlier lines said, kept between appends.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CodexFile {
    #[serde(default)]
    pub meta_seen: bool,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub via: String,
    #[serde(default)]
    pub subagent: bool,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub last_total: Option<u64>,
}

#[derive(Deserialize)]
struct Line {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<Value>,
}

#[derive(Default, Deserialize)]
struct TokenUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

/// Whether the line's opening bytes name one of the kinds read here (the
/// kinds come first; the rest of a line can be megabytes of content).
fn wanted(line: &str) -> bool {
    let head = &line.as_bytes()[..line.len().min(240)];
    let has = |needle: &[u8]| head.windows(needle.len()).any(|window| window == needle);
    has(b"\"session_meta\"")
        || has(b"\"turn_context\"")
        || has(b"\"token_count\"")
        || has(b"\"task_started\"")
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The call or turn on `line`, if any; `file` learns from every line.
pub fn parse_line(line: &str, file: &mut CodexFile, resolver: &mut Resolver) -> Option<UsageEvent> {
    if !wanted(line) {
        return None;
    }
    let parsed: Line = serde_json::from_str(line).ok()?;
    let payload = parsed.payload?;
    let at_ms = parsed.timestamp.as_deref().and_then(timestamp_ms);
    match parsed.kind.as_deref()? {
        "session_meta" => {
            // A forked or spawned thread repeats its parent's meta later.
            if !file.meta_seen {
                learn_meta(&payload, file, resolver);
            }
            None
        }
        "turn_context" => {
            let model = text(&payload, "model");
            if !model.is_empty() {
                file.model = model.to_string();
            }
            let turn = text(&payload, "turn_id");
            if turn.is_empty() {
                return None;
            }
            Some(turn_event(format!("xt:{turn}"), at_ms?, file))
        }
        "event_msg" => match text(&payload, "type") {
            "task_started" => {
                let turn = text(&payload, "turn_id");
                let key = if turn.is_empty() {
                    format!("xt:{}:{}", file.session, at_ms?)
                } else {
                    format!("xt:{turn}")
                };
                Some(turn_event(key, at_ms?, file))
            }
            "token_count" => {
                let info = payload.get("info").filter(|info| !info.is_null())?;
                let total: TokenUsage = info
                    .get("total_token_usage")
                    .and_then(|value| serde_json::from_value(value.clone()).ok())
                    .unwrap_or_default();
                if file.last_total == Some(total.total_tokens) {
                    return None;
                }
                file.last_total = Some(total.total_tokens);
                let last: TokenUsage =
                    serde_json::from_value(info.get("last_token_usage")?.clone()).ok()?;
                let tokens = buckets(&last);
                if tokens.total() == 0 {
                    return None;
                }
                Some(UsageEvent {
                    key: format!("x:{}:{}:{}", file.session, at_ms?, total.total_tokens),
                    kind: EventKind::Call,
                    at_ms: at_ms?,
                    runtime: Runtime::Codex,
                    model: file.model.clone(),
                    via: Via::parse(&file.via),
                    project: file.project.clone(),
                    agent: file.agent.clone(),
                    session: file.session.clone(),
                    subagent: file.subagent,
                    tokens,
                })
            }
            _ => None,
        },
        _ => None,
    }
}

/// Codex's input holds the cached and written part: fresh = the rest.
fn buckets(usage: &TokenUsage) -> TokenBuckets {
    TokenBuckets {
        input: usage
            .input_tokens
            .saturating_sub(usage.cached_input_tokens + usage.cache_write_input_tokens),
        cache_read: usage.cached_input_tokens,
        cache_write: usage.cache_write_input_tokens,
        output: usage.output_tokens,
        reasoning: usage.reasoning_output_tokens,
    }
}

fn turn_event(key: String, at_ms: i64, file: &CodexFile) -> UsageEvent {
    UsageEvent {
        key,
        kind: EventKind::Turn,
        at_ms,
        runtime: Runtime::Codex,
        model: file.model.clone(),
        via: Via::parse(&file.via),
        project: file.project.clone(),
        agent: file.agent.clone(),
        session: file.session.clone(),
        subagent: file.subagent,
        tokens: TokenBuckets::default(),
    }
}

fn learn_meta(payload: &Value, file: &mut CodexFile, resolver: &mut Resolver) {
    file.meta_seen = true;
    file.session = [text(payload, "id"), text(payload, "session_id")]
        .into_iter()
        .find(|id| !id.is_empty())
        .unwrap_or("")
        .to_string();
    file.project = resolver.project(text(payload, "cwd"));
    let source = payload.get("source");
    let spawned = source
        .and_then(|source| source.get("subagent"))
        .filter(|value| !value.is_null());
    file.subagent = spawned.is_some() || text(payload, "thread_source") == "subagent";
    if file.subagent {
        file.agent = spawned
            .and_then(|spawn| {
                spawn
                    .get("thread_spawn")
                    .and_then(|thread| thread.get("agent_role"))
                    .and_then(Value::as_str)
                    .or_else(|| spawn.as_str())
            })
            .filter(|name| !name.is_empty())
            .unwrap_or("subagent")
            .to_string();
    }
    let source = source.and_then(Value::as_str).unwrap_or("");
    file.via = codex_via(text(payload, "originator"), source)
        .as_str()
        .to_string();
}
