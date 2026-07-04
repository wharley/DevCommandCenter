use std::{
    collections::HashMap,
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
        provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
        session::SessionId,
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
    claude_streamed_text_emitted: bool,
    pub(crate) gemini_streamed_text_emitted: bool,
    codex_last_agent_message_id: Option<String>,
}

#[derive(Debug, Clone)]
enum ClaudeBlockState {
    Reasoning { id: String },
    ToolCall { id: String },
}

#[derive(Debug)]
pub(crate) enum ParsedProviderLine {
    Event(ProviderEvent),
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

    if let Some(event) = parse_custom_envelope(&value) {
        return ParsedProviderLine::Event(event);
    }
    if let Some(event) = parse_claude_stream_value(&value, state) {
        return ParsedProviderLine::Event(event);
    }
    if let Some(event) = parse_codex_stream_value(&value, state) {
        return ParsedProviderLine::Event(event);
    }

    ParsedProviderLine::Ignored
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

    let event = value.get("event")?.as_object()?;
    let event_type = event.get("type").and_then(Value::as_str)?;
    let at = now_iso();

    match event_type {
        "message_start" => {
            state.claude_blocks.clear();
            state.claude_streamed_text_emitted = false;
            None
        }
        "content_block_start" => {
            let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
            let block = event.get("content_block")?.as_object()?;
            match block.get("type").and_then(Value::as_str)? {
                "thinking" | "redacted_thinking" => {
                    let id = claude_reasoning_id(index);
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
                        .unwrap_or_else(|| claude_tool_call_id(index));
                    state
                        .claude_blocks
                        .insert(index, ClaudeBlockState::ToolCall { id: id.clone() });
                    let input = block.get("input");
                    let (command, file) = claude_tool_input_metadata(input);
                    Some(ProviderEvent::ToolCallStarted {
                        id,
                        action: block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("Tool call")
                            .to_string(),
                        command,
                        file,
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
                    state.claude_streamed_text_emitted = true;
                    Some(ProviderEvent::TextDelta {
                        content: content.to_string(),
                    })
                }
                "input_json_delta" => {
                    let id = claude_block_id_for_tool_call(state, index);
                    let content = delta
                        .get("partial_json")
                        .and_then(Value::as_str)
                        .unwrap_or("");
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
                    Some(ProviderEvent::ToolCallCompleted { id, at })
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
    state: &ProviderStreamState,
) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    let at = now_iso();
    match kind {
        "assistant" => {
            if state.claude_streamed_text_emitted {
                return None;
            }

            let message = value.get("message")?.as_object()?;
            let content = message.get("content")?.as_array()?;
            let text = content
                .iter()
                .filter_map(|block| block.as_object())
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");

            if text.is_empty() {
                None
            } else {
                Some(ProviderEvent::TextDelta { content: text })
            }
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
            if value
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
                Some(ProviderEvent::Failed { message, at })
            } else {
                Some(ProviderEvent::Completed { at })
            }
        }
        _ => None,
    }
}

fn codex_agent_message_delta_content(
    item_id: Option<&str>,
    delta: &str,
    last_agent_message_id: &mut Option<String>,
) -> String {
    let should_prefix_separator = item_id.is_some_and(|current_id| {
        last_agent_message_id
            .as_deref()
            .is_some_and(|previous_id| previous_id != current_id)
    }) && !delta.is_empty();

    if let Some(current_id) = item_id.filter(|value| !value.is_empty()) {
        *last_agent_message_id = Some(current_id.to_string());
    }

    if should_prefix_separator {
        format!("\n\n{delta}")
    } else {
        delta.to_string()
    }
}

fn parse_codex_stream_value(
    value: &Value,
    state: &mut ProviderStreamState,
) -> Option<ProviderEvent> {
    let kind = value.get("type").and_then(Value::as_str)?;
    let at = now_iso();

    match kind {
        "item/started" => {
            let item = value.get("item")?.as_object()?;
            match item.get("type").and_then(Value::as_str)? {
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
        "item/agentMessage/delta" => Some(ProviderEvent::TextDelta {
            content: codex_agent_message_delta_content(
                value.get("itemId").and_then(Value::as_str),
                value.get("delta").and_then(Value::as_str).unwrap_or(""),
                &mut state.codex_last_agent_message_id,
            ),
        }),
        "item/completed" => {
            let item = value.get("item")?.as_object()?;
            match item.get("type").and_then(Value::as_str)? {
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
        "turn/completed" | "result" => {
            state.codex_last_agent_message_id = None;
            Some(ProviderEvent::Completed { at })
        }
        "turn/aborted" => {
            state.codex_last_agent_message_id = None;
            Some(ProviderEvent::Failed {
                message: value
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("turn aborted")
                    .to_string(),
                at,
            })
        }
        _ => None,
    }
}

fn claude_reasoning_id(index: u64) -> String {
    format!("claude:reasoning:{index}")
}

fn claude_tool_call_id(index: u64) -> String {
    format!("claude:tool-call:{index}")
}

fn claude_block_id_for_reasoning(state: &ProviderStreamState, index: u64) -> String {
    state
        .claude_blocks
        .get(&index)
        .and_then(|block| match block {
            ClaudeBlockState::Reasoning { id } => Some(id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| claude_reasoning_id(index))
}

fn claude_block_id_for_tool_call(state: &ProviderStreamState, index: u64) -> String {
    state
        .claude_blocks
        .get(&index)
        .and_then(|block| match block {
            ClaudeBlockState::ToolCall { id } => Some(id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| claude_tool_call_id(index))
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
    let Some(instructions) = tool_instructions.map(str::trim).filter(|value| !value.is_empty())
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

pub fn stable_cli_capabilities() -> Capabilities {
    Capabilities {
        streaming: true,
        mcp: true,
        tools: true,
        vision: true,
        resumable: true,
        experimental: false,
        can_be_delegation_target: true,
        can_request_delegation: false,
        supports_read_only_delegation: true,
        supports_edit_delegation: true,
    }
}

pub fn experimental_cli_capabilities() -> Capabilities {
    Capabilities {
        streaming: true,
        mcp: false,
        tools: true,
        vision: false,
        resumable: false,
        experimental: true,
        can_be_delegation_target: true,
        can_request_delegation: false,
        supports_read_only_delegation: true,
        supports_edit_delegation: true,
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
            provider_runtime: Some(ProviderRuntimeConfig {
                home_path: Some(shared_home.to_string_lossy().to_string()),
                shadow_home_path: Some(shadow_home.to_string_lossy().to_string()),
            }),
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
    fn parses_claude_terminal_assistant_text_when_stream_is_absent() {
        let mut state = ProviderStreamState::default();

        let assistant = parse_provider_stream_line(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Not logged in \u00b7 Please run /login"}]}}"#,
            &mut state,
        );
        match assistant {
            ParsedProviderLine::Event(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "Not logged in · Please run /login");
            }
            other => panic!("expected assistant fallback text, got {other:?}"),
        }
    }

    #[test]
    fn ignores_claude_terminal_assistant_text_after_streamed_text() {
        let mut state = ProviderStreamState::default();

        let _ = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
            &mut state,
        );
        let _ = parse_provider_stream_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
            &mut state,
        );
        let assistant = parse_provider_stream_line(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello world"}]}}"#,
            &mut state,
        );
        match assistant {
            ParsedProviderLine::Ignored => {}
            other => panic!("expected assistant fallback to stay ignored, got {other:?}"),
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
            ParsedProviderLine::Event(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "Listing");
            }
            other => panic!("expected text delta, got {other:?}"),
        }

        let separated_delta = parse_provider_stream_line(
            r#"{"type":"item/agentMessage/delta","threadId":"thread_1","turnId":"turn_1","itemId":"msg_2","delta":"Finished listing."}"#,
            &mut state,
        );
        match separated_delta {
            ParsedProviderLine::Event(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "\n\nFinished listing.");
            }
            other => panic!("expected separated text delta, got {other:?}"),
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
