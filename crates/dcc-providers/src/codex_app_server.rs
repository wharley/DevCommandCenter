use std::{
    collections::{HashMap, HashSet},
    process::{Command as StdCommand, Stdio},
    str,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock as StdRwLock,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::{
    stream::{self, BoxStream},
    StreamExt,
};
use reqwest::Url;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, oneshot, Mutex},
    time::timeout,
};
use uuid::Uuid;

use dcc_core::{
    application::{compose_fallback_prompt_for_provider, PromptInjectionOptions},
    domain::{
        mcp::{
            McpDefinitionId, McpErrorCategory, McpRuntimeState, McpRuntimeStatus,
            McpToolPolicyDecision,
        },
        provider::{
            Capabilities, HealthStatus, ProviderAccountUsage, ProviderAccountUsageState,
            ProviderApprovalPolicy, ProviderEvent, ProviderId, ProviderUsageWindow, SessionHandle,
        },
        session::SessionId,
    },
    ports::{
        provider::{ProviderPermissionRequest, ProviderPermissionResponse},
        Input, Provider, ProviderMcpOauthStart, ProviderRuntimeConfig, SessionConfig,
    },
    CoreError, Result,
};

use crate::codex_mcp::{
    codex_mcp_approval_policy, codex_mcp_approval_policy_with_native, codex_mcp_runtime_version,
    failed_codex_mcp_status_snapshot, initial_codex_mcp_status_snapshot, merge_codex_mcp_status,
    parse_codex_mcp_startup_status, parse_codex_mcp_status_snapshot, prepare_thread_start_request,
    CodexMcpDefinitionMap, CodexMcpToolPolicyMap,
};
use crate::common::{append_tool_instructions, augmented_path};

// ── JSON-RPC helpers ────────────────────────────────────────────────────────

fn rpc_request(id: u64, method: &str, params: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
    .to_string()
}

fn rpc_notification(method: &str) -> String {
    json!({ "jsonrpc": "2.0", "method": method }).to_string()
}

fn rpc_response(id: &Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
    .to_string()
}

fn initialize_params(experimental_api: bool) -> Value {
    json!({
        "clientInfo": { "name": "dcc", "version": env!("CARGO_PKG_VERSION") },
        // runtimeWorkspaceRoots is currently part of the Codex app-server
        // experimental API. Opt in only for multi-root sessions so the existing
        // single-workspace and account-usage flows stay on the stable protocol.
        "capabilities": if experimental_api {
            json!({ "experimentalApi": true })
        } else {
            json!({})
        },
    })
}

fn thread_start_params(cwd: &str, additional_working_directories: &[String]) -> Value {
    let mut params = json!({
        "cwd": cwd,
        "approvalPolicy": "never",
        "sandbox": "workspace-write",
    });
    if !additional_working_directories.is_empty() {
        let mut runtime_workspace_roots = vec![cwd.to_string()];
        runtime_workspace_roots.extend(additional_working_directories.iter().cloned());
        params["runtimeWorkspaceRoots"] = json!(runtime_workspace_roots);
    }
    params
}

fn parse_codex_cli_version_output(output: &str) -> Option<&str> {
    const MAX_CODEX_VERSION_CHARS: usize = 64;
    let mut fields = output.split_whitespace();
    match (fields.next(), fields.next(), fields.next()) {
        (Some("codex-cli"), Some(version), None)
            if !version.is_empty()
                && version.chars().count() <= MAX_CODEX_VERSION_CHARS
                && version.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')
                }) =>
        {
            Some(version)
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodexMcpProjection {
    cli_version: String,
    runtime_version: String,
}

impl CodexMcpProjection {
    fn from_cli_output(output: &str) -> Option<Self> {
        let cli_version = parse_codex_cli_version_output(output)?.to_string();
        Some(Self {
            runtime_version: codex_mcp_runtime_version(&cli_version),
            cli_version,
        })
    }
}

fn detect_codex_mcp_projection() -> Option<CodexMcpProjection> {
    let output = StdCommand::new("codex")
        .arg("--version")
        .env("PATH", augmented_path())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    CodexMcpProjection::from_cli_output(str::from_utf8(&output.stdout).ok()?)
}

fn initialize_result_codex_version(result: &Value) -> Option<&str> {
    let user_agent = result.get("userAgent")?.as_str()?;
    user_agent
        .split_whitespace()
        .next()?
        .strip_prefix("dcc/")
        .filter(|version| !version.is_empty())
}

fn validate_codex_mcp_projection(
    projection: Option<&CodexMcpProjection>,
    initialize_result: &Value,
) -> Result<()> {
    let detected = projection.ok_or_else(|| {
        CoreError::Provider(
            "Codex MCP bridge could not detect the installed codex-cli version".to_string(),
        )
    })?;
    let reported = initialize_result_codex_version(initialize_result).ok_or_else(|| {
        CoreError::Provider(
            "Codex MCP bridge initialize response omitted the runtime version".to_string(),
        )
    })?;
    if reported != detected.cli_version {
        return Err(CoreError::Provider(format!(
            "Codex MCP bridge version mismatch: detected {}, app-server reported {}",
            detected.cli_version, reported
        )));
    }
    Ok(())
}

fn codex_reasoning_effort(effort: Option<&str>) -> Option<&'static str> {
    match effort.map(str::trim).filter(|value| !value.is_empty()) {
        Some("none") => Some("none"),
        Some("minimal") => Some("minimal"),
        Some("low") => Some("low"),
        Some("balanced") | Some("medium") => Some("medium"),
        Some("high") => Some("high"),
        Some("xhigh") | Some("max") | Some("ultrathink") => Some("xhigh"),
        Some(_) | None => None,
    }
}

async fn write_line(stdin: &mut ChildStdin, line: &str) -> Result<()> {
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| CoreError::Provider(format!("codex stdin write: {e}")))?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|e| CoreError::Provider(format!("codex stdin write newline: {e}")))?;
    stdin
        .flush()
        .await
        .map_err(|e| CoreError::Provider(format!("codex stdin flush: {e}")))
}

// ── Incoming JSON-RPC message ───────────────────────────────────────────────

#[derive(Deserialize)]
struct Incoming {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    params: Option<Value>,
}

fn codex_reset_time(value: &Value) -> Option<String> {
    let timestamp = value.as_i64()?;
    DateTime::<Utc>::from_timestamp(timestamp, 0).map(|value| value.to_rfc3339())
}

fn codex_usage_window(id: &str, value: &Value) -> Option<ProviderUsageWindow> {
    let used_percent = value.get("usedPercent")?.as_f64()?.clamp(0.0, 100.0);
    Some(ProviderUsageWindow {
        id: id.to_string(),
        used_percent,
        remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
        resets_at: value.get("resetsAt").and_then(codex_reset_time),
        window_duration_minutes: value.get("windowDurationMins").and_then(Value::as_u64),
        is_exhausted: used_percent >= 100.0,
    })
}

fn parse_codex_account_usage(value: &Value) -> Result<ProviderAccountUsage> {
    let limits = value.get("rateLimits").unwrap_or(value);
    let mut windows = Vec::new();
    for id in ["primary", "secondary"] {
        if let Some(window) = limits
            .get(id)
            .and_then(|value| codex_usage_window(id, value))
        {
            windows.push(window);
        }
    }
    let limit_reached = limits
        .get("rateLimitReachedType")
        .is_some_and(|value| !value.is_null());
    if limit_reached {
        for window in &mut windows {
            window.is_exhausted = true;
        }
    }

    if windows.is_empty() {
        return Err(CoreError::Provider(
            "codex account/rateLimits/read returned no usage windows".to_string(),
        ));
    }

    Ok(ProviderAccountUsage {
        provider_id: ProviderId("codex".to_string()),
        state: ProviderAccountUsageState::Available,
        windows,
        plan_type: limits
            .get("planType")
            .and_then(Value::as_str)
            .map(str::to_string),
        updated_at: Utc::now().to_rfc3339(),
        is_cached: false,
    })
}

async fn read_rpc_response_with_timeout<R>(
    lines: &mut Lines<R>,
    expected_id: u64,
    response_timeout: Duration,
) -> Result<Value>
where
    R: AsyncBufRead + Unpin,
{
    timeout(response_timeout, async {
        loop {
            let line = lines
                .next_line()
                .await
                .map_err(|error| CoreError::Provider(format!("codex stdout read: {error}")))?
                .ok_or_else(|| {
                    CoreError::Provider("codex app-server exited before responding".to_string())
                })?;
            let message = serde_json::from_str::<Incoming>(&line).map_err(|error| {
                CoreError::Provider(format!("invalid codex app-server response: {error}"))
            })?;
            if message.id.as_ref().and_then(Value::as_u64) != Some(expected_id) {
                continue;
            }
            if let Some(error) = message.error {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("rpc error");
                return Err(CoreError::Provider(format!(
                    "codex account usage error: {message}"
                )));
            }
            return message.result.ok_or_else(|| {
                CoreError::Provider("codex account usage response had no result".to_string())
            });
        }
    })
    .await
    .map_err(|_| CoreError::Provider("codex account usage request timed out".to_string()))?
}

async fn read_rpc_response<R>(lines: &mut Lines<R>, expected_id: u64) -> Result<Value>
where
    R: AsyncBufRead + Unpin,
{
    read_rpc_response_with_timeout(lines, expected_id, Duration::from_secs(15)).await
}

