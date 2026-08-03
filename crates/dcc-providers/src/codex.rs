use dcc_core::domain::{
    model_registry,
    provider::{
        Capabilities, HealthStatus, McpOauthSupport, McpSupportLevel, ProviderApprovalPolicy,
        ProviderDescriptor, ProviderId,
    },
};

use crate::{codex_app_server::CodexAppServerAdapter, common::stable_cli_capabilities};

pub fn adapter() -> CodexAppServerAdapter {
    CodexAppServerAdapter::new(stable_codex_capabilities())
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId("codex".to_string()),
        label: "Codex".to_string(),
        description: "OpenAI Codex provider via app-server protocol.".to_string(),
        models: model_registry::CODEX
            .iter()
            .map(|m| m.to_descriptor())
            .collect(),
        capabilities: stable_codex_capabilities(),
        health,
        stable: true,
    }
}

pub fn stable_codex_capabilities() -> Capabilities {
    let mut capabilities = stable_cli_capabilities();
    capabilities.supports_steering = true;
    capabilities.mcp_support = McpSupportLevel::NativeConfig;
    capabilities.mcp_oauth_support = McpOauthSupport::InteractivePreflight;
    capabilities.can_request_delegation = true;
    capabilities.supports_multi_root = true;
    capabilities.approval_policies = vec![
        ProviderApprovalPolicy::Ask,
        ProviderApprovalPolicy::Auto,
        ProviderApprovalPolicy::FullAccess,
    ];
    capabilities
}
