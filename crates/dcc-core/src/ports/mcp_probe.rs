use async_trait::async_trait;

use crate::domain::mcp::{McpDefinition, McpProbeReport, McpRuntimeError};

pub type McpProbeResult<T> = std::result::Result<T, McpRuntimeError>;

#[async_trait]
pub trait McpProbe: Send + Sync {
    async fn probe(&self, definition: &McpDefinition) -> McpProbeResult<McpProbeReport>;
}
