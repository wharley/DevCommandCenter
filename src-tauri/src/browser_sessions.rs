//! Explicit, local import of an existing macOS Chromium profile into DCC's
//! WebKit cookie store. Discovery never opens a cookie database or Keychain.

use serde::Serialize;
use tauri::{State, Webview, Wry};

use crate::browser_commands::BrowserState;
pub(crate) use dcc_infra::browser_sessions::PreparedSessionCookie;
use dcc_infra::browser_sessions::{list_profiles, prepare_import, BrowserSessionProfile};

const BROWSER_WEBVIEW_LABEL: &str = "main";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionProfilesResult {
    pub profiles: Vec<BrowserSessionProfile>,
    pub supported: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BrowserSessionImportInput {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub lifecycle_token: u64,
    pub profile_id: String,
    /// The URL currently displayed in the DCC Browser. Only cookies whose
    /// domain can be sent to this host are considered.
    pub current_url: String,
}

#[cfg(target_os = "macos")]
mod native_cookie_store {
    use super::PreparedSessionCookie;
    use std::{
        collections::HashMap,
        ffi::{c_char, c_void, CString},
        sync::{
            atomic::{AtomicU64, Ordering},
            Mutex, OnceLock,
        },
    };
    use tauri::{Webview, Wry};
    use tokio::sync::oneshot;
    #[repr(C)]
    struct NativeCookie {
        name: *const c_char,
        value: *const c_char,
        domain: *const c_char,
        path: *const c_char,
        secure: i32,
        http_only: i32,
        same_site: i32,
        expires: i64,
    }
    type ResultCounts = Result<(usize, usize), String>;
    type Pending = HashMap<u64, oneshot::Sender<ResultCounts>>;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    fn pending() -> &'static Mutex<Pending> {
        PENDING.get_or_init(|| Mutex::new(HashMap::new()))
    }
    struct PendingGuard(u64);
    impl Drop for PendingGuard {
        fn drop(&mut self) {
            if let Ok(mut pending) = pending().lock() {
                pending.remove(&self.0);
            }
        }
    }
    unsafe extern "C" {
        fn dcc_browser_set_session_cookies(
            view: *mut c_void,
            request: u64,
            items: *const NativeCookie,
            count: usize,
            callback: extern "C" fn(u64, usize, usize, *const c_char),
        );
    }
    extern "C" fn done(request: u64, imported: usize, skipped: usize, error: *const c_char) {
        if let Some(sender) = pending()
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request))
        {
            let result = if error.is_null() {
                Ok((imported, skipped))
            } else {
                Err("Browser cookie store could not apply the session".into())
            };
            let _ = sender.send(result);
        }
    }
    pub async fn apply(view: Webview<Wry>, cookies: Vec<PreparedSessionCookie>) -> ResultCounts {
        if cookies.is_empty() {
            return Ok((0, 0));
        }
        if cookies.len() > 200 {
            return Err("Browser session exceeds the import limit".into());
        }
        let strings: Result<Vec<_>, std::ffi::NulError> = cookies
            .iter()
            .map(|cookie| {
                Ok((
                    CString::new(cookie.name.as_str())?,
                    CString::new(cookie.value.as_str())?,
                    CString::new(cookie.domain.as_str())?,
                    CString::new(cookie.path.as_str())?,
                ))
            })
            .collect();
        let strings = strings.map_err(|_| "Browser cookie contains invalid text")?;
        let request = NEXT.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        pending()
            .lock()
            .map_err(|_| "Browser cookie import is unavailable")?
            .insert(request, sender);
        let _guard = PendingGuard(request);
        view.with_webview(move |view| {
            // Owned CStrings stay alive until Objective-C has copied every field.
            let native: Vec<_> = cookies
                .iter()
                .zip(&strings)
                .map(|(cookie, strings)| NativeCookie {
                    name: strings.0.as_ptr(),
                    value: strings.1.as_ptr(),
                    domain: strings.2.as_ptr(),
                    path: strings.3.as_ptr(),
                    secure: cookie.secure as i32,
                    http_only: cookie.http_only as i32,
                    same_site: cookie.same_site as i32,
                    expires: cookie.expires_unix_seconds.unwrap_or(-1),
                })
                .collect();
            unsafe {
                dcc_browser_set_session_cookies(
                    view.inner(),
                    request,
                    native.as_ptr(),
                    native.len(),
                    done,
                );
            }
        })
        .map_err(|_| "Browser cookie store is unavailable")?;
        tokio::time::timeout(std::time::Duration::from_secs(8), receiver)
            .await
            .map_err(|_| "Browser cookie import timed out")?
            .map_err(|_| "Browser cookie import was cancelled")?
    }
}

