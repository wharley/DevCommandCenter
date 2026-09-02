use dcc_core::domain::{
    model_registry,
    provider::{
        Capabilities, HealthStatus, McpOauthSupport, McpSupportLevel, ProviderApprovalPolicy,
        ProviderDescriptor, ProviderId,
    },
};

use crate::{claude_sdk_sidecar::ClaudeSdkSidecarAdapter, common::stable_cli_capabilities};

pub fn adapter() -> ClaudeSdkSidecarAdapter {
    ClaudeSdkSidecarAdapter::new(
        "claude_code",
        "Claude Code",
        "Claude SDK-backed provider for agentic coding and tool use.",
        claude_code_capabilities(),
        true,
    )
}

fn claude_code_capabilities() -> Capabilities {
    let mut capabilities = stable_cli_capabilities();
    capabilities.mcp_support = McpSupportLevel::NativeConfig;
    capabilities.mcp_oauth_support = McpOauthSupport::ManagedDuringTurn;
    capabilities.can_request_delegation = true;
    capabilities.supports_multi_root = true;
    capabilities.approval_policies = vec![
        ProviderApprovalPolicy::Ask,
        ProviderApprovalPolicy::Auto,
        ProviderApprovalPolicy::FullAccess,
    ];
    capabilities
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId("claude_code".to_string()),
        label: "Claude Code".to_string(),
        description: "Claude SDK-backed provider for agentic coding and tool use.".to_string(),
        models: model_registry::CLAUDE_CODE
            .iter()
            .map(|m| m.to_descriptor())
            .collect(),
        capabilities: claude_code_capabilities(),
        health,
        enabled: true,
        availability_generation: 0,
        stable: true,
    }
}
