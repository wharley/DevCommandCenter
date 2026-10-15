use dcc_tauri::commands::provider_commands::{
    AntigravityStatusInput, AntigravityStatusOutput, CheckProviderExecutableInput,
    CheckProviderExecutableOutput, ConnectAntigravityInput,
    ConnectAntigravityOutput, CreateProviderAccountInput, InstallAntigravityOutput,
    ListProviderAccountsOutput, ListProvidersOutput, ProviderAccountIdInput, ProviderAccountOutput,
    ProviderAccountResetInput, ProviderAccountResetOutput, ProviderAccountStatusInput,
    ProviderAccountStatusOutput, ProviderAccountUsageInput, ProviderAccountUsageOutput,
    ProviderAvailabilityInput, ProviderAvailabilityOutput, RenameProviderAccountInput,
    ProviderAccountSignInOutput, SetProviderAvailabilityInput,
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
pub async fn check_provider_executable(
    input: CheckProviderExecutableInput,
) -> Result<CheckProviderExecutableOutput, String> {
    Ok(dcc_tauri::commands::provider_commands::check_provider_executable(input))
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
pub async fn provider_account_reset(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountResetInput,
) -> Result<ProviderAccountResetOutput, String> {
    dcc_tauri::commands::provider_commands::provider_account_reset(state, input).await
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

#[tauri::command]
pub async fn list_provider_accounts(
    state: State<'_, SessionCommandState>,
) -> Result<ListProviderAccountsOutput, String> {
    dcc_tauri::commands::provider_commands::list_provider_accounts(state).await
}

#[tauri::command]
pub async fn create_provider_account(
    state: State<'_, SessionCommandState>,
    input: CreateProviderAccountInput,
) -> Result<ProviderAccountOutput, String> {
    dcc_tauri::commands::provider_commands::create_provider_account(state, input).await
}

#[tauri::command]
pub async fn rename_provider_account(
    state: State<'_, SessionCommandState>,
    input: RenameProviderAccountInput,
) -> Result<ProviderAccountOutput, String> {
    dcc_tauri::commands::provider_commands::rename_provider_account(state, input).await
}

#[tauri::command]
pub async fn remove_provider_account(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountIdInput,
) -> Result<(), String> {
    dcc_tauri::commands::provider_commands::remove_provider_account(state, input).await
}

#[tauri::command]
pub async fn provider_account_status(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountStatusInput,
) -> Result<ProviderAccountStatusOutput, String> {
    dcc_tauri::commands::provider_commands::provider_account_status(state, input).await
}

#[tauri::command]
pub async fn sign_in_provider_account(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountStatusInput,
) -> Result<ProviderAccountSignInOutput, String> {
    dcc_tauri::commands::provider_commands::sign_in_provider_account(state, input).await
}

#[tauri::command]
pub async fn cancel_provider_account_sign_in(
    input: ProviderAccountStatusInput,
) -> Result<(), String> {
    dcc_tauri::commands::provider_commands::cancel_provider_account_sign_in(input).await
}
