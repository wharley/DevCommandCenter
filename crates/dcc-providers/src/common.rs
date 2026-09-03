use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{self, BoxStream};
use serde::Deserialize;
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, Mutex},
};
use uuid::Uuid;

use dcc_core::{
    application::compose_wire_prompt_for_provider,
    domain::{
        provider::{
            Capabilities, HealthStatus, McpSupportLevel, NativeSubagentStatus, ProviderEvent,
            ProviderId, SessionHandle,
        },
        session::{AssistantMessagePhase, SessionId},
        usage::ModelTokenUsage,
        workspace::WorkspaceId,
    },
    ports::{Input, Provider, ProviderRuntimeConfig, SessionConfig},
    CoreError, Result,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ProviderEnvelope {
    ReasoningStarted {
        id: String,
        label: Option<String>,
    },
    ReasoningDelta {
        id: String,
        content: String,
    },
    ReasoningCompleted {
        id: String,
    },
    ToolCallStarted {
        id: String,
        action: String,
        command: Option<String>,
        file: Option<String>,
    },
    ToolCallDelta {
        id: String,
        content: String,
    },
    ToolCallCompleted {
        id: String,
    },
    ToolCallFailed {
        id: String,
        reason: Option<String>,
    },
}

#[derive(Debug, Default)]
pub(crate) struct ProviderStreamState {
    claude_blocks: HashMap<u64, ClaudeBlockState>,
    claude_pending_tool_calls: HashSet<String>,
    claude_native_subagents: HashMap<String, ClaudeNativeSubagent>,
    claude_native_subagent_inputs: HashMap<String, String>,
    claude_native_subagent_event_ids: HashMap<String, String>,
    claude_native_subagent_terminal_statuses: HashMap<String, NativeSubagentStatus>,
    claude_active_message_id: Option<String>,
    claude_text_message_started: bool,
    /// Events discovered while parsing one envelope that must be emitted
    /// alongside its primary event (for example Agent activity plus the
    /// requested model from the same tool_use block).
    claude_pending_events: Vec<ProviderEvent>,
    pub(crate) gemini_streamed_text_emitted: bool,
    pub(crate) gemini_active_message_id: Option<String>,
}

#[derive(Debug, Clone)]
enum ClaudeBlockState {
    Reasoning { id: String },
    ToolCall { id: String },
}

#[derive(Debug, Clone)]
struct ClaudeNativeSubagent {
    agent_id: Option<String>,
    hook_agent_id: Option<String>,
    name: Option<String>,
    role: Option<String>,
    model: Option<String>,
}

fn claude_native_subagent_metadata(input: Option<&Value>) -> ClaudeNativeSubagent {
    let input = input.and_then(Value::as_object);
    let read = |key: &str| {
        input
            .and_then(|input| input.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };

    ClaudeNativeSubagent {
        // Claude's structured Agent tool reports the tool-use identity here,
        // not the runtime agent ID. Keep that distinction explicit.
        agent_id: None,
        hook_agent_id: None,
        name: read("name"),
        role: read("subagent_type"),
        model: read("model"),
    }
}

fn nonempty_json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn single_unbound_claude_agent_tool(state: &ProviderStreamState) -> Option<String> {
    let mut candidates = state
        .claude_native_subagents
        .iter()
        .filter(|(id, metadata)| {
            state.claude_pending_tool_calls.contains(*id) && metadata.hook_agent_id.is_none()
        })
        .map(|(id, _)| id.clone());
    let candidate = candidates.next()?;
    candidates.next().is_none().then_some(candidate)
}

fn parse_claude_native_subagent_hook(
    value: &Value,
    state: &mut ProviderStreamState,
    at: String,
) -> Option<ProviderEvent> {
    if value.get("type").and_then(Value::as_str) != Some("dcc_native_subagent_activity") {
        return None;
    }
    let agent_id = nonempty_json_string(value.get("agent_id"))?;
    let role = nonempty_json_string(value.get("agent_type"))?;
    let mut status = match value.get("status").and_then(Value::as_str) {
        Some("running") => NativeSubagentStatus::Running,
        Some("completed") => NativeSubagentStatus::Completed,
        Some("failed") => NativeSubagentStatus::Failed,
        _ => return None,
    };
    let sdk_correlation_id = nonempty_json_string(value.get("correlation_id"));

    // Claude's hook callback can report an internal UUID in `toolUseID`
    // instead of the `toolu_...` identity from the structured Agent block.
    // Trust it only when it is an Anthropic tool-use ID or already names an
    // observed Agent invocation. Otherwise, a start hook can be linked safely
    // when exactly one unbound Agent invocation is pending. Multiple pending
    // invocations remain separate instead of being guessed by role or timing.
    let direct_correlation_id = sdk_correlation_id
        .filter(|id| id.starts_with("toolu_") || state.claude_native_subagents.contains_key(id));
    let matched_id = state
        .claude_native_subagent_event_ids
        .get(&agent_id)
        .cloned()
        .or(direct_correlation_id)
        .or_else(|| {
            (status == NativeSubagentStatus::Running)
                .then(|| single_unbound_claude_agent_tool(state))
                .flatten()
        });
    let id = matched_id.unwrap_or_else(|| format!("claude:subagent:{agent_id}"));
    if status == NativeSubagentStatus::Completed
        && state.claude_native_subagent_terminal_statuses.get(&id)
            == Some(&NativeSubagentStatus::Failed)
    {
        // If the Agent tool result was observed before SubagentStop, keep the
        // provider's explicit failure instead of downgrading it to completed.
        status = NativeSubagentStatus::Failed;
    }
    let metadata = state
        .claude_native_subagents
        .entry(id.clone())
        .or_insert_with(|| ClaudeNativeSubagent {
            agent_id: None,
            hook_agent_id: None,
            name: None,
            role: None,
            model: None,
        });
    metadata.agent_id = Some(agent_id.clone());
    metadata.hook_agent_id = Some(agent_id.clone());
    metadata.role = Some(role.clone());

    if status == NativeSubagentStatus::Running {
        state
            .claude_native_subagent_event_ids
            .insert(agent_id.clone(), id.clone());
    } else {
        state.claude_native_subagent_event_ids.remove(&agent_id);
        state.claude_native_subagent_terminal_statuses.remove(&id);
    }

    Some(ProviderEvent::NativeSubagentActivity {
        id,
        agent_id: Some(agent_id),
        agent_thread_id: None,
        path: None,
        name: None,
        role: Some(role),
        model: None,
        status,
        at,
    })
}

#[derive(Debug)]
pub(crate) enum ParsedProviderLine {
    Event(ProviderEvent),
    Events(Vec<ProviderEvent>),
    Text(String),
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CodexHomeLayoutMode {
    Direct,
    AuthOverlay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodexHomeLayout {
    mode: CodexHomeLayoutMode,
    shared_home_path: PathBuf,
    effective_home_path: Option<PathBuf>,
    continuation_key: String,
}

const CODEX_KNOWN_SHARED_DIRECTORIES: &[&str] = &[
    "sessions",
    "archived_sessions",
    "sqlite",
    "shell_snapshots",
    "worktrees",
    "skills",
    "plugins",
    "cache",
    "logs",
];

const CODEX_PRIVATE_ENTRY_NAMES: &[&str] = &["auth.json", "models_cache.json"];
const CODEX_SHADOW_LOCAL_ENTRY_NAMES: &[&str] = &["log", "memories", "tmp"];

fn resolve_home_path(path: &str, value: &str) -> PathBuf {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return PathBuf::from(path);
    }

    let expanded = expand_home_path(trimmed);
    if expanded.is_absolute() {
        expanded
    } else {
        PathBuf::from(path).join(expanded)
    }
}

fn resolve_codex_home_layout(cfg: &SessionConfig) -> CodexHomeLayout {
    let runtime_cfg = runtime_config(cfg);
    let shared_home_path = runtime_cfg
        .and_then(|config| config.home_path.as_deref())
        .and_then(|home_path| resolve_runtime_home_path(Some(home_path)))
        .unwrap_or_else(|| resolve_home_path(".", "~/.codex"));
    let shadow_home_path = runtime_cfg
        .and_then(|config| config.shadow_home_path.as_deref())
        .and_then(|home_path| resolve_runtime_home_path(Some(home_path)));
    let continuation_key = format!("codex:home:{}", shared_home_path.to_string_lossy());

    match shadow_home_path {
        Some(effective_home_path) => CodexHomeLayout {
            mode: CodexHomeLayoutMode::AuthOverlay,
            shared_home_path,
            effective_home_path: Some(effective_home_path),
            continuation_key,
        },
        None => CodexHomeLayout {
            mode: CodexHomeLayoutMode::Direct,
            shared_home_path: shared_home_path.clone(),
            effective_home_path: Some(shared_home_path.clone()),
            continuation_key,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LinkState {
    Missing,
    NotSymlink,
    Symlink(PathBuf),
}

fn path_resolve(link: &Path, target: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        link.parent().unwrap_or_else(|| Path::new(".")).join(target)
    }
}

fn read_link_state(path: &Path) -> Result<LinkState> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                let target =
                    fs::read_link(path).map_err(|error| CoreError::Provider(error.to_string()))?;
                Ok(LinkState::Symlink(target))
            } else {
                Ok(LinkState::NotSymlink)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LinkState::Missing),
        Err(error) => Err(CoreError::Provider(error.to_string())),
    }
}

fn remove_path_if_symlink(path: &Path) -> Result<()> {
    match read_link_state(path)? {
        LinkState::Symlink(_) => {
            fs::remove_file(path).map_err(|error| CoreError::Provider(error.to_string()))?;
        }
        LinkState::Missing | LinkState::NotSymlink => {}
    }
    Ok(())
}

fn ensure_symlink(target: &Path, link: &Path) -> Result<()> {
    match read_link_state(link)? {
        LinkState::Missing => create_symlink(target, link),
        LinkState::NotSymlink => Err(CoreError::Provider(format!(
            "cannot create Codex shadow home because '{}' already exists and is not a symlink",
            link.display()
        ))),
        LinkState::Symlink(existing_target) => {
            let resolved_existing = path_resolve(link, &existing_target);
            if resolved_existing == target {
                Ok(())
            } else {
                fs::remove_file(link).map_err(|error| CoreError::Provider(error.to_string()))?;
                create_symlink(target, link)
            }
        }
    }
}

fn ensure_shadow_auth_is_private(shadow_home_path: &Path) -> Result<()> {
    let auth_path = shadow_home_path.join("auth.json");
    if let LinkState::Symlink(_) = read_link_state(&auth_path)? {
        return Err(CoreError::Provider(format!(
            "Codex shadow auth file '{}' must be a real file, not a symlink",
            auth_path.display()
        )));
    }
    Ok(())
}

fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
            .map_err(|error| CoreError::Provider(error.to_string()))?;
    }

    #[cfg(windows)]
    {
        let metadata =
            fs::metadata(target).map_err(|error| CoreError::Provider(error.to_string()))?;
        if metadata.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
                .map_err(|error| CoreError::Provider(error.to_string()))?;
        } else {
            std::os::windows::fs::symlink_file(target, link)
                .map_err(|error| CoreError::Provider(error.to_string()))?;
        }
    }

    Ok(())
}

