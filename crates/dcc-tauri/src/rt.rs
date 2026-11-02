//! The async runtime the backend's background work runs on: the Tauri app's
//! own inside the desktop, the caller's tokio runtime without it.

pub use tokio::task::{spawn, spawn_blocking};
