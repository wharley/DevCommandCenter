use dcc_core::domain::{
    model_registry,
    provider::{HealthStatus, ProviderApprovalPolicy, ProviderDescriptor, ProviderId},
};

use crate::{
    common::stable_cli_capabilities,
    headless_cli::{HeadlessCliKind, HeadlessCliProviderAdapter},
};

fn gemini_capabilities() -> dcc_core::domain::provider::Capabilities {
    let mut capabilities = stable_cli_capabilities();
    capabilities.supports_multi_root = true;
    capabilities.approval_policies = vec![
        ProviderApprovalPolicy::Ask,
        ProviderApprovalPolicy::Auto,
        ProviderApprovalPolicy::FullAccess,
    ];
    capabilities.supports_runtime_home = true;
    capabilities.supports_runtime_binary = true;
    capabilities.plan_mode_support = dcc_core::domain::provider::TurnControlSupport::Native;
    capabilities
}

pub fn adapter() -> HeadlessCliProviderAdapter {
    HeadlessCliProviderAdapter::new(
        "gemini",
        "Gemini CLI (legacy)",
        "Legacy Gemini CLI provider for API keys, Vertex AI, and eligible enterprise accounts. Personal Google sign-in has moved to Antigravity.",
        "gemini",
        gemini_capabilities(),
        true,
        HeadlessCliKind::Gemini,
    )
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId("gemini".to_string()),
        label: "Gemini CLI (legacy)".to_string(),
        description: "Legacy Gemini CLI provider for API keys, Vertex AI, and eligible enterprise accounts. Personal Google sign-in has moved to Antigravity.".to_string(),
        models: model_registry::GEMINI
            .iter()
            .map(|m| m.to_descriptor())
            .collect(),
        capabilities: gemini_capabilities(),
        health,
        enabled: true,
        availability_generation: 0,
        stable: true,
    }
}
