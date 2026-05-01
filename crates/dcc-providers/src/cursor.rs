use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor, ProviderId};

use crate::common::{experimental_cli_capabilities, CliProviderAdapter};

pub fn adapter() -> CliProviderAdapter {
	CliProviderAdapter::new(
		"cursor",
		"Cursor",
		"Experimental Cursor adapter kept behind the migration boundary.",
		"cursor-agent",
		experimental_cli_capabilities(),
		false,
	)
}

pub fn descriptor(health: HealthStatus) -> ProviderDescriptor {
	ProviderDescriptor {
		id: ProviderId("cursor".to_string()),
		label: "Cursor".to_string(),
		description: "Experimental Cursor adapter kept behind the migration boundary.".to_string(),
		capabilities: experimental_cli_capabilities(),
		health,
		stable: false,
	}
}
