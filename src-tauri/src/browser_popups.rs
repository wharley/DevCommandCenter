//! Native windows opened by the human-facing Browser surface.
//!
//! A remote page may request a popup during an OAuth flow.  We create a
//! separate, ordinary native window instead of navigating the privileged
//! Browser child view.  The registry makes each popup disposable with the
//! Browser lifecycle that created it.

use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::ffi::{CStr, CString};
#[cfg(target_os = "macos")]
use std::os::raw::c_char;
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::webview::{DownloadEvent, NewWindowFeatures, NewWindowResponse};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry};
use url::Url;

/// A stable, browser-only WebKit data-store identity.  It is deliberately
/// different from the app webview's default store, while all Browser views
/// (including popups) use this one identity and therefore share login state.
/// Wry uses it for persistent named data stores on macOS 14 and newer.
pub(crate) const BROWSER_DATA_STORE_IDENTIFIER: [u8; 16] = [
    0x71, 0x92, 0x4b, 0x2c, 0x8d, 0x51, 0x4a, 0xde, 0xb4, 0xc7, 0x19, 0x4f, 0x33, 0x80, 0x5d, 0xa1,
];

const POPUP_TITLE: &str = "Browser sign-in";
const MAX_BROWSER_POPUPS: usize = 4;
#[cfg(target_os = "macos")]
static DOWNLOAD_SAVE_PANEL_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupScope {
    scope: String,
    lifecycle_token: u64,
}

#[derive(Clone, Default)]
pub(crate) struct BrowserPopupRegistry {
    labels: Arc<Mutex<HashMap<String, PopupScope>>>,
    next_label: Arc<AtomicU64>,
}

impl BrowserPopupRegistry {
    fn reserve_label(&self, scope: String, lifecycle_token: u64) -> Option<String> {
        let serial = self.next_label.fetch_add(1, Ordering::Relaxed);
        let label = format!("dcc-browser-popup-{lifecycle_token}-{serial}");
        let mut labels = self.labels.lock().ok()?;
        if labels.len() >= MAX_BROWSER_POPUPS {
            return None;
        }
        labels.insert(
            label.clone(),
            PopupScope {
                scope,
                lifecycle_token,
            },
        );
        Some(label)
    }

    fn remove(&self, label: &str) {
        if let Ok(mut labels) = self.labels.lock() {
            labels.remove(label);
        }
    }

    fn take_matching(&self, scope: Option<&str>, lifecycle_token: Option<u64>) -> Vec<String> {
        let Ok(mut labels) = self.labels.lock() else {
            return Vec::new();
        };
        let selected = labels
            .iter()
            .filter(|(_, popup)| {
                scope.is_none_or(|scope| popup.scope == scope)
                    && lifecycle_token.is_none_or(|token| popup.lifecycle_token == token)
            })
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>();
        for label in &selected {
            labels.remove(label);
        }
        selected
    }

    #[cfg(test)]
    fn entries(&self) -> usize {
        self.labels
            .lock()
            .map(|labels| labels.len())
            .unwrap_or_default()
    }
}

pub(crate) fn browser_popup_registry() -> BrowserPopupRegistry {
    static REGISTRY: OnceLock<BrowserPopupRegistry> = OnceLock::new();
    REGISTRY.get_or_init(BrowserPopupRegistry::default).clone()
}

/// The non-WebKit fallback profile is still private to Browser views. The
/// parent Browser creates it before any popup needs this path.
#[cfg(not(target_os = "macos"))]
pub(crate) fn browser_profile_data_dir(app: &AppHandle<Wry>) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|path| path.join("browser-profile"))
}

/// Wry maps a named store to `WKWebsiteDataStore::dataStoreForIdentifier`,
/// which Apple introduced in macOS 14. Older releases use a nonpersistent
/// Browser-only store; they never fall back to the app's default profile.
#[cfg(target_os = "macos")]
pub(crate) fn named_browser_store_is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|version| {
                version
                    .trim()
                    .split('.')
                    .next()
                    .and_then(|major| major.parse::<u32>().ok())
            })
            .is_some_and(|major| major >= 14)
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn named_browser_store_is_available() -> bool {
    false
}

