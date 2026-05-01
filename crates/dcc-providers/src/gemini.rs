use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor, ProviderId};

use crate::common::{stable_cli_capabilities, CliProviderAdapter};

pub fn adapter() -> CliProviderAdapter {
	CliProviderAdapter::new(
		"gemini",
		"Gemini",
		"Stable Gemini CLI provider for workspace tasks.",
		"gemini",
		stable_cli_capabilities(),
		true,
	)
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
	ProviderDescriptor {
		id: ProviderId("gemini".to_string()),
		label: "Gemini".to_string(),
		description: "Stable Gemini CLI provider for workspace tasks.".to_string(),
		capabilities: stable_cli_capabilities(),
		health,
		stable: true,
	}
}
