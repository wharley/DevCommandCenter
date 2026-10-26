//! Sign-in windows (`browser_popups.rs`): a page's `window.open` or
//! `target=_blank` (an OAuth button, a payment) opens "Browser sign-in", a
//! window of its own above the app's, with no DCC chrome. It keeps
//! `window.opener` (WebKit's target configuration) and the Browser's logins
//! (the opener's store, which WebKit does not carry over). At most four at
//! a time; one opens only on `about:blank` or an allowed URL; inside, the
//! blank page is allowed once, then only what the Browser allows. They
//! close with the page that opened them (the Browser closed, another task).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dcc_core::domain::browser::validate_browser_url;

/// `MAX_BROWSER_POPUPS`
pub const MAX_POPUPS: usize = 4;
/// `POPUP_TITLE`
pub const POPUP_TITLE: &str = "Browser sign-in";

/// Whether a window may open on this URL.
pub fn opens_on(url: &str) -> bool {
    url == "about:blank" || validate_browser_url(url).is_ok()
}

/// `is_popup_navigation_allowed`: the blank document once, as the first;
/// then only an allowed URL.
pub fn navigation_allowed(url: &str, blank_used: &Cell<bool>) -> bool {
    if url == "about:blank" {
        return !blank_used.replace(true);
    }
    blank_used.set(true);
    validate_browser_url(url).is_ok()
}

#[cfg(target_os = "macos")]
struct Popup {
    window: objc2::rc::Retained<objc2_app_kit::NSWindow>,
    _webview: wry::WebView,
}

/// The sign-in windows a page opened.
#[derive(Default, Clone)]
pub struct Popups {
    #[cfg(target_os = "macos")]
    open: Rc<RefCell<Vec<Popup>>>,
    #[cfg(not(target_os = "macos"))]
    open: Rc<RefCell<Vec<()>>>,
}

impl Popups {
    /// Every one of them closed (`close_browser_popups_for_lifecycle`).
    pub fn close_all(&self) {
        #[cfg(target_os = "macos")]
        for popup in self.open.borrow_mut().drain(..) {
            popup.window.close();
        }
        #[cfg(not(target_os = "macos"))]
        self.open.borrow_mut().clear();
    }

    /// The new-window request's answer.
    pub fn request(&self, url: String, features: wry::NewWindowFeatures) -> wry::NewWindowResponse {
        if !opens_on(&url) {
            return wry::NewWindowResponse::Deny;
        }
        #[cfg(target_os = "macos")]
        {
            self.open_macos(features)
                .unwrap_or(wry::NewWindowResponse::Deny)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = features;
            wry::NewWindowResponse::Deny
        }
    }

    #[cfg(target_os = "macos")]
    fn open_macos(&self, features: wry::NewWindowFeatures) -> Option<wry::NewWindowResponse> {
        use objc2::MainThreadMarker;
        use objc2::rc::Retained;
        use objc2_app_kit::{
            NSBackingStoreType, NSWindow, NSWindowOrderingMode, NSWindowStyleMask,
        };
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
        use wry::{WebViewBuilderExtMacos, WebViewExtMacOS};

        // Ones the person closed leave the count.
        self.open
            .borrow_mut()
            .retain(|popup| popup.window.isVisible());
        if self.open.borrow().len() >= MAX_POPUPS {
            return None;
        }
        let mtm = MainThreadMarker::new()?;
        let opener = features.opener;
        let configuration = opener.target_configuration;
        // The Browser's logins: WebKit hands a configuration without the
        // named store.
        unsafe {
            let store = opener.webview.configuration().websiteDataStore();
            configuration.setWebsiteDataStore(&store);
        }
        let parent = opener.webview.window();
        let size = features
            .size
            .map(|size| NSSize::new(size.width, size.height))
            .unwrap_or_else(|| NSSize::new(480.0, 640.0));
        let origin = match (features.position, parent.as_ref()) {
            (Some(position), _) => NSPoint::new(position.x, position.y),
            (None, Some(parent)) => {
                let frame = parent.frame();
                NSPoint::new(
                    frame.origin.x + (frame.size.width - size.width) / 2.0,
                    frame.origin.y + (frame.size.height - size.height) / 2.0,
                )
            }
            (None, None) => NSPoint::new(200.0, 200.0),
        };
        let mask = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSWindow>(),
                NSRect::new(origin, size),
                mask,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(POPUP_TITLE));
        let view = window.contentView()?;
        let handle = ContentView(std::ptr::NonNull::from(&*view).cast());
        let blank_used = Cell::new(false);
        let webview = wry::WebViewBuilder::new()
            .with_webview_configuration(configuration)
            .with_navigation_handler(move |url| navigation_allowed(&url, &blank_used))
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_download_started_handler(|_, destination| super::download::requested(destination))
            .with_download_completed_handler(|_, _, _| {})
            .build(&handle)
            .ok()?;
        let target: Retained<objc2_web_kit::WKWebView> = Retained::into_super(webview.webview());
        if let Some(parent) = parent {
            unsafe { parent.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
        }
        window.makeKeyAndOrderFront(None);
        self.open.borrow_mut().push(Popup {
            window,
            _webview: webview,
        });
        Some(wry::NewWindowResponse::Create { webview: target })
    }
}

/// The popup window's content view, for wry to fill.
#[cfg(target_os = "macos")]
struct ContentView(std::ptr::NonNull<std::ffi::c_void>);

#[cfg(target_os = "macos")]
impl raw_window_handle::HasWindowHandle for ContentView {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = raw_window_handle::RawWindowHandle::AppKit(
            raw_window_handle::AppKitWindowHandle::new(self.0),
        );
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_opens_only_on_blank_or_an_allowed_url() {
        assert!(opens_on("about:blank"));
        assert!(opens_on("https://accounts.example.com/oauth"));
        assert!(opens_on("http://localhost:5173/callback"));
        assert!(!opens_on("http://example.com/"));
        assert!(!opens_on("javascript:alert(1)"));
        assert!(!opens_on("https://user:pass@example.com/"));
    }

    #[test]
    fn inside_it_the_blank_page_goes_once_then_the_policy() {
        let used = Cell::new(false);
        assert!(navigation_allowed("about:blank", &used));
        assert!(!navigation_allowed("about:blank", &used));
        assert!(navigation_allowed("https://accounts.example.com/", &used));
        assert!(!navigation_allowed("file:///etc/passwd", &used));
        let fresh = Cell::new(false);
        assert!(navigation_allowed("https://example.com/", &fresh));
        assert!(
            !navigation_allowed("about:blank", &fresh),
            "not after a page"
        );
    }
}
