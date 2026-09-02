use dcc_tauri::commands::provider_commands::{
    ListProvidersOutput, ProviderAccountUsageInput, ProviderAccountUsageOutput,
    ProviderAvailabilityInput, ProviderAvailabilityOutput, SetProviderAvailabilityInput,
};
use dcc_tauri::state::SessionCommandState;
use tauri::State;

#[tauri::command]
pub async fn list_providers(
    state: State<'_, SessionCommandState>,
) -> Result<ListProvidersOutput, String> {
    dcc_tauri::commands::provider_commands::list_providers_for_state(&state).await
}

#[tauri::command]
pub async fn get_provider_availability(
    state: State<'_, SessionCommandState>,
    input: ProviderAvailabilityInput,
) -> Result<ProviderAvailabilityOutput, String> {
    dcc_tauri::commands::provider_commands::get_provider_availability(state, input).await
}

#[tauri::command]
pub async fn set_provider_availability(
    state: State<'_, SessionCommandState>,
    input: SetProviderAvailabilityInput,
) -> Result<ProviderAvailabilityOutput, String> {
    dcc_tauri::commands::provider_commands::set_provider_availability(state, input).await
}

#[tauri::command]
pub async fn provider_account_usage(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountUsageInput,
) -> Result<ProviderAccountUsageOutput, String> {
    dcc_tauri::commands::provider_commands::provider_account_usage(state, input).await
}
