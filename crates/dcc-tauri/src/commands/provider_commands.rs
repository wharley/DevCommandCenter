use serde::{Deserialize, Serialize};
use specta::Type;

use dcc_core::{
    domain::provider::{ProviderAccountUsage, ProviderCatalog},
    ports::ProviderRuntimeConfig,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListProvidersOutput {
    pub catalog: ProviderCatalog,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountUsageInput {
    pub provider_id: String,
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountUsageOutput {
    pub usage: Option<ProviderAccountUsage>,
}

#[tauri::command]
pub async fn list_providers() -> Result<ListProvidersOutput, String> {
    Ok(ListProvidersOutput {
        catalog: dcc_providers::provider_catalog().await,
    })
}

#[tauri::command]
pub async fn provider_account_usage(
    input: ProviderAccountUsageInput,
) -> Result<ProviderAccountUsageOutput, String> {
    let Some(provider) = dcc_providers::provider_runtime(&input.provider_id) else {
        return Ok(ProviderAccountUsageOutput { usage: None });
    };
    let usage = provider
        .account_usage(input.provider_runtime.as_ref())
        .await
        .map_err(|error| error.to_string())?;
    Ok(ProviderAccountUsageOutput { usage })
}
