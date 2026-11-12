//! The agent's cursor window on Linux, when DCC runs on X11 (natively or
//! through XWayland, which is how it runs on a Wayland session). GPUI
//! already opens a `PopUp` as an override-redirect window with a
//! transparent visual: the window manager neither decorates it, focuses it,
//! nor lists it, and it shows on every workspace. What is left is letting
//! clicks through (an empty input shape) and keeping it above the windows
//! the agent acts on as it moves (a raise with each placement).
//!
//! On a session with no X server (GPUI on native Wayland) the cursor stays
//! off: Wayland lets a client neither place its window at a screen point
//! nor keep it above other apps (GNOME has no layer-shell).

use std::sync::OnceLock;

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::connection::Connection;
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{ClipOrdering, ConfigureWindowAux, ConnectionExt as _, StackMode};
use x11rb::rust_connection::RustConnection;

/// A connection of its own: window ids are the X server's, not GPUI's.
fn connection() -> Option<&'static RustConnection> {
    static CONNECTION: OnceLock<Option<RustConnection>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| {
            std::env::var_os("DISPLAY")
                .is_some_and(|display| !display.is_empty())
                .then(|| x11rb::connect(None).ok().map(|(connection, _)| connection))
                .flatten()
        })
        .as_ref()
}

fn x_window(window: &Window) -> Option<u32> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Xcb(xcb) => Some(xcb.window.get()),
        RawWindowHandle::Xlib(xlib) => u32::try_from(xlib.window).ok(),
        _ => None,
    }
}

/// Lets every click and scroll through to whatever is below.
pub fn make_overlay(window: &Window) {
    let (Some(connection), Some(id)) = (connection(), x_window(window)) else {
        return;
    };
    let _ = connection.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, id, 0, 0, &[]);
    let _ = connection.flush();
}

/// Puts the window's top-left corner at a root-window point and raises it.
pub fn place(window: &Window, x: f64, y: f64) {
    let (Some(connection), Some(id)) = (connection(), x_window(window)) else {
        return;
    };
    let aux = ConfigureWindowAux::new()
        .x(x.round() as i32)
        .y(y.round() as i32)
        .stack_mode(StackMode::ABOVE);
    let _ = connection.configure_window(id, &aux);
    let _ = connection.flush();
}
