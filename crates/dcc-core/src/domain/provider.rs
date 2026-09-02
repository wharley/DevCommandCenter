use serde::{Deserialize, Serialize};
use specta::Type;

use super::session::{AssistantMessagePhase, SessionId};
use super::{
    mcp::McpRuntimeStatus,
    mcp_conformance::{McpConformanceEvidence, McpConformanceEvidenceError},
};
use crate::domain::usage::ModelTokenUsage;
use crate::ports::provider::{
    ProviderPermissionRequest, ProviderUserInputAnswer, ProviderUserInputQuestion,
};

/// Lifecycle reported by a provider for one of its own native subagents.  This
/// is deliberately separate from a DCC delegation: it has no DCC child
/// session and is emitted only when the provider supplies structured data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NativeSubagentStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct ProviderId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SessionHandle {
    pub provider_id: ProviderId,
    pub session_id: SessionId,
    pub handle_id: String,
}

/// Describes the MCP attachment contract that the DCC adapter can actually
/// guarantee. Parsing MCP-shaped tool events alone does not raise this level.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum McpSupportLevel {
    /// DCC cannot reliably attach an external MCP server through this adapter.
    #[default]
    Unsupported,
    /// The provider may load its own MCP configuration, but DCC does not own or
    /// verify attachment, lifecycle, permissions, or tool visibility.
    NativeConfig,
    /// DCC has a backend-only projection path that negotiates the required MCP
    /// contract when a session starts. The reported provider version is
    /// diagnostic metadata, not a compatibility allowlist or conformance
    /// evidence; the live per-session runtime status remains authoritative.
    RuntimeBridge { provider_version: String },
    /// DCC owns a tested bridge that attaches servers and verifies tools
    /// end-to-end through the provider adapter. The evidence value can only be
    /// produced by a successful run of the shared conformance harness.
    VerifiedBridge { evidence: McpConformanceEvidence },
}

/// Declares how an adapter handles interactive OAuth for DCC-projected MCP
/// servers. This is an executable adapter contract, not provider-name
/// inference in the renderer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpOauthSupport {
    /// The adapter exposes no DCC-controlled interactive OAuth operation.
    #[default]
    Unsupported,
    /// The adapter completes OAuth as part of normal turn attachment and
    /// withholds the user prompt until the MCP server is ready.
    ManagedDuringTurn,
    /// DCC can start OAuth before the turn and observe completion through the
    /// provider runtime status channel.
    InteractivePreflight,
}

impl McpSupportLevel {
    pub fn verified_evidence(&self) -> Option<&McpConformanceEvidence> {
        match self {
            Self::VerifiedBridge { evidence } => Some(evidence),
            Self::Unsupported | Self::NativeConfig | Self::RuntimeBridge { .. } => None,
        }
    }

    pub fn validate(&self) -> Result<(), McpConformanceEvidenceError> {
        if let Self::VerifiedBridge { evidence } = self {
            evidence.validate()?;
        }
        if let Self::RuntimeBridge { provider_version } = self {
            validate_provider_version(provider_version)?;
        }
        Ok(())
    }

    pub fn validate_for_provider(
        &self,
        provider_id: &ProviderId,
        provider_version: &str,
    ) -> Result<(), McpConformanceEvidenceError> {
        if let Self::VerifiedBridge { evidence } = self {
            evidence.validate_for_provider(provider_id, provider_version)?;
        }
        Ok(())
    }
}

