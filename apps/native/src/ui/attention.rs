//! An agent's request waiting on the person, only while they are in another
//! app (the desktop's `agent_request_attention`): the Dock icon bounces once
//! and a system notification names it. The caller passes the text, already
//! in the person's language.

/// The notification's title and body, cut like the desktop's (120 and 240
/// characters).
pub fn clipped(title: &str, body: &str) -> (String, String) {
    (
        title.chars().take(120).collect(),
        body.chars().take(240).collect(),
    )
}

pub fn request_attention(title: &str, body: &str) {
    #[cfg(not(test))]
    {
        if app_is_active() {
            return;
        }
        bounce();
        let (title, body) = clipped(title, body);
        // Posting can wait on the system; never on the UI thread.
        std::thread::spawn(move || notify(&title, &body));
    }
    #[cfg(test)]
    record(title, body, true);
}

/// A system notification while the person is in another app, without
/// bouncing the Dock: news that can wait, unlike a waiting agent.
pub fn notify_away(title: &str, body: &str) {
    #[cfg(not(test))]
    {
        if app_is_active() {
            return;
        }
        let (title, body) = clipped(title, body);
        std::thread::spawn(move || notify(&title, &body));
    }
    #[cfg(test)]
    record(title, body, false);
}

#[cfg(test)]
thread_local! {
    static SENT: std::cell::RefCell<Vec<(String, String, bool)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests post nothing; what would have been posted is kept here.
#[cfg(test)]
fn record(title: &str, body: &str, bounced: bool) {
    SENT.with(|sent| {
        sent.borrow_mut()
            .push((title.to_string(), body.to_string(), bounced))
    });
}

/// What would have been posted, as `(title, body, bounced)`, taken.
#[cfg(test)]
pub fn take_sent() -> Vec<(String, String, bool)> {
    SENT.with(|sent| std::mem::take(&mut *sent.borrow_mut()))
}

#[cfg(all(target_os = "macos", not(test)))]
fn app_is_active() -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    MainThreadMarker::new().is_some_and(|mtm| NSApplication::sharedApplication(mtm).isActive())
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn app_is_active() -> bool {
    false
}

#[cfg(all(target_os = "macos", not(test)))]
fn bounce() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSRequestUserAttentionType};
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm)
            .requestUserAttention(NSRequestUserAttentionType::InformationalRequest);
    }
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn bounce() {}

#[cfg(not(test))]
fn notify(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        // Outside a bundle (`cargo run`) the system has no app to post
        // as; the Terminal stands in, as in the desktop's dev builds.
        use objc2_foundation::NSBundle;
        let identifier = NSBundle::mainBundle()
            .bundleIdentifier()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "com.apple.Terminal".to_string());
        let _ = notify_rust::set_application(&identifier);
    }
    let _ = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .auto_icon()
        .show();
}

#[cfg(test)]
mod tests {
    use super::clipped;

    #[test]
    fn the_notification_text_is_cut_like_the_desktop() {
        let (title, body) = clipped(&"t".repeat(130), &"é".repeat(250));
        assert_eq!(title.chars().count(), 120);
        assert_eq!(body.chars().count(), 240);
        let (title, body) = clipped("Abrir uma página?", "https://example.com");
        assert_eq!(title, "Abrir uma página?");
        assert_eq!(body, "https://example.com");
    }
}
