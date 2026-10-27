//! A real click, an allowed key and the viewport's image for the desktop's
//! agent tools (the desktop's `browser_input_macos.m` and
//! `browser_capture_macos.m`, here in Rust). The events go to the page's own
//! WKWebView only — never posted to the system, no Accessibility or Screen
//! Recording permission — and only while the view shows; the click must
//! land inside it. The image is the view's own rendering at its logical
//! size, at most 2048px and 4 MB of PNG.

/// `MAX_SNAPSHOT_BYTES`
#[cfg(target_os = "macos")]
pub const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

/// The key, its macOS key code and its characters (`browser_input_macos.m`).
#[cfg(any(target_os = "macos", test))]
pub fn key_entry(key: &str) -> Option<(u16, &'static str)> {
    Some(match key {
        "Enter" => (36, "\r"),
        "Tab" | "Shift+Tab" => (48, "\t"),
        "Escape" => (53, "\u{1b}"),
        "Space" => (49, " "),
        "Backspace" => (51, "\u{7f}"),
        "Delete" => (117, "\u{f728}"),
        "Home" => (115, "\u{f729}"),
        "End" => (119, "\u{f72b}"),
        "ArrowLeft" => (123, "\u{f702}"),
        "ArrowRight" => (124, "\u{f703}"),
        "ArrowDown" => (125, "\u{f701}"),
        "ArrowUp" => (126, "\u{f700}"),
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSView};
    use objc2_foundation::{NSPoint, NSProcessInfo, NSString};
    use objc2_web_kit::WKWebView;

    fn shown(view: &WKWebView) -> bool {
        view.window().is_some() && !view.isHidden()
    }

    /// `dcc_browser_click_fraction`
    pub fn click(view: &WKWebView, x: f64, y: f64) -> bool {
        if MainThreadMarker::new().is_none()
            || !x.is_finite()
            || !y.is_finite()
            || !(0.0..1.0).contains(&x)
            || !(0.0..1.0).contains(&y)
            || !shown(view)
        {
            return false;
        }
        let Some(window) = view.window() else {
            return false;
        };
        let bounds = view.bounds();
        let (x, y) = (x * bounds.size.width, y * bounds.size.height);
        let local = NSPoint::new(
            x,
            if view.isFlipped() {
                y
            } else {
                bounds.size.height - y
            },
        );
        let window_point = view.convertPoint_toView(local, None);
        let Some(superview) = (unsafe { view.superview() }) else {
            return false;
        };
        let Some(target) = view.hitTest(view.convertPoint_toView(local, Some(&superview))) else {
            return false;
        };
        let own: &NSView = view;
        if !std::ptr::eq(Retained::as_ptr(&target), own) && !target.isDescendantOf(own) {
            return false;
        }
        window.makeFirstResponder(Some(view));
        let time = NSProcessInfo::processInfo().systemUptime();
        for (kind, pressure) in [
            (NSEventType::LeftMouseDown, 1.0),
            (NSEventType::LeftMouseUp, 0.0),
        ] {
            let Some(event) = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                kind,
                window_point,
                NSEventModifierFlags::empty(),
                time,
                window.windowNumber(),
                None,
                0,
                1,
                pressure,
            ) else {
                return false;
            };
            if kind == NSEventType::LeftMouseDown {
                target.mouseDown(&event);
            } else {
                target.mouseUp(&event);
            }
        }
        true
    }

    /// `dcc_browser_press_key`: to WebKit's responder, nowhere else.
    pub fn press(view: &WKWebView, key: &str) -> bool {
        let Some((code, characters)) = super::key_entry(key) else {
            return false;
        };
        if MainThreadMarker::new().is_none() || !shown(view) {
            return false;
        }
        let Some(window) = view.window() else {
            return false;
        };
        if !window.makeFirstResponder(Some(view)) {
            return false;
        }
        let mut flags = if key == "Shift+Tab" {
            NSEventModifierFlags::Shift
        } else {
            NSEventModifierFlags::empty()
        };
        if key.starts_with("Arrow") || matches!(key, "Home" | "End" | "Delete") {
            flags |= NSEventModifierFlags::Function;
        }
        let characters = NSString::from_str(characters);
        let time = NSProcessInfo::processInfo().systemUptime();
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let Some(event) = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0.0, 0.0),
                flags,
                time,
                window.windowNumber(),
                None,
                &characters,
                &characters,
                false,
                code,
            ) else {
                return false;
            };
            if kind == NSEventType::KeyDown {
                view.keyDown(&event);
            } else {
                view.keyUp(&event);
            }
        }
        true
    }

    /// `dcc_browser_snapshot`: the view's rendering, as PNG.
    pub fn snapshot(view: &WKWebView, done: Box<dyn FnOnce(Result<Vec<u8>, String>)>) {
        use std::cell::RefCell;

        use block2::RcBlock;
        use objc2_app_kit::{
            NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace,
            NSGraphicsContext, NSImage,
        };
        use objc2_foundation::{NSDictionary, NSError, NSNumber, NSRect, NSSize};
        use objc2_web_kit::WKSnapshotConfiguration;

        let Some(mtm) = MainThreadMarker::new() else {
            return done(Err("Browser viewport is unavailable".into()));
        };
        let bounds = view.bounds();
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return done(Err("Browser viewport is unavailable".into()));
        }
        let scale = (2048.0 / bounds.size.width.max(bounds.size.height)).min(1.0);
        let size = NSSize::new(
            (bounds.size.width * scale).floor().max(1.0),
            (bounds.size.height * scale).floor().max(1.0),
        );
        let configuration = unsafe { WKSnapshotConfiguration::new(mtm) };
        unsafe {
            configuration.setRect(bounds);
            configuration.setSnapshotWidth(Some(&NSNumber::new_f64(bounds.size.width * scale)));
        }
        let done = RefCell::new(Some(done));
        let handler = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            let Some(done) = done.borrow_mut().take() else {
                return;
            };
            if !error.is_null() || image.is_null() {
                return done(Err("Browser snapshot failed".into()));
            }
            let image = unsafe { &*image };
            // Rendered at logical pixels, so a Retina view stays bounded too.
            let bitmap = unsafe {
                NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    mtm.alloc::<NSBitmapImageRep>(),
                    std::ptr::null_mut(),
                    size.width as isize,
                    size.height as isize,
                    8,
                    4,
                    true,
                    false,
                    NSDeviceRGBColorSpace,
                    0,
                    0,
                )
            };
            let Some(bitmap) = bitmap else {
                return done(Err("Browser snapshot allocation failed".into()));
            };
            NSGraphicsContext::saveGraphicsState_class();
            let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap);
            NSGraphicsContext::setCurrentContext(context.as_deref());
            image.drawInRect_fromRect_operation_fraction(
                NSRect::new(NSPoint::new(0.0, 0.0), size),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
                NSCompositingOperation::Copy,
                1.0,
            );
            NSGraphicsContext::restoreGraphicsState_class();
            let png = unsafe {
                bitmap.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            };
            match png {
                Some(png) if png.length() <= super::MAX_SNAPSHOT_BYTES => done(Ok(png.to_vec())),
                _ => done(Err("Browser snapshot exceeds the size limit".into())),
            }
        });
        unsafe {
            view.takeSnapshotWithConfiguration_completionHandler(Some(&configuration), &handler)
        };
    }
}

#[cfg(target_os = "macos")]
pub use mac::{click, press, snapshot};

#[cfg(test)]
mod tests {
    use super::key_entry;

    #[test]
    fn only_the_desktops_keys_have_codes() {
        assert_eq!(key_entry("Enter"), Some((36, "\r")));
        assert_eq!(key_entry("Shift+Tab"), key_entry("Tab"));
        assert_eq!(key_entry("ArrowUp"), Some((126, "\u{f700}")));
        for key in ["Meta+Q", "Control+C", "", "a"] {
            assert_eq!(key_entry(key), None);
        }
    }
}
