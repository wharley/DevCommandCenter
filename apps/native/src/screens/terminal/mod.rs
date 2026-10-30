//! Screen 7: the terminal dock under the conversation. The shells live in
//! the desktop's process (its dock's PTYs, through the loopback API); the
//! screen and its scrollback are emulated here with `alacritty_terminal`.

pub mod agent;
pub mod command_dialog;
pub mod dev_server;
pub mod dock;
mod dock_actions;
mod dock_dialogs;
mod dock_menus;
mod dock_view;
pub mod emulator;
pub mod keys;
pub mod palette;
mod select;
pub mod session;
pub mod tabs;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_bar;
mod view;

pub use dock::{DockEvent, ScopeKind, TerminalDock, TerminalPlace};
pub use session::{ShellContext, TerminalSession};