async fn fetch_codex_account_usage(
    runtime_config: Option<&ProviderRuntimeConfig>,
) -> Result<ProviderAccountUsage> {
    let mut command = Command::new("codex");
    command
        .arg("app-server")
        .arg("-c")
        .arg("notify=[]")
        .kill_on_drop(true)
        .env("PATH", augmented_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = runtime_config.and_then(|config| config.home_path.as_deref()) {
        command.env("CODEX_HOME", home);
    }

    let mut child = command.spawn().map_err(|error| {
        CoreError::Provider(format!("failed to start codex for account usage: {error}"))
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| CoreError::Provider("codex app-server missing stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CoreError::Provider("codex app-server missing stdout".to_string()))?;
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(_)) = lines.next_line().await {}
        }
    });
    let mut lines = BufReader::new(stdout).lines();

    let result = async {
        write_line(
            &mut stdin,
            &rpc_request(1, "initialize", initialize_params(false)),
        )
        .await?;
        read_rpc_response(&mut lines, 1).await?;
        write_line(&mut stdin, &rpc_notification("initialized")).await?;
        write_line(
            &mut stdin,
            &rpc_request(2, "account/rateLimits/read", Value::Null),
        )
        .await?;
        let response = read_rpc_response(&mut lines, 2).await?;
        parse_codex_account_usage(&response)
    }
    .await;

    drop(stdin);
    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(2), child.wait()).await;
    stderr_task.abort();
    let _ = stderr_task.await;
    result
}

// ── Notification → ProviderEvent ────────────────────────────────────────────

