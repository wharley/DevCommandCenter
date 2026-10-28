//! The desktop's backend lives in `dcc-server` (no Tauri), so the native app
//! can host the same one; the desktop and its binaries reach it here.
pub use dcc_server::*;
