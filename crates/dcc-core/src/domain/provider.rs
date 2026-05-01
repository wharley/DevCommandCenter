use serde::{Deserialize, Serialize};
use specta::Type;

use super::session::SessionId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
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
pub enum ProviderEvent {
	Started { at: String },
	TextDelta { content: String },
	Completed { at: String },
	Failed { message: String, at: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum HealthStatus {
	Healthy,
	Degraded { reason: String },
	Unhealthy { reason: String },
}
