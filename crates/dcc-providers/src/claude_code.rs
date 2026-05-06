use dcc_core::domain::{
    model_registry,
    provider::{HealthStatus, ProviderDescriptor, ProviderId},
};

use crate::{claude_sdk_sidecar::ClaudeSdkSidecarAdapter, common::stable_cli_capabilities};

pub fn adapter() -> ClaudeSdkSidecarAdapter {
    ClaudeSdkSidecarAdapter::new(
        "claude_code",
        "Claude Code",
        "Claude SDK-backed provider for agentic coding and tool use.",
        stable_cli_capabilities(),
        true,
    )
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
        capabilities: stable_cli_capabilities(),
        health,
        stable: true,
    }
}
