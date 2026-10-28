//! The Browser's Tauri commands for the React app; the work is `dcc-server`'s.

pub use dev_command_center_tauri::browser_commands::*;

#[allow(unused_imports)]
use dcc_tauri::state::SessionCommandState;
use tauri::State;

#[tauri::command]
pub async fn browser_arm_control(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserControlStatus, String> {
    dev_command_center_tauri::browser_commands::browser_arm_control(
        &state,
        &sessions,
        workspace_id,
        session_id,
        lifecycle_token,
    )
    .await
}

#[tauri::command]
pub async fn browser_control_status(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserControlStatus, String> {
    dev_command_center_tauri::browser_commands::browser_control_status(
        &state,
        &sessions,
        workspace_id,
        session_id,
        lifecycle_token,
    )
    .await
}

#[tauri::command]
pub async fn browser_read_audit(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    limit: usize,
) -> Result<Vec<BrowserAuditViewRecord>, String> {
    dev_command_center_tauri::browser_commands::browser_read_audit(
        &state,
        &sessions,
        workspace_id,
        session_id,
        lifecycle_token,
        limit,
    )
    .await
}

#[tauri::command]
pub async fn browser_disarm_control(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    requests: State<'_, crate::browser_agent_requests::BrowserAgentRequestBroker>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserControlStatus, String> {
    dev_command_center_tauri::browser_commands::browser_disarm_control(
        &state,
        &sessions,
        &requests,
        workspace_id,
        session_id,
        lifecycle_token,
    )
    .await
}

#[tauri::command]
pub async fn browser_execute_action(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    anchor: BrowserActionAnchor,
    action: BrowserControlAction,
) -> Result<BrowserActionResult, String> {
    dev_command_center_tauri::browser_commands::browser_execute_action(
        &state, &sessions, anchor, action,
    )
    .await
}

#[tauri::command]
pub async fn browser_start_evidence_capture(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    anchor: BrowserActionAnchor,
) -> Result<BrowserEvidenceCaptureHandle, String> {
    dev_command_center_tauri::browser_commands::browser_start_evidence_capture(
        &state, &sessions, anchor,
    )
    .await
}

#[tauri::command]
pub async fn browser_read_evidence_capture(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    capture_id: String,
) -> Result<BrowserEvidenceResult, String> {
    dev_command_center_tauri::browser_commands::browser_read_evidence_capture(
        &state,
        &sessions,
        workspace_id,
        session_id,
        capture_id,
    )
    .await
}

#[tauri::command]
pub async fn browser_extract_context(
    state: State<'_, BrowserState>,
    sessions: State<'_, SessionCommandState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
) -> Result<BrowserAgentContext, String> {
    dev_command_center_tauri::browser_commands::browser_extract_context(
        &state,
        &sessions,
        workspace_id,
        session_id,
        lifecycle_token,
    )
    .await
}
