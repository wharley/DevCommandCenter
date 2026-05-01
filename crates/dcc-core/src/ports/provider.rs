use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
	domain::provider::{Capabilities, HealthStatus, ProviderEvent, ProviderId, SessionHandle},
	domain::session::SessionId,
	domain::workspace::WorkspaceId,
	Result,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SessionConfig {
	pub workspace_id: WorkspaceId,
	pub session_id: SessionId,
	pub model: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum Input {
	Text(String),
}

#[async_trait]
pub trait Provider: Send + Sync {
	fn id(&self) -> ProviderId;
	fn capabilities(&self) -> Capabilities;
	async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle>;
	async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()>;
	fn stream_events(
		&self,
		handle: &SessionHandle,
	) -> BoxStream<'static, Result<ProviderEvent>>;
	async fn cancel(&self, handle: &SessionHandle) -> Result<()>;
	async fn resume(&self, previous: &SessionId) -> Result<SessionHandle>;
	async fn healthcheck(&self) -> Result<HealthStatus>;
}