fn codex_agent_message_delta_content(
    params: &Value,
    last_agent_message_id: &mut Option<String>,
) -> String {
    let item_id = params.get("itemId").and_then(Value::as_str);
    let delta = params.get("delta").and_then(Value::as_str).unwrap_or("");
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

fn notification_to_event(
    method: &str,
    params: &Value,
    last_agent_message_id: &mut Option<String>,
) -> Option<ProviderEvent> {
    let at = Utc::now().to_rfc3339();
    match method {
        "item/agentMessage/delta" => Some(ProviderEvent::TextDelta {
            content: codex_agent_message_delta_content(params, last_agent_message_id),
        }),
        "turn/completed" => {
            *last_agent_message_id = None;
            let status = params
                .get("turn")
                .and_then(|t| t.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("completed");
            if status == "failed" {
                Some(ProviderEvent::Failed {
                    message: params
                        .get("turn")
                        .and_then(|t| t.get("error"))
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("turn failed")
                        .to_string(),
                    at,
                })
            } else {
                Some(ProviderEvent::Completed { at })
            }
        }
        "error" => {
            *last_agent_message_id = None;
            Some(ProviderEvent::Failed {
                message: codex_error_message(params),
                at,
            })
        }
        "item/started" => {
            let item = params.get("item")?;
            let kind = item.get("type").and_then(Value::as_str)?;
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("item")
                .to_string();
            match kind {
                "commandExecution" => Some(ProviderEvent::ToolCallStarted {
                    id,
                    action: "Bash".to_string(),
                    command: item
                        .get("command")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    file: None,
                    at,
                }),
                "file_change" | "fileChange" => Some(ProviderEvent::ToolCallStarted {
                    id,
                    action: "apply_patch".to_string(),
                    command: None,
                    file: item
                        .get("file_path")
                        .or_else(|| item.get("filePath"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    at,
                }),
                "reasoning" => Some(ProviderEvent::ReasoningStarted {
                    id,
                    label: Some("Thinking".to_string()),
                    at,
                }),
                "mcpToolCall" => Some(ProviderEvent::ToolCallStarted {
                    id,
                    action: item
                        .get("tool")
                        .and_then(Value::as_str)
                        .filter(|tool| !tool.is_empty())
                        .unwrap_or("MCP")
                        .chars()
                        .take(128)
                        .collect(),
                    command: None,
                    file: None,
                    at,
                }),
                _ => None,
            }
        }
        "item/completed" => {
            let item = params.get("item")?;
            let kind = item.get("type").and_then(Value::as_str)?;
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("item")
                .to_string();
            match kind {
                "mcpToolCall" => {
                    let failed = item
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|status| status == "failed");
                    if failed {
                        Some(ProviderEvent::ToolCallFailed {
                            id,
                            reason: Some("MCP tool call failed".to_string()),
                            at,
                        })
                    } else {
                        Some(ProviderEvent::ToolCallCompleted { id, at })
                    }
                }
                "commandExecution" | "file_change" | "fileChange" | "web_search" => {
                    let failed = item
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s == "failed");
                    if failed {
                        Some(ProviderEvent::ToolCallFailed {
                            id,
                            reason: item
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            at,
                        })
                    } else {
                        Some(ProviderEvent::ToolCallCompleted { id, at })
                    }
                }
                "reasoning" => Some(ProviderEvent::ReasoningCompleted { id, at }),
                _ => None,
            }
        }
        "item/reasoning/textDelta" | "item/reasoning/summaryTextDelta" => {
            Some(ProviderEvent::ReasoningDelta {
                id: params
                    .get("itemId")
                    .and_then(Value::as_str)
                    .unwrap_or("reasoning")
                    .to_string(),
                content: params
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        }
        _ => None,
    }
}

// ── Per-session runtime ─────────────────────────────────────────────────────

const MAX_PENDING_CODEX_MCP_APPROVALS: usize = 64;
const MAX_PENDING_CODEX_NATIVE_APPROVALS: usize = 64;
const MAX_ACTIVE_CODEX_MCP_TOOL_CALLS: usize = 128;
const MAX_CODEX_RPC_STRING_ID_CHARS: usize = 256;
const MAX_CODEX_MCP_ITEM_ID_CHARS: usize = 256;
const MAX_CODEX_MCP_TOOL_NAME_CHARS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveCodexMcpToolCall {
    item_id: String,
    turn_id: Option<String>,
    wire_server_name: String,
    tool_name: String,
    approval_claimed: bool,
}

#[derive(Debug)]
struct PendingCodexMcpApproval {
    rpc_id: Value,
    item_id: String,
}

#[derive(Clone, Debug, PartialEq)]
enum CodexNativeApprovalKind {
    Command,
    FileChange,
    Permissions { requested: Value },
}

#[derive(Debug)]
struct PendingCodexNativeApproval {
    rpc_id: Value,
    kind: CodexNativeApprovalKind,
}

fn bounded_nonempty_string(value: &Value, max_chars: usize) -> Option<String> {
    value
        .as_str()
        .filter(|value| {
            !value.is_empty()
                && value.chars().count() <= max_chars
                && !value.chars().any(char::is_control)
        })
        .map(str::to_string)
}

fn validated_codex_server_request_id(value: &Value) -> Option<Value> {
    match value {
        Value::String(_) => {
            bounded_nonempty_string(value, MAX_CODEX_RPC_STRING_ID_CHARS).map(Value::String)
        }
        Value::Number(number) if number.as_i64().is_some() || number.as_u64().is_some() => {
            Some(value.clone())
        }
        _ => None,
    }
}

fn update_active_codex_mcp_tool_calls(
    method: &str,
    params: &Value,
    active_calls: &mut HashMap<String, ActiveCodexMcpToolCall>,
) -> Option<String> {
    let item = params.get("item")?;
    if item.get("type").and_then(Value::as_str) != Some("mcpToolCall") {
        return None;
    }
    let item_id = bounded_nonempty_string(item.get("id")?, MAX_CODEX_MCP_ITEM_ID_CHARS)?;

    if method == "item/completed" {
        active_calls.remove(&item_id);
        return Some(item_id);
    }
    if method != "item/started"
        || active_calls.len() >= MAX_ACTIVE_CODEX_MCP_TOOL_CALLS
        || active_calls.contains_key(&item_id)
    {
        return None;
    }

    let wire_server_name =
        bounded_nonempty_string(item.get("server")?, MAX_CODEX_RPC_STRING_ID_CHARS)?;
    let tool_name = bounded_nonempty_string(item.get("tool")?, MAX_CODEX_MCP_TOOL_NAME_CHARS)?;
    let turn_id = params
        .get("turnId")
        .and_then(|value| bounded_nonempty_string(value, MAX_CODEX_RPC_STRING_ID_CHARS));
    active_calls.insert(
        item_id.clone(),
        ActiveCodexMcpToolCall {
            item_id,
            turn_id,
            wire_server_name,
            tool_name,
            approval_claimed: false,
        },
    );
    None
}

fn claim_codex_mcp_tool_approval(
    rpc_id: &Value,
    params: &Value,
    current_thread_id: Option<&str>,
    definitions: &CodexMcpDefinitionMap,
    active_calls: &mut HashMap<String, ActiveCodexMcpToolCall>,
) -> Option<(
    String,
    PendingCodexMcpApproval,
    ProviderPermissionRequest,
    dcc_core::domain::mcp::McpDefinitionId,
)> {
    let rpc_id = validated_codex_server_request_id(rpc_id)?;
    if params.get("mode").and_then(Value::as_str) != Some("form")
        || params
            .get("_meta")
            .and_then(Value::as_object)
            .and_then(|meta| meta.get("codex_approval_kind"))
            .and_then(Value::as_str)
            != Some("mcp_tool_call")
        || params
            .pointer("/requestedSchema/type")
            .and_then(Value::as_str)
            != Some("object")
        || !params
            .pointer("/requestedSchema/properties")
            .and_then(Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
        || params.get("threadId").and_then(Value::as_str) != current_thread_id
    {
        return None;
    }

    let wire_server_name =
        bounded_nonempty_string(params.get("serverName")?, MAX_CODEX_RPC_STRING_ID_CHARS)?;
    let definition_id = definitions.get(&wire_server_name)?.clone();
    let requested_turn_id = match params.get("turnId") {
        None | Some(Value::Null) => None,
        Some(value) => Some(bounded_nonempty_string(
            value,
            MAX_CODEX_RPC_STRING_ID_CHARS,
        )?),
    };

    let mut candidates = active_calls.values_mut().filter(|call| {
        !call.approval_claimed
            && call.wire_server_name == wire_server_name
            && requested_turn_id
                .as_ref()
                .is_none_or(|turn_id| call.turn_id.as_ref() == Some(turn_id))
    });
    let call = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }

    call.approval_claimed = true;
    let request_id = Uuid::new_v4().to_string();
    let pending = PendingCodexMcpApproval {
        rpc_id,
        item_id: call.item_id.clone(),
    };
    let request = ProviderPermissionRequest {
        request_id: request_id.clone(),
        tool_name: call.tool_name.clone(),
        title: Some("Approve MCP tool call".to_string()),
        description: Some(
            "Codex requested permission to run this tool through a DCC-managed MCP integration."
                .to_string(),
        ),
        command: None,
        file: None,
    };
    Some((request_id, pending, request, definition_id))
}

fn codex_mcp_elicitation_result(behavior: &str) -> Option<Value> {
    let action = match behavior {
        "allow" => "accept",
        "deny" => "decline",
        _ => return None,
    };
    Some(json!({
        "action": action,
        "content": null,
        "_meta": null,
    }))
}

fn codex_mcp_tool_policy(
    policies: &CodexMcpToolPolicyMap,
    definition_id: &dcc_core::domain::mcp::McpDefinitionId,
    tool_name: &str,
) -> McpToolPolicyDecision {
    policies
        .get(definition_id)
        .and_then(|tools| tools.get(tool_name))
        .cloned()
        .unwrap_or(McpToolPolicyDecision::Ask)
}

fn codex_turn_execution_policy(
    approval_policy: Option<ProviderApprovalPolicy>,
    plan_mode: Option<bool>,
    has_dcc_mcp_servers: bool,
) -> (Value, Value) {
    if plan_mode == Some(true) {
        let approval = if has_dcc_mcp_servers {
            json!(codex_mcp_approval_policy())
        } else {
            json!("never")
        };
        return (approval, json!({ "type": "readOnly" }));
    }

    let sandbox_approval = matches!(
        approval_policy,
        Some(ProviderApprovalPolicy::Ask | ProviderApprovalPolicy::Auto)
    );
    let rules = approval_policy == Some(ProviderApprovalPolicy::Ask);
    let approval = if has_dcc_mcp_servers {
        json!(codex_mcp_approval_policy_with_native(
            sandbox_approval,
            rules
        ))
    } else {
        json!(match approval_policy {
            Some(ProviderApprovalPolicy::Ask) => "untrusted",
            Some(ProviderApprovalPolicy::Auto) => "on-request",
            Some(ProviderApprovalPolicy::FullAccess) | None => "never",
        })
    };
    let sandbox = match approval_policy {
        Some(ProviderApprovalPolicy::Ask | ProviderApprovalPolicy::Auto) => {
            json!({ "type": "workspaceWrite" })
        }
        Some(ProviderApprovalPolicy::FullAccess) | None => {
            json!({ "type": "dangerFullAccess" })
        }
    };
    (approval, sandbox)
}

fn bounded_codex_approval_text(value: Option<&Value>, max_chars: usize) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty() && value.chars().count() <= max_chars && !value.contains('\0')
        })
        .map(str::to_string)
}

fn claim_codex_native_approval(
    rpc_id: &Value,
    method: &str,
    params: &Value,
    current_thread_id: Option<&str>,
    current_turn_id: Option<&str>,
) -> Option<(
    String,
    PendingCodexNativeApproval,
    ProviderPermissionRequest,
)> {
    let rpc_id = validated_codex_server_request_id(rpc_id)?;
    if params.get("threadId").and_then(Value::as_str) != current_thread_id
        || params.get("turnId").and_then(Value::as_str) != current_turn_id
    {
        return None;
    }
    bounded_nonempty_string(params.get("itemId")?, MAX_CODEX_MCP_ITEM_ID_CHARS)?;

    let (kind, tool_name, title, command, file) = match method {
        "item/commandExecution/requestApproval" => (
            CodexNativeApprovalKind::Command,
            "codex_command",
            "Approve command",
            bounded_codex_approval_text(params.get("command"), 16_384),
            bounded_codex_approval_text(params.get("cwd"), 4_096),
        ),
        "item/fileChange/requestApproval" => (
            CodexNativeApprovalKind::FileChange,
            "codex_file_change",
            "Approve file change",
            None,
            bounded_codex_approval_text(params.get("grantRoot"), 4_096),
        ),
        "item/permissions/requestApproval" => (
            CodexNativeApprovalKind::Permissions {
                requested: params.get("permissions")?.clone(),
            },
            "codex_permissions",
            "Approve additional access",
            None,
            bounded_codex_approval_text(params.get("cwd"), 4_096),
        ),
        _ => return None,
    };
    let request_id = Uuid::new_v4().to_string();
    Some((
        request_id.clone(),
        PendingCodexNativeApproval { rpc_id, kind },
        ProviderPermissionRequest {
            request_id,
            tool_name: tool_name.to_string(),
            title: Some(title.to_string()),
            description: bounded_codex_approval_text(params.get("reason"), 2_048),
            command,
            file,
        },
    ))
}

fn codex_native_approval_result(kind: CodexNativeApprovalKind, behavior: &str) -> Option<Value> {
    let decision = match behavior {
        "allow" => "accept",
        "deny" => "decline",
        _ => return None,
    };
    match kind {
        CodexNativeApprovalKind::Command | CodexNativeApprovalKind::FileChange => {
            Some(json!({ "decision": decision }))
        }
        CodexNativeApprovalKind::Permissions { requested } => Some(json!({
            "permissions": if behavior == "allow" { requested } else { json!({}) },
            "scope": "turn",
        })),
    }
}

type PendingResponse = oneshot::Sender<std::result::Result<Value, String>>;
type PendingMap = Arc<Mutex<HashMap<u64, PendingResponse>>>;

struct SessionRuntime {
    handle: SessionHandle,
    mcp_provider_version: Option<String>,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    thread_id: Mutex<Option<String>>,
    active_turn_id: Mutex<Option<String>>,
    mcp_definitions_by_wire_name: Mutex<CodexMcpDefinitionMap>,
    mcp_tool_policies: Mutex<CodexMcpToolPolicyMap>,
    mcp_status_snapshot: StdRwLock<Option<Vec<McpRuntimeStatus>>>,
    pending: PendingMap,
    pending_mcp_approvals: Mutex<HashMap<String, PendingCodexMcpApproval>>,
    pending_native_approvals: Mutex<HashMap<String, PendingCodexNativeApproval>>,
    next_id: AtomicU64,
    events_tx: broadcast::Sender<ProviderEvent>,
    last_retry_at: Mutex<Option<Instant>>,
}

impl SessionRuntime {
    fn mcp_provider_version(&self) -> Result<&str> {
        self.mcp_provider_version.as_deref().ok_or_else(|| {
            CoreError::Provider("Codex MCP bridge runtime version is unavailable".to_string())
        })
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        {
            let mut stdin = self.stdin.lock().await;
            write_line(&mut stdin, &rpc_request(id, method, params)).await?;
        }
        timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| CoreError::Provider(format!("codex {method} timed out")))?
            .map_err(|_| CoreError::Provider(format!("codex {method} cancelled")))?
            .map_err(|e| CoreError::Provider(format!("codex {method} error: {e}")))
    }

    async fn send_mcp_thread_start_request(
        &self,
        cwd: &str,
        additional_working_directories: &[String],
        servers: &[dcc_core::ports::ProviderMcpServerConfig],
    ) -> Result<Value> {
        let method = "thread/start";
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let prepared =
            prepare_thread_start_request(id, cwd, additional_working_directories, servers)?;
        let definitions = prepared.definitions_by_wire_name().clone();
        let tool_policies = prepared.tool_policies_by_definition().clone();
        *self.mcp_definitions_by_wire_name.lock().await = definitions.clone();
        *self.mcp_tool_policies.lock().await = tool_policies;
        let provider_version = self.mcp_provider_version()?;
        self.publish_mcp_status_snapshot(initial_codex_mcp_status_snapshot(
            &definitions,
            &self.handle.provider_id,
            provider_version,
            &self.handle.session_id,
        ));
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let write_result = {
            let mut stdin = self.stdin.lock().await;
            prepared.write_to(&mut *stdin).await
        };
        if let Err(error) = write_result {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        let result = timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| CoreError::Provider(format!("codex {method} timed out")))?
            .map_err(|_| CoreError::Provider(format!("codex {method} cancelled")))?
            .map_err(|_| {
                CoreError::Provider("Codex MCP thread configuration failed".to_string())
            })?;
        Ok(result)
    }

    async fn send_notification(&self, method: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        write_line(&mut stdin, &rpc_notification(method)).await
    }

    async fn send_server_result(&self, id: &Value, result: Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        write_line(&mut stdin, &rpc_response(id, result)).await
    }

    async fn resolve_mcp_approval(&self, response: ProviderPermissionResponse) -> Result<()> {
        let result = codex_mcp_elicitation_result(&response.behavior).ok_or_else(|| {
            CoreError::InvalidInput("Codex MCP permission behavior is invalid".to_string())
        })?;
        let pending = self
            .pending_mcp_approvals
            .lock()
            .await
            .remove(&response.request_id)
            .ok_or_else(|| {
                CoreError::InvalidInput("Codex MCP permission request is not pending".to_string())
            })?;

        self.send_server_result(&pending.rpc_id, result).await?;
        let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
            id: response.request_id,
            behavior: response.behavior,
            at: now_iso(),
        });
        Ok(())
    }

    async fn resolve_permission(&self, response: ProviderPermissionResponse) -> Result<()> {
        let native = self
            .pending_native_approvals
            .lock()
            .await
            .remove(&response.request_id);
        let Some(pending) = native else {
            return self.resolve_mcp_approval(response).await;
        };
        let result =
            codex_native_approval_result(pending.kind, &response.behavior).ok_or_else(|| {
                CoreError::InvalidInput("Codex permission behavior is invalid".to_string())
            })?;
        self.send_server_result(&pending.rpc_id, result).await?;
        let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
            id: response.request_id,
            behavior: response.behavior,
            at: now_iso(),
        });
        Ok(())
    }

    async fn resolve_cleared_mcp_approval(&self, rpc_id: &Value) {
        let cleared_id = {
            let mut pending = self.pending_mcp_approvals.lock().await;
            let request_id = pending.iter().find_map(|(request_id, approval)| {
                (&approval.rpc_id == rpc_id).then(|| request_id.clone())
            });
            request_id.and_then(|request_id| pending.remove(&request_id).map(|_| request_id))
        };
        if let Some(request_id) = cleared_id {
            let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
                id: request_id,
                behavior: "deny".to_string(),
                at: now_iso(),
            });
        }
    }

    async fn resolve_completed_mcp_approval(&self, item_id: &str) {
        let cleared_id = {
            let mut pending = self.pending_mcp_approvals.lock().await;
            let request_id = pending.iter().find_map(|(request_id, approval)| {
                (approval.item_id == item_id).then(|| request_id.clone())
            });
            request_id.and_then(|request_id| pending.remove(&request_id).map(|_| request_id))
        };
        if let Some(request_id) = cleared_id {
            let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
                id: request_id,
                behavior: "deny".to_string(),
                at: now_iso(),
            });
        }
    }

    async fn cancel_pending_mcp_approvals(&self) {
        let pending = self
            .pending_mcp_approvals
            .lock()
            .await
            .drain()
            .collect::<Vec<_>>();
        for (request_id, approval) in pending {
            let _ = self
                .send_server_result(
                    &approval.rpc_id,
                    json!({
                        "action": "cancel",
                        "content": null,
                        "_meta": null,
                    }),
                )
                .await;
            let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
                id: request_id,
                behavior: "deny".to_string(),
                at: now_iso(),
            });
        }
    }

    async fn cancel_pending_native_approvals(&self) {
        let pending = self
            .pending_native_approvals
            .lock()
            .await
            .drain()
            .collect::<Vec<_>>();
        for (request_id, approval) in pending {
            if let Some(result) = codex_native_approval_result(approval.kind, "deny") {
                let _ = self.send_server_result(&approval.rpc_id, result).await;
            }
            let _ = self.events_tx.send(ProviderEvent::PermissionResolved {
                id: request_id,
                behavior: "deny".to_string(),
                at: now_iso(),
            });
        }
    }

    async fn has_dcc_mcp_servers(&self) -> bool {
        !self.mcp_definitions_by_wire_name.lock().await.is_empty()
    }

    fn publish_mcp_status_snapshot(&self, statuses: Vec<McpRuntimeStatus>) {
        *self
            .mcp_status_snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(statuses.clone());
        let _ = self
            .events_tx
            .send(ProviderEvent::McpRuntimeStatusSnapshot { statuses });
    }

    fn merge_and_publish_mcp_status(&self, status: McpRuntimeStatus) {
        let statuses = {
            let mut snapshot = self
                .mcp_status_snapshot
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let statuses = snapshot.get_or_insert_with(Vec::new);
            merge_codex_mcp_status(statuses, status);
            statuses.clone()
        };
        let _ = self
            .events_tx
            .send(ProviderEvent::McpRuntimeStatusSnapshot { statuses });
    }

    fn latest_mcp_status_snapshot(&self) -> Option<Vec<McpRuntimeStatus>> {
        self.mcp_status_snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

async fn handle_codex_native_approval_request(
    runtime: &Arc<SessionRuntime>,
    rpc_id: &Value,
    method: &str,
    params: &Value,
) {
    let current_thread_id = runtime.thread_id.lock().await.clone();
    let current_turn_id = runtime.active_turn_id.lock().await.clone();
    let claimed = claim_codex_native_approval(
        rpc_id,
        method,
        params,
        current_thread_id.as_deref(),
        current_turn_id.as_deref(),
    );

    if let Some((request_id, pending, request)) = claimed {
        let inserted = {
            let mut approvals = runtime.pending_native_approvals.lock().await;
            if approvals.len() >= MAX_PENDING_CODEX_NATIVE_APPROVALS {
                false
            } else {
                approvals.insert(request_id.clone(), pending);
                true
            }
        };
        if inserted {
            let _ = runtime.events_tx.send(ProviderEvent::PermissionRequested {
                request,
                at: now_iso(),
            });
            return;
        }
    }

    if let Some(rpc_id) = validated_codex_server_request_id(rpc_id) {
        let kind = match method {
            "item/fileChange/requestApproval" => CodexNativeApprovalKind::FileChange,
            "item/permissions/requestApproval" => CodexNativeApprovalKind::Permissions {
                requested: params
                    .get("permissions")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            },
            _ => CodexNativeApprovalKind::Command,
        };
        if let Some(result) = codex_native_approval_result(kind, "deny") {
            let _ = runtime.send_server_result(&rpc_id, result).await;
        }
    }
}

async fn handle_codex_mcp_elicitation_request(
    runtime: &Arc<SessionRuntime>,
    rpc_id: &Value,
    params: &Value,
    active_calls: &mut HashMap<String, ActiveCodexMcpToolCall>,
) {
    let definitions = runtime.mcp_definitions_by_wire_name.lock().await.clone();
    let current_thread_id = runtime.thread_id.lock().await.clone();
    let claimed = claim_codex_mcp_tool_approval(
        rpc_id,
        params,
        current_thread_id.as_deref(),
        &definitions,
        active_calls,
    );

    if let Some((request_id, pending, request, definition_id)) = claimed {
        let decision = codex_mcp_tool_policy(
            &*runtime.mcp_tool_policies.lock().await,
            &definition_id,
            &request.tool_name,
        );
        if matches!(
            decision,
            McpToolPolicyDecision::Allow | McpToolPolicyDecision::Deny
        ) {
            let behavior = match decision {
                McpToolPolicyDecision::Allow => "allow",
                McpToolPolicyDecision::Deny => "deny",
                McpToolPolicyDecision::Ask => unreachable!(),
            };
            if runtime
                .send_server_result(
                    &pending.rpc_id,
                    codex_mcp_elicitation_result(behavior)
                        .expect("explicit tool policies map to provider results"),
                )
                .await
                .is_ok()
            {
                let _ = runtime.events_tx.send(ProviderEvent::PermissionRequested {
                    request,
                    at: now_iso(),
                });
                let _ = runtime.events_tx.send(ProviderEvent::PermissionResolved {
                    id: request_id,
                    behavior: behavior.to_string(),
                    at: now_iso(),
                });
            }
            return;
        }
        let inserted = {
            let mut pending_approvals = runtime.pending_mcp_approvals.lock().await;
            if pending_approvals.len() >= MAX_PENDING_CODEX_MCP_APPROVALS
                || pending_approvals.contains_key(&request_id)
            {
                false
            } else {
                pending_approvals.insert(request_id, pending);
                true
            }
        };
        if inserted {
            let _ = runtime.events_tx.send(ProviderEvent::PermissionRequested {
                request,
                at: now_iso(),
            });
            return;
        }
    }

    if let Some(rpc_id) = validated_codex_server_request_id(rpc_id) {
        let _ = runtime
            .send_server_result(
                &rpc_id,
                json!({
                    "action": "decline",
                    "content": null,
                    "_meta": null,
                }),
            )
            .await;
    }
}

const MAX_CODEX_MCP_STATUS_PAGES: usize = 8;
const MAX_CODEX_MCP_STATUS_ITEMS: usize = 512;
const MAX_CODEX_MCP_CURSOR_CHARS: usize = 1_024;
const MAX_CODEX_MCP_AUTHORIZATION_URL_CHARS: usize = 8_192;

async fn refresh_codex_mcp_statuses(runtime: &Arc<SessionRuntime>) {
    let definitions = runtime.mcp_definitions_by_wire_name.lock().await.clone();
    if definitions.is_empty() {
        return;
    }
    let provider_version = match runtime.mcp_provider_version() {
        Ok(provider_version) => provider_version,
        Err(_) => return,
    };
    let thread_id = runtime.thread_id.lock().await.clone();
    let result = match thread_id {
        Some(thread_id) => fetch_codex_mcp_status_snapshot(runtime, &thread_id, &definitions).await,
        None => Err(CoreError::Provider(
            "Codex MCP status requested without a thread".to_string(),
        )),
    };
    match result {
        Ok(mut statuses) => {
            if let Some(current) = runtime.latest_mcp_status_snapshot() {
                for current_status in current {
                    let fetched_is_still_attaching = statuses.iter().any(|status| {
                        status.definition_id == current_status.definition_id
                            && status.state == McpRuntimeState::AttachingProvider
                    });
                    if fetched_is_still_attaching
                        && current_status.state != McpRuntimeState::AttachingProvider
                    {
                        merge_codex_mcp_status(&mut statuses, current_status);
                    }
                }
            }
            runtime.publish_mcp_status_snapshot(statuses);
        }
        Err(_) => runtime.publish_mcp_status_snapshot(failed_codex_mcp_status_snapshot(
            &definitions,
            &runtime.handle.provider_id,
            provider_version,
            &runtime.handle.session_id,
            McpErrorCategory::Protocol,
            "Unable to read Codex MCP status",
        )),
    }
}

fn validate_codex_mcp_authorization_url(value: &Value) -> Result<String> {
    let raw = value
        .get("authorizationUrl")
        .and_then(Value::as_str)
        .filter(|value| {
            !value.trim().is_empty()
                && value.chars().count() <= MAX_CODEX_MCP_AUTHORIZATION_URL_CHARS
        })
        .ok_or_else(|| {
            CoreError::Provider("Codex returned an invalid MCP OAuth authorization URL".to_string())
        })?;
    let parsed = Url::parse(raw).map_err(|_| {
        CoreError::Provider("Codex returned an invalid MCP OAuth authorization URL".to_string())
    })?;
    let is_loopback_http = parsed.scheme() == "http"
        && parsed
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
    if (parsed.scheme() != "https" && !is_loopback_http)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(CoreError::Provider(
            "Codex returned an invalid MCP OAuth authorization URL".to_string(),
        ));
    }
    Ok(raw.to_string())
}