fn materialize_codex_shadow_home(layout: &CodexHomeLayout) -> Result<()> {
    if layout.mode != CodexHomeLayoutMode::AuthOverlay {
        return Ok(());
    }

    let effective_home_path = layout.effective_home_path.clone().ok_or_else(|| {
        CoreError::Provider("Codex shadow home path is missing from layout".to_string())
    })?;

    if layout.shared_home_path == effective_home_path {
        return Err(CoreError::Provider(
            "Codex shadow home path must be different from the shared home path".to_string(),
        ));
    }

    fs::create_dir_all(&layout.shared_home_path)
        .map_err(|error| CoreError::Provider(error.to_string()))?;
    fs::create_dir_all(&effective_home_path)
        .map_err(|error| CoreError::Provider(error.to_string()))?;

    for directory in CODEX_KNOWN_SHARED_DIRECTORIES {
        fs::create_dir_all(layout.shared_home_path.join(directory))
            .map_err(|error| CoreError::Provider(error.to_string()))?;
    }

    let entries = fs::read_dir(&layout.shared_home_path)
        .map_err(|error| CoreError::Provider(error.to_string()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let mut entries_to_link = CODEX_KNOWN_SHARED_DIRECTORIES
        .iter()
        .map(|entry| (*entry).to_string())
        .collect::<Vec<_>>();

    for entry_name in entries {
        if !CODEX_PRIVATE_ENTRY_NAMES.contains(&entry_name.as_str())
            && !CODEX_SHADOW_LOCAL_ENTRY_NAMES.contains(&entry_name.as_str())
        {
            entries_to_link.push(entry_name);
        }
    }

    for entry_name in CODEX_PRIVATE_ENTRY_NAMES {
        if *entry_name != "auth.json" {
            remove_path_if_symlink(&effective_home_path.join(entry_name))?;
        }
    }

    for entry_name in entries_to_link {
        if CODEX_PRIVATE_ENTRY_NAMES.contains(&entry_name.as_str()) {
            continue;
        }

        ensure_symlink(
            &layout.shared_home_path.join(&entry_name),
            &effective_home_path.join(&entry_name),
        )?;
    }

    ensure_shadow_auth_is_private(&effective_home_path)?;
    Ok(())
}

pub(crate) fn parse_provider_stream_line(
    line: &str,
    state: &mut ProviderStreamState,
) -> ParsedProviderLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedProviderLine::Ignored;
    }

    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => return ParsedProviderLine::Text(trimmed.to_string()),
    };

    let event = parse_custom_envelope(&value)
        .or_else(|| parse_claude_stream_value(&value, state))
        .or_else(|| parse_codex_stream_value(&value, state));
    let mut events = state.claude_pending_events.drain(..).collect::<Vec<_>>();
    if let Some(event) = event {
        events.insert(0, event);
    }
    match events.len() {
        0 => ParsedProviderLine::Ignored,
        1 => ParsedProviderLine::Event(events.pop().expect("one event")),
        _ => ParsedProviderLine::Events(events),
    }
}

fn parse_custom_envelope(value: &Value) -> Option<ProviderEvent> {
    let envelope = serde_json::from_value::<ProviderEnvelope>(value.clone()).ok()?;
    let at = now_iso();

    Some(match envelope {
        ProviderEnvelope::ReasoningStarted { id, label } => {
            ProviderEvent::ReasoningStarted { id, label, at }
        }
        ProviderEnvelope::ReasoningDelta { id, content } => {
            ProviderEvent::ReasoningDelta { id, content }
        }
        ProviderEnvelope::ReasoningCompleted { id } => ProviderEvent::ReasoningCompleted { id, at },
        ProviderEnvelope::ToolCallStarted {
            id,
            action,
            command,
            file,
        } => ProviderEvent::ToolCallStarted {
            id,
            action,
            command,
            file,
            at,
        },
        ProviderEnvelope::ToolCallDelta { id, content } => {
            ProviderEvent::ToolCallDelta { id, content }
        }
        ProviderEnvelope::ToolCallCompleted { id } => ProviderEvent::ToolCallCompleted { id, at },
        ProviderEnvelope::ToolCallFailed { id, reason } => {
            ProviderEvent::ToolCallFailed { id, reason, at }
        }
    })
}

fn parse_claude_stream_value(
    value: &Value,
    state: &mut ProviderStreamState,
) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    if kind != "stream_event" {
        return parse_claude_terminal_value(value, state);
    }
    if value
        .get("parent_tool_use_id")
        .is_some_and(|parent| !parent.is_null())
    {
        return None;
    }

    let event = value.get("event")?.as_object()?;
    let event_type = event.get("type").and_then(Value::as_str)?;
    let at = now_iso();

    match event_type {
        "message_start" => {
            state.claude_blocks.clear();
            state.claude_active_message_id = event
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("id"))
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .or_else(|| claude_envelope_uuid(value));
            state.claude_text_message_started = false;
            None
        }
        "content_block_start" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
            let block = event.get("content_block")?.as_object()?;
            match block.get("type").and_then(Value::as_str)? {
                "thinking" | "redacted_thinking" => {
                    let id = claude_reasoning_id(state, index);
                    state
                        .claude_blocks
                        .insert(index, ClaudeBlockState::Reasoning { id: id.clone() });
                    Some(ProviderEvent::ReasoningStarted {
                        id,
                        label: Some("Thinking".to_string()),
                        at,
                    })
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| claude_tool_call_id(state, index));
                    let action = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Tool call");
                    // The Claude Agent tool is an explicit, structured signal
                    // that a native subagent was invoked.  Do not use message
                    // text or a parent model as a proxy. `model` is only kept
                    // when the tool input itself reports it.
                    if action == "Agent" {
                        let mut metadata = claude_native_subagent_metadata(block.get("input"));
                        if let Some(hook_metadata) = state.claude_native_subagents.remove(&id) {
                            metadata.agent_id = hook_metadata.agent_id;
                            metadata.hook_agent_id = hook_metadata.hook_agent_id;
                            metadata.name = metadata.name.or(hook_metadata.name);
                            metadata.role = metadata.role.or(hook_metadata.role);
                            metadata.model = metadata.model.or(hook_metadata.model);
                        }
                        state.claude_pending_tool_calls.insert(id.clone());
                        state
                            .claude_native_subagents
                            .insert(id.clone(), metadata.clone());
                        if block
                            .get("input")
                            .and_then(Value::as_object)
                            .is_none_or(|input| input.is_empty())
                        {
                            state
                                .claude_native_subagent_inputs
                                .insert(id.clone(), String::new());
                        }
                        state
                            .claude_blocks
                            .insert(index, ClaudeBlockState::ToolCall { id: id.clone() });
                        if let Some(model) = metadata.model.clone() {
                            state.claude_pending_events.push(
                                ProviderEvent::NativeSubagentModelRequested {
                                    correlation_id: id.clone(),
                                    model,
                                    at: at.clone(),
                                },
                            );
                        }
                        return Some(ProviderEvent::NativeSubagentActivity {
                            id,
                            agent_id: metadata.agent_id,
                            agent_thread_id: None,
                            path: None,
                            name: metadata.name,
                            role: metadata.role,
                            model: None,
                            status: NativeSubagentStatus::Running,
                            at,
                        });
                    }
                    state.claude_pending_tool_calls.insert(id.clone());
                    state
                        .claude_blocks
                        .insert(index, ClaudeBlockState::ToolCall { id: id.clone() });
                    let input = block.get("input");
                    let (command, file) = claude_tool_input_metadata(input);
                    Some(ProviderEvent::ToolCallStarted {
                        id,
                        action: action.to_string(),
                        command,
                        file,
                        at,
                    })
                }
                "text" => {
                    if state.claude_text_message_started {
                        return None;
                    }
                    let id = claude_active_message_id(state, value);
                    state.claude_text_message_started = true;
                    Some(ProviderEvent::AssistantMessageStarted {
                        id,
                        phase: AssistantMessagePhase::Unknown,
                        at,
                    })
                }
                _ => None,
            }
        }
        "content_block_delta" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
            let delta = event.get("delta")?.as_object()?;
            match delta.get("type").and_then(Value::as_str)? {
                "thinking_delta" => {
                    let id = claude_block_id_for_reasoning(state, index);
                    let content = delta.get("thinking").and_then(Value::as_str).unwrap_or("");
                    Some(ProviderEvent::ReasoningDelta {
                        id,
                        content: content.to_string(),
                    })
                }
                "text_delta" => {
                    let content = delta.get("text").and_then(Value::as_str).unwrap_or("");
                    let id = claude_active_message_id(state, value);
                    Some(ProviderEvent::AssistantMessageDelta {
                        id,
                        content: content.to_string(),
                    })
                }
                "input_json_delta" => {
                    let id = claude_block_id_for_tool_call(state, index);
                    let content = delta
                        .get("partial_json")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if state.claude_native_subagents.contains_key(&id) {
                        let input = state.claude_native_subagent_inputs.entry(id).or_default();
                        input.push_str(content);
                        return None;
                    }
                    Some(ProviderEvent::ToolCallDelta {
                        id,
                        content: content.to_string(),
                    })
                }
                "signature_delta" => None,
                _ => None,
            }
        }
        "content_block_stop" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
            match state.claude_blocks.remove(&index) {
                Some(ClaudeBlockState::Reasoning { id }) => {
                    Some(ProviderEvent::ReasoningCompleted { id, at })
                }
                Some(ClaudeBlockState::ToolCall { id }) => {
                    let input = state.claude_native_subagent_inputs.remove(&id)?;
                    if input.is_empty() {
                        return None;
                    }
                    let input = serde_json::from_str::<Value>(&input).ok()?;
                    let reported = claude_native_subagent_metadata(Some(&input));
                    let metadata = state.claude_native_subagents.get_mut(&id)?;
                    metadata.name = reported.name.or_else(|| metadata.name.clone());
                    metadata.role = reported.role.or_else(|| metadata.role.clone());
                    metadata.model = reported.model.or_else(|| metadata.model.clone());
                    let metadata = metadata.clone();
                    let requested_model = metadata.model.clone();
                    if let Some(model) = requested_model {
                        return Some(ProviderEvent::NativeSubagentModelRequested {
                            correlation_id: id,
                            model,
                            at,
                        });
                    }
                    Some(ProviderEvent::NativeSubagentActivity {
                        id,
                        agent_id: metadata.agent_id,
                        agent_thread_id: None,
                        path: None,
                        name: metadata.name,
                        role: metadata.role,
                        model: None,
                        status: NativeSubagentStatus::Running,
                        at,
                    })
                }
                None => None,
            }
        }
        "message_delta" | "message_stop" => None,
        _ => None,
    }
}

