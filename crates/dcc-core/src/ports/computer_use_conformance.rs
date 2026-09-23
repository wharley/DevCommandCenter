use async_trait::async_trait;

use crate::domain::{mcp::McpTransportKind, provider::ProviderId};

/// Provider-backed steps for the Computer Use conformance contract. Adapters
/// must exercise the actual runtime and MCP tool-result path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputerUseConformanceStep {
    Reset,
    AttachFixture,
    CreateSession,
    InspectImage,
    StartInterruptibleOperation,
    InterruptTurn,
    ConfirmTurnStopped,
    FinalCleanup,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputerUseConformanceObservation {
    Acknowledged,
    SessionCreated,
    ImageUnderstood,
    InterruptRequested,
    TurnStopped,
    CleanupConfirmed,
}

/// Errors are categorical so prompts, provider output, and credentials cannot
/// leak through evidence or routine CI diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputerUseConformanceAdapterError {
    Attachment,
    ProviderSession,
    ImageResult,
    ToolNotRequested,
    FixtureCallNotStarted,
    Interruption,
    Lifecycle,
    Unavailable,
}

pub type ComputerUseConformanceAdapterResult<T> =
    std::result::Result<T, ComputerUseConformanceAdapterError>;

#[async_trait]
pub trait ComputerUseConformanceAdapter: Send {
    fn provider_id(&self) -> ProviderId;

    fn provider_version(&self) -> String;

    async fn execute(
        &mut self,
        transport: McpTransportKind,
        step: ComputerUseConformanceStep,
    ) -> ComputerUseConformanceAdapterResult<ComputerUseConformanceObservation>;
}
