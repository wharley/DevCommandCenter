//! Keyboard input is delivered only to the already focused Browser webview.
//! It never posts global events or requires macOS Accessibility access.
pub(crate) fn valid_key(key: &str) -> bool {
    matches!(
        key,
        "Enter"
            | "Tab"
            | "Shift+Tab"
            | "Escape"
            | "ArrowUp"
            | "ArrowDown"
            | "ArrowLeft"
            | "ArrowRight"
            | "Backspace"
            | "Delete"
            | "Home"
            | "End"
            | "Space"
    )
}

pub(crate) async fn press(webview: tauri::Webview<tauri::Wry>, key: &str) -> Result<(), String> {
    if !valid_key(key) {
        return Err("browser key is invalid".into());
    }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn dcc_browser_press_key(
                view: *mut std::ffi::c_void,
                key: *const std::ffi::c_char,
            ) -> bool;
        }
        let key = std::ffi::CString::new(key).map_err(|_| "browser key is invalid")?;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        webview
            .with_webview(move |view| {
                let ok = unsafe { dcc_browser_press_key(view.inner(), key.as_ptr()) };
                let _ = sender.send(ok);
            })
            .map_err(|_| "browser input view is unavailable")?;
        match tokio::time::timeout(std::time::Duration::from_secs(3), receiver).await {
            Ok(Ok(true)) => Ok(()),
            _ => Err("browser keyboard input could not be delivered".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = webview;
        Err("browser keyboard input is unavailable on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys_cannot_trigger_system_shortcuts_or_arbitrary_text() {
        for key in ["Enter", "Tab", "Shift+Tab", "ArrowDown", "Space"] {
            assert!(super::valid_key(key));
        }
        for key in [
            "Meta+Q",
            "Control+C",
            "Command+V",
            "",
            "password",
            "Enter\n",
        ] {
            assert!(!super::valid_key(key));
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) async fn click(view: tauri::Webview<tauri::Wry>, x: f64, y: f64) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() || !(0.0..1.0).contains(&x) || !(0.0..1.0).contains(&y) {
        return Err("browser click point is invalid".into());
    }
    unsafe extern "C" {
        fn dcc_browser_click_fraction(view: *mut std::ffi::c_void, x: f64, y: f64) -> bool;
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    view.with_webview(move |view| {
        let result = unsafe { dcc_browser_click_fraction(view.inner(), x, y) };
        let _ = sender.send(result);
    })
    .map_err(|_| "browser input view is unavailable")?;
    match tokio::time::timeout(std::time::Duration::from_secs(3), receiver).await {
        Ok(Ok(true)) => Ok(()),
        _ => Err("browser click could not be delivered".into()),
    }
}
