//! Computer use's Tauri commands for the React app; the work is `dcc-server`'s.

pub use dev_command_center_tauri::computer_use_commands::*;

#[allow(unused_imports)]
use dcc_tauri::state::SessionCommandState;
use tauri::State;

#[tauri::command]
pub fn computer_use_status(
    state: State<'_, ComputerUseState>,
    input: ComputerUseStatusInput,
) -> ComputerUseStatus {
    dev_command_center_tauri::computer_use_commands::computer_use_status(&state, input)
}

#[tauri::command]
pub fn computer_use_arm(
    app: tauri::AppHandle,
    state: State<'_, ComputerUseState>,
    input: ComputerUseArmInput,
) -> Result<ComputerGrantStatus, String> {
    dev_command_center_tauri::computer_use_commands::computer_use_arm(
        &crate::named_events(&app),
        &state,
        input,
    )
}

#[tauri::command]
pub fn computer_use_disarm(
    app: tauri::AppHandle,
    state: State<'_, ComputerUseState>,
    input: ComputerUseDisarmInput,
) -> ComputerGrantStatus {
    dev_command_center_tauri::computer_use_commands::computer_use_disarm(
        &crate::named_events(&app),
        &state,
        input,
    )
}

#[tauri::command]
pub fn computer_use_request_access(
    state: State<'_, ComputerUseState>,
    input: ComputerUseRequestAccessInput,
) -> Result<ComputerUseStatus, String> {
    dev_command_center_tauri::computer_use_commands::computer_use_request_access(&state, input)
}

#[tauri::command]
pub fn computer_use_list_pending_requests(
    state: State<'_, ComputerUseState>,
) -> ComputerUsePendingRequestsOutput {
    dev_command_center_tauri::computer_use_commands::computer_use_list_pending_requests(&state)
}

#[tauri::command]
pub async fn computer_use_respond_control_request(
    requests: State<'_, crate::desktop_agent_requests::DesktopAgentRequests>,
    input: ComputerUseRespondControlRequestInput,
) -> Result<ComputerUseRespondControlRequestOutput, String> {
    dev_command_center_tauri::computer_use_commands::computer_use_respond_control_request(
        &requests, input,
    )
    .await
}

#[tauri::command]
pub fn computer_use_preview_frame(
    state: State<'_, ComputerUseState>,
    input: ComputerUsePreviewFrameInput,
) -> Option<ComputerUsePreviewFrame> {
    dev_command_center_tauri::computer_use_commands::computer_use_preview_frame(&state, input)
}

#[tauri::command]
pub async fn computer_use_refresh_preview_frame(
    state: State<'_, ComputerUseState>,
    input: ComputerUsePreviewFrameInput,
) -> Result<Option<ComputerUsePreviewFrame>, String> {
    dev_command_center_tauri::computer_use_commands::computer_use_refresh_preview_frame(
        &state, input,
    )
    .await
}
