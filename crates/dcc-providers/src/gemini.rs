use dcc_core::domain::provider::{
	HealthStatus, ProviderDescriptor, ProviderId, ProviderModelDescriptor,
};

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
		models: vec![
			ProviderModelDescriptor {
				id: "gemini-2.5-pro".to_string(),
				label: "Gemini 2.5 Pro".to_string(),
				description: "Balanced default for long-context work.".to_string(),
				recommended: true,
			},
			ProviderModelDescriptor {
				id: "gemini-2.5-flash".to_string(),
				label: "Gemini 2.5 Flash".to_string(),
				description: "Fast variant for lightweight responses.".to_string(),
				recommended: false,
			},
		],
		capabilities: stable_cli_capabilities(),
		health,
		stable: true,
	}
}
