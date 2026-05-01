use dcc_core::domain::provider::{
	HealthStatus, ProviderDescriptor, ProviderId, ProviderModelDescriptor,
};

use crate::common::{stable_cli_capabilities, CliProviderAdapter};

pub fn adapter() -> CliProviderAdapter {
	CliProviderAdapter::new(
		"claude_code",
		"Claude Code",
		"Stable Claude CLI provider for agentic coding and tool use.",
		"claude",
		stable_cli_capabilities(),
		true,
	)
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
	ProviderDescriptor {
		id: ProviderId("claude_code".to_string()),
		label: "Claude Code".to_string(),
		description: "Stable Claude CLI provider for agentic coding and tool use.".to_string(),
		models: vec![
			ProviderModelDescriptor {
				id: "claude-opus-4-6".to_string(),
				label: "Claude Opus 4.6".to_string(),
				description: "Highest capability, best for deep reasoning and large refactors."
					.to_string(),
				recommended: false,
			},
			ProviderModelDescriptor {
				id: "claude-sonnet-4-6".to_string(),
				label: "Claude Sonnet 4.6".to_string(),
				description: "Balanced default for coding and analysis.".to_string(),
				recommended: true,
			},
			ProviderModelDescriptor {
				id: "claude-haiku-4-5".to_string(),
				label: "Claude Haiku 4.5".to_string(),
				description: "Fast, lightweight option for quick follow-ups.".to_string(),
				recommended: false,
			},
		],
		capabilities: stable_cli_capabilities(),
		health,
		stable: true,
	}
}
