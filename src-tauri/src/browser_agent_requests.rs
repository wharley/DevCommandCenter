//! The agents' Browser requests as Tauri commands for the React app; the
//! queue is `dcc-server`'s.

pub use dev_command_center_tauri::browser_agent_requests::*;

#[allow(unused_imports)]
use dcc_tauri::state::SessionCommandState;
use dev_command_center_tauri::browser_commands::BrowserState;
use tauri::State;

#[tauri::command]
pub fn browser_agent_pending(
    broker: State<'_, BrowserAgentRequestBroker>,
    input: BrowserAgentPendingInput,
) -> Vec<BrowserAgentRequestView> {
    dev_command_center_tauri::browser_agent_requests::browser_agent_pending(&broker, input)
}

#[tauri::command]
pub fn browser_agent_pending_all(
    broker: State<'_, BrowserAgentRequestBroker>,
) -> Vec<BrowserAgentRequestView> {
    dev_command_center_tauri::browser_agent_requests::browser_agent_pending_all(&broker)
}

#[tauri::command]
pub fn browser_agent_resolve(
    broker: State<'_, BrowserAgentRequestBroker>,
    browser: State<'_, BrowserState>,
    requests: State<'_, crate::desktop_agent_requests::DesktopAgentRequests>,
    input: BrowserAgentResolveInput,
) -> BrowserAgentOpenResult {
    dev_command_center_tauri::browser_agent_requests::browser_agent_resolve(
        &broker, &browser, &requests, input,
    )
}

#[tauri::command]
pub fn browser_agent_cancel(
    broker: State<'_, BrowserAgentRequestBroker>,
    input: BrowserAgentCancelInput,
) -> bool {
    dev_command_center_tauri::browser_agent_requests::browser_agent_cancel(&broker, input)
}
