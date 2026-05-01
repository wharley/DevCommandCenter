pub mod claude_code;
pub mod codex;
pub mod common;
pub mod cursor;
pub mod gemini;

use futures::future::join_all;

use dcc_core::domain::provider::{
	HealthStatus, ProviderCatalog, ProviderDescriptor,
};
use dcc_core::ports::Provider;

pub const PROVIDER_IDS: [&str; 4] = ["claude_code", "codex", "gemini", "cursor"];

async fn provider_health_statuses() -> Vec<HealthStatus> {
	let adapters = [claude_code::adapter(), codex::adapter(), gemini::adapter(), cursor::adapter()];
	let futures = adapters.into_iter().map(|adapter| async move { adapter.healthcheck().await });
	let results = join_all(futures).await;
	results
		.into_iter()
		.map(|result| match result {
			Ok(status) => status,
			Err(error) => HealthStatus::Unhealthy {
				reason: error.to_string(),
			},
		})
		.collect()
}

pub async fn provider_catalog() -> ProviderCatalog {
	let health = provider_health_statuses().await;
	let mut providers = Vec::with_capacity(PROVIDER_IDS.len());
	providers.push(claude_code::descriptor(health[0].clone()));
	providers.push(codex::descriptor(health[1].clone()));
	providers.push(gemini::descriptor(health[2].clone()));
	providers.push(cursor::descriptor(health[3].clone()));
	ProviderCatalog { providers }
}

pub async fn provider_descriptors() -> Vec<ProviderDescriptor> {
	provider_catalog().await.providers
}
