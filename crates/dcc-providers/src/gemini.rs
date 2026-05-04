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
        "gemini",
        "Gemini",
        "Stable Gemini CLI provider for workspace tasks.",
        "gemini",
        stable_cli_capabilities(),
        true,
        HeadlessCliKind::Gemini,
    )
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId("gemini".to_string()),
        label: "Gemini".to_string(),
        description: "Stable Gemini CLI provider for workspace tasks.".to_string(),
        models: model_registry::GEMINI
            .iter()
            .map(|m| m.to_descriptor())
            .collect(),
        capabilities: stable_cli_capabilities(),
        health,
        stable: true,
    }
}