async fn fetch_codex_mcp_status_snapshot(
    runtime: &Arc<SessionRuntime>,
    thread_id: &str,
    definitions: &CodexMcpDefinitionMap,
) -> Result<Vec<McpRuntimeStatus>> {
    let mut data = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();
    let mut completed = false;

    for _ in 0..MAX_CODEX_MCP_STATUS_PAGES {
        let response = runtime
            .send_request(
                "mcpServerStatus/list",
                json!({
                    "cursor": cursor,
                    "detail": "toolsAndAuthOnly",
                    "limit": 100,
                    "threadId": thread_id,
                }),
            )
            .await?;
        let page = response
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| CoreError::Provider("invalid Codex MCP status payload".to_string()))?;
        if data.len().saturating_add(page.len()) > MAX_CODEX_MCP_STATUS_ITEMS {
            return Err(CoreError::Provider(
                "invalid Codex MCP status payload".to_string(),
            ));
        }
        data.extend(page.iter().cloned());

        let next_cursor = match response.get("nextCursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(value))
                if !value.is_empty() && value.chars().count() <= MAX_CODEX_MCP_CURSOR_CHARS =>
            {
                Some(value.clone())
            }
            _ => {
                return Err(CoreError::Provider(
                    "invalid Codex MCP status payload".to_string(),
                ));
            }
        };
        let Some(next_cursor) = next_cursor else {
            completed = true;
            break;
        };
        if !seen_cursors.insert(next_cursor.clone()) {
            return Err(CoreError::Provider(
                "invalid Codex MCP status payload".to_string(),
            ));
        }
        cursor = Some(next_cursor);
    }
    if !completed {
        return Err(CoreError::Provider(
            "invalid Codex MCP status payload".to_string(),
        ));
    }

    parse_codex_mcp_status_snapshot(
        &json!({ "data": data }),
        definitions,
        &runtime.handle.provider_id,
        runtime.mcp_provider_version()?,
        &runtime.handle.session_id,
    )
}

