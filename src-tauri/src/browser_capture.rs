//! Capture only the integrated Browser viewport. Authorization belongs to the
//! BrowserState caller and must be checked again after the asynchronous capture.
#[cfg(target_os = "macos")]
mod native {
    use std::{
        collections::HashMap,
        ffi::{c_char, c_void},
        sync::{
            atomic::{AtomicU64, Ordering},
            Mutex, OnceLock,
        },
        time::Duration,
    };
    use tauri::{Webview, Wry};
    use tokio::sync::oneshot;

    type SnapshotResult = Result<Vec<u8>, String>;
    type Pending = HashMap<u64, oneshot::Sender<SnapshotResult>>;
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
    fn pending() -> &'static Mutex<Pending> {
        PENDING.get_or_init(|| Mutex::new(HashMap::new()))
    }

    unsafe extern "C" {
        fn dcc_browser_snapshot(
            webview: *mut c_void,
            request: u64,
            callback: extern "C" fn(u64, *const u8, usize, *const c_char),
        );
    }
    extern "C" fn completed(request: u64, bytes: *const u8, length: usize, error: *const c_char) {
        let sender = pending()
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request));
        let Some(sender) = sender else {
            return;
        };
        let result =
            if !error.is_null() || bytes.is_null() || length == 0 || length > 4 * 1024 * 1024 {
                Err("Browser snapshot could not be captured".to_string())
            } else {
                // Native data remains alive throughout this synchronous callback.
                Ok(unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec())
            };
        let _ = sender.send(result);
    }
    struct PendingGuard(u64);
    impl Drop for PendingGuard {
        fn drop(&mut self) {
            if let Ok(mut pending) = pending().lock() {
                pending.remove(&self.0);
            }
        }
    }
    pub async fn capture(webview: Webview<Wry>) -> SnapshotResult {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        pending()
            .lock()
            .map_err(|_| "Browser snapshot lock failed".to_string())?
            .insert(request, sender);
        let _guard = PendingGuard(request);
        webview
            .with_webview(move |webview| unsafe {
                dcc_browser_snapshot(webview.inner(), request, completed)
            })
            .map_err(|_| "Browser viewport is unavailable".to_string())?;
        tokio::time::timeout(Duration::from_secs(8), receiver)
            .await
            .map_err(|_| "Browser snapshot timed out".to_string())?
            .map_err(|_| "Browser snapshot was cancelled".to_string())?
    }
}

pub async fn capture(webview: tauri::Webview<tauri::Wry>) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "macos")]
    {
        native::capture(webview).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = webview;
        Err("Browser screenshots are not supported on this platform".to_string())
    }
}
