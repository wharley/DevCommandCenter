use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor, ProviderId};

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
		capabilities: stable_cli_capabilities(),
		health,
		stable: true,
	}
}
