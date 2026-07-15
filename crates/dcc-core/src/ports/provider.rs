use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    domain::provider::{
        Capabilities, HealthStatus, ProviderAccountUsage, ProviderEvent, ProviderId, SessionHandle,
    },
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
    pub tool_instructions: Option<String>,
    #[serde(default)]
    pub plan_mode: Option<bool>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub fast_mode: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<ProviderUserInputOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputAnswer {
    pub question: String,
    pub answer: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUserInputResponse {
    pub request_id: String,
    #[serde(default)]
    pub answers: Vec<ProviderUserInputAnswer>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPermissionRequest {
    pub request_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPermissionResponse {
    pub request_id: String,
    pub behavior: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum Input {
    Text(String),
    Turn(ProviderTurnInput),
    UserInputResponse(ProviderUserInputResponse),
    PermissionResponse(ProviderPermissionResponse),
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
    async fn account_usage(
        &self,
        _runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<Option<ProviderAccountUsage>> {
        Ok(None)
    }
}
