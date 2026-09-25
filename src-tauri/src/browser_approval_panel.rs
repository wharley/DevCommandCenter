//! Background approval presentation. The broker remains authoritative and an
//! allow still requires the main renderer's native Browser lifecycle ACK.
use serde::Deserialize;
use tauri::{AppHandle, Webview};

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
    title: String,
    description: String,
    destination: String,
    conversation: String,
    provider: String,
    reason: String,
    remaining: String,
    allow: String,
    deny: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    workspace_id: String,
    session_id: String,
    title: String,
}

#[tauri::command]
pub fn browser_approval_panel_configure(
    window: Webview,
    app: AppHandle,
    labels: Labels,
    conversations: Vec<Conversation>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("unavailable".into());
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        let state = app.state::<macos::PanelState>();
        *state.config.lock().map_err(|e| e.to_string())? = Some((labels, conversations));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, labels, conversations);
    Ok(())
}

#[tauri::command]
pub fn browser_approval_panel_dismiss(
    window: Webview,
    app: AppHandle,
    request_id: String,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("unavailable".into());
    }
    #[cfg(target_os = "macos")]
    macos::suppress(&app, &request_id);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, request_id);
    Ok(())
}

pub fn setup(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    macos::setup(app);
    let _ = app;
}

pub fn shutdown(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    macos::shutdown(app);
    let _ = app;
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::browser_agent_requests::{
        BrowserAgentRequestBroker, BrowserAgentRequestDecision, BrowserAgentResolveInput,
    };
    use crate::browser_commands::BrowserState;
    use std::{
        collections::HashSet,
        ffi::{c_char, CStr, CString},
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex, OnceLock,
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use tauri::{Emitter, Manager};

    static APP: OnceLock<AppHandle> = OnceLock::new();
    #[derive(Default)]
    pub(super) struct PanelState {
        pub config: Mutex<Option<(Labels, Vec<Conversation>)>>,
        suppressed: Mutex<HashSet<String>>,
        stopped: AtomicBool,
    }
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    pub(super) fn suppress(app: &AppHandle, id: &str) {
        if let Ok(mut ids) = app.state::<PanelState>().suppressed.lock() {
            ids.insert(id.to_owned());
        }
    }
    extern "C" fn decide(id: *const c_char, allowed: bool) {
        if id.is_null() {
            return;
        }
        let Some(app) = APP.get() else {
            return;
        };
        let id = unsafe { CStr::from_ptr(id) }.to_string_lossy();
        let broker = app.state::<BrowserAgentRequestBroker>();
        let Some(request) = broker
            .pending_all()
            .into_iter()
            .find(|r| r.request_id == id && r.expires_at_ms > now_ms())
        else {
            return;
        };
        suppress(app, &id);
        if allowed {
            // Open only the integrated Browser. Preserve the main window's
            // hidden/minimized state and leave focus with the user's other app.
            // The renderer rechecks the request and completes the lifecycle ACK.
            let _ = app.emit_to("main", "browser-approval-panel-allow", &request.request_id);
        } else {
            broker.resolve(
                &app.state::<BrowserState>(),
                BrowserAgentResolveInput {
                    request_id: request.request_id,
                    workspace_id: request.workspace_id,
                    session_id: request.session_id,
                    decision: BrowserAgentRequestDecision::Deny,
                    lifecycle_token: None,
                },
            );
        }
    }
    pub(super) fn setup(app: &AppHandle) {
        let _ = APP.set(app.clone());
        app.manage(PanelState::default());
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if app.state::<PanelState>().stopped.load(Ordering::Relaxed) {
                    break;
                }
                let handle = app.clone();
                // Read the broker on the UI thread, immediately before presentation.
                // No hidden WebView timer is required to discover or expire requests.
                let _ = app.run_on_main_thread(move || refresh(&handle));
            }
        });
    }
    fn refresh(app: &AppHandle) {
        let state = app.state::<PanelState>();
        if state.stopped.load(Ordering::Relaxed) {
            return;
        }
        let pending = app.state::<BrowserAgentRequestBroker>().pending_all();
        let Ok(mut suppressed) = state.suppressed.lock() else {
            return;
        };
        suppressed.retain(|id| pending.iter().any(|r| &r.request_id == id));
        // Keep the same queue order as the in-app modal; do not skip an opening request.
        let request = pending
            .first()
            .filter(|r| r.expires_at_ms > now_ms() && !suppressed.contains(&r.request_id));
        let config = request.and_then(|_| state.config.lock().ok().and_then(|value| value.clone()));
        let Some((request, (labels, conversations))) = request.zip(config) else {
            unsafe {
                dcc_browser_approval_update(std::ptr::null_mut(), std::ptr::null(), decide);
            }
            return;
        };
        let conversation = conversations
            .iter()
            .find(|c| c.workspace_id == request.workspace_id && c.session_id == request.session_id)
            .map(|c| c.title.as_str())
            .unwrap_or(&request.session_id);
        let payload = serde_json::json!({ "request": request, "labels": labels, "conversation": conversation });
        let Ok(payload) = CString::new(payload.to_string()) else {
            return;
        };
        let Some(main) = app.get_webview_window("main") else {
            return;
        };
        let Ok(window) = main.ns_window() else {
            return;
        };
        unsafe {
            dcc_browser_approval_update(window, payload.as_ptr(), decide);
        }
    }
    pub(super) fn shutdown(app: &AppHandle) {
        if let Some(state) = app.try_state::<PanelState>() {
            state.stopped.store(true, Ordering::Relaxed);
        }
        unsafe {
            dcc_browser_approval_destroy();
        }
    }
    extern "C" {
        fn dcc_browser_approval_update(
            main: *mut std::ffi::c_void,
            payload: *const c_char,
            callback: extern "C" fn(*const c_char, bool),
        );
        fn dcc_browser_approval_destroy();
    }
}
