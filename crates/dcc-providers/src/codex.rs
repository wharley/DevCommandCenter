use dcc_core::domain::provider::{
	HealthStatus, ProviderDescriptor, ProviderId, ProviderModelDescriptor,
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
		models: vec![
			ProviderModelDescriptor {
				id: "gpt-5-codex".to_string(),
				label: "GPT-5 Codex".to_string(),
				description: "Default Codex model for agentic coding.".to_string(),
				recommended: true,
			},
			ProviderModelDescriptor {
				id: "gpt-5.3-codex".to_string(),
				label: "GPT-5.3 Codex".to_string(),
				description: "Latest Codex variant with stronger reasoning.".to_string(),
				recommended: false,
			},
		],
		capabilities: stable_cli_capabilities(),
		health,
		stable: true,
	}
}