/// The Browser only loads network pages. `about:blank` is allowed exactly as
/// a popup's initial document because providers commonly create a blank
/// opener before assigning its authorization URL.
pub(crate) fn is_initial_popup_url_allowed(url: &Url) -> bool {
    url.as_str() == "about:blank"
        || crate::browser_commands::validate_browser_url(url.as_str()).is_ok()
}

fn is_popup_navigation_allowed(url: &Url, may_use_initial_blank: &AtomicBool) -> bool {
    if url.as_str() == "about:blank" {
        return may_use_initial_blank
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
    }
    if crate::browser_commands::validate_browser_url(url.as_str()).is_ok() {
        may_use_initial_blank.store(false, Ordering::Release);
        return true;
    }
    false
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn dcc_browser_choose_download_path(suggested_filename: *const c_char) -> *mut c_char;
    fn dcc_browser_free_download_path(path: *mut c_char);
}

/// Wry invokes the macOS download delegate on AppKit's main thread.  The
/// native implementation shows an `NSSavePanel` synchronously, which is the
/// only point at which WebKit accepts a download destination.
#[cfg(target_os = "macos")]
pub(crate) fn choose_download_destination(suggested: &Path) -> Option<PathBuf> {
    if DOWNLOAD_SAVE_PANEL_ACTIVE.swap(true, Ordering::AcqRel) {
        return None;
    }
    struct SavePanelGuard;
    impl Drop for SavePanelGuard {
        fn drop(&mut self) {
            DOWNLOAD_SAVE_PANEL_ACTIVE.store(false, Ordering::Release);
        }
    }
    let _guard = SavePanelGuard;
    let filename = suggested
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("download");
    let filename = CString::new(filename).ok()?;
    // SAFETY: the native helper returns either null or an owned `strdup`
    // allocation and provides the matching release function.
    let selected = unsafe { dcc_browser_choose_download_path(filename.as_ptr()) };
    if selected.is_null() {
        return None;
    }
    // SAFETY: `selected` is non-null and valid until released below.
    let path = unsafe { CStr::from_ptr(selected) }
        .to_str()
        .ok()
        .map(PathBuf::from);
    // SAFETY: this releases precisely the allocation returned above.
    unsafe { dcc_browser_free_download_path(selected) };
    path
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn choose_download_destination(
    _suggested: &std::path::Path,
) -> Option<std::path::PathBuf> {
    None
}

/// Creates an owned native popup for a Browser `window.open` request.
///
/// The caller supplies the scope and lifecycle copied from the Browser state
/// at request time. The window has no app-specific title or injected browser
/// controls, and every download requires an AppKit save-location choice.
pub(crate) fn open_browser_popup(
    app: &AppHandle<Wry>,
    parent: &tauri::WebviewWindow<Wry>,
    registry: BrowserPopupRegistry,
    scope: String,
    lifecycle_token: u64,
    url: Url,
    features: NewWindowFeatures,
    is_current: impl Fn(&str, u64) -> bool,
) -> NewWindowResponse<Wry> {
    if !is_current(&scope, lifecycle_token) || !is_initial_popup_url_allowed(&url) {
        return NewWindowResponse::Deny;
    }
    let Some(label) = registry.reserve_label(scope.clone(), lifecycle_token) else {
        return NewWindowResponse::Deny;
    };

    #[cfg(target_os = "macos")]
    let target_configuration = {
        let configuration = features.opener().target_configuration.clone();
        // WebKit supplies a target configuration to preserve `window.opener`,
        // but it does not carry our named website-data-store. Copy that store
        // from the Browser opener before passing the configuration to Tauri.
        // SAFETY: both objects are retained WebKit objects owned on the native
        // callback's main thread, and this is WebKit's documented setter.
        unsafe {
            let opener_configuration = features.opener().webview.configuration();
            let data_store = opener_configuration.websiteDataStore();
            configuration.setWebsiteDataStore(&data_store);
        }
        configuration
    };
    #[cfg(windows)]
    let target_environment = features.opener().environment.clone();
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let related_view = features.opener().webview.clone();
    let may_use_initial_blank = Arc::new(AtomicBool::new(url.as_str() == "about:blank"));
    let navigation_blank = may_use_initial_blank.clone();
    let registry_on_close = registry.clone();
    let label_on_close = label.clone();

    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(POPUP_TITLE)
        .window_features(features)
        .on_navigation(move |navigation| {
            is_popup_navigation_allowed(navigation, navigation_blank.as_ref())
        })
        .on_download(|_, event| match event {
            DownloadEvent::Requested { destination, .. } => {
                let Some(path) = choose_download_destination(destination) else {
                    return false;
                };
                *destination = path;
                true
            }
            DownloadEvent::Finished { .. } => true,
            _ => false,
        });
    #[cfg(target_os = "macos")]
    let builder = builder.with_webview_configuration(target_configuration);
    #[cfg(windows)]
    let builder = builder.with_environment(target_environment);
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let builder = builder.with_related_view(related_view);
    #[cfg(not(target_os = "macos"))]
    let builder = match browser_profile_data_dir(app) {
        Some(path) => builder.data_directory(path),
        None => {
            registry.remove(&label);
            return NewWindowResponse::Deny;
        }
    };
    let builder = match builder.parent(parent) {
        Ok(builder) => builder,
        Err(_) => {
            registry.remove(&label);
            return NewWindowResponse::Deny;
        }
    };
    let window = match builder.build() {
        Ok(window) => window,
        Err(_) => {
            registry.remove(&label);
            return NewWindowResponse::Deny;
        }
    };
    // The native build can re-enter the event loop. A scope switch or Browser
    // close while it is being created must not leave an orphaned login window.
    if !is_current(&scope, lifecycle_token) {
        let _ = window.destroy();
        registry.remove(&label);
        return NewWindowResponse::Deny;
    }
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            registry_on_close.remove(&label_on_close);
        }
    });
    NewWindowResponse::Create { window }
}

