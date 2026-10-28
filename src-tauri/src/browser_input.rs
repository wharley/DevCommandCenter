//! The keys an agent may press in the native app's Browser (delivered to its
//! focused page by the bridge; never global events).
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
