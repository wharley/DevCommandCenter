use dcc_core::domain::{
    model_registry,
    provider::{HealthStatus, ProviderDescriptor, ProviderId},
};

use crate::{
    common::stable_cli_capabilities,
    headless_cli::{HeadlessCliKind, HeadlessCliProviderAdapter},
};

pub fn adapter() -> HeadlessCliProviderAdapter {
    HeadlessCliProviderAdapter::new(
        "claude_code",
        "Claude Code",
        "Stable Claude CLI provider for agentic coding and tool use.",
        "claude",
        stable_cli_capabilities(),
        true,
        HeadlessCliKind::Claude,
    )
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId("claude_code".to_string()),
        label: "Claude Code".to_string(),
        description: "Stable Claude CLI provider for agentic coding and tool use.".to_string(),
        models: model_registry::CLAUDE_CODE
            .iter()
            .map(|m| m.to_descriptor())
            .collect(),
        capabilities: stable_cli_capabilities(),
        health,
        stable: true,
    }
}