fn parse_claude_terminal_value(
    value: &Value,
    state: &mut ProviderStreamState,
) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    let at = now_iso();
    if let Some(event) = parse_claude_native_subagent_hook(value, state, at.clone()) {
        return Some(event);
    }
    match kind {
        "assistant" => {
            let message = value.get("message")?.as_object()?;
            let message_model = nonempty_json_string(message.get("model"));
            let parent_tool = nonempty_json_string(value.get("parent_tool_use_id"));
            if let Some(parent_tool) = parent_tool {
                return message_model.map(|model| ProviderEvent::NativeSubagentModelConfirmed {
                    correlation_id: parent_tool,
                    model,
                    at,
                });
            }
            let content = message.get("content")?.as_array()?;
            let text = content
                .iter()
                .filter_map(|block| block.as_object())
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");

            // The Agent SDK can expose the stream envelope UUID for partial
            // text while the authoritative `assistant` snapshot carries the
            // underlying Anthropic message ID. Once text streaming started,
            // the active stream ID is the lifecycle identity already exposed
            // to consumers and must also close that item.
            let streamed_text_id = state
                .claude_text_message_started
                .then(|| state.claude_active_message_id.clone())
                .flatten();
            let id = streamed_text_id
                .or_else(|| {
                    message
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string)
                })
                .or_else(|| state.claude_active_message_id.clone())
                .or_else(|| claude_envelope_uuid(value))
                .unwrap_or_else(|| format!("claude:assistant:{}", Uuid::new_v4()));
            state.claude_active_message_id = None;
            state.claude_text_message_started = false;

            if text.is_empty() && message_model.is_none() {
                None
            } else {
                Some(ProviderEvent::AssistantMessageCompleted {
                    id,
                    phase: AssistantMessagePhase::Unknown,
                    content: (!text.is_empty()).then_some(text),
                    model: message_model,
                    at,
                })
            }
        }
        "user" => {
            let content = value.pointer("/message/content")?.as_array()?;
            for block in content.iter().filter_map(Value::as_object) {
                if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                    continue;
                };
                if !state.claude_pending_tool_calls.remove(id) {
                    continue;
                }
                if let Some(metadata) = state.claude_native_subagents.remove(id) {
                    state.claude_native_subagent_inputs.remove(id);
                    let status = if block
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        NativeSubagentStatus::Failed
                    } else {
                        NativeSubagentStatus::Completed
                    };
                    if state
                        .claude_native_subagent_event_ids
                        .values()
                        .any(|event_id| event_id == id)
                    {
                        state
                            .claude_native_subagent_terminal_statuses
                            .insert(id.to_string(), status.clone());
                    }
                    return Some(ProviderEvent::NativeSubagentActivity {
                        id: id.to_string(),
                        agent_id: metadata.agent_id,
                        agent_thread_id: None,
                        path: None,
                        name: metadata.name,
                        role: metadata.role,
                        model: None,
                        status,
                        at,
                    });
                }
                return if block
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    Some(ProviderEvent::ToolCallFailed {
                        id: id.to_string(),
                        reason: Some("tool execution failed".to_string()),
                        at,
                    })
                } else {
                    Some(ProviderEvent::ToolCallCompleted {
                        id: id.to_string(),
                        at,
                    })
                };
            }
            None
        }
        "dcc_user_input_request" => {
            let id = value
                .get("request_id")
                .and_then(Value::as_str)
                .unwrap_or("user-input")
                .to_string();
            let questions = value
                .get("questions")
                .cloned()
                .and_then(|raw| serde_json::from_value(raw).ok())
                .unwrap_or_default();
            Some(ProviderEvent::UserInputRequested { id, questions, at })
        }
        "dcc_user_input_resolved" => {
            let id = value
                .get("request_id")
                .and_then(Value::as_str)
                .unwrap_or("user-input")
                .to_string();
            let answers = value
                .get("answers")
                .cloned()
                .and_then(|raw| serde_json::from_value(raw).ok())
                .unwrap_or_default();
            Some(ProviderEvent::UserInputResolved { id, answers, at })
        }
        "dcc_permission_request" => {
            let request = dcc_core::ports::provider::ProviderPermissionRequest {
                request_id: value
                    .get("request_id")
                    .and_then(Value::as_str)
                    .unwrap_or("permission")
                    .to_string(),
                tool_name: value
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("Tool")
                    .to_string(),
                title: value
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                description: value
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                command: value
                    .get("command")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                file: value
                    .get("file")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
            Some(ProviderEvent::PermissionRequested { request, at })
        }
        "dcc_permission_resolved" => {
            let id = value
                .get("request_id")
                .and_then(Value::as_str)
                .unwrap_or("permission")
                .to_string();
            let behavior = value
                .get("behavior")
                .and_then(Value::as_str)
                .unwrap_or("deny")
                .to_string();
            Some(ProviderEvent::PermissionResolved { id, behavior, at })
        }
        "dcc_plan_captured" => {
            value
                .get("plan")
                .and_then(Value::as_str)
                .map(|plan| ProviderEvent::TextDelta {
                    content: plan.to_string(),
                })
        }
        "result" => {
            let terminal_event = if value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let message = value
                    .get("result")
                    .and_then(Value::as_str)
                    .or_else(|| value.get("terminal_reason").and_then(Value::as_str))
                    .map(str::to_string)
                    .unwrap_or_else(|| "provider result reported an error".to_string());
                ProviderEvent::Failed {
                    message,
                    at: at.clone(),
                }
            } else {
                ProviderEvent::Completed { at: at.clone() }
            };
            let models = parse_claude_result_usage(value);
            if models.is_empty() {
                Some(terminal_event)
            } else {
                state.claude_pending_events.push(terminal_event);
                Some(ProviderEvent::TurnUsage { models, at })
            }
        }
        _ => None,
    }
}

fn json_u64(object: &serde_json::Map<String, Value>, key: &str) -> u64 {
    object.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn parse_claude_result_usage(value: &Value) -> Vec<ModelTokenUsage> {
    let mut models = value
        .get("modelUsage")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|model_usage| model_usage.iter())
        .filter_map(|(model, raw)| {
            let raw = raw.as_object()?;
            let input_tokens = json_u64(raw, "inputTokens");
            let output_tokens = json_u64(raw, "outputTokens");
            let cached_input_tokens = json_u64(raw, "cacheReadInputTokens");
            let cache_write_input_tokens = json_u64(raw, "cacheCreationInputTokens");
            Some(ModelTokenUsage {
                model: Some(model.clone()),
                input_tokens,
                output_tokens,
                cached_input_tokens,
                cache_write_input_tokens,
                reasoning_output_tokens: 0,
                total_tokens: input_tokens
                    .saturating_add(output_tokens)
                    .saturating_add(cached_input_tokens)
                    .saturating_add(cache_write_input_tokens),
                cost_usd: raw.get("costUSD").and_then(Value::as_f64),
            })
        })
        .collect::<Vec<_>>();
    if !models.is_empty() {
        return models;
    }

    let Some(usage) = value.get("usage").and_then(Value::as_object) else {
        return models;
    };
    let input_tokens = json_u64(usage, "input_tokens");
    let output_tokens = json_u64(usage, "output_tokens");
    let cached_input_tokens = json_u64(usage, "cache_read_input_tokens");
    let cache_write_input_tokens = json_u64(usage, "cache_creation_input_tokens");
    let total_tokens = input_tokens
        .saturating_add(output_tokens)
        .saturating_add(cached_input_tokens)
        .saturating_add(cache_write_input_tokens);
    if total_tokens > 0 {
        models.push(ModelTokenUsage {
            model: None,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            reasoning_output_tokens: 0,
            total_tokens,
            cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
        });
    }
    models
}

