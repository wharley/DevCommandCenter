//! The page itself: the system's engine as a child view of the GPUI window
//! (`wry`, what the Tauri app's Browser is built on), with the desktop's
//! policy — only HTTPS or local HTTP, no background throttling, a
//! Browser-only data store, sign-in windows of their own, downloads only
//! where the person picks — and what it reports back (loading, the URL,
//! the title). Tests drive a fake page instead: the headless GPUI window
//! has no native view to host one.

use std::rc::Rc;

use dcc_core::domain::browser::validate_browser_url;
use futures::channel::mpsc::UnboundedSender;
use gpui::{Bounds, Pixels};

/// What the page tells the surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageEvent {
    /// A navigation started (`PageLoadEvent::Started`).
    Loading(String),
    /// The page finished loading at this URL.
    Loaded(String),
    Title(String),
}

/// What the surface asks of its page.
pub trait Page {
    fn load_url(&self, url: &str);
    fn reload(&self);
    /// Window-relative, logical pixels.
    fn set_bounds(&self, bounds: Bounds<Pixels>);
    fn set_visible(&self, visible: bool);
    /// The URL shown now (a single-page app moves it without a load).
    fn url(&self) -> Option<String>;
    /// The keyboard back to the GPUI window.
    fn blur(&self);
    /// Runs `script` in the page; its result's JSON comes back on `reply`.
    fn evaluate(&self, script: &str, reply: futures::channel::oneshot::Sender<String>);
    /// A real click at these fractions of the viewport (delivered or not).
    fn click(&self, x: f64, y: f64) -> bool;
    /// One allowed key to the focused page (delivered or not).
    fn press(&self, key: &str) -> bool;
    /// The viewport as PNG.
    fn snapshot(&self, reply: futures::channel::oneshot::Sender<Result<Vec<u8>, String>>);
    /// Imported cookies into the page's store; imported and skipped back.
    fn set_cookies(
        &self,
        cookies: Vec<dcc_infra::browser_sessions::PreparedSessionCookie>,
        reply: futures::channel::oneshot::Sender<(usize, usize)>,
    );
}

/// A Browser-only WebKit store (macOS 14+), apart from anything else in
/// the app; every Browser view of the native app shares its logins.
#[cfg(target_os = "macos")]
const DATA_STORE_IDENTIFIER: [u8; 16] = [
    0x3d, 0xc1, 0x8e, 0x52, 0x07, 0x6a, 0x4f, 0x19, 0x9b, 0x2e, 0xd4, 0x61, 0xa8, 0x35, 0x7c, 0x0f,
];

/// `named_browser_store_is_available`: `dataStoreForIdentifier` is macOS 14+.
#[cfg(target_os = "macos")]
fn named_store_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|version| version.trim().split('.').next()?.parse::<u32>().ok())
            .is_some_and(|major| major >= 14)
    })
}

pub struct WryPage {
    view: Rc<wry::WebView>,
    /// The sign-in windows it opened, closed with it.
    popups: super::popup::Popups,
}

#[cfg(target_os = "macos")]
impl WryPage {
    /// The page's own WKWebView.
    fn wk(&self) -> objc2::rc::Retained<objc2_web_kit::WKWebView> {
        use wry::WebViewExtMacOS;
        objc2::rc::Retained::into_super(self.view.webview())
    }
}

impl Drop for WryPage {
    fn drop(&mut self) {
        self.popups.close_all();
    }
}

impl WryPage {
    /// The page as a child of `window`, opening on `url` (already checked).
    pub fn new(
        window: &gpui::Window,
        url: &str,
        events: UnboundedSender<PageEvent>,
    ) -> Result<Self, String> {
        let loads = events.clone();
        let titles = events;
        let popups = super::popup::Popups::default();
        let requests = popups.clone();
        let builder = wry::WebViewBuilder::new()
            .with_url(url)
            .with_visible(false)
            .with_background_throttling(wry::BackgroundThrottlingPolicy::Disabled)
            // `on_navigation`: anything else is cancelled, like the desktop.
            .with_navigation_handler(|url| validate_browser_url(&url).is_ok())
            // Sign-in windows of their own; a download only where the person picks.
            .with_new_window_req_handler(move |url, features| requests.request(url, features))
            .with_download_started_handler(|_, destination| super::download::requested(destination))
            .with_download_completed_handler(|_, _, _| {})
            .with_on_page_load_handler(move |event, url| {
                let _ = loads.unbounded_send(match event {
                    wry::PageLoadEvent::Started => PageEvent::Loading(url),
                    wry::PageLoadEvent::Finished => PageEvent::Loaded(url),
                });
            })
            .with_document_title_changed_handler(move |title| {
                let _ = titles.unbounded_send(PageEvent::Title(title));
            });
        #[cfg(target_os = "macos")]
        let builder = {
            use wry::WebViewBuilderExtDarwin;
            if named_store_available() {
                builder.with_data_store_identifier(DATA_STORE_IDENTIFIER)
            } else {
                builder.with_incognito(true)
            }
        };
        #[cfg(not(target_os = "macos"))]
        let builder = {
            let profile = crate::prefs::native_file("browser-profile");
            std::fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
            let context = Box::leak(Box::new(wry::WebContext::new(Some(profile))));
            builder.with_web_context(context)
        };
        builder
            .build_as_child(window)
            .map(|webview| Self {
                view: Rc::new(webview),
                popups,
            })
            .map_err(|error| error.to_string())
    }
}

