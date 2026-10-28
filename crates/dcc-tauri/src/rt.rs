//! The async runtime the backend's background work runs on: the Tauri app's
//! own inside the desktop, the caller's tokio runtime without it.

#[cfg(feature = "tauri")]
pub use tauri::async_runtime::{spawn, spawn_blocking};
#[cfg(not(feature = "tauri"))]
pub use tokio::task::{spawn, spawn_blocking};
