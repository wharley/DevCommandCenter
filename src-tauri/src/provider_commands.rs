use dcc_tauri::commands::provider_commands::{
    AntigravityStatusInput, AntigravityStatusOutput, ConnectAntigravityInput,
    ConnectAntigravityOutput, InstallAntigravityOutput, ListProvidersOutput,
    ProviderAccountUsageInput, ProviderAccountUsageOutput, ProviderAvailabilityInput,
    ProviderAvailabilityOutput, SetProviderAvailabilityInput,
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

#[tauri::command]
pub async fn install_antigravity(
    state: State<'_, SessionCommandState>,
) -> Result<InstallAntigravityOutput, String> {
    dcc_tauri::commands::provider_commands::install_antigravity(state).await
}

#[tauri::command]
pub async fn get_antigravity_status(
    state: State<'_, SessionCommandState>,
    input: AntigravityStatusInput,
) -> Result<AntigravityStatusOutput, String> {
    dcc_tauri::commands::provider_commands::get_antigravity_status(state, input).await
}

#[tauri::command]
pub async fn connect_antigravity(
    state: State<'_, SessionCommandState>,
    input: ConnectAntigravityInput,
) -> Result<ConnectAntigravityOutput, String> {
    dcc_tauri::commands::provider_commands::connect_antigravity(state, input).await
}
