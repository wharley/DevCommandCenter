use std::{collections::HashMap, process::Stdio, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::{self, BoxStream};
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
			let _ = runtime_for_task.events_tx.send(ProviderEvent::Started {
				at: now_iso(),
			});

			let mut reader = BufReader::new(stdout).lines();
			while let Ok(Some(line)) = reader.next_line().await {
				let content = line.trim_end().to_string();
				if content.is_empty() {
					continue;
				}
				let _ = runtime_for_task
					.events_tx
					.send(ProviderEvent::TextDelta { content });
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
