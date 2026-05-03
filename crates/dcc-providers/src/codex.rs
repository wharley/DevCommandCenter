use dcc_core::domain::{
	model_registry,
	provider::{HealthStatus, ProviderDescriptor, ProviderId},
};

use crate::common::{stable_cli_capabilities, CliProviderAdapter};

pub fn adapter() -> CliProviderAdapter {
	CliProviderAdapter::new(
		"codex",
		"Codex",
		"Stable OpenAI Codex provider for repo-aware coding workflows.",
		"codex",
		stable_cli_capabilities(),
		true,
	)
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
	ProviderDescriptor {
		id: ProviderId("codex".to_string()),
		label: "Codex".to_string(),
		description: "Stable OpenAI Codex provider for repo-aware coding workflows.".to_string(),
		models: model_registry::CODEX
			.iter()
			.map(|m| m.to_descriptor())
			.collect(),
		capabilities: stable_cli_capabilities(),
		health,
		stable: true,
	}
}
