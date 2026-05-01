use dcc_core::domain::provider::{
	HealthStatus, ProviderDescriptor, ProviderId, ProviderModelDescriptor,
};

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
		models: vec![
			ProviderModelDescriptor {
				id: "cursor-agent".to_string(),
				label: "Cursor Agent".to_string(),
				description: "Primary Cursor agent flow.".to_string(),
				recommended: true,
			},
			ProviderModelDescriptor {
				id: "cursor-editor".to_string(),
				label: "Cursor Editor".to_string(),
				description: "More direct editor-centric workflow.".to_string(),
				recommended: false,
			},
		],
		capabilities: experimental_cli_capabilities(),
		health,
		stable: false,
	}
}
