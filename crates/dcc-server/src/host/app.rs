//! What the API needs from the app it runs in (`http_api::DesktopApp`): a
//! created task's events, the connectors, the ai-memory sidecar and
//! restarting.

use std::sync::Arc;

use dcc_tauri::app_events::AppEvents;
use dcc_tauri::connector_gateway::ConnectorGateway;
use dcc_tauri::mcp_connect::McpConnectCoordinator;

use crate::ai_memory_sidecar::{AiMemorySidecar, AiMemorySidecarStatus};
use crate::http_api::DesktopApp;

pub struct HostApp {
    pub events: AppEvents,
    pub connectors: Option<(McpConnectCoordinator, ConnectorGateway)>,
    pub ai_memory: Arc<AiMemorySidecar>,
    pub restart: Arc<dyn Fn() + Send + Sync>,
}

impl DesktopApp for HostApp {
    fn events(&self) -> AppEvents {
        self.events.clone()
    }

    fn connectors(&self) -> Option<(McpConnectCoordinator, ConnectorGateway)> {
        self.connectors.clone()
    }

    fn ai_memory_status(&self) -> Option<AiMemorySidecarStatus> {
        Some(self.ai_memory.status())
    }

    fn restart(&self) {
        (self.restart)();
    }
}