fn validate_provider_version(provider_version: &str) -> Result<(), McpConformanceEvidenceError> {
    if provider_version.trim().is_empty() || provider_version.chars().count() > 128 {
        return Err(McpConformanceEvidenceError::InvalidProviderMetadata);
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub streaming: bool,
    /// The provider can append user guidance to an in-flight turn without
    /// interrupting it or starting a second turn.
    #[serde(default)]
    pub supports_steering: bool,
    /// The adapter can append user guidance to a provider-native subagent's
    /// currently active turn.
    #[serde(default)]
    pub supports_native_subagent_steering: bool,
    /// The adapter can interrupt a provider-native subagent without stopping
    /// the parent session.
    #[serde(default)]
    pub supports_native_subagent_interrupt: bool,
    pub mcp_support: McpSupportLevel,
    #[serde(default)]
    pub mcp_oauth_support: McpOauthSupport,
    pub tools: bool,
    pub vision: bool,
    pub resumable: bool,
    pub experimental: bool,
    pub can_be_delegation_target: bool,
    pub can_request_delegation: bool,
    pub supports_read_only_delegation: bool,
    pub supports_edit_delegation: bool,
    /// The adapter can safely expose more than one DCC-managed workspace root.
    #[serde(default)]
    pub supports_multi_root: bool,
    /// User-selectable approval policies that this adapter can enforce natively.
    #[serde(default)]
    pub approval_policies: Vec<ProviderApprovalPolicy>,
    /// The adapter applies a DCC-managed `home_path` from the runtime config
    /// to the provider process. Adapters without this ignore the field, so the
    /// backend rejects it instead of silently dropping the preference.
    #[serde(default)]
    pub supports_runtime_home: bool,
    /// The adapter materializes a DCC-managed `shadow_home_path` (isolated
    /// auth/config copy) from the runtime config.
    #[serde(default)]
    pub supports_shadow_home: bool,
    /// The adapter enforces `max_concurrent_subagents` from the runtime config.
    #[serde(default)]
    pub supports_subagent_concurrency: bool,
    /// The adapter can report account usage windows through `account_usage`.
    /// Providers without this never spawn a runtime to answer usage queries.
    #[serde(default)]
    pub supports_account_usage: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderApprovalPolicy {
    Ask,
    Auto,
    FullAccess,
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
    /// Server-backed DCC runtime authority. This is independent from health:
    /// a healthy adapter may be disabled deliberately.
    #[serde(default = "provider_enabled_by_default")]
    pub enabled: bool,
    /// Monotonic server-backed availability generation, zero for legacy
    /// catalogs that predate the availability table.
    #[serde(default)]
    pub availability_generation: u64,
    pub stable: bool,
}

fn provider_enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalog {
    pub providers: Vec<ProviderDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageWindow {
    pub id: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub window_duration_minutes: Option<u64>,
    pub is_exhausted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProviderAccountUsageState {
    Available,
    AwaitingActivity,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountUsage {
    pub provider_id: ProviderId,
    pub state: ProviderAccountUsageState,
    pub windows: Vec<ProviderUsageWindow>,
    #[serde(default)]
    pub plan_type: Option<String>,
    pub updated_at: String,
    pub is_cached: bool,
}

#[cfg(test)]
mod tests {
    use super::McpSupportLevel;

    #[test]
    fn runtime_bridge_serializes_camel_case_diagnostic_version() {
        let support = McpSupportLevel::RuntimeBridge {
            provider_version: "codex-cli@0.146.0+app-server-protocol-v2".to_string(),
        };
        support.validate().expect("valid runtime bridge");

        let value = serde_json::to_value(support).expect("serialize runtime bridge");
        assert_eq!(
            value["runtimeBridge"]["providerVersion"],
            "codex-cli@0.146.0+app-server-protocol-v2"
        );
        assert!(value["runtimeBridge"].get("provider_version").is_none());
    }

    #[test]
    fn runtime_bridge_rejects_missing_or_unbounded_version_metadata() {
        assert!(McpSupportLevel::RuntimeBridge {
            provider_version: String::new(),
        }
        .validate()
        .is_err());
        assert!(McpSupportLevel::RuntimeBridge {
            provider_version: "v".repeat(129),
        }
        .validate()
        .is_err());
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum ProviderEvent {
    Started {
        at: String,
    },
    /// Complete ephemeral MCP status snapshot for the provider session.
    ///
    /// It is intentionally separate from the persisted session transcript.
    McpRuntimeStatusSnapshot {
        statuses: Vec<McpRuntimeStatus>,
    },
    /// Backend-only signal that an adapter captured new OAuth state. The
    /// credential bytes remain in the provider's private drain channel.
    McpOauthStateChanged {
        definition_id: crate::domain::mcp::McpDefinitionId,
    },
    NativeSubagentActivity {
        id: String,
        agent_id: Option<String>,
        agent_thread_id: Option<String>,
        /// Canonical provider task path (for example `/root/reviewer`).
        /// Kept separate from the user-facing nickname so older flat
        /// presentations do not lose their existing identity.
        path: Option<String>,
        name: Option<String>,
        role: Option<String>,
        /// Confirmed/effective model reported by the provider. Requested
        /// models are emitted through NativeSubagentModelRequested instead;
        /// this field is never inferred from the parent session model.
        model: Option<String>,
        status: NativeSubagentStatus,
        at: String,
    },
    NativeSubagentModelConfirmed {
        correlation_id: String,
        model: String,
        at: String,
    },
    NativeSubagentModelRequested {
        correlation_id: String,
        model: String,
        at: String,
    },
    ModelEffective {
        model: String,
        at: String,
    },
    TextDelta {
        content: String,
    },
    /// Starts a semantically distinct assistant message within the current
    /// turn. Rich providers should use their native item identifier.
    AssistantMessageStarted {
        id: String,
        phase: AssistantMessagePhase,
        at: String,
    },
    AssistantMessageDelta {
        id: String,
        content: String,
    },
    /// Completes an assistant message. `content`, when present, is the
    /// provider's authoritative final snapshot and replaces streamed deltas.
    AssistantMessageCompleted {
        id: String,
        phase: AssistantMessagePhase,
        content: Option<String>,
        model: Option<String>,
        at: String,
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
    PermissionRequested {
        request: ProviderPermissionRequest,
        at: String,
    },
    PermissionResolved {
        id: String,
        behavior: String,
        at: String,
    },
    /// Exact per-turn usage reported by the provider runtime. Missing categories
    /// remain zero and cost stays absent instead of being estimated here.
    TurnUsage {
        models: Vec<ModelTokenUsage>,
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
