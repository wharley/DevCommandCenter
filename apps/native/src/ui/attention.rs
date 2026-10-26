//! The Dock icon bounces once for an agent's request, only while the
//! person is in another app (the desktop's `agent_request_attention`).

pub fn request_attention() {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSRequestUserAttentionType};
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            if !app.isActive() {
                app.requestUserAttention(NSRequestUserAttentionType::InformationalRequest);
            }
        }
    }
}
