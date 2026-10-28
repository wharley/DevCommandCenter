//! The agents' requests as Tauri commands for the React app (the queue is
//! `dcc-server`'s), and calling for the person from the desktop's window.

pub use dev_command_center_tauri::desktop_agent_requests::*;

#[allow(unused_imports)]
use dcc_tauri::state::SessionCommandState;
use dev_command_center_tauri::browser_agent_requests::BrowserAgentRequestView;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn browser_agent_claim(
    requests: State<'_, DesktopAgentRequests>,
    input: BrowserAgentClaimInput,
) -> Result<BrowserAgentRequestView, String> {
    dev_command_center_tauri::desktop_agent_requests::browser_agent_claim(&requests, input)
}

/// Gets the person's attention for an agent request waiting on them, only
/// when DCC is not the window they are looking at: the Dock icon bounces and
/// a system notification names it. The renderer passes the text, already in
/// the person's language. While the native app answers the requests it
/// calls for the person itself (one notification, not two).
#[tauri::command]
pub fn agent_request_attention(app: AppHandle, title: String, body: String) {
    use dev_command_center_tauri::http_api::{native_browser_opens, native_computer};
    use tauri::Manager;
    use tauri_plugin_notification::NotificationExt;
    if native_browser_opens().connected() || native_computer().connected() {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_focused().unwrap_or(false) {
        return;
    }
    let _ = window.request_user_attention(Some(tauri::UserAttentionType::Informational));
    let title: String = title.chars().take(120).collect();
    let body: String = body.chars().take(240).collect();
    let _ = app.notification().builder().title(title).body(body).show();
}
