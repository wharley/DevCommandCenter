use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::process::Command;
use uuid::Uuid;

use dcc_core::{
	domain::{
		provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
		session::SessionId,
		workspace::WorkspaceId,
	},
	ports::{Input, Provider, SessionConfig},
	Result,
};

#[derive(Clone, Debug)]
pub struct CliProviderAdapter {
	pub id: ProviderId,
	pub label: String,
	pub description: String,
	pub binary: String,
	pub capabilities: Capabilities,
	pub stable: bool,
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
		}
	}

	fn binary_command(&self) -> Command {
		let mut command = Command::new(&self.binary);
		command.arg("--version");
		command
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
		Ok(SessionHandle {
			provider_id: self.id.clone(),
			session_id: cfg.session_id,
			handle_id: Uuid::new_v4().to_string(),
		})
	}

	async fn send_input(&self, _handle: &SessionHandle, _input: Input) -> Result<()> {
		Ok(())
	}

	fn stream_events(&self, _handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>> {
		Box::pin(stream::empty())
	}

	async fn cancel(&self, _handle: &SessionHandle) -> Result<()> {
		Ok(())
	}

	async fn resume(&self, previous: &SessionId) -> Result<SessionHandle> {
		Ok(SessionHandle {
			provider_id: self.id.clone(),
			session_id: previous.clone(),
			handle_id: Uuid::new_v4().to_string(),
		})
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