fn codex_agent_message_phase(item: &serde_json::Map<String, Value>) -> AssistantMessagePhase {
    match item.get("phase").and_then(Value::as_str) {
        Some("commentary") => AssistantMessagePhase::Commentary,
        Some("final_answer") | Some("finalAnswer") => AssistantMessagePhase::FinalAnswer,
        _ => AssistantMessagePhase::Unknown,
    }
}

fn parse_codex_stream_value(
    value: &Value,
    _state: &mut ProviderStreamState,
) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    let at = now_iso();

    match kind {
        "item/started" => {
            let item = value.get("item")?.as_object()?;
            match item.get("type").and_then(Value::as_str)? {
                "agentMessage" | "agent_message" => Some(ProviderEvent::AssistantMessageStarted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("codex-agent-message")
                        .to_string(),
                    phase: codex_agent_message_phase(item),
                    at,
                }),
                "reasoning" => Some(ProviderEvent::ReasoningStarted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("reasoning")
                        .to_string(),
                    label: codex_reasoning_label(item),
                    at,
                }),
                "commandExecution" => Some(ProviderEvent::ToolCallStarted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("command")
                        .to_string(),
                    action: "Bash".to_string(),
                    command: item
                        .get("command")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    file: codex_command_file(item),
                    at,
                }),
                "web_search" => Some(ProviderEvent::ToolCallStarted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("web-search")
                        .to_string(),
                    action: "WebSearch".to_string(),
                    command: item
                        .get("query")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    file: None,
                    at,
                }),
                "mcp_tool_call" => Some(ProviderEvent::ToolCallStarted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("mcp-tool")
                        .to_string(),
                    action: codex_mcp_action(item),
                    command: item.get("tool").and_then(Value::as_str).map(str::to_string),
                    file: None,
                    at,
                }),
                "file_change" => Some(ProviderEvent::ToolCallStarted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("file-change")
                        .to_string(),
                    action: "apply_patch".to_string(),
                    command: None,
                    file: item
                        .get("file_path")
                        .or_else(|| item.get("filePath"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    at,
                }),
                "todo_list" => Some(ProviderEvent::ToolCallStarted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("todo-list")
                        .to_string(),
                    action: "TodoWrite".to_string(),
                    command: None,
                    file: None,
                    at,
                }),
                _ => None,
            }
        }
        "item/agentMessage/delta" => Some(ProviderEvent::AssistantMessageDelta {
            id: value
                .get("itemId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .unwrap_or("codex-agent-message")
                .to_string(),
            content: value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "item/completed" => {
            let item = value.get("item")?.as_object()?;
            match item.get("type").and_then(Value::as_str)? {
                "agentMessage" | "agent_message" => {
                    Some(ProviderEvent::AssistantMessageCompleted {
                        id: item
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("codex-agent-message")
                            .to_string(),
                        phase: codex_agent_message_phase(item),
                        content: item.get("text").and_then(Value::as_str).map(str::to_string),
                        model: None,
                        at,
                    })
                }
                "reasoning" => Some(ProviderEvent::ReasoningCompleted {
                    id: item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("reasoning")
                        .to_string(),
                    at,
                }),
                "commandExecution" => {
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("command")
                        .to_string();
                    let failed = codex_item_failed(item);
                    if failed {
                        Some(ProviderEvent::ToolCallFailed {
                            id,
                            reason: codex_failure_reason(item),
                            at,
                        })
                    } else {
                        Some(ProviderEvent::ToolCallCompleted { id, at })
                    }
                }
                "web_search" | "mcp_tool_call" | "file_change" | "todo_list" => {
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    let failed = codex_item_failed(item);
                    if failed {
                        Some(ProviderEvent::ToolCallFailed {
                            id,
                            reason: codex_failure_reason(item),
                            at,
                        })
                    } else {
                        Some(ProviderEvent::ToolCallCompleted { id, at })
                    }
                }
                _ => None,
            }
        }
        "turn/completed" | "result" => Some(ProviderEvent::Completed { at }),
        "turn/aborted" => Some(ProviderEvent::Failed {
            message: value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("turn aborted")
                .to_string(),
            at,
        }),
        _ => None,
    }
}

fn claude_envelope_uuid(value: &Value) -> Option<String> {
    value
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn claude_active_message_id(state: &mut ProviderStreamState, value: &Value) -> String {
    if let Some(id) = state.claude_active_message_id.clone() {
        return id;
    }
    let id = claude_envelope_uuid(value)
        .unwrap_or_else(|| format!("claude:assistant:{}", Uuid::new_v4()));
    state.claude_active_message_id = Some(id.clone());
    id
}

fn claude_reasoning_id(state: &ProviderStreamState, index: u64) -> String {
    match state.claude_active_message_id.as_deref() {
        Some(message_id) => format!("{message_id}:reasoning:{index}"),
        None => format!("claude:reasoning:{index}"),
    }
}

fn claude_tool_call_id(state: &ProviderStreamState, index: u64) -> String {
    match state.claude_active_message_id.as_deref() {
        Some(message_id) => format!("{message_id}:tool-call:{index}"),
        None => format!("claude:tool-call:{index}"),
    }
}

fn claude_block_id_for_reasoning(state: &ProviderStreamState, index: u64) -> String {
    state
        .claude_blocks
        .get(&index)
        .and_then(|block| match block {
            ClaudeBlockState::Reasoning { id } => Some(id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| claude_reasoning_id(state, index))
}

fn claude_block_id_for_tool_call(state: &ProviderStreamState, index: u64) -> String {
    state
        .claude_blocks
        .get(&index)
        .and_then(|block| match block {
            ClaudeBlockState::ToolCall { id } => Some(id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| claude_tool_call_id(state, index))
}

fn claude_tool_input_metadata(input: Option<&Value>) -> (Option<String>, Option<String>) {
    let Some(input) = input.and_then(Value::as_object) else {
        return (None, None);
    };
    let command = input
        .get("command")
        .or_else(|| input.get("query"))
        .or_else(|| input.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let file = input
        .get("file_path")
        .or_else(|| input.get("filePath"))
        .or_else(|| input.get("path"))
        .or_else(|| input.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    (command, file)
}

fn codex_reasoning_label(item: &serde_json::Map<String, Value>) -> Option<String> {
    item.get("summary")
        .and_then(Value::as_array)
        .and_then(|summary| summary.first())
        .and_then(Value::as_object)
        .and_then(|entry| entry.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| Some("Thinking".to_string()))
}

fn codex_command_file(item: &serde_json::Map<String, Value>) -> Option<String> {
    item.get("commandActions")
        .and_then(Value::as_array)
        .and_then(|actions| actions.first())
        .and_then(Value::as_object)
        .and_then(|action| {
            action
                .get("path")
                .or_else(|| action.get("name"))
                .or_else(|| action.get("file"))
        })
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn codex_mcp_action(item: &serde_json::Map<String, Value>) -> String {
    let server = item.get("server").and_then(Value::as_str).unwrap_or("");
    let tool = item.get("tool").and_then(Value::as_str).unwrap_or("");
    if server.is_empty() && tool.is_empty() {
        "mcp_tool_call".to_string()
    } else {
        format!("mcp__{server}__{tool}")
    }
}

fn codex_item_failed(item: &serde_json::Map<String, Value>) -> bool {
    item.get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "failed")
        || item
            .get("exitCode")
            .and_then(Value::as_i64)
            .is_some_and(|code| code != 0)
        || item
            .get("exit_code")
            .and_then(Value::as_i64)
            .is_some_and(|code| code != 0)
        || item
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn codex_failure_reason(item: &serde_json::Map<String, Value>) -> Option<String> {
    item.get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            item.get("exitCode")
                .and_then(Value::as_i64)
                .map(|code| format!("exit code {code}"))
        })
        .or_else(|| {
            item.get("exit_code")
                .and_then(Value::as_i64)
                .map(|code| format!("exit code {code}"))
        })
        .or_else(|| Some("tool call failed".to_string()))
}

#[derive(Clone)]
pub struct CliProviderAdapter {
    pub id: ProviderId,
    pub label: String,
    pub description: String,
    pub binary: String,
    pub capabilities: Capabilities,
    pub stable: bool,
    runtime: Arc<ProviderRuntimeState>,
}

#[derive(Default)]
struct ProviderRuntimeState {
    sessions: Mutex<HashMap<String, Arc<SessionRuntime>>>,
}

struct SessionRuntime {
    handle: SessionHandle,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    events_tx: broadcast::Sender<ProviderEvent>,
}

pub(crate) fn append_tool_instructions(prompt: String, tool_instructions: Option<&str>) -> String {
    let Some(instructions) = tool_instructions
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return prompt;
    };
    format!(
        "{prompt}\n\n[DCC provider tool instructions]\n{instructions}\n[/DCC provider tool instructions]"
    )
}

/// Namespaced env for stdin-only CLI adapters (no undocumented vendor-specific vars).
pub(crate) fn apply_cli_spawn_environment(
    command: &mut Command,
    provider_registry_id: &str,
    cfg: &SessionConfig,
) -> Result<()> {
    command.env("PATH", augmented_path());
    command.env("DCC_PROVIDER_ID", provider_registry_id);
    command.env("DCC_WORKSPACE_ID", &cfg.workspace_id.0);
    command.env("DCC_SESSION_ID", &cfg.session_id.0);
    if let Some(ref m) = cfg.model {
        command.env("DCC_MODEL", m);
    }

    let runtime_cfg = runtime_config(cfg);
    match provider_registry_id {
        "claude_code" => {
            command.env("DCC_AGENT_RUNTIME", "claude_code");
            if let Some(home_path) = runtime_cfg.and_then(|config| config.home_path.as_deref()) {
                if let Some(resolved_home) = resolve_runtime_home_path(Some(home_path)) {
                    command.env("HOME", resolved_home);
                }
            }
        }
        "codex" => {
            command.env("DCC_AGENT_RUNTIME", "codex");
            let layout = resolve_codex_home_layout(cfg);
            materialize_codex_shadow_home(&layout)?;
            if let Some(effective_home_path) = layout.effective_home_path.as_ref() {
                command.env("CODEX_HOME", effective_home_path);
            }
        }
        "gemini" => {
            command.env("DCC_AGENT_RUNTIME", "gemini");
            if let Some(home_path) = runtime_cfg.and_then(|config| config.home_path.as_deref()) {
                if let Some(resolved_home) = resolve_runtime_home_path(Some(home_path)) {
                    command.env("HOME", resolved_home);
                }
            }
        }
        "droid" => {
            command.env("DCC_AGENT_RUNTIME", "droid");
        }
        "cursor" => {
            command.env("DCC_AGENT_RUNTIME", "cursor");
            command.env("DCC_CURSOR_ADAPTER", "experimental");
        }
        "grok" => {
            command.env("DCC_AGENT_RUNTIME", "grok");
            if let Some(home_path) = runtime_cfg.and_then(|config| config.home_path.as_deref()) {
                if let Some(resolved_home) = resolve_runtime_home_path(Some(home_path)) {
                    command.env("GROK_HOME", resolved_home);
                }
            }
        }
        _ => {}
    }

    Ok(())
}

impl CliProviderAdapter {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        binary: impl Into<String>,
        capabilities: Capabilities,
        stable: bool,
    ) -> Self {
        Self {
            id: ProviderId(id.into()),
            label: label.into(),
            description: description.into(),
            binary: binary.into(),
            capabilities,
            stable,
            runtime: Arc::new(ProviderRuntimeState::default()),
        }
    }

    fn binary_command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.arg("--version");
        command.env("PATH", augmented_path());
        command
    }

    fn interactive_command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());
        command
    }

    async fn start_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let mut command = self.interactive_command();
        apply_cli_spawn_environment(&mut command, &self.id.0, &cfg)?;
        if let Some(ref working_directory) = cfg.working_directory {
            let cwd = std::path::PathBuf::from(working_directory);
            if !working_directory.trim().is_empty() {
                command.current_dir(cwd);
            }
        }
        let mut child = command.spawn().map_err(|error| {
            CoreError::Provider(format!("failed to spawn {}: {}", self.binary, error))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CoreError::Provider(format!("{} did not expose stdin", self.binary)))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Provider(format!("{} did not expose stdout", self.binary)))?;

        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id,
            handle_id: Uuid::new_v4().to_string(),
        };
        let session_key = handle.session_id.0.clone();
        let (events_tx, _) = broadcast::channel(64);
        let runtime = Arc::new(SessionRuntime {
            handle: handle.clone(),
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(child),
            events_tx: events_tx.clone(),
        });

        self.runtime
            .sessions
            .lock()
            .await
            .insert(session_key.clone(), runtime.clone());

        let runtime_for_task = runtime.clone();
        let runtime_state = Arc::clone(&self.runtime);
        let binary = self.binary.clone();
        tokio::spawn(async move {
            let mut stream_state = ProviderStreamState::default();
            let _ = runtime_for_task
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });

            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let content = line.trim_end().to_string();
                if content.is_empty() {
                    continue;
                }
                match parse_provider_stream_line(&content, &mut stream_state) {
                    ParsedProviderLine::Event(event) => {
                        let _ = runtime_for_task.events_tx.send(event);
                    }
                    ParsedProviderLine::Events(events) => {
                        for event in events {
                            let _ = runtime_for_task.events_tx.send(event);
                        }
                    }
                    ParsedProviderLine::Text(text) => {
                        let _ = runtime_for_task
                            .events_tx
                            .send(ProviderEvent::TextDelta { content: text });
                    }
                    ParsedProviderLine::Ignored => {}
                }
            }

            let exit_result = {
                let mut child = runtime_for_task.child.lock().await;
                child.wait().await
            };

            let at = now_iso();
            match exit_result {
                Ok(exit) if exit.success() => {
                    let _ = runtime_for_task
                        .events_tx
                        .send(ProviderEvent::Completed { at });
                }
                Ok(exit) => {
                    let _ = runtime_for_task.events_tx.send(ProviderEvent::Failed {
                        message: format!("{binary} exited with status {exit}"),
                        at,
                    });
                }
                Err(error) => {
                    let _ = runtime_for_task.events_tx.send(ProviderEvent::Failed {
                        message: format!("failed to wait for {binary}: {error}"),
                        at,
                    });
                }
            }

            runtime_state.sessions.lock().await.remove(&session_key);
        });

        Ok(handle)
    }

    async fn runtime_for_session(&self, session_id: &SessionId) -> Option<Arc<SessionRuntime>> {
        self.runtime
            .sessions
            .lock()
            .await
            .get(&session_id.0)
            .cloned()
    }
}

