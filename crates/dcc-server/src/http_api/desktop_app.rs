//! What the API needs from the app it runs in, beyond the sessions: where a
//! new task's events go, the connector sign-in and gateway, the ai-memory
//! sidecar's state and restarting. The desktop answers it from its Tauri
//! app; a headless server has none (those routes answer 404/503).

use dcc_tauri::app_events::AppEvents;
use dcc_tauri::connector_gateway::ConnectorGateway;
use dcc_tauri::mcp_connect::McpConnectCoordinator;

use crate::ai_memory_sidecar::AiMemorySidecarStatus;

pub trait DesktopApp: Send + Sync {
    /// Where a created task's events go.
    fn events(&self) -> AppEvents;
    /// The sign-in coordinator and the gateway (one of each serves Settings
    /// and the agents); `None` when the app could not start them.
    fn connectors(&self) -> Option<(McpConnectCoordinator, ConnectorGateway)>;
    /// The ai-memory sidecar's state; `None` without one.
    fn ai_memory_status(&self) -> Option<AiMemorySidecarStatus>;
    /// Restarts the whole app (`restart_dcc`); ends this process.
    fn restart(&self);
}
