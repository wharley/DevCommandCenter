//! GPUI leaves its Metal layer untagged, which leaves color matching to
//! whatever the system assumes for the display. The desktop's WebView treats
//! CSS colors as sRGB, so the layer is tagged sRGB too (Apple's guidance for
//! CAMetalLayer). It matters most on wide-gamut (P3) screens.

use gpui::Window;

#[cfg(target_os = "macos")]
pub fn tag_as_srgb(window: &Window) {
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, sel};
    use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let view = appkit.ns_view.as_ptr().cast::<AnyObject>();
    // SAFETY: `view` is GPUI's live NSView for this window; its backing layer is
    // the CAMetalLayer GPUI renders into, and `setColorspace:` takes a CGColorSpaceRef.
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        let Some(layer) = layer.as_ref() else {
            return;
        };
        let can_tag: bool = msg_send![layer, respondsToSelector: sel!(setColorspace:)];
        let Some(srgb) = CGColorSpace::with_name(Some(kCGColorSpaceSRGB)) else {
            return;
        };
        if can_tag {
            let _: () = msg_send![layer, setColorspace: &*srgb];
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn tag_as_srgb(_window: &Window) {}
