//! A download from the Browser or one of its sign-in windows goes only
//! where the person picks. WebKit writes it to a private staging folder
//! (owner-only, emptied on each launch); when it finishes, the save panel
//! asks where it goes, prefilled with the page's name for it, and a cancel
//! deletes it. The panel never opens inside WebKit's callback: a modal there
//! runs the event loop while wry holds its download handler, and a second
//! download then aborts the app. One panel at a time, at most `MAX_OPEN`
//! downloads pending. Elsewhere downloads are refused until the Browser
//! ports there (8e).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use futures::channel::mpsc::UnboundedSender;
use gpui::{App, AppContext};

use super::page::PageEvent;
use crate::i18n::Translate;
use crate::ui::Toaster;

/// Downloads running or waiting for the panel; more are refused.
const MAX_OPEN: usize = 3;

thread_local! {
    static OPEN: Cell<usize> = const { Cell::new(0) };
    /// By the request's URL, in the order they started.
    static RUNNING: RefCell<HashMap<String, VecDeque<Rc<Staged>>>> = RefCell::default();
    static WAITING: RefCell<VecDeque<Rc<Staged>>> = RefCell::default();
    static ASKING: Cell<bool> = const { Cell::new(false) };
}

/// One download in its own staging folder, deleted with it.
#[derive(Debug)]
pub struct Staged {
    folder: PathBuf,
    name: String,
}

impl PartialEq for Staged {
    fn eq(&self, other: &Self) -> bool {
        self.folder == other.folder
    }
}

impl Eq for Staged {}

impl Staged {
    fn new(root: &Path, name: String) -> Option<Self> {
        if OPEN.get() >= MAX_OPEN {
            return None;
        }
        private_folder(root).ok()?;
        let folder = root.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&folder).ok()?;
        OPEN.set(OPEN.get() + 1);
        Some(Self { folder, name })
    }

    pub fn file(&self) -> PathBuf {
        self.folder.join(&self.name)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
        OPEN.set(OPEN.get().saturating_sub(1));
    }
}

fn private_folder(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// The staging folder, emptied the first time this run uses it.
fn root() -> PathBuf {
    static EMPTIED: std::sync::Once = std::sync::Once::new();
    let root = crate::prefs::native_file("browser-downloads");
    EMPTIED.call_once(|| {
        let _ = std::fs::remove_dir_all(&root);
    });
    root
}

/// `DownloadEvent::Requested`: staged at once, never a panel here.
pub fn requested(url: String, destination: &mut PathBuf) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    stage(&root(), url, destination)
}

pub(super) fn stage(root: &Path, url: String, destination: &mut PathBuf) -> bool {
    let Some(staged) = Staged::new(root, suggested_name(destination)) else {
        return false;
    };
    *destination = staged.file();
    RUNNING.with_borrow_mut(|running| {
        running.entry(url).or_default().push_back(Rc::new(staged));
    });
    true
}

/// `DownloadEvent::Finished`: a complete file goes to the surface, which
/// asks where it goes; a failed one is deleted.
pub fn finished(url: &str, success: bool, events: &UnboundedSender<PageEvent>) {
    let staged = RUNNING.with_borrow_mut(|running| {
        let queue = running.get_mut(url)?;
        let staged = queue.pop_front();
        if queue.is_empty() {
            running.remove(url);
        }
        staged
    });
    if let (Some(staged), true) = (staged, success) {
        let _ = events.unbounded_send(PageEvent::Downloaded(staged));
    }
}

/// The save panel for a finished download, after any already asking.
pub fn ask(staged: Rc<Staged>, cx: &mut App) {
    WAITING.with_borrow_mut(|waiting| waiting.push_back(staged));
    if !ASKING.get() {
        ask_next(cx);
    }
}

fn ask_next(cx: &mut App) {
    let Some(staged) = WAITING.with_borrow_mut(VecDeque::pop_front) else {
        ASKING.set(false);
        return;
    };
    ASKING.set(true);
    let start = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    let answer = cx.prompt_for_new_path(&start, Some(&staged.name));
    let failed = cx.t("fileSurface.saveFailed");
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(target))) = answer.await {
            let file = staged.file();
            let moved = cx
                .background_spawn(async move { move_file(&file, &target) })
                .await;
            if moved.is_err() {
                cx.update(|cx| Toaster::error(failed, cx));
            }
        }
        drop(staged);
        cx.update(ask_next);
    })
    .detach();
}

/// A rename, or a copy when the target is on another volume.
fn move_file(file: &Path, target: &Path) -> std::io::Result<()> {
    if std::fs::rename(file, target).is_ok() {
        return Ok(());
    }
    std::fs::copy(file, target).map(|_| ())
}

/// "download" when the page names nothing.
fn suggested_name(destination: &Path) -> String {
    destination
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("download")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc::unbounded;

    #[test]
    fn the_panel_starts_on_the_pages_name() {
        assert_eq!(
            suggested_name(Path::new("/tmp/relatorio.pdf")),
            "relatorio.pdf"
        );
        assert_eq!(suggested_name(Path::new("/")), "download");
        assert_eq!(suggested_name(Path::new("")), "download");
        assert_eq!(suggested_name(Path::new("/tmp/..")), "download");
    }

    #[test]
    fn a_download_is_staged_privately_and_deleted_unless_kept() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("staging");
        let (events, mut received) = unbounded();
        let mut destination = PathBuf::from("/Users/x/Downloads/relatorio.pdf");
        assert!(stage(&root, "https://a.test/r".into(), &mut destination));
        assert!(destination.starts_with(&root));
        assert!(destination.ends_with("relatorio.pdf"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&root).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        std::fs::write(&destination, b"pdf").unwrap();

        finished("https://a.test/r", true, &events);
        let Some(PageEvent::Downloaded(staged)) = received.try_recv().ok() else {
            panic!("the finished download goes to the surface");
        };
        assert_eq!(staged.file(), destination);
        drop(staged);
        assert!(!destination.exists(), "not kept: deleted");
        assert_eq!(OPEN.get(), 0);
    }

    #[test]
    fn a_failed_download_is_deleted_and_a_flood_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("staging");
        let (events, mut received) = unbounded();
        let mut staged = Vec::new();
        for index in 0..MAX_OPEN {
            let mut destination = PathBuf::from(format!("/d/file-{index}.zip"));
            assert!(stage(&root, "https://a.test/f".into(), &mut destination));
            staged.push(destination);
        }
        let mut refused = PathBuf::from("/d/more.zip");
        assert!(!stage(&root, "https://a.test/f".into(), &mut refused));
        assert_eq!(refused, PathBuf::from("/d/more.zip"));

        finished("https://a.test/f", false, &events);
        assert!(received.try_recv().is_err(), "nothing to ask");
        assert!(!staged[0].parent().unwrap().exists());
        assert!(stage(&root, "https://a.test/g".into(), &mut refused));
    }
}
