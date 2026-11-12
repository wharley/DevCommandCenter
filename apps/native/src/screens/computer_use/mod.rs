//! Screen 8d: computer use through the desktop — the agents' control
//! requests (8d1), the live preview of what they do (8d2), and their cursor
//! on the screen.

pub mod agent_cursor;
pub mod approval;
mod approval_view;
mod cursor_window;
#[cfg(target_os = "linux")]
mod cursor_window_x11;
pub mod preview;
mod preview_view;
#[cfg(test)]
mod tests;