pub(crate) fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn expand_home_path(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return PathBuf::new();
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
        return PathBuf::from(home).join(rest);
    }

    if trimmed == "~" {
        let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
        return PathBuf::from(home);
    }

    PathBuf::from(trimmed)
}

pub(crate) fn resolve_runtime_home_path(path: Option<&str>) -> Option<PathBuf> {
    let value = path?.trim();
    if value.is_empty() {
        return None;
    }
    let resolved = expand_home_path(value);
    if resolved.as_os_str().is_empty() {
        return None;
    }
    if resolved.is_absolute() {
        Some(resolved)
    } else {
        std::env::current_dir().ok().map(|cwd| cwd.join(resolved))
    }
}

pub(crate) fn runtime_config(cfg: &SessionConfig) -> Option<&ProviderRuntimeConfig> {
    cfg.provider_runtime.as_ref()
}

pub(crate) fn augmented_path() -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let existing = std::env::var("PATH").unwrap_or_default();

    let Ok(home) = std::env::var("HOME") else {
        return existing;
    };

    let mut extra: Vec<PathBuf> = Vec::new();

    let nvm_base = PathBuf::from(&home)
        .join(".nvm")
        .join("versions")
        .join("node");
    if let Ok(entries) = fs::read_dir(&nvm_base) {
        let mut dirs: Vec<_> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path().join("bin"))
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort_by(|left, right| right.cmp(left));
        extra.extend(dirs);
    }

    extra.push(PathBuf::from(&home).join(".local").join("bin"));
    extra.push(PathBuf::from(&home).join("node_modules").join(".bin"));

    let prefix = extra
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();

    if prefix.is_empty() {
        existing
    } else {
        format!("{}{}{}", prefix.join(&sep.to_string()), sep, existing)
    }
}

#[async_trait]
impl Provider for CliProviderAdapter {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        self.start_runtime(cfg).await
    }

    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()> {
        let runtime = self
            .runtime_for_session(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no runtime for session {} on provider {}",
                    handle.session_id.0, self.binary
                ))
            })?;

        match input {
            Input::Text(text) => {
                let mut stdin = runtime.stdin.lock().await;
                let stream = stdin.as_mut().ok_or_else(|| {
                    CoreError::Provider(format!(
                        "stdin closed for session {} on provider {}",
                        handle.session_id.0, self.binary
                    ))
                })?;
                stream.write_all(text.as_bytes()).await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to write input for {}: {}",
                        self.binary, error
                    ))
                })?;
                stream.write_all(b"\n").await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to terminate input for {}: {}",
                        self.binary, error
                    ))
                })?;
                stream.flush().await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to flush input for {}: {}",
                        self.binary, error
                    ))
                })?;
            }
            Input::Turn(turn) => {
                let text = compose_wire_prompt_for_provider(
                    &self.id.0,
                    &turn.prompt,
                    turn.plan_mode,
                    turn.effort.as_deref(),
                    turn.fast_mode,
                );
                let text = append_tool_instructions(text, turn.tool_instructions.as_deref());
                let mut stdin = runtime.stdin.lock().await;
                let stream = stdin.as_mut().ok_or_else(|| {
                    CoreError::Provider(format!(
                        "stdin closed for session {} on provider {}",
                        handle.session_id.0, self.binary
                    ))
                })?;
                stream.write_all(text.as_bytes()).await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to write input for {}: {}",
                        self.binary, error
                    ))
                })?;
                stream.write_all(b"\n").await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to terminate input for {}: {}",
                        self.binary, error
                    ))
                })?;
                stream.flush().await.map_err(|error| {
                    CoreError::Provider(format!(
                        "failed to flush input for {}: {}",
                        self.binary, error
                    ))
                })?;
            }
            Input::UserInputResponse(_) => {
                return Err(CoreError::Provider(format!(
                    "{} does not support mid-turn user input responses",
                    self.binary
                )));
            }
            Input::PermissionResponse(_) => {
                return Err(CoreError::Provider(format!(
                    "{} does not support mid-turn permission responses",
                    self.binary
                )));
            }
        }

        Ok(())
    }

    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
        let runtime = self
            .runtime
            .sessions
            .try_lock()
            .ok()
            .and_then(|sessions| sessions.get(&handle.session_id.0).cloned());

        let Some(runtime) = runtime else {
            return Box::pin(stream::empty());
        };

        let receiver = runtime.events_tx.subscribe();
        let stream = stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => return Some((Ok(event), receiver)),
                    Err(broadcast::error::RecvError::Closed) => return None,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });

        Box::pin(stream)
    }

    async fn cancel(&self, handle: &SessionHandle) -> Result<()> {
        let runtime = self
            .runtime_for_session(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no runtime for session {} on provider {}",
                    handle.session_id.0, self.binary
                ))
            })?;

        let mut child = runtime.child.lock().await;
        child.kill().await.map_err(|error| {
            CoreError::Provider(format!("failed to cancel {}: {}", self.binary, error))
        })?;
        Ok(())
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        let runtime = self.runtime_for_session(previous).await.ok_or_else(|| {
            CoreError::Provider(format!(
                "no resumable runtime for session {} on provider {}",
                previous.0, self.binary
            ))
        })?;

        Ok(runtime.handle.clone())
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        match self.binary_command().output().await {
            Ok(output) if output.status.success() => Ok(HealthStatus::Healthy),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let reason = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    format!("{} exited with status {}", self.binary, output.status)
                };
                Ok(HealthStatus::Degraded { reason })
            }
            Err(error) => Ok(HealthStatus::Unhealthy {
                reason: format!("failed to execute {}: {}", self.binary, error),
            }),
        }
    }
}