/// Revoke popups from exactly one Browser lifecycle. Call this before a
/// scope/token changes and whenever the Browser surface closes.
pub(crate) fn close_browser_popups_for_lifecycle(
    app: &AppHandle<Wry>,
    scope: &str,
    lifecycle_token: u64,
) {
    for label in browser_popup_registry().take_matching(Some(scope), Some(lifecycle_token)) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    }
}

/// Revoke every Browser popup during app shutdown.
pub(crate) fn close_all_browser_popups(app: &AppHandle<Wry>) {
    for label in browser_popup_registry().take_matching(None, None) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_initial_popup_url_allowed, is_popup_navigation_allowed, BrowserPopupRegistry,
        BROWSER_DATA_STORE_IDENTIFIER,
    };
    use std::sync::atomic::AtomicBool;
    use url::Url;

    #[test]
    fn popup_uses_a_stable_non_default_browser_store_identity() {
        assert_ne!(BROWSER_DATA_STORE_IDENTIFIER, [0; 16]);
        assert_eq!(BROWSER_DATA_STORE_IDENTIFIER.len(), 16);
    }

    #[test]
    fn initial_blank_is_allowed_once_then_only_network_navigation_is_allowed() {
        let blank = Url::parse("about:blank").unwrap();
        let https = Url::parse("https://accounts.example.test/authorize").unwrap();
        let non_local_http = Url::parse("http://accounts.example.test/authorize").unwrap();
        let file = Url::parse("file:///tmp/credential.txt").unwrap();
        let initial_blank = AtomicBool::new(true);
        assert!(is_initial_popup_url_allowed(&blank));
        assert!(!is_initial_popup_url_allowed(&non_local_http));
        assert!(is_popup_navigation_allowed(&blank, &initial_blank));
        assert!(!is_popup_navigation_allowed(&blank, &initial_blank));
        assert!(is_popup_navigation_allowed(&https, &initial_blank));
        assert!(!is_popup_navigation_allowed(&file, &initial_blank));
    }

    #[test]
    fn lifecycle_registry_keeps_popups_shared_until_that_lifecycle_is_revoked() {
        let registry = BrowserPopupRegistry::default();
        let first = registry
            .reserve_label("workspace\u{1f}|session".into(), 7)
            .unwrap();
        let second = registry
            .reserve_label("workspace\u{1f}|session".into(), 7)
            .unwrap();
        let other = registry
            .reserve_label("workspace\u{1f}|session".into(), 8)
            .unwrap();
        assert_eq!(registry.entries(), 3);
        let revoked = registry.take_matching(Some("workspace\u{1f}|session"), Some(7));
        assert_eq!(revoked.len(), 2);
        assert!(revoked.contains(&first));
        assert!(revoked.contains(&second));
        assert_eq!(registry.entries(), 1);
        assert_eq!(registry.take_matching(None, None), vec![other]);
    }
}
