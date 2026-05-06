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
    pub working_directory: Option<String>,
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeConfig {
    #[serde(default)]
    pub home_path: Option<String>,
    #[serde(default)]
    pub shadow_home_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTurnInput {
    pub prompt: String,
    #[serde(default)]
    pub plan_mode: Option<bool>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub fast_mode: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum Input {
    Text(String),
    Turn(ProviderTurnInput),
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> Capabilities;
    async fn prepare_session(&self, cfg: SessionConfig) -> Result<SessionHandle>;
    async fn send_input(&self, handle: &SessionHandle, input: Input) -> Result<()>;
    fn stream_events(&self, handle: &SessionHandle) -> BoxStream<'static, Result<ProviderEvent>>;
    async fn cancel(&self, handle: &SessionHandle) -> Result<()>;
    async fn resume(&self, previous: &SessionId) -> Result<SessionHandle>;
    async fn healthcheck(&self) -> Result<HealthStatus>;
}