use dcc_core::domain::provider::TurnControlSupport;

pub fn stable_cli_capabilities() -> Capabilities {
    Capabilities {
        streaming: true,
        supports_steering: false,
        supports_native_subagent_steering: false,
        supports_native_subagent_interrupt: false,
        mcp_support: McpSupportLevel::Unsupported,
        mcp_oauth_support: dcc_core::domain::provider::McpOauthSupport::Unsupported,
        tools: true,
        vision: true,
        resumable: true,
        experimental: false,
        can_be_delegation_target: true,
        can_request_delegation: false,
        supports_read_only_delegation: true,
        supports_edit_delegation: true,
        supports_multi_root: false,
        approval_policies: Vec::new(),
        supports_runtime_home: false,
        supports_runtime_binary: false,
        supports_shadow_home: false,
        supports_subagent_concurrency: false,
        supports_account_usage: false,
        plan_mode_support: TurnControlSupport::PromptFallback,
        fast_mode_support: TurnControlSupport::PromptFallback,
        supports_dynamic_models: false,
        supports_compaction_command: false,
    }
}

pub fn experimental_cli_capabilities() -> Capabilities {
    Capabilities {
        streaming: true,
        supports_steering: false,
        supports_native_subagent_steering: false,
        supports_native_subagent_interrupt: false,
        mcp_support: McpSupportLevel::Unsupported,
        mcp_oauth_support: dcc_core::domain::provider::McpOauthSupport::Unsupported,
        tools: true,
        vision: false,
        resumable: false,
        experimental: true,
        can_be_delegation_target: true,
        can_request_delegation: false,
        supports_read_only_delegation: true,
        supports_edit_delegation: true,
        supports_multi_root: false,
        approval_policies: Vec::new(),
        supports_runtime_home: false,
        supports_runtime_binary: false,
        supports_shadow_home: false,
        supports_subagent_concurrency: false,
        supports_account_usage: false,
        plan_mode_support: TurnControlSupport::PromptFallback,
        fast_mode_support: TurnControlSupport::PromptFallback,
        supports_dynamic_models: false,
        supports_compaction_command: false,
    }
}

