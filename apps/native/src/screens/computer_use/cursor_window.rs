//! The agent's cursor window on the Mac: borderless, see-through, never
//! takes a click or the keyboard, above every app and on every Space, and
//! placed by its top-left corner at a screen point (top-left origin, as the
//! desktop reports where the agent acted).

use gpui::Window;

#[cfg(target_os = "macos")]
fn ns_window(window: &Window) -> Option<*mut objc2::runtime::AnyObject> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    let view = appkit.ns_view.as_ptr().cast::<AnyObject>();
    // SAFETY: `view` is GPUI's live NSView for this window.
    let ns_window: *mut AnyObject = unsafe { msg_send![view, window] };
    (!ns_window.is_null()).then_some(ns_window)
}

/// Turns the window into an overlay the person clicks through.
#[cfg(target_os = "macos")]
pub fn make_overlay(window: &Window) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSWindowCollectionBehavior, NSWindowStyleMask};

    let Some(ns_window) = ns_window(window) else {
        return;
    };
    // Above menus and full-screen apps, below the system cursor.
    const OVERLAY_LEVEL: isize = 102;
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle
        | NSWindowCollectionBehavior::FullScreenAuxiliary;
    let mask = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    // SAFETY: `ns_window` is this window's live NSPanel; these are plain
    // NSWindow setters, called on the main thread where GPUI renders.
    unsafe {
        let _: () = msg_send![ns_window, setStyleMask: mask];
        let _: () = msg_send![ns_window, setIgnoresMouseEvents: true];
        let _: () = msg_send![ns_window, setHasShadow: false];
        let _: () = msg_send![ns_window, setOpaque: false];
        let clear: *mut AnyObject = msg_send![objc2::class!(NSColor), clearColor];
        let _: () = msg_send![ns_window, setBackgroundColor: clear];
        let _: () = msg_send![ns_window, setLevel: OVERLAY_LEVEL];
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
    }
}

/// Puts the window's top-left corner at `(x, y)`, in screen points from the
/// main screen's top-left corner.
#[cfg(target_os = "macos")]
pub fn place(window: &Window, x: f64, y: f64) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSPoint, NSRect};

    let Some(ns_window) = ns_window(window) else {
        return;
    };
    // SAFETY: NSScreen and NSWindow geometry on the main thread.
    unsafe {
        let screens: *mut AnyObject = msg_send![objc2::class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        if count == 0 {
            return;
        }
        // Cocoa counts y up from the main (first) screen's bottom.
        let main: *mut AnyObject = msg_send![screens, objectAtIndex: 0usize];
        let frame: NSRect = msg_send![main, frame];
        let top_left = NSPoint::new(x, frame.size.height - y);
        let _: () = msg_send![ns_window, setFrameTopLeftPoint: top_left];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn make_overlay(_window: &Window) {}

#[cfg(not(target_os = "macos"))]
pub fn place(_window: &Window, _x: f64, _y: f64) {}