// ── Adapter runtime state ───────────────────────────────────────────────────

#[derive(Default)]
struct AdapterState {
    sessions: Mutex<HashMap<String, Arc<SessionRuntime>>>,
}

// ── Public adapter ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CodexAppServerAdapter {
    pub id: ProviderId,
    pub label: String,
    pub description: String,
    pub capabilities: Capabilities,
    pub stable: bool,
    mcp_projection: Option<CodexMcpProjection>,
    state: Arc<AdapterState>,
}

impl CodexAppServerAdapter {
    pub fn new(capabilities: Capabilities) -> Self {
        Self::with_mcp_projection(capabilities, detect_codex_mcp_projection())
    }

    fn with_mcp_projection(
        capabilities: Capabilities,
        mcp_projection: Option<CodexMcpProjection>,
    ) -> Self {
        Self {
            id: ProviderId("codex".to_string()),
            label: "Codex".to_string(),
            description: "OpenAI Codex provider via app-server protocol.".to_string(),
            capabilities,
            stable: true,
            mcp_projection,
            state: Arc::new(AdapterState::default()),
        }
    }

    async fn session_runtime(&self, id: &SessionId) -> Option<Arc<SessionRuntime>> {
        self.state.sessions.lock().await.get(&id.0).cloned()
    }

    async fn start_runtime(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        let mut cmd = Command::new("codex");
        cmd.arg("app-server");
        cmd.arg("-c");
        cmd.arg("notify=[]");
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        cmd.env("PATH", augmented_path());
        cmd.env("DCC_PROVIDER_ID", "codex");
        cmd.env("DCC_WORKSPACE_ID", &cfg.workspace_id.0);
        cmd.env("DCC_SESSION_ID", &cfg.session_id.0);
        if let Some(ref m) = cfg.model {
            cmd.env("DCC_MODEL", m);
        }
        if let Some(ref rc) = cfg.provider_runtime {
            if let Some(ref home) = rc.home_path {
                cmd.env("CODEX_HOME", home);
            }
        }
        if let Some(ref wd) = cfg.working_directory {
            if !wd.trim().is_empty() {
                cmd.current_dir(wd);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| CoreError::Provider(format!("failed to spawn codex app-server: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::Provider("codex app-server missing stderr".to_string()))?;

        let handle = SessionHandle {
            provider_id: self.id.clone(),
            session_id: cfg.session_id.clone(),
            handle_id: Uuid::new_v4().to_string(),
        };

        let (events_tx, _) = broadcast::channel(128);
        let runtime = Arc::new(SessionRuntime {
            handle: handle.clone(),
            mcp_provider_version: self
                .mcp_projection
                .as_ref()
                .map(|projection| projection.runtime_version.clone()),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            thread_id: Mutex::new(None),
            active_turn_id: Mutex::new(None),
            mcp_definitions_by_wire_name: Mutex::new(HashMap::new()),
            mcp_tool_policies: Mutex::new(HashMap::new()),
            mcp_status_snapshot: StdRwLock::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            pending_mcp_approvals: Mutex::new(HashMap::new()),
            pending_native_approvals: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            events_tx: events_tx.clone(),
            last_retry_at: Mutex::new(None),
        });

        let session_key = cfg.session_id.0.clone();
        self.state
            .sessions
            .lock()
            .await
            .insert(session_key.clone(), runtime.clone());

        // Start background reader before initializing (needs to be live to route responses)
        Self::spawn_reader(
            runtime.clone(),
            stdout,
            stderr,
            Arc::clone(&self.state),
            session_key.clone(),
        );

        // Handshake + thread/start
        if let Err(e) = Self::handshake(&runtime, &cfg, self.mcp_projection.as_ref()).await {
            self.state.sessions.lock().await.remove(&session_key);
            let _ = runtime.child.lock().await.start_kill();
            return Err(e);
        }

        Ok(handle)
    }

    async fn handshake(
        runtime: &Arc<SessionRuntime>,
        cfg: &SessionConfig,
        mcp_projection: Option<&CodexMcpProjection>,
    ) -> Result<()> {
        // initialize
        let uses_experimental_api =
            !cfg.additional_working_directories.is_empty() || !cfg.mcp_servers.is_empty();
        let initialize_result = runtime
            .send_request("initialize", initialize_params(uses_experimental_api))
            .await?;
        if !cfg.mcp_servers.is_empty() {
            validate_codex_mcp_projection(mcp_projection, &initialize_result)?;
        }

        // initialized notification (no response expected)
        runtime.send_notification("initialized").await?;

        // thread/start
        let cwd = cfg
            .working_directory
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(".");
        let result = if cfg.mcp_servers.is_empty() {
            runtime
                .send_request(
                    "thread/start",
                    thread_start_params(cwd, &cfg.additional_working_directories),
                )
                .await?
        } else {
            runtime
                .send_mcp_thread_start_request(
                    cwd,
                    &cfg.additional_working_directories,
                    &cfg.mcp_servers,
                )
                .await?
        };

        let thread_id = result
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Provider("codex thread/start missing thread.id".to_string()))?
            .to_string();

        *runtime.thread_id.lock().await = Some(thread_id);
        if !cfg.mcp_servers.is_empty() {
            refresh_codex_mcp_statuses(runtime).await;
        }
        Ok(())
    }

    fn spawn_reader(
        runtime: Arc<SessionRuntime>,
        stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr,
        state: Arc<AdapterState>,
        session_key: String,
    ) {
        tokio::spawn(async move {
            let _ = runtime
                .events_tx
                .send(ProviderEvent::Started { at: now_iso() });

            let retry_runtime = runtime.clone();
            let stderr_task = tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let trimmed = line.trim();
                    if is_codex_reconnect_notice(trimmed) {
                        *retry_runtime.last_retry_at.lock().await = Some(Instant::now());
                    }
                }
            });

            let mut reader = BufReader::new(stdout).lines();
            let mut last_agent_message_id = None;
            let mut active_mcp_tool_calls = HashMap::new();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }

                let Ok(msg) = serde_json::from_str::<Incoming>(&trimmed) else {
                    continue;
                };

                // Response: has numeric id + result/error
                if let Some(id_val) = &msg.id {
                    if let Some(id) = id_val.as_u64() {
                        if msg.result.is_some() || msg.error.is_some() {
                            let mut pending = runtime.pending.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                if let Some(r) = msg.result {
                                    let _ = tx.send(Ok(r));
                                } else if let Some(e) = msg.error {
                                    let msg = e
                                        .get("message")
                                        .and_then(Value::as_str)
                                        .unwrap_or("rpc error")
                                        .to_string();
                                    let _ = tx.send(Err(msg));
                                }
                            }
                            continue;
                        }
                    }
                }

                // Server-initiated MCP request: DCC handles only the exact
                // audited tool-approval elicitation shape.
                if let (Some(id), Some(method), Some(params)) = (&msg.id, &msg.method, &msg.params)
                {
                    if matches!(
                        method.as_str(),
                        "item/commandExecution/requestApproval"
                            | "item/fileChange/requestApproval"
                            | "item/permissions/requestApproval"
                    ) {
                        handle_codex_native_approval_request(&runtime, id, method, params).await;
                        continue;
                    }
                    if method == "mcpServer/elicitation/request" {
                        handle_codex_mcp_elicitation_request(
                            &runtime,
                            id,
                            params,
                            &mut active_mcp_tool_calls,
                        )
                        .await;
                        continue;
                    }
                }

                // Notification: has method + params and no response payload.
                if let (Some(method), Some(params)) = (&msg.method, &msg.params) {
                    if method == "turn/completed" || method == "error" {
                        *runtime.active_turn_id.lock().await = None;
                    }
                    if let Some(completed_item_id) = update_active_codex_mcp_tool_calls(
                        method,
                        params,
                        &mut active_mcp_tool_calls,
                    ) {
                        runtime
                            .resolve_completed_mcp_approval(&completed_item_id)
                            .await;
                    }
                    if method == "serverRequest/resolved" {
                        if let Some(request_id) = params.get("requestId") {
                            runtime.resolve_cleared_mcp_approval(request_id).await;
                        }
                        continue;
                    }
                    if method == "mcpServer/startupStatus/updated" {
                        let definitions = runtime.mcp_definitions_by_wire_name.lock().await.clone();
                        let thread_id = runtime.thread_id.lock().await.clone();
                        let provider_version = match runtime.mcp_provider_version() {
                            Ok(provider_version) => provider_version,
                            Err(_) => continue,
                        };
                        match parse_codex_mcp_startup_status(
                            params,
                            &definitions,
                            &runtime.handle.provider_id,
                            provider_version,
                            &runtime.handle.session_id,
                            thread_id.as_deref(),
                        ) {
                            Some(Ok(status)) => {
                                let should_refresh = status.state == McpRuntimeState::Connected
                                    && thread_id.is_some();
                                runtime.merge_and_publish_mcp_status(status);
                                if should_refresh {
                                    let refresh_runtime = runtime.clone();
                                    tokio::spawn(async move {
                                        refresh_codex_mcp_statuses(&refresh_runtime).await;
                                    });
                                }
                            }
                            Some(Err(_)) => {
                                runtime.publish_mcp_status_snapshot(
                                    failed_codex_mcp_status_snapshot(
                                        &definitions,
                                        &runtime.handle.provider_id,
                                        provider_version,
                                        &runtime.handle.session_id,
                                        McpErrorCategory::Protocol,
                                        "Invalid Codex MCP startup status",
                                    ),
                                );
                            }
                            None => {}
                        }
                        continue;
                    }
                    if method == "mcpServer/oauthLogin/completed" {
                        let name = params.get("name").and_then(Value::as_str);
                        let notified_thread_id = params.get("threadId").and_then(Value::as_str);
                        let thread_id = runtime.thread_id.lock().await.clone();
                        let definitions = runtime.mcp_definitions_by_wire_name.lock().await.clone();
                        let belongs_to_runtime =
                            name.is_some_and(|name| definitions.contains_key(name));
                        let belongs_to_thread = match (notified_thread_id, thread_id.as_deref()) {
                            (Some(notified), Some(active)) => notified == active,
                            (None, Some(_)) => true,
                            _ => false,
                        };
                        if belongs_to_runtime && belongs_to_thread {
                            let refresh_runtime = runtime.clone();
                            tokio::spawn(async move {
                                refresh_codex_mcp_statuses(&refresh_runtime).await;
                            });
                        }
                        continue;
                    }
                    if method == "error" && should_suppress_codex_error(params, &runtime).await {
                        continue;
                    }
                    if let Some(event) =
                        notification_to_event(method, params, &mut last_agent_message_id)
                    {
                        let _ = runtime.events_tx.send(event);
                    }
                }
            }

