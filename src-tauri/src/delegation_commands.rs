use tauri::{AppHandle, State};

use dcc_tauri::{
    commands::delegation_commands::{
        self as delegation_command_impl, CancelDelegationInput, CancelDelegationOutput,
        CreateDelegationInput, CreateDelegationOutput, GetDelegationInput, GetDelegationOutput,
        ListDelegationsInput, ListDelegationsOutput,
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
