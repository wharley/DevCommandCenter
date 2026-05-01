use std::{collections::HashMap, process::Stdio, sync::Arc};

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
	domain::{
		provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
		session::SessionId,
		workspace::WorkspaceId,
	},
	ports::{Input, Provider, SessionConfig},
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
struct ProviderStreamState {
	claude_blocks: HashMap<u64, ClaudeBlockState>,
}

#[derive(Debug, Clone)]
enum ClaudeBlockState {
	Reasoning { id: String },
	ToolCall { id: String },
}

#[derive(Debug)]
enum ParsedProviderLine {
	Event(ProviderEvent),
	Text(String),
	Ignored,
}

fn parse_provider_stream_line(line: &str, state: &mut ProviderStreamState) -> ParsedProviderLine {
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
	if let Some(event) = parse_codex_stream_value(&value) {
		return ParsedProviderLine::Event(event);
	}

	ParsedProviderLine::Ignored
}

fn parse_custom_envelope(value: &Value) -> Option<ProviderEvent> {
	let envelope = serde_json::from_value::<ProviderEnvelope>(value.clone()).ok()?;
	let at = now_iso();

	Some(match envelope {
		ProviderEnvelope::ReasoningStarted { id, label } => ProviderEvent::ReasoningStarted {
			id,
			label,
			at,
		},
		ProviderEnvelope::ReasoningDelta { id, content } => {
			ProviderEvent::ReasoningDelta { id, content }
		}
		ProviderEnvelope::ReasoningCompleted { id } => {
			ProviderEvent::ReasoningCompleted { id, at }
		}
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
		ProviderEnvelope::ToolCallCompleted { id } => {
			ProviderEvent::ToolCallCompleted { id, at }
		}
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
		return parse_claude_terminal_value(value);
	}

	let event = value.get("event")?.as_object()?;
	let event_type = event.get("type").and_then(Value::as_str)?;
	let at = now_iso();

	match event_type {
		"message_start" => {
			state.claude_blocks.clear();
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

fn parse_claude_terminal_value(value: &Value) -> Option<ProviderEvent> {
	let kind = value.get("type").and_then(Value::as_str)?;
	let at = now_iso();
	match kind {
		"result" => {
			if value.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
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

fn parse_codex_stream_value(value: &Value) -> Option<ProviderEvent> {
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
					command: item
						.get("tool")
						.and_then(Value::as_str)
						.map(str::to_string),
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
			content: value
				.get("delta")
				.and_then(Value::as_str)
				.unwrap_or("")
				.to_string(),
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
		|| item.get("exitCode").and_then(Value::as_i64).is_some_and(|code| code != 0)
		|| item
			.get("exit_code")
			.and_then(Value::as_i64)
			.is_some_and(|code| code != 0)
		|| item.get("is_error").and_then(Value::as_bool).unwrap_or(false)
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
		let mut child = command.spawn().map_err(|error| {
			CoreError::Provider(format!("failed to spawn {}: {}", self.binary, error))
		})?;

		let stdin = child.stdin.take().ok_or_else(|| {
			CoreError::Provider(format!("{} did not expose stdin", self.binary))
		})?;
		let stdout = child.stdout.take().ok_or_else(|| {
			CoreError::Provider(format!("{} did not expose stdout", self.binary))
		})?;

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
			let _ = runtime_for_task.events_tx.send(ProviderEvent::Started {
				at: now_iso(),
			});

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

fn now_iso() -> String {
	Utc::now().to_rfc3339()
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
				stream
					.write_all(text.as_bytes())
					.await
					.map_err(|error| {
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
					CoreError::Provider(format!("failed to flush input for {}: {}", self.binary, error))
				})?;
			}
		}

		Ok(())
	}

	fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
		let runtime = self.runtime.sessions.try_lock().ok().and_then(|sessions| {
			sessions.get(&handle.session_id.0).cloned()
		});

		let Some(runtime) = runtime else {
			return Box::pin(stream::empty());
		};

		let receiver = runtime.events_tx.subscribe();
		let stream = stream::unfold(receiver, |mut receiver| async move {
			match receiver.recv().await {
				Ok(event) => Some((Ok(event), receiver)),
				Err(broadcast::error::RecvError::Closed) => None,
				Err(broadcast::error::RecvError::Lagged(_)) => {
					Some((Ok(ProviderEvent::Failed {
						message: "provider event stream lagged".to_string(),
						at: now_iso(),
					}), receiver))
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
	}
}

pub fn local_workspace_id() -> WorkspaceId {
	WorkspaceId("local".to_string())
}

#[cfg(test)]
mod tests {
	use super::*;

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
}