pub fn local_workspace_id() -> WorkspaceId {
    WorkspaceId("local".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{}-{}", prefix, Uuid::new_v4()))
    }

    fn write_text_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(path, contents).expect("write text file");
    }

    #[test]
    fn expands_tilde_home_paths() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let resolved = resolve_runtime_home_path(Some("~/.dcc/codex"))
            .expect("runtime home path should resolve");
        assert_eq!(resolved, PathBuf::from(home).join(".dcc/codex"));
    }

    #[test]
    fn resolves_codex_shadow_layout() {
        let shared_home = temp_path("dcc-codex-shared");
        let shadow_home = temp_path("dcc-codex-shadow");
        let cfg = SessionConfig {
            workspace_id: WorkspaceId("workspace".to_string()),
            session_id: SessionId("session".to_string()),
            model: None,
            working_directory: None,
            additional_working_directories: Vec::new(),
            provider_runtime: Some(ProviderRuntimeConfig {
                binary_path: None,
                home_path: Some(shared_home.to_string_lossy().to_string()),
                shadow_home_path: Some(shadow_home.to_string_lossy().to_string()),
                max_concurrent_subagents: None,
            }),
            mcp_servers: Vec::new(),
        };

        let layout = resolve_codex_home_layout(&cfg);

        assert_eq!(layout.mode, CodexHomeLayoutMode::AuthOverlay);
        assert_eq!(layout.shared_home_path, shared_home);
        assert_eq!(layout.effective_home_path.as_ref(), Some(&shadow_home));
        assert_eq!(
            layout.continuation_key,
            format!("codex:home:{}", layout.shared_home_path.to_string_lossy())
        );
    }

    #[test]
    fn materializes_codex_shadow_home() {
        let shared_home = temp_path("dcc-codex-shared");
        let shadow_home = temp_path("dcc-codex-shadow");

        fs::create_dir_all(shared_home.join("sessions")).expect("create shared sessions");
        fs::create_dir_all(shared_home.join("log")).expect("create shared log");
        fs::create_dir_all(shared_home.join("memories")).expect("create shared memories");
        fs::create_dir_all(shared_home.join("tmp")).expect("create shared tmp");
        fs::create_dir_all(shadow_home.join("log")).expect("create shadow log");
        fs::create_dir_all(shadow_home.join("memories")).expect("create shadow memories");
        fs::create_dir_all(shadow_home.join("tmp")).expect("create shadow tmp");
        write_text_file(&shared_home.join("config.toml"), "model = \"gpt-5.4\"\n");
        write_text_file(&shared_home.join("auth.json"), "{\"shared\":true}\n");
        write_text_file(&shadow_home.join("auth.json"), "{\"shadow\":true}\n");
        write_text_file(
            &shadow_home.join("models_cache.json"),
            "{\"shadow\":true}\n",
        );

        let layout = CodexHomeLayout {
            mode: CodexHomeLayoutMode::AuthOverlay,
            shared_home_path: shared_home.clone(),
            effective_home_path: Some(shadow_home.clone()),
            continuation_key: format!("codex:home:{}", shared_home.to_string_lossy()),
        };

        materialize_codex_shadow_home(&layout).expect("materialize shadow home");

        let sessions_link = fs::read_link(shadow_home.join("sessions")).expect("sessions symlink");
        let config_link = fs::read_link(shadow_home.join("config.toml")).expect("config symlink");
        let log_metadata = fs::symlink_metadata(shadow_home.join("log")).expect("log metadata");
        let memories_metadata =
            fs::symlink_metadata(shadow_home.join("memories")).expect("memories metadata");
        let tmp_metadata = fs::symlink_metadata(shadow_home.join("tmp")).expect("tmp metadata");
        let auth_contents =
            fs::read_to_string(shadow_home.join("auth.json")).expect("shadow auth contents");
        let models_cache_contents =
            fs::read_to_string(shadow_home.join("models_cache.json")).expect("shadow cache");

        assert_eq!(sessions_link, shared_home.join("sessions"));
        assert_eq!(config_link, shared_home.join("config.toml"));
        assert!(log_metadata.file_type().is_dir());
        assert!(memories_metadata.file_type().is_dir());
        assert!(tmp_metadata.file_type().is_dir());
        assert!(auth_contents.contains("shadow"));
        assert!(models_cache_contents.contains("shadow"));
    }

    #[test]
    fn parses_claude_stream_reasoning_and_completion() {
        let mut state = ProviderStreamState::default();

        let start = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}}"#,
            &mut state,
        );
        match start {
            ParsedProviderLine::Event(ProviderEvent::ReasoningStarted { id, label, .. }) => {
                assert_eq!(id, "claude:reasoning:0");
                assert_eq!(label.as_deref(), Some("Thinking"));
            }
            other => panic!("expected reasoning start, got {other:?}"),
        }

        let delta = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think."}}}"#,
            &mut state,
        );
        match delta {
            ParsedProviderLine::Event(ProviderEvent::ReasoningDelta { id, content }) => {
                assert_eq!(id, "claude:reasoning:0");
                assert_eq!(content, "Let me think.");
            }
            other => panic!("expected reasoning delta, got {other:?}"),
        }

        let stop = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            &mut state,
        );
        match stop {
            ParsedProviderLine::Event(ProviderEvent::ReasoningCompleted { id, .. }) => {
                assert_eq!(id, "claude:reasoning:0");
            }
            other => panic!("expected reasoning completed, got {other:?}"),
        }

        let result = parse_provider_stream_line(
            r#"{"type":"result","is_error":false,"result":"done"}"#,
            &mut state,
        );
        match result {
            ParsedProviderLine::Event(ProviderEvent::Completed { .. }) => {}
            other => panic!("expected turn completion, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_result_model_usage_before_completion() {
        let mut state = ProviderStreamState::default();
        let parsed = parse_provider_stream_line(
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.42,"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":2,"cache_creation_input_tokens":1},"modelUsage":{"claude-sonnet-4-6":{"inputTokens":10,"outputTokens":5,"cacheReadInputTokens":2,"cacheCreationInputTokens":1,"webSearchRequests":0,"costUSD":0.42,"contextWindow":200000,"maxOutputTokens":64000}}}"#,
            &mut state,
        );

        match parsed {
            ParsedProviderLine::Events(events) => {
                assert_eq!(events.len(), 2);
                match &events[0] {
                    ProviderEvent::TurnUsage { models, .. } => {
                        assert_eq!(models.len(), 1);
                        assert_eq!(models[0].model.as_deref(), Some("claude-sonnet-4-6"));
                        assert_eq!(models[0].total_tokens, 18);
                        assert_eq!(models[0].cost_usd, Some(0.42));
                    }
                    other => panic!("expected turn usage, got {other:?}"),
                }
                assert!(matches!(events[1], ProviderEvent::Completed { .. }));
            }
            other => panic!("expected usage and completion events, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_terminal_assistant_text_when_stream_is_absent() {
        let mut state = ProviderStreamState::default();

        let assistant = parse_provider_stream_line(
            r#"{"type":"assistant","uuid":"sdk-message-1","parent_tool_use_id":null,"message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Not logged in \u00b7 Please run /login"}]}}"#,
            &mut state,
        );
        match assistant {
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageCompleted {
                id,
                phase,
                content,
                ..
            }) => {
                assert_eq!(id, "msg_1");
                assert_eq!(phase, AssistantMessagePhase::Unknown);
                assert_eq!(
                    content.as_deref(),
                    Some("Not logged in · Please run /login")
                );
            }
            other => panic!("expected authoritative assistant message, got {other:?}"),
        }
    }

    #[test]
    fn reconciles_claude_stream_with_authoritative_terminal_assistant_text() {
        let mut state = ProviderStreamState::default();

        let message_start = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-1","parent_tool_use_id":null,"event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
            &mut state,
        );
        assert!(matches!(message_start, ParsedProviderLine::Ignored));

        let content_start = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-2","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            &mut state,
        );
        assert!(matches!(
            content_start,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageStarted {
                ref id,
                phase: AssistantMessagePhase::Unknown,
                ..
            }) if id == "msg_1"
        ));

        let delta = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-3","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
            &mut state,
        );
        assert!(matches!(
            delta,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageDelta {
                ref id,
                ref content,
            }) if id == "msg_1" && content == "Hello"
        ));

        let assistant = parse_provider_stream_line(
            r#"{"type":"assistant","uuid":"sdk-message-1","parent_tool_use_id":null,"message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Hello world"}]}}"#,
            &mut state,
        );
        assert!(matches!(
            assistant,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageCompleted {
                ref id,
                phase: AssistantMessagePhase::Unknown,
                content: Some(ref content),
                ..
            }) if id == "msg_1" && content == "Hello world"
        ));
    }

    #[test]
    fn projects_claude_agent_tool_lifecycle_without_inferring_model() {
        let mut state = ProviderStreamState::default();
        let started = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_agent","name":"Agent","input":{"subagent_type":"Explore"}}}}"#,
            &mut state,
        );
        assert!(matches!(
            started,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: None,
                role: Some(ref role),
                model: None,
                status: NativeSubagentStatus::Running,
                ..
            }) if id == "toolu_agent" && role == "Explore"
        ));

        let completed = parse_provider_stream_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_agent","is_error":false,"content":"done"}]}}"#,
            &mut state,
        );
        assert!(matches!(
            completed,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                model: None,
                status: NativeSubagentStatus::Completed,
                ..
            }) if id == "toolu_agent"
        ));
    }

    #[test]
    fn captures_claude_effective_models_without_leaking_child_text() {
        let mut state = ProviderStreamState::default();
        let root = parse_provider_stream_line(
            r#"{"type":"assistant","message":{"id":"msg_root","model":"claude-opus-effective","content":[{"type":"text","text":"root answer"}]}}"#,
            &mut state,
        );
        assert!(matches!(
            root,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageCompleted {
                content: Some(ref content), model: Some(ref model), ..
            }) if content == "root answer" && model == "claude-opus-effective"
        ));

        let child = parse_provider_stream_line(
            r#"{"type":"assistant","parent_tool_use_id":"toolu_agent","message":{"id":"msg_child","model":"claude-sonnet-effective","content":[{"type":"text","text":"private child text"}]}}"#,
            &mut state,
        );
        assert!(matches!(
            child,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentModelConfirmed {
                ref correlation_id, ref model, ..
            }) if correlation_id == "toolu_agent" && model == "claude-sonnet-effective"
        ));
    }

    #[test]
    fn enriches_claude_agent_activity_with_a_hook_tool_use_id() {
        let mut state = ProviderStreamState::default();
        let _ = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_agent","name":"Agent","input":{"subagent_type":"Explore"}}}}"#,
            &mut state,
        );

        let started = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-1","agent_type":"Explore","status":"running","correlation_id":"toolu_agent"}"#,
            &mut state,
        );
        assert!(matches!(
            started,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: Some(ref agent_id),
                role: Some(ref role),
                model: None,
                status: NativeSubagentStatus::Running,
                ..
            }) if id == "toolu_agent" && agent_id == "native-agent-1" && role == "Explore"
        ));

        let stopped = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-1","agent_type":"Explore","status":"completed","correlation_id":null}"#,
            &mut state,
        );
        assert!(matches!(
            stopped,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: Some(ref agent_id),
                status: NativeSubagentStatus::Completed,
                ..
            }) if id == "toolu_agent" && agent_id == "native-agent-1"
        ));
    }

    #[test]
    fn correlates_claude_hook_callback_uuid_with_the_only_pending_agent() {
        let mut state = ProviderStreamState::default();
        let _ = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_agent","name":"Agent","input":{"subagent_type":"general-purpose","model":"sonnet"}}}}"#,
            &mut state,
        );

        let started = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-uuid","agent_type":"general-purpose","status":"running","correlation_id":"2736fc94-19c4-4678-9abf-c6c012fdd851"}"#,
            &mut state,
        );
        assert!(matches!(
            started,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: Some(ref agent_id),
                role: Some(ref role),
                status: NativeSubagentStatus::Running,
                ..
            }) if id == "toolu_agent" && agent_id == "native-agent-uuid"
                && role == "general-purpose"
        ));

        let stopped = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-uuid","agent_type":"general-purpose","status":"completed","correlation_id":"be9bc4eb-1243-4493-9573-5b5ee39c252c"}"#,
            &mut state,
        );
        assert!(matches!(
            stopped,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                status: NativeSubagentStatus::Completed,
                ..
            }) if id == "toolu_agent"
        ));
    }

    #[test]
    fn does_not_guess_a_claude_hook_identity_with_multiple_pending_agents() {
        let mut state = ProviderStreamState::default();
        for (index, id) in [(0, "toolu_agent_one"), (1, "toolu_agent_two")] {
            let line = format!(
                r#"{{"type":"stream_event","uuid":"stream-agent-{index}","parent_tool_use_id":null,"event":{{"type":"content_block_start","index":{index},"content_block":{{"type":"tool_use","id":"{id}","name":"Agent","input":{{"subagent_type":"general-purpose"}}}}}}}}"#
            );
            let _ = parse_provider_stream_line(&line, &mut state);
        }

        let event = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-ambiguous","agent_type":"general-purpose","status":"running","correlation_id":"2736fc94-19c4-4678-9abf-c6c012fdd851"}"#,
            &mut state,
        );
        assert!(matches!(
            event,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                ..
            }) if id == "claude:subagent:native-agent-ambiguous"
        ));
    }

    #[test]
    fn preserves_hook_identity_when_claude_reports_start_before_the_agent_block() {
        let mut state = ProviderStreamState::default();
        let hook = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-early","agent_type":"reviewer","status":"running","correlation_id":"toolu_agent_early"}"#,
            &mut state,
        );
        assert!(matches!(
            hook,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: Some(ref agent_id),
                ..
            }) if id == "toolu_agent_early" && agent_id == "native-agent-early"
        ));

        let tool = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_agent_early","name":"Agent","input":{"subagent_type":"reviewer","model":"opus"}}}}"#,
            &mut state,
        );
        assert!(matches!(
            tool,
            ParsedProviderLine::Events(events) if matches!(events.as_slice(), [
                ProviderEvent::NativeSubagentActivity {
                    id,
                    agent_id: Some(agent_id),
                    role: Some(role),
                    model: None,
                    ..
                },
                ProviderEvent::NativeSubagentModelRequested { correlation_id, model, .. }
            ] if id == "toolu_agent_early" && agent_id == "native-agent-early"
                && role == "reviewer" && correlation_id == "toolu_agent_early" && model == "opus")
        ));
    }

    #[test]
    fn keeps_a_failed_agent_result_when_subagent_stop_arrives_later() {
        let mut state = ProviderStreamState::default();
        let _ = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_agent_failed","name":"Agent","input":{"subagent_type":"Explore"}}}}"#,
            &mut state,
        );
        let _ = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-failed","agent_type":"Explore","status":"running","correlation_id":"toolu_agent_failed"}"#,
            &mut state,
        );
        let failed = parse_provider_stream_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_agent_failed","is_error":true,"content":"failed"}]}}"#,
            &mut state,
        );
        assert!(matches!(
            failed,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                status: NativeSubagentStatus::Failed,
                ..
            })
        ));

        let stopped = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-failed","agent_type":"Explore","status":"completed","correlation_id":null}"#,
            &mut state,
        );
        assert!(matches!(
            stopped,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: Some(ref agent_id),
                status: NativeSubagentStatus::Failed,
                ..
            }) if id == "toolu_agent_failed" && agent_id == "native-agent-failed"
        ));
    }

    #[test]
    fn keeps_an_uncorrelated_claude_hook_separate_from_agent_tool_calls() {
        let mut state = ProviderStreamState::default();
        let event = parse_provider_stream_line(
            r#"{"type":"dcc_native_subagent_activity","agent_id":"native-agent-2","agent_type":"Explore","status":"running","correlation_id":null}"#,
            &mut state,
        );
        assert!(matches!(
            event,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: Some(ref agent_id),
                ..
            }) if id == "claude:subagent:native-agent-2" && agent_id == "native-agent-2"
        ));
    }

    #[test]
    fn collects_streamed_claude_agent_metadata_without_emitting_a_generic_tool_call() {
        let mut state = ProviderStreamState::default();
        let started = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_agent_streamed","name":"Agent","input":{}}}}"#,
            &mut state,
        );
        assert!(matches!(
            started,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                agent_id: None,
                name: None,
                role: None,
                model: None,
                status: NativeSubagentStatus::Running,
                ..
            })
        ));

        let input_delta = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"subagent_type\":\"reviewer\",\"model\":\"opus\",\"name\":\"Opus reviewer\"}"}}}"#,
            &mut state,
        );
        assert!(matches!(input_delta, ParsedProviderLine::Ignored));

        let input_completed = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-agent","parent_tool_use_id":null,"event":{"type":"content_block_stop","index":0}}"#,
            &mut state,
        );
        assert!(matches!(
            input_completed,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentModelRequested {
                ref correlation_id, ref model, ..
            }) if correlation_id == "toolu_agent_streamed" && model == "opus"
        ));

        let completed = parse_provider_stream_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_agent_streamed","is_error":false,"content":"done"}]}}"#,
            &mut state,
        );
        assert!(matches!(
            completed,
            ParsedProviderLine::Event(ProviderEvent::NativeSubagentActivity {
                ref id,
                agent_id: None,
                name: Some(ref name),
                role: Some(ref role),
                model: None,
                status: NativeSubagentStatus::Completed,
                ..
            }) if id == "toolu_agent_streamed"
                && name == "Opus reviewer"
                && role == "reviewer"
        ));
    }

    #[test]
    fn keeps_claude_stream_identity_when_terminal_snapshot_uses_another_id() {
        let mut state = ProviderStreamState::default();

        let message_start = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"stream-envelope-id","parent_tool_use_id":null,"event":{"type":"message_start","message":{"type":"message","role":"assistant","content":[]}}}"#,
            &mut state,
        );
        assert!(matches!(message_start, ParsedProviderLine::Ignored));

        let content_start = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"content-envelope-id","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            &mut state,
        );
        assert!(matches!(
            content_start,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageStarted {
                ref id,
                phase: AssistantMessagePhase::Unknown,
                ..
            }) if id == "stream-envelope-id"
        ));

        let delta = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"delta-envelope-id","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Toda a mensagem"}}}"#,
            &mut state,
        );
        assert!(matches!(
            delta,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageDelta {
                ref id,
                ref content,
            }) if id == "stream-envelope-id" && content == "Toda a mensagem"
        ));

        let assistant = parse_provider_stream_line(
            r#"{"type":"assistant","uuid":"sdk-message-id","parent_tool_use_id":null,"message":{"id":"msg_authoritative","role":"assistant","content":[{"type":"text","text":"Toda a mensagem, sem perder o final."}]}}"#,
            &mut state,
        );
        assert!(matches!(
            assistant,
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageCompleted {
                ref id,
                phase: AssistantMessagePhase::Unknown,
                content: Some(ref content),
                ..
            }) if id == "stream-envelope-id" && content == "Toda a mensagem, sem perder o final."
        ));
    }

    #[test]
    fn does_not_flatten_claude_subagent_text_into_the_root_timeline() {
        let mut state = ProviderStreamState::default();
        let partial = parse_provider_stream_line(
            r#"{"type":"stream_event","uuid":"sdk-subagent-stream","parent_tool_use_id":"toolu_parent","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"private subagent delta"}}}"#,
            &mut state,
        );
        assert!(matches!(partial, ParsedProviderLine::Ignored));

        let assistant = parse_provider_stream_line(
            r#"{"type":"assistant","uuid":"sdk-subagent-message","parent_tool_use_id":"toolu_parent","message":{"id":"msg_subagent","role":"assistant","content":[{"type":"text","text":"private subagent progress"}]}}"#,
            &mut state,
        );
        assert!(matches!(assistant, ParsedProviderLine::Ignored));
    }

    #[test]
    fn completes_claude_tools_only_after_the_sdk_reports_the_tool_result() {
        let mut state = ProviderStreamState::default();

        let started = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool-success","name":"mcp__dcc-fixture__fixture_echo","input":{}}}}"#,
            &mut state,
        );
        match started {
            ParsedProviderLine::Event(ProviderEvent::ToolCallStarted { id, action, .. }) => {
                assert_eq!(id, "tool-success");
                assert_eq!(action, "mcp__dcc-fixture__fixture_echo");
            }
            other => panic!("expected tool call start, got {other:?}"),
        }

        let proposed = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            &mut state,
        );
        assert!(matches!(proposed, ParsedProviderLine::Ignored));

        let completed = parse_provider_stream_line(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-success","content":"fixture result"}]}}"#,
            &mut state,
        );
        match completed {
            ParsedProviderLine::Event(ProviderEvent::ToolCallCompleted { id, .. }) => {
                assert_eq!(id, "tool-success");
            }
            other => panic!("expected executed tool completion, got {other:?}"),
        }

        let _ = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool-denied","name":"mcp__dcc-fixture__fixture_mutate","input":{}}}}"#,
            &mut state,
        );
        let _ = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
            &mut state,
        );
        let denied = parse_provider_stream_line(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-denied","content":"User denied tool execution.","is_error":true}]}}"#,
            &mut state,
        );
        match denied {
            ParsedProviderLine::Event(ProviderEvent::ToolCallFailed { id, reason, .. }) => {
                assert_eq!(id, "tool-denied");
                assert_eq!(reason.as_deref(), Some("tool execution failed"));
            }
            other => panic!("expected denied tool failure, got {other:?}"),
        }
    }

    #[test]
    fn parses_codex_command_execution_and_turn_completion() {
        let mut state = ProviderStreamState::default();

        let started = parse_provider_stream_line(
            r#"{"type":"item/started","item":{"type":"commandExecution","id":"call_1","command":"/bin/zsh -lc 'cat package.json'","source":"unifiedExecStartup","commandActions":[{"type":"read","command":"cat package.json","name":"package.json","path":"$DUMMY_REPO/package.json"}]}}"#,
            &mut state,
        );
        match started {
            ParsedProviderLine::Event(ProviderEvent::ToolCallStarted {
                id,
                action,
                command,
                file,
                ..
            }) => {
                assert_eq!(id, "call_1");
                assert_eq!(action, "Bash");
                assert_eq!(command.as_deref(), Some("/bin/zsh -lc 'cat package.json'"));
                assert_eq!(file.as_deref(), Some("$DUMMY_REPO/package.json"));
            }
            other => panic!("expected tool call start, got {other:?}"),
        }

        let delta = parse_provider_stream_line(
            r#"{"type":"item/agentMessage/delta","threadId":"thread_1","turnId":"turn_1","itemId":"msg_1","delta":"Listing"}"#,
            &mut state,
        );
        match delta {
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageDelta { id, content }) => {
                assert_eq!(id, "msg_1");
                assert_eq!(content, "Listing");
            }
            other => panic!("expected assistant delta, got {other:?}"),
        }

        let separated_delta = parse_provider_stream_line(
            r#"{"type":"item/agentMessage/delta","threadId":"thread_1","turnId":"turn_1","itemId":"msg_2","delta":"Finished listing."}"#,
            &mut state,
        );
        match separated_delta {
            ParsedProviderLine::Event(ProviderEvent::AssistantMessageDelta { id, content }) => {
                assert_eq!(id, "msg_2");
                assert_eq!(content, "Finished listing.");
            }
            other => panic!("expected second assistant delta, got {other:?}"),
        }

        let completed = parse_provider_stream_line(
            r#"{"type":"item/completed","item":{"type":"commandExecution","id":"call_1","status":"completed","exitCode":0}}"#,
            &mut state,
        );
        match completed {
            ParsedProviderLine::Event(ProviderEvent::ToolCallCompleted { id, .. }) => {
                assert_eq!(id, "call_1");
            }
            other => panic!("expected tool call completion, got {other:?}"),
        }

        let turn_completed = parse_provider_stream_line(
            r#"{"type":"turn/completed","threadId":"thread_1","turn":{"id":"turn_1","status":"completed"}}"#,
            &mut state,
        );
        match turn_completed {
            ParsedProviderLine::Event(ProviderEvent::Completed { .. }) => {}
            other => panic!("expected provider completion, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_sdk_custom_permission_events() {
        let mut state = ProviderStreamState::default();

        let requested = parse_provider_stream_line(
            r#"{"type":"dcc_permission_request","request_id":"perm-1","tool_name":"Bash","title":"Run shell command","description":"The agent wants to run npm test","command":"npm test","file":"package.json"}"#,
            &mut state,
        );
        match requested {
            ParsedProviderLine::Event(ProviderEvent::PermissionRequested { request, .. }) => {
                assert_eq!(request.request_id, "perm-1");
                assert_eq!(request.tool_name, "Bash");
                assert_eq!(request.title.as_deref(), Some("Run shell command"));
                assert_eq!(
                    request.description.as_deref(),
                    Some("The agent wants to run npm test")
                );
                assert_eq!(request.command.as_deref(), Some("npm test"));
                assert_eq!(request.file.as_deref(), Some("package.json"));
            }
            other => panic!("expected permission request, got {other:?}"),
        }

        let resolved = parse_provider_stream_line(
            r#"{"type":"dcc_permission_resolved","request_id":"perm-1","behavior":"allow"}"#,
            &mut state,
        );
        match resolved {
            ParsedProviderLine::Event(ProviderEvent::PermissionResolved {
                id, behavior, ..
            }) => {
                assert_eq!(id, "perm-1");
                assert_eq!(behavior, "allow");
            }
            other => panic!("expected permission resolution, got {other:?}"),
        }
    }
}
