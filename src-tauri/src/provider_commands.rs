use dcc_tauri::commands::provider_commands::{
    ProviderAccountUsageInput, ProviderAccountUsageOutput,
};

#[tauri::command]
pub async fn provider_account_usage(
    input: ProviderAccountUsageInput,
) -> Result<ProviderAccountUsageOutput, String> {
    dcc_tauri::commands::provider_commands::provider_account_usage(input).await
}