            // Process exited
            let at = now_iso();
            let exit = {
                let mut child = runtime.child.lock().await;
                child.wait().await
            };

            // Drain pending requests
            for (_, tx) in runtime.pending.lock().await.drain() {
                let _ = tx.send(Err("codex process exited".to_string()));
            }
            for (request_id, _) in runtime.pending_mcp_approvals.lock().await.drain() {
                let _ = runtime.events_tx.send(ProviderEvent::PermissionResolved {
                    id: request_id,
                    behavior: "deny".to_string(),
                    at: now_iso(),
                });
            }
            for (request_id, _) in runtime.pending_native_approvals.lock().await.drain() {
                let _ = runtime.events_tx.send(ProviderEvent::PermissionResolved {
                    id: request_id,
                    behavior: "deny".to_string(),
                    at: now_iso(),
                });
            }

            match exit {
                Ok(status) if status.success() => {
                    let _ = runtime.events_tx.send(ProviderEvent::Completed { at });
                }
                Ok(status) => {
                    let _ = runtime.events_tx.send(ProviderEvent::Failed {
                        message: format!("codex exited with status {status}"),
                        at,
                    });
                }
                Err(e) => {
                    let _ = runtime.events_tx.send(ProviderEvent::Failed {
                        message: format!("codex wait error: {e}"),
                        at,
                    });
                }
            }

            let _ = stderr_task.await;
            state.sessions.lock().await.remove(&session_key);
        });
    }
}

fn codex_error_message(params: &Value) -> String {
    params
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| params.get("message").and_then(Value::as_str))
        .unwrap_or("codex app-server error")
        .to_string()
}

