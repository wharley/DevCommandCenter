use serde::{Deserialize, Serialize};
use specta::Type;

use dcc_core::domain::provider::ProviderCatalog;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListProvidersOutput {
	pub catalog: ProviderCatalog,
}

#[tauri::command]
pub async fn list_providers() -> Result<ListProvidersOutput, String> {
	Ok(ListProvidersOutput {
		catalog: dcc_providers::provider_catalog().await,
	})
}
