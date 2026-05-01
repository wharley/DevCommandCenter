pub mod claude_code;
pub mod codex;
pub mod common;
pub mod cursor;
pub mod gemini;

use std::{collections::HashMap, sync::{Arc, OnceLock}};

use futures::future::join_all;

use dcc_core::domain::provider::{
	HealthStatus, ProviderCatalog, ProviderDescriptor,
};
use dcc_core::ports::Provider;

pub const PROVIDER_IDS: [&str; 4] = ["claude_code", "codex", "gemini", "cursor"];

fn provider_registry() -> &'static HashMap<String, Arc<dyn Provider>> {
	static REGISTRY: OnceLock<HashMap<String, Arc<dyn Provider>>> = OnceLock::new();
	REGISTRY.get_or_init(|| {
		let providers: [Arc<dyn Provider>; 4] = [
			Arc::new(claude_code::adapter()),
			Arc::new(codex::adapter()),
			Arc::new(gemini::adapter()),
			Arc::new(cursor::adapter()),
		];

		let mut registry = HashMap::with_capacity(PROVIDER_IDS.len());
		for provider in providers {
			registry.insert(provider.id().0.clone(), provider);
		}
		registry
	})
}

pub fn provider_runtime(provider_id: &str) -> Option<Arc<dyn Provider>> {
	provider_registry().get(provider_id).cloned()
}

async fn provider_health_statuses() -> Vec<HealthStatus> {
	let providers = provider_registry();
	let futures = PROVIDER_IDS
		.into_iter()
		.filter_map(|provider_id| providers.get(provider_id).cloned())
		.map(|provider| async move { provider.healthcheck().await });
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
