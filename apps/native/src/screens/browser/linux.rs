//! The Browser on Linux: WebKitGTK in an X11 child window of the GPUI
//! window (the way `gpui-webview` does it). GTK cannot embed into a
//! Wayland surface it does not own, so the app runs on X11 — natively or
//! through XWayland (`app::run`). GTK starts once, on its X11 backend, the
//! first time a page opens, and its loop runs between GPUI's frames. Wry
//! scales bounds by GDK's integer factor, which ignores GPUI's, so the page
//! gets device pixels and a zoom that matches GPUI's scale. GPUI does not
//! see presses on the page, so they are handed to it: a menu or popover
//! closes when the page is clicked, as it does on macOS.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyWindowHandle, App, Bounds, Global, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, PlatformInput, Point, point, px,
};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
};

/// How often GTK's pending events run between GPUI frames.
const GTK_POLL: Duration = Duration::from_millis(16);

struct GtkLoop;

impl Global for GtkLoop {}

/// A press on a page, for its GPUI window.
struct Press {
    window: AnyWindowHandle,
    position: Point<Pixels>,
    button: MouseButton,
}

thread_local! {
    static PRESSES: RefCell<Vec<Press>> = const { RefCell::new(Vec::new()) };
}

/// GTK on X11, once, with its loop driven from GPUI.
pub fn ensure_gtk(cx: &mut App) -> Result<(), String> {
    if cx.has_global::<GtkLoop>() {
        return Ok(());
    }
    if !gtk::is_initialized_main_thread() {
        // Wry takes the default GDK display as an X11 one.
        gtk::gdk::set_allowed_backends("x11");
        gtk::init().map_err(|error| error.to_string())?;
    }
    cx.set_global(GtkLoop);
    cx.spawn(async move |cx| {
        loop {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            let presses = PRESSES.with(|presses| std::mem::take(&mut *presses.borrow_mut()));
            for press in presses {
                let _ = press.window.update(cx, |_, window, cx| {
                    window.dispatch_event(
                        PlatformInput::MouseDown(MouseDownEvent {
                            button: press.button,
                            position: press.position,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                            first_mouse: false,
                        }),
                        cx,
                    );
                    window.dispatch_event(
                        PlatformInput::MouseUp(MouseUpEvent {
                            button: press.button,
                            position: press.position,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                        }),
                        cx,
                    );
                });
            }
            cx.background_executor().timer(GTK_POLL).await;
        }
    })
    .detach();
    Ok(())
}

/// The GPUI window as the Xlib parent wry needs; GPUI gives it through XCB.
pub struct X11Parent(XlibWindowHandle);

impl X11Parent {
    pub fn new(window: &gpui::Window) -> Result<Self, String> {
        let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
        match handle.as_raw() {
            RawWindowHandle::Xcb(handle) => {
                let mut xlib = XlibWindowHandle::new(handle.window.get().into());
                xlib.visual_id = handle.visual_id.map_or(0, |id| id.get().into());
                Ok(Self(xlib))
            }
            RawWindowHandle::Xlib(handle) => Ok(Self(handle)),
            _ => Err("the Browser needs an X11 window on Linux (XWayland on Wayland)".into()),
        }
    }
}

impl HasWindowHandle for X11Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // The X11 window outlives the page, which goes with the GPUI window.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Xlib(self.0)) })
    }
}

/// Window-relative bounds in device pixels.
pub fn device_rect(bounds: Bounds<Pixels>, scale: f32) -> wry::Rect {
    let device = |value: Pixels| f32::from(value) * scale;
    wry::Rect {
        position: PhysicalPosition::new(device(bounds.origin.x), device(bounds.origin.y)).into(),
        size: PhysicalSize::new(
            device(bounds.size.width).max(1.0),
            device(bounds.size.height).max(1.0),
        )
        .into(),
    }
}

/// The page's CSS pixels at GPUI's scale (GDK only scales by integers).
pub fn match_scale(webview: &wry::WebView, scale: f32) {
    use gtk::prelude::WidgetExt as _;
    use wry::WebViewExtUnix as _;
    let gdk = webview.webview().scale_factor().max(1);
    let _ = webview.zoom(f64::from(scale) / f64::from(gdk));
}

/// Presses on the page go to its GPUI window too, at `origin` (the page's
/// window-relative top left, kept by `set_bounds`).
pub fn forward_presses(
    webview: &wry::WebView,
    window: AnyWindowHandle,
    gpui_scale: f32,
    origin: Rc<Cell<Point<Pixels>>>,
) {
    use gtk::gdk::EventType;
    use gtk::glib::Propagation;
    use gtk::prelude::WidgetExt as _;
    use wry::WebViewExtUnix as _;
    webview
        .webview()
        .connect_button_press_event(move |widget, event| {
            let button = match event.button() {
                1 => MouseButton::Left,
                2 => MouseButton::Middle,
                3 => MouseButton::Right,
                _ => return Propagation::Proceed,
            };
            if event.event_type() != EventType::ButtonPress {
                return Propagation::Proceed;
            }
            let (x, y) = event.position();
            let scale = widget.scale_factor() as f32 / gpui_scale;
            let position = origin.get() + point(px(x as f32 * scale), px(y as f32 * scale));
            PRESSES.with(|presses| {
                presses.borrow_mut().push(Press {
                    window,
                    position,
                    button,
                })
            });
            Propagation::Proceed
        });
}