fn is_codex_reconnect_notice(message: &str) -> bool {
    let trimmed = message.trim_start();
    let suffix = if let Some(rest) = trimmed.strip_prefix("Reconnecting...") {
        Some(rest.trim_start())
    } else if let Some(rest) = trimmed.strip_prefix("Reconnecting…") {
        Some(rest.trim_start())
    } else {
        None
    };

    match suffix {
        None => false,
        Some("") => true,
        Some(rest) => {
            let mut parts = rest.split('/');
            let left = parts.next().map(str::trim);
            let right = parts.next().map(str::trim);
            left.is_some_and(|value| value.chars().all(|ch| ch.is_ascii_digit()))
                && right.is_some_and(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        }
    }
}

async fn should_suppress_codex_error(params: &Value, runtime: &SessionRuntime) -> bool {
    if params
        .get("willRetry")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }

    let message = codex_error_message(params);
    if !is_codex_reconnect_notice(&message) {
        return false;
    }

    runtime
        .last_retry_at
        .lock()
        .await
        .as_ref()
        .is_some_and(|at| at.elapsed() <= Duration::from_secs(30))
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

// ── Provider trait ──────────────────────────────────────────────────────────

#[async_trait]
impl Provider for CodexAppServerAdapter {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    fn dcc_mcp_projection_version(&self) -> Option<&str> {
        self.mcp_projection
            .as_ref()
            .map(|projection| projection.runtime_version.as_str())
    }

    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle> {
        self.start_runtime(cfg).await
    }

    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()> {
        let runtime = self
            .session_runtime(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no codex runtime for session {}",
                    handle.session_id.0
                ))
            })?;

        if let Input::PermissionResponse(response) = input {
            return runtime.resolve_permission(response).await;
        }

        let thread_id = runtime
            .thread_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| CoreError::Provider("codex session has no thread ID".to_string()))?;

        let has_dcc_mcp_servers = runtime.has_dcc_mcp_servers().await;
        let (prompt, effort, summary, approval_policy, sandbox_policy) = match input {
            Input::Text(text) => (
                text,
                None,
                None,
                json!("never"),
                json!({ "type": "dangerFullAccess" }),
            ),
            Input::Turn(turn) => {
                let (approval_policy, sandbox_policy) = codex_turn_execution_policy(
                    turn.approval_policy,
                    turn.plan_mode,
                    has_dcc_mcp_servers,
                );
                (
                    append_tool_instructions(
                        compose_fallback_prompt_for_provider(
                            "codex",
                            &turn.prompt,
                            turn.plan_mode,
                            turn.effort.as_deref(),
                            turn.fast_mode,
                            PromptInjectionOptions {
                                plan: true,
                                effort: false,
                                fast: true,
                            },
                        ),
                        turn.tool_instructions.as_deref(),
                    ),
                    codex_reasoning_effort(turn.effort.as_deref()),
                    if turn.fast_mode.unwrap_or(false) {
                        Some("concise")
                    } else {
                        Some("auto")
                    },
                    approval_policy,
                    sandbox_policy,
                )
            }
            Input::UserInputResponse(_) => {
                return Err(CoreError::Provider(
                    "Codex does not support mid-turn user input responses".to_string(),
                ));
            }
            Input::PermissionResponse(_) => unreachable!("handled before starting a turn"),
        };
        let result = runtime
            .send_request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{ "type": "text", "text": prompt }],
                    "effort": effort,
                    "approvalPolicy": approval_policy,
                    "sandboxPolicy": sandbox_policy,
                    "summary": summary,
                }),
            )
            .await?;

        let turn_id = result
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Provider("codex turn/start missing turn.id".to_string()))?
            .to_string();
        *runtime.active_turn_id.lock().await = Some(turn_id);

        Ok(())
    }

    async fn steer(&self, handle: &SessionHandle, prompt: &str) -> Result<()> {
        let runtime = self
            .session_runtime(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no codex runtime for session {}",
                    handle.session_id.0
                ))
            })?;
        let thread_id = runtime
            .thread_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| CoreError::Provider("codex session has no thread ID".to_string()))?;
        let expected_turn_id =
            runtime.active_turn_id.lock().await.clone().ok_or_else(|| {
                CoreError::Provider("codex session has no active turn".to_string())
            })?;

        runtime
            .send_request(
                "turn/steer",
                json!({
                    "threadId": thread_id,
                    "input": [{ "type": "text", "text": prompt }],
                    "expectedTurnId": expected_turn_id,
                }),
            )
            .await?;
        Ok(())
    }

    async fn start_mcp_oauth(
        &self,
        handle: &SessionHandle,
        definition_id: &McpDefinitionId,
    ) -> Result<ProviderMcpOauthStart> {
        let runtime = self
            .session_runtime(&handle.session_id)
            .await
            .ok_or_else(|| CoreError::Provider("Codex MCP runtime is unavailable".to_string()))?;
        let thread_id =
            runtime.thread_id.lock().await.clone().ok_or_else(|| {
                CoreError::Provider("Codex MCP runtime is unavailable".to_string())
            })?;
        let definitions = runtime.mcp_definitions_by_wire_name.lock().await;
        let wire_name = definitions
            .iter()
            .find_map(|(wire_name, candidate)| {
                (candidate == definition_id).then(|| wire_name.clone())
            })
            .ok_or_else(|| {
                CoreError::Provider(
                    "MCP integration is not attached to this Codex session".to_string(),
                )
            })?;
        drop(definitions);

        let response = runtime
            .send_request(
                "mcpServer/oauth/login",
                json!({
                    "name": wire_name,
                    "scopes": null,
                    "threadId": thread_id,
                    "timeoutSecs": 300,
                }),
            )
            .await
            .map_err(|_| CoreError::Provider("MCP OAuth login could not start".to_string()))?;
        Ok(ProviderMcpOauthStart {
            authorization_url: validate_codex_mcp_authorization_url(&response)?,
        })
    }

    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
        let runtime = self
            .state
            .sessions
            .try_lock()
            .ok()
            .and_then(|s| s.get(&handle.session_id.0).cloned());

        let Some(runtime) = runtime else {
            return Box::pin(stream::empty());
        };

        let rx = runtime.events_tx.subscribe();
        let initial = runtime
            .latest_mcp_status_snapshot()
            .map(|statuses| Ok(ProviderEvent::McpRuntimeStatusSnapshot { statuses }));
        let live = stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(event) => return Some((Ok(event), rx)),
                    Err(broadcast::error::RecvError::Closed) => return None,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });
        Box::pin(stream::iter(initial).chain(live))
    }

    async fn cancel(&self, handle: &SessionHandle) -> Result<()> {
        let runtime = self
            .session_runtime(&handle.session_id)
            .await
            .ok_or_else(|| {
                CoreError::Provider(format!(
                    "no codex runtime for session {}",
                    handle.session_id.0
                ))
            })?;

        runtime.cancel_pending_mcp_approvals().await;
        runtime.cancel_pending_native_approvals().await;
        let mut child = runtime.child.lock().await;
        child
            .kill()
            .await
            .map_err(|e| CoreError::Provider(format!("failed to kill codex: {e}")))?;
        Ok(())
    }

    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
        let runtime = self.session_runtime(previous).await.ok_or_else(|| {
            CoreError::Provider(format!(
                "no resumable codex runtime for session {}",
                previous.0
            ))
        })?;
        Ok(runtime.handle.clone())
    }

    async fn healthcheck(&self) -> Result<HealthStatus> {
        let mut command = Command::new("codex");
        command.arg("--version");
        command.env("PATH", augmented_path());
        match command.output().await {
            Ok(output) if output.status.success() => Ok(HealthStatus::Healthy),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let reason = if !stderr.is_empty() { stderr } else { stdout };
                Ok(HealthStatus::Degraded { reason })
            }
            Err(e) => Ok(HealthStatus::Unhealthy {
                reason: format!("failed to execute codex: {e}"),
            }),
        }
    }

    async fn account_usage(
        &self,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<Option<ProviderAccountUsage>> {
        fetch_codex_account_usage(runtime).await.map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_mcp_projection_identity_from_any_well_formed_codex_version() {
        assert_eq!(
            parse_codex_cli_version_output("codex-cli 0.145.0\n"),
            Some("0.145.0")
        );
        assert_eq!(
            CodexMcpProjection::from_cli_output("codex-cli 0.145.0\n"),
            Some(CodexMcpProjection {
                cli_version: "0.145.0".to_string(),
                runtime_version: codex_mcp_runtime_version("0.145.0"),
            })
        );
        assert_eq!(
            CodexMcpProjection::from_cli_output("codex-cli 0.146.0\n"),
            Some(CodexMcpProjection {
                cli_version: "0.146.0".to_string(),
                runtime_version: codex_mcp_runtime_version("0.146.0"),
            })
        );
        assert_eq!(CodexMcpProjection::from_cli_output("codex 0.145.0\n"), None);
        assert_eq!(
            CodexMcpProjection::from_cli_output(&format!("codex-cli {}\n", "1".repeat(65))),
            None
        );

        let adapter = CodexAppServerAdapter::with_mcp_projection(
            crate::codex::stable_codex_capabilities(),
            CodexMcpProjection::from_cli_output("codex-cli 0.146.0\n"),
        );
        assert_eq!(
            adapter.dcc_mcp_projection_version(),
            Some("codex-cli@0.146.0+app-server-protocol-v2")
        );
    }

    #[test]
    fn validates_the_initialized_app_server_version() {
        assert_eq!(
            initialize_result_codex_version(&json!({
                "userAgent": "dcc/0.145.0 (macOS 15.5; arm64)"
            })),
            Some("0.145.0")
        );
        assert_eq!(
            initialize_result_codex_version(&json!({
                "userAgent": "other/0.145.0 (macOS 15.5; arm64)"
            })),
            None
        );

        let projection = CodexMcpProjection::from_cli_output("codex-cli 0.146.0\n");
        assert!(validate_codex_mcp_projection(
            projection.as_ref(),
            &json!({ "userAgent": "dcc/0.146.0 (macOS 26.5; arm64)" }),
        )
        .is_ok());
        assert!(validate_codex_mcp_projection(
            projection.as_ref(),
            &json!({ "userAgent": "dcc/0.147.0 (macOS 26.5; arm64)" }),
        )
        .expect_err("runtime replacement must require a fresh negotiation")
        .to_string()
        .contains("detected 0.146.0, app-server reported 0.147.0"));
        assert!(validate_codex_mcp_projection(
            projection.as_ref(),
            &json!({ "userAgent": "malformed" }),
        )
        .is_err());
    }

    #[test]
    fn validates_mcp_oauth_authorization_urls_before_exposing_them() {
        assert_eq!(
            validate_codex_mcp_authorization_url(
                &json!({ "authorizationUrl": "https://auth.example.test/authorize?state=test" })
            )
            .expect("https authorization URL should be accepted"),
            "https://auth.example.test/authorize?state=test"
        );
        assert!(validate_codex_mcp_authorization_url(
            &json!({ "authorizationUrl": "http://127.0.0.1:43123/callback" })
        )
        .is_ok());
        assert!(validate_codex_mcp_authorization_url(
            &json!({ "authorizationUrl": "http://example.com/oauth" })
        )
        .is_err());
        assert!(validate_codex_mcp_authorization_url(
            &json!({ "authorizationUrl": "https://user:secret@example.com/oauth" })
        )
        .is_err());
    }

    #[test]
    fn builds_experimental_runtime_roots_and_mcp_only_approval_policy() {
        let initialize = initialize_params(true);
        assert_eq!(
            initialize
                .pointer("/capabilities/experimentalApi")
                .and_then(Value::as_bool),
            Some(true)
        );

        let thread = thread_start_params(
            "/tmp/app",
            &["/tmp/api".to_string(), "/tmp/shared".to_string()],
        );
        assert_eq!(
            thread
                .get("runtimeWorkspaceRoots")
                .and_then(Value::as_array)
                .expect("runtime roots should be sent"),
            &vec![json!("/tmp/app"), json!("/tmp/api"), json!("/tmp/shared")]
        );

        let stable_initialize = initialize_params(false);
        assert!(stable_initialize
            .pointer("/capabilities/experimentalApi")
            .is_none());
        let single_thread = thread_start_params("/tmp/app", &[]);
        assert!(single_thread.get("runtimeWorkspaceRoots").is_none());

        assert_eq!(
            serde_json::to_value(codex_mcp_approval_policy()).expect("serialize policy"),
            json!({
                "granular": {
                    "sandbox_approval": false,
                    "rules": false,
                    "skill_approval": false,
                    "request_permissions": false,
                    "mcp_elicitations": true
                }
            })
        );
    }

    #[test]
    fn maps_normalized_approval_policies_to_codex_execution_boundaries() {
        assert_eq!(
            codex_turn_execution_policy(Some(ProviderApprovalPolicy::Ask), Some(false), false),
            (json!("untrusted"), json!({ "type": "workspaceWrite" }))
        );
        assert_eq!(
            codex_turn_execution_policy(Some(ProviderApprovalPolicy::Auto), Some(false), false),
            (json!("on-request"), json!({ "type": "workspaceWrite" }))
        );
        assert_eq!(
            codex_turn_execution_policy(
                Some(ProviderApprovalPolicy::FullAccess),
                Some(false),
                false,
            ),
            (json!("never"), json!({ "type": "dangerFullAccess" }))
        );
        assert_eq!(
            codex_turn_execution_policy(
                Some(ProviderApprovalPolicy::FullAccess),
                Some(true),
                false,
            ),
            (json!("never"), json!({ "type": "readOnly" }))
        );

        let (mcp_approval, sandbox) =
            codex_turn_execution_policy(Some(ProviderApprovalPolicy::Auto), Some(false), true);
        assert_eq!(sandbox, json!({ "type": "workspaceWrite" }));
        assert_eq!(
            mcp_approval.pointer("/granular/mcp_elicitations"),
            Some(&json!(true))
        );
        assert_eq!(
            mcp_approval.pointer("/granular/sandbox_approval"),
            Some(&json!(true))
        );
        assert_eq!(
            mcp_approval.pointer("/granular/request_permissions"),
            Some(&json!(true))
        );
        assert_eq!(mcp_approval.pointer("/granular/rules"), Some(&json!(false)));

        let (ask_mcp_approval, _) =
            codex_turn_execution_policy(Some(ProviderApprovalPolicy::Ask), Some(false), true);
        assert_eq!(
            ask_mcp_approval.pointer("/granular/rules"),
            Some(&json!(true))
        );
    }

    #[test]
    fn claims_only_native_approval_requests_for_the_active_codex_turn() {
        let claimed = claim_codex_native_approval(
            &json!(41),
            "item/commandExecution/requestApproval",
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "command": "pnpm test",
                "cwd": "/tmp/app",
                "reason": "Run tests"
            }),
            Some("thread-1"),
            Some("turn-1"),
        )
        .expect("active turn request should be claimed");
        assert_eq!(claimed.1.rpc_id, json!(41));
        assert_eq!(claimed.2.command.as_deref(), Some("pnpm test"));
        assert_eq!(claimed.2.file.as_deref(), Some("/tmp/app"));
        assert!(claim_codex_native_approval(
            &json!(42),
            "item/commandExecution/requestApproval",
            &json!({
                "threadId": "another-thread",
                "turnId": "turn-1",
                "itemId": "item-2"
            }),
            Some("thread-1"),
            Some("turn-1"),
        )
        .is_none());
    }

    #[test]
    fn separates_codex_agent_message_items() {
        let mut last_agent_message_id = None;

        let first = notification_to_event(
            "item/agentMessage/delta",
            &json!({
                "itemId": "msg_1",
                "delta": "Primeira mensagem.",
            }),
            &mut last_agent_message_id,
        );
        match first {
            Some(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "Primeira mensagem.");
            }
            other => panic!("expected first text delta, got {other:?}"),
        }

        let second = notification_to_event(
            "item/agentMessage/delta",
            &json!({
                "itemId": "msg_2",
                "delta": "Segunda mensagem.",
            }),
            &mut last_agent_message_id,
        );
        match second {
            Some(ProviderEvent::TextDelta { content }) => {
                assert_eq!(content, "\n\nSegunda mensagem.");
            }
            other => panic!("expected separated text delta, got {other:?}"),
        }
    }

    #[test]
    fn normalizes_schema_backed_mcp_tool_lifecycle_without_payloads() {
        let mut last_agent_message_id = None;
        let started = notification_to_event(
            "item/started",
            &json!({
                "item": {
                    "id": "mcp-call-1",
                    "type": "mcpToolCall",
                    "server": "dcc-session-0",
                    "tool": "fixture.echo",
                    "arguments": { "secret": "must-not-cross" },
                    "status": "inProgress"
                }
            }),
            &mut last_agent_message_id,
        );
        match started {
            Some(ProviderEvent::ToolCallStarted {
                id,
                action,
                command,
                file,
                ..
            }) => {
                assert_eq!(id, "mcp-call-1");
                assert_eq!(action, "fixture.echo");
                assert_eq!(command, None);
                assert_eq!(file, None);
            }
            other => panic!("expected MCP tool start, got {other:?}"),
        }

        let failed = notification_to_event(
            "item/completed",
            &json!({
                "item": {
                    "id": "mcp-call-1",
                    "type": "mcpToolCall",
                    "server": "dcc-session-0",
                    "tool": "fixture.echo",
                    "arguments": { "secret": "must-not-cross" },
                    "status": "failed",
                    "error": { "message": "secret-bearing provider error" }
                }
            }),
            &mut last_agent_message_id,
        );
        match failed {
            Some(ProviderEvent::ToolCallFailed { id, reason, .. }) => {
                assert_eq!(id, "mcp-call-1");
                assert_eq!(reason.as_deref(), Some("MCP tool call failed"));
            }
            other => panic!("expected MCP tool failure, got {other:?}"),
        }
    }

    #[test]
    fn correlates_only_owned_unambiguous_mcp_tool_approvals() {
        let mut active = HashMap::new();
        assert_eq!(
            update_active_codex_mcp_tool_calls(
                "item/started",
                &json!({
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "item": {
                        "id": "mcp-call-1",
                        "type": "mcpToolCall",
                        "server": "dcc-session-0",
                        "tool": "fixture.mutate",
                        "arguments": { "secret": "must-not-cross" }
                    }
                }),
                &mut active,
            ),
            None
        );
        let definitions = HashMap::from([(
            "dcc-session-0".to_string(),
            dcc_core::domain::mcp::McpDefinitionId("fixture".to_string()),
        )]);
        let (request_id, pending, request, definition_id) = claim_codex_mcp_tool_approval(
            &json!(41),
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "serverName": "dcc-session-0",
                "mode": "form",
                "message": "provider-controlled text",
                "requestedSchema": { "type": "object", "properties": {} },
                "_meta": {
                    "codex_approval_kind": "mcp_tool_call",
                    "tool_params": { "secret": "must-not-cross" }
                }
            }),
            Some("thread-1"),
            &definitions,
            &mut active,
        )
        .expect("owned call should correlate");

        assert!(Uuid::parse_str(&request_id).is_ok());
        assert_eq!(pending.rpc_id, json!(41));
        assert_eq!(pending.item_id, "mcp-call-1");
        assert_eq!(request.tool_name, "fixture.mutate");
        assert_eq!(definition_id.0, "fixture");
        assert!(!request
            .description
            .as_deref()
            .unwrap_or_default()
            .contains("must-not-cross"));
        assert!(active["mcp-call-1"].approval_claimed);
        assert!(claim_codex_mcp_tool_approval(
            &json!(42),
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "serverName": "dcc-session-0",
                "mode": "form",
                "requestedSchema": { "type": "object", "properties": {} },
                "_meta": { "codex_approval_kind": "mcp_tool_call" }
            }),
            Some("thread-1"),
            &definitions,
            &mut active,
        )
        .is_none());
    }

    #[test]
    fn declines_unowned_malformed_or_ambiguous_mcp_approvals() {
        let definitions = HashMap::from([(
            "dcc-session-0".to_string(),
            dcc_core::domain::mcp::McpDefinitionId("fixture".to_string()),
        )]);
        let mut active = HashMap::from([
            (
                "call-1".to_string(),
                ActiveCodexMcpToolCall {
                    item_id: "call-1".to_string(),
                    turn_id: Some("turn-1".to_string()),
                    wire_server_name: "dcc-session-0".to_string(),
                    tool_name: "fixture.one".to_string(),
                    approval_claimed: false,
                },
            ),
            (
                "call-2".to_string(),
                ActiveCodexMcpToolCall {
                    item_id: "call-2".to_string(),
                    turn_id: Some("turn-1".to_string()),
                    wire_server_name: "dcc-session-0".to_string(),
                    tool_name: "fixture.two".to_string(),
                    approval_claimed: false,
                },
            ),
        ]);
        let approval = json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "serverName": "dcc-session-0",
            "mode": "form",
            "requestedSchema": { "type": "object", "properties": {} },
            "_meta": { "codex_approval_kind": "mcp_tool_call" }
        });
        assert!(claim_codex_mcp_tool_approval(
            &json!("request-1"),
            &approval,
            Some("thread-1"),
            &definitions,
            &mut active,
        )
        .is_none());

        active.remove("call-2");
        let mut native = approval.clone();
        native["serverName"] = json!("user-native-server");
        assert!(claim_codex_mcp_tool_approval(
            &json!("request-2"),
            &native,
            Some("thread-1"),
            &definitions,
            &mut active,
        )
        .is_none());

        let mut generic = approval;
        generic["_meta"]["codex_approval_kind"] = json!("tool_suggestion");
        assert!(claim_codex_mcp_tool_approval(
            &json!("request-3"),
            &generic,
            Some("thread-1"),
            &definitions,
            &mut active,
        )
        .is_none());
    }

    #[test]
    fn maps_only_explicit_dcc_permission_behaviors() {
        assert_eq!(
            codex_mcp_elicitation_result("allow"),
            Some(json!({
                "action": "accept",
                "content": null,
                "_meta": null
            }))
        );
        assert_eq!(
            codex_mcp_elicitation_result("deny"),
            Some(json!({
                "action": "decline",
                "content": null,
                "_meta": null
            }))
        );
        assert_eq!(codex_mcp_elicitation_result("allow_session"), None);
    }

    #[test]
    fn defaults_unknown_codex_mcp_tools_to_ask() {
        let definition_id = dcc_core::domain::mcp::McpDefinitionId("fixture".to_string());
        let policies = HashMap::from([(
            definition_id.clone(),
            HashMap::from([("fixture.mutate".to_string(), McpToolPolicyDecision::Deny)]),
        )]);

        assert_eq!(
            codex_mcp_tool_policy(&policies, &definition_id, "fixture.mutate"),
            McpToolPolicyDecision::Deny
        );
        assert_eq!(
            codex_mcp_tool_policy(&policies, &definition_id, "fixture.read"),
            McpToolPolicyDecision::Ask
        );
    }

    #[test]
    fn recognizes_codex_reconnect_notices() {
        assert!(is_codex_reconnect_notice("Reconnecting... 2/5"));
        assert!(is_codex_reconnect_notice("Reconnecting… 1/5"));
        assert!(!is_codex_reconnect_notice("plain error"));
    }

    #[test]
    fn reads_retryable_error_message() {
        let payload = json!({
            "willRetry": true,
            "error": { "message": "Reconnecting... 2/5" }
        });
        assert_eq!(codex_error_message(&payload), "Reconnecting... 2/5");
    }

    #[test]
    fn parses_codex_account_usage_windows() {
        let usage = parse_codex_account_usage(&json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": 82.5,
                    "windowDurationMins": 300,
                    "resetsAt": 1_800_000_000
                },
                "secondary": {
                    "usedPercent": 25.0,
                    "windowDurationMins": 10_080,
                    "resetsAt": 1_800_100_000
                },
                "planType": "plus"
            }
        }))
        .expect("usage should parse");

        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[0].remaining_percent, 17.5);
        assert_eq!(usage.plan_type.as_deref(), Some("plus"));
    }

    #[tokio::test]
    async fn account_usage_response_timeout_is_not_extended_by_notifications() {
        let (reader, mut writer) = tokio::io::duplex(1_024);
        let writer_task = tokio::spawn(async move {
            loop {
                if writer
                    .write_all(b"{\"method\":\"status/changed\",\"params\":{}}\n")
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        let mut lines = BufReader::new(reader).lines();

        let error = read_rpc_response_with_timeout(&mut lines, 2, Duration::from_millis(25))
            .await
            .expect_err("notifications must not keep the request alive indefinitely");

        assert!(error.to_string().contains("timed out"));
        writer_task.abort();
    }
}