pub(crate) async fn apply_prepared_cookies(
    view: Webview<Wry>,
    cookies: Vec<PreparedSessionCookie>,
) -> Result<(usize, usize), String> {
    #[cfg(target_os = "macos")]
    {
        native_cookie_store::apply(view, cookies).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (view, cookies);
        Err("Session import is currently available on macOS only.".to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub supported: bool,
    pub message: Option<String>,
}

/// Safe metadata-only discovery. It intentionally neither opens the Cookies
/// SQLite DB nor asks macOS Keychain for access.
#[tauri::command]
pub fn browser_session_profiles(webview: Webview<Wry>) -> BrowserSessionProfilesResult {
    if webview.label() != BROWSER_WEBVIEW_LABEL {
        return BrowserSessionProfilesResult {
            profiles: Vec::new(),
            supported: false,
            message: Some("Session import must be requested from the DCC window.".to_string()),
        };
    }
    #[cfg(target_os = "macos")]
    {
        BrowserSessionProfilesResult {
            profiles: list_profiles(),
            supported: true,
            message: None,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        BrowserSessionProfilesResult {
            profiles: Vec::new(),
            supported: false,
            message: Some("Session import is currently available on macOS only.".to_string()),
        }
    }
}

#[tauri::command]
pub async fn browser_session_import(
    webview: Webview<Wry>,
    browser: State<'_, BrowserState>,
    workspace_id: String,
    session_id: Option<String>,
    lifecycle_token: u64,
    profile_id: String,
    current_url: String,
) -> Result<BrowserSessionImportResult, String> {
    if webview.label() != BROWSER_WEBVIEW_LABEL {
        return Err("Session import must be requested from the DCC window.".to_string());
    }
    let input = BrowserSessionImportInput {
        workspace_id,
        session_id,
        lifecycle_token,
        profile_id,
        current_url,
    };
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (browser, input);
        return Ok(BrowserSessionImportResult {
            imported: 0,
            skipped: 0,
            supported: false,
            message: Some("Session import is currently available on macOS only.".to_string()),
        });
    }
    #[cfg(target_os = "macos")]
    {
        let browser = browser.inner().clone();
        // Validate the caller's native Browser surface before the potentially
        // interactive Keychain request, but never hold its operation lock while
        // the user responds to that dialog.
        let (_, expected_revision) = crate::browser_commands::browser_session_import_target(
            &browser,
            &input.workspace_id,
            input.session_id.as_deref(),
            input.lifecycle_token,
            &input.current_url,
        )
        .await?;
        let preparation_input = input.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_import(
                &preparation_input.profile_id,
                &preparation_input.current_url,
            )
        })
        .await
        .map_err(|_| "Session import preparation was interrupted.".to_string())??;
        let (imported, apply_skipped) = crate::browser_commands::apply_browser_session_cookies(
            &browser,
            &input.workspace_id,
            input.session_id.as_deref(),
            input.lifecycle_token,
            &input.current_url,
            expected_revision,
            prepared.cookies,
        )
        .await?;
        Ok(BrowserSessionImportResult {
            imported,
            skipped: prepared.skipped + apply_skipped,
            supported: true,
            message: None,
        })
    }
}
