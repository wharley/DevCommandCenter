use tauri::{AppHandle, State};

use dcc_tauri::{
    commands::delegation_commands::{
        self as delegation_command_impl, CancelDelegationInput, CancelDelegationOutput,
        CompleteDelegationInput, CompleteDelegationOutput, CreateDelegationInput,
        CreateDelegationOutput, FailDelegationInput, FailDelegationOutput, GetDelegationInput,
        GetDelegationOutput, ListDelegationsInput, ListDelegationsOutput, StartDelegationInput,
        StartDelegationOutput,
    },
    state::SessionCommandState,
};

#[tauri::command]
pub async fn create_delegation(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: CreateDelegationInput,
) -> Result<CreateDelegationOutput, String> {
    delegation_command_impl::create_delegation(state, app, input).await
}

#[tauri::command]
pub async fn list_delegations(
    state: State<'_, SessionCommandState>,
    input: ListDelegationsInput,
) -> Result<ListDelegationsOutput, String> {
    delegation_command_impl::list_delegations(state, input).await
}

#[tauri::command]
pub async fn get_delegation(
    state: State<'_, SessionCommandState>,
    input: GetDelegationInput,
) -> Result<GetDelegationOutput, String> {
    delegation_command_impl::get_delegation(state, input).await
}

#[tauri::command]
pub async fn cancel_delegation(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: CancelDelegationInput,
) -> Result<CancelDelegationOutput, String> {
    delegation_command_impl::cancel_delegation(state, app, input).await
}

#[tauri::command]
pub async fn start_delegation(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: StartDelegationInput,
) -> Result<StartDelegationOutput, String> {
    delegation_command_impl::start_delegation(state, app, input).await
}

#[tauri::command]
pub async fn complete_delegation(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: CompleteDelegationInput,
) -> Result<CompleteDelegationOutput, String> {
    delegation_command_impl::complete_delegation(state, app, input).await
}

#[tauri::command]
pub async fn fail_delegation(
    state: State<'_, SessionCommandState>,
    app: AppHandle,
    input: FailDelegationInput,
) -> Result<FailDelegationOutput, String> {
    delegation_command_impl::fail_delegation(state, app, input).await
}