fn rect(bounds: Bounds<Pixels>) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(
            f32::from(bounds.origin.x).into(),
            f32::from(bounds.origin.y).into(),
        )),
        size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
            f32::from(bounds.size.width).max(1.0).into(),
            f32::from(bounds.size.height).max(1.0).into(),
        )),
    }
}

impl Page for WryPage {
    fn load_url(&self, url: &str) {
        let _ = self.view.load_url(url);
    }

    fn reload(&self) {
        let _ = self.view.reload();
    }

    fn set_bounds(&self, bounds: Bounds<Pixels>) {
        let _ = self.view.set_bounds(rect(bounds));
    }

    fn set_visible(&self, visible: bool) {
        if !visible {
            let _ = self.view.focus_parent();
        }
        let _ = self.view.set_visible(visible);
    }

    fn url(&self) -> Option<String> {
        self.view.url().ok()
    }

    fn blur(&self) {
        let _ = self.view.focus_parent();
    }

    fn evaluate(&self, script: &str, reply: futures::channel::oneshot::Sender<String>) {
        let reply = std::sync::Mutex::new(Some(reply));
        let _ = self.view.evaluate_script_with_callback(script, move |raw| {
            if let Some(reply) = reply.lock().ok().and_then(|mut reply| reply.take()) {
                let _ = reply.send(raw);
            }
        });
    }

    fn click(&self, x: f64, y: f64) -> bool {
        #[cfg(target_os = "macos")]
        {
            super::input::click(&self.wk(), x, y)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (x, y);
            false
        }
    }

    fn press(&self, key: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            super::input::press(&self.wk(), key)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = key;
            false
        }
    }

    fn snapshot(&self, reply: futures::channel::oneshot::Sender<Result<Vec<u8>, String>>) {
        #[cfg(target_os = "macos")]
        super::input::snapshot(
            &self.wk(),
            Box::new(move |result| {
                let _ = reply.send(result);
            }),
        );
        #[cfg(not(target_os = "macos"))]
        let _ = reply.send(Err(
            "Browser screenshots are not supported on this platform".into(),
        ));
    }

    fn set_cookies(
        &self,
        cookies: Vec<dcc_infra::browser_sessions::PreparedSessionCookie>,
        reply: futures::channel::oneshot::Sender<(usize, usize)>,
    ) {
        let done: super::cookies::Done = Box::new(move |imported, skipped| {
            let _ = reply.send((imported, skipped));
        });
        #[cfg(target_os = "macos")]
        {
            use wry::WebViewExtMacOS;
            let view: objc2::rc::Retained<objc2_web_kit::WKWebView> =
                objc2::rc::Retained::into_super(self.view.webview());
            super::cookies::set_cookies(&view, cookies, done);
        }
        #[cfg(not(target_os = "macos"))]
        super::cookies::set_cookies(&(), cookies, done);
    }
}

/// A page that only records what it is asked, for headless tests.
#[cfg(test)]
#[derive(Default)]
pub struct FakePage {
    pub calls: std::cell::RefCell<Vec<String>>,
    pub url: std::cell::RefCell<Option<String>>,
    /// What a script answers (none: it never does).
    pub answer: std::cell::RefCell<Option<String>>,
}

#[cfg(test)]
impl Page for FakePage {
    fn load_url(&self, url: &str) {
        self.calls.borrow_mut().push(format!("load {url}"));
        *self.url.borrow_mut() = Some(url.to_string());
    }

    fn reload(&self) {
        self.calls.borrow_mut().push("reload".into());
    }

    fn set_bounds(&self, _: Bounds<Pixels>) {}

    fn set_visible(&self, visible: bool) {
        self.calls.borrow_mut().push(format!("visible {visible}"));
    }

    fn url(&self) -> Option<String> {
        self.url.borrow().clone()
    }

    fn blur(&self) {}

    fn click(&self, x: f64, y: f64) -> bool {
        self.calls.borrow_mut().push(format!("click {x} {y}"));
        true
    }

    fn press(&self, key: &str) -> bool {
        self.calls.borrow_mut().push(format!("press {key}"));
        super::input::key_entry(key).is_some()
    }

    fn snapshot(&self, reply: futures::channel::oneshot::Sender<Result<Vec<u8>, String>>) {
        self.calls.borrow_mut().push("snapshot".into());
        let _ = reply.send(Ok(b"png".to_vec()));
    }

    fn set_cookies(
        &self,
        cookies: Vec<dcc_infra::browser_sessions::PreparedSessionCookie>,
        reply: futures::channel::oneshot::Sender<(usize, usize)>,
    ) {
        self.calls
            .borrow_mut()
            .push(format!("cookies {}", cookies.len()));
        let _ = reply.send((cookies.len(), 0));
    }

    fn evaluate(&self, _: &str, reply: futures::channel::oneshot::Sender<String>) {
        self.calls.borrow_mut().push("evaluate".into());
        match self.answer.borrow().clone() {
            Some(answer) => {
                let _ = reply.send(answer);
            }
            // Kept so the reply never arrives (a page that hangs).
            None => std::mem::forget(reply),
        }
    }
}
