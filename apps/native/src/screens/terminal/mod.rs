//! Screen 7: the terminal dock under the conversation. The shells live in
//! the desktop's process (its dock's PTYs, through the loopback API); the
//! screen and its scrollback are emulated here with `alacritty_terminal`.

pub mod dock;
mod dock_view;
pub mod emulator;
pub mod keys;
pub mod palette;
mod select;
pub mod session;
pub mod tabs;
#[cfg(test)]
mod tests;
mod view;

pub use dock::{DockEvent, ScopeKind, TerminalDock, TerminalPlace};
pub use session::{ShellContext, TerminalSession};
