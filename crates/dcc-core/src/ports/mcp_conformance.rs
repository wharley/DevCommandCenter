use async_trait::async_trait;

use crate::domain::{mcp::McpTransportKind, provider::ProviderId};

pub const MCP_CONFORMANCE_ECHO_VALUE: &str = "dcc-conformance-echo-v1";

/// Stable steps in the provider bridge conformance protocol. Adapters should
/// drive the real provider surface for each step instead of simulating the
/// expected observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpConformanceStep {
    Reset,
    AttachFixture,
    CreateSession,
    ListTools,
    CallReadOnlyTool,
    RequestMutatingTool,
    DenyMutatingTool,
    ConfirmMutationNotExecuted,
    DisableFixture,
    RefreshAfterDisable,
    RemoveFixture,
    ConfirmCleanup,
    AttachFixtureForServerFailure,
    MakeServerUnavailable,
    ConfirmServerFailure,
    ResetAfterServerFailure,
    AttachFixtureForCredentialFailure,
    MakeCredentialUnavailable,
    ConfirmCredentialFailure,
    FinalCleanup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpConformanceUnavailableKind {
    Server,
    Credential,
}

/// Bounded observations returned to the harness. Full provider payloads,
/// credentials, stderr, and tool arguments are deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpConformanceObservation {
    Acknowledged,
    SessionCreated,
    ToolsVisible(Vec<String>),
    ReadOnlyResult(String),
    ApprovalRequired { tool_name: String },
    MutationDenied,
    MutationNotExecuted,
    FixtureUnavailable,
    CleanupConfirmed,
    FailedClosed(McpConformanceUnavailableKind),
}

/// Categorical adapter errors prevent provider output or credentials from
/// entering CI logs and snapshots through the conformance result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpConformanceAdapterError {
    Attachment,
    ProviderSession,
    Protocol,
    PermissionBoundary,
    Lifecycle,
    Unavailable,
}

pub type McpConformanceAdapterResult<T> = std::result::Result<T, McpConformanceAdapterError>;

#[async_trait]
pub trait McpConformanceAdapter: Send {
    fn provider_id(&self) -> ProviderId;

    fn provider_version(&self) -> String;

    async fn execute(
        &mut self,
        transport: McpTransportKind,
        step: McpConformanceStep,
    ) -> McpConformanceAdapterResult<McpConformanceObservation>;
}
