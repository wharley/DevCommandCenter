//! Appshots (`appshot_commands`): the open windows of other apps, a capture
//! of each to choose from, and the chosen ones saved as PNGs the message
//! attaches. Started by the person (+ → "Capturar janela"), never by an
//! agent: it needs Screen Recording, not the Computer Use grant. Captures
//! stay in memory until attached; only those reach the disk, in the app's
//! `appshots` folder, as the desktop keeps them. Blocking: off the UI thread.

use std::path::{Path, PathBuf};

pub use dcc_server::computer_use_commands::ComputerTarget;
use dcc_server::computer_use_commands::{
    appshot_capture, appshot_request_access, appshot_status, appshot_targets,
};

/// At most this many windows go in one message (`MAX_IMAGES`), shown this
/// many to a page.
pub const MAX_IMAGES: usize = 6;
/// A capture larger than this is refused (`MAX_BYTES`).
const MAX_BYTES: usize = 4 * 1024 * 1024;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Where the macOS pane for Screen Recording opens.
pub const SCREEN_RECORDING_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Not macOS 14 or later.
    Unsupported,
    /// Screen Recording not granted yet.
    Permission,
    Ready,
}

pub fn availability() -> Availability {
    match appshot_status() {
        (false, _) => Availability::Unsupported,
        (true, false) => Availability::Permission,
        (true, true) => Availability::Ready,
    }
}

/// Asks macOS for Screen Recording (it lists the app in the pane).
pub fn request_access() {
    appshot_request_access();
}

/// The other apps' windows; errors are `appshots.errors.*` codes.
pub fn targets() -> Result<Vec<ComputerTarget>, String> {
    appshot_targets()
}

/// `appshotTargetKey`: one window of one process.
pub fn key(target: &ComputerTarget) -> String {
    format!("{}:{}", target.pid, target.window_id)
}

/// A window's capture as PNG bytes, refused when too large or not a PNG.
pub fn capture(target: &ComputerTarget) -> Result<Vec<u8>, String> {
    checked(appshot_capture(target)?)
}

fn checked(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.len() > MAX_BYTES || !bytes.starts_with(PNG) {
        return Err("captureFailed".into());
    }
    Ok(bytes)
}

/// The app's `appshots` folder, next to its database.
pub fn folder() -> PathBuf {
    crate::data::loader::database_path()
        .parent()
        .map_or_else(std::env::temp_dir, Path::to_path_buf)
        .join("appshots")
}

/// `Appshot-<app>-<uuid>.png`: the app's name kept to letters, digits and
/// `-`, so no window title or name can steer the path.
fn file_name(app: &str) -> String {
    let name: String = app
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(40)
        .collect();
    format!("Appshot-{name}-{}.png", uuid::Uuid::new_v4())
}

/// Writes one chosen capture into `folder` (only the owner can read it).
pub fn save(folder: &Path, app: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|_| "saveFailed")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "saveFailed")?;
    }
    let path = folder.join(file_name(app));
    std::fs::write(&path, bytes).map_err(|_| "saveFailed")?;
    Ok(path)
}

/// `appshotErrorKey`: a known code's message, else "unavailable".
pub fn error_key(code: &str) -> String {
    const CODES: [&str; 9] = [
        "unsupported",
        "permission",
        "unavailable",
        "captureFailed",
        "windowUnavailable",
        "saveFailed",
        "selectionInvalid",
        "previewExpired",
        "shortcutUnavailable",
    ];
    let code = if CODES.contains(&code) {
        code
    } else {
        "unavailable"
    };
    format!("appshots.errors.{code}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_capture_keeps_only_a_safe_name() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("appshots");
        let mut png = PNG.to_vec();
        png.extend_from_slice(b"pixels");
        let path = save(&folder, "../Google Chrome", &checked(png).unwrap()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("Appshot-GoogleChrome-"), "{name}");
        assert!(name.ends_with(".png"));
        assert_eq!(path.parent(), Some(folder.as_path()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&folder).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    #[test]
    fn only_a_png_within_the_limit_is_taken() {
        assert_eq!(checked(b"GIF89a".to_vec()).unwrap_err(), "captureFailed");
        let mut huge = PNG.to_vec();
        huge.resize(MAX_BYTES + 1, 0);
        assert!(checked(huge).is_err());
        assert_eq!(error_key("permission"), "appshots.errors.permission");
        assert_eq!(error_key("weird"), "appshots.errors.unavailable");
    }
}
