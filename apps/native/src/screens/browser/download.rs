//! A download from the Browser or one of its sign-in windows goes only
//! where the person picks (`choose_download_destination`): the macOS save
//! panel, prefilled with the page's name for it. There is no default
//! folder, so a page never writes a file without a human choice; a second
//! download while the panel is open is refused. Elsewhere downloads are
//! refused until the Browser ports there (8e).

use std::path::{Path, PathBuf};

/// `DownloadEvent::Requested`: whether it goes, and where.
pub fn requested(destination: &mut PathBuf) -> bool {
    match choose(destination) {
        Some(path) => {
            *destination = path;
            true
        }
        None => false,
    }
}

/// "download" when the page names nothing.
#[cfg(any(target_os = "macos", test))]
pub fn suggested_name(destination: &Path) -> String {
    destination
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("download")
        .to_string()
}

#[cfg(target_os = "macos")]
fn choose(destination: &Path) -> Option<PathBuf> {
    use std::sync::atomic::{AtomicBool, Ordering};

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSSavePanel};
    use objc2_foundation::NSString;

    /// One save panel at a time.
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    let mtm = MainThreadMarker::new()?;
    if ACTIVE.swap(true, Ordering::SeqCst) {
        return None;
    }
    let panel = NSSavePanel::savePanel(mtm);
    panel.setCanCreateDirectories(true);
    panel.setNameFieldStringValue(&NSString::from_str(&suggested_name(destination)));
    let chosen = (panel.runModal() == NSModalResponseOK)
        .then(|| panel.URL())
        .flatten()
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()));
    ACTIVE.store(false, Ordering::SeqCst);
    chosen
}

#[cfg(not(target_os = "macos"))]
fn choose(_: &Path) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::suggested_name;
    use std::path::Path;

    #[test]
    fn the_panel_starts_on_the_pages_name() {
        assert_eq!(
            suggested_name(Path::new("/tmp/relatorio.pdf")),
            "relatorio.pdf"
        );
        assert_eq!(suggested_name(Path::new("/")), "download");
        assert_eq!(suggested_name(Path::new("")), "download");
    }
}
