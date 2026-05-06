use serde::{Deserialize, Serialize};
use specta::Type;

use super::session::SessionId;
use crate::ports::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct ProviderId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SessionHandle {
    pub provider_id: ProviderId,
    pub session_id: SessionId,
    pub handle_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Capabilities {
    pub streaming: bool,
    pub mcp: bool,
    pub tools: bool,
    pub vision: bool,
    pub resumable: bool,
    pub experimental: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelDescriptor {
    pub id: String,
    pub label: String,
    pub description: String,
    pub recommended: bool,
    /// Ordered effort levels this model supports (e.g. `["low", "medium", "high", "xhigh"]`).
    /// Frontend uses this to drive the effort picker and clamp when switching models.
    pub effort_levels: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub label: String,
    pub description: String,
    pub models: Vec<ProviderModelDescriptor>,
    pub capabilities: Capabilities,
    pub health: HealthStatus,
    pub stable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalog {
    pub providers: Vec<ProviderDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum ProviderEvent {
    Started {
        at: String,
    },
    TextDelta {
        content: String,
    },
    ReasoningStarted {
        id: String,
        label: Option<String>,
        at: String,
    },
    ReasoningDelta {
        id: String,
        content: String,
    },
    ReasoningCompleted {
        id: String,
        at: String,
    },
    ToolCallStarted {
        id: String,
        action: String,
        command: Option<String>,
        file: Option<String>,
        at: String,
    },
    ToolCallDelta {
        id: String,
        content: String,
    },
    ToolCallCompleted {
        id: String,
        at: String,
    },
    ToolCallFailed {
        id: String,
        reason: Option<String>,
        at: String,
    },
    UserInputRequested {
        id: String,
        questions: Vec<ProviderUserInputQuestion>,
        at: String,
    },
    UserInputResolved {
        id: String,
        answers: Vec<ProviderUserInputAnswer>,
        at: String,
    },
    Completed {
        at: String,
    },
    Failed {
        message: String,
        at: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}
