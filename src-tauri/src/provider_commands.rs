use dcc_tauri::commands::provider_commands::{
    ListProvidersOutput, ProviderAccountUsageInput, ProviderAccountUsageOutput,
};

#[tauri::command]
pub async fn list_providers() -> Result<ListProvidersOutput, String> {
    dcc_tauri::commands::provider_commands::list_providers().await
}

#[tauri::command]
pub async fn provider_account_usage(
    input: ProviderAccountUsageInput,
) -> Result<ProviderAccountUsageOutput, String> {
    dcc_tauri::commands::provider_commands::provider_account_usage(input).await
}
