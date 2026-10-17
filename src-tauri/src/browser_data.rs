//! The Browser's WebKit store keeps an HTTP disk cache that WebKit sizes from
//! free disk space (692 MB was observed). Above [`CACHE_LIMIT_BYTES`] only the
//! caches are removed: logins and site storage stay. Settings can also clear
//! the caches, or everything, on demand.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Wry};

const CACHE_LIMIT_BYTES: u64 = 500 * 1024 * 1024;
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(180);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Folders WebKit fills with cached responses (HTTP and Fetch caches).
const CACHE_FOLDERS: [&str; 2] = ["NetworkCache", "CacheStorage"];

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStorageUsage {
    /// False where the Browser keeps nothing on disk (before macOS 14).
    pub supported: bool,
    pub cache_bytes: u64,
    pub total_bytes: u64,
}

fn store_dir(app: &AppHandle<Wry>) -> Option<PathBuf> {
    let identifier = uuid::Uuid::from_bytes(crate::browser_popups::BROWSER_DATA_STORE_IDENTIFIER);
    Some(
        crate::home_dir_path()?
            .join("Library/WebKit")
            .join(&app.config().identifier)
            .join("WebsiteDataStore")
            .join(identifier.to_string()),
    )
}

fn dir_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::browser_popups::named_browser_store_is_available()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn usage(app: &AppHandle<Wry>) -> BrowserStorageUsage {
    let Some(store) = store_dir(app).filter(|_| supported()) else {
        return BrowserStorageUsage {
            supported: false,
            cache_bytes: 0,
            total_bytes: 0,
        };
    };
    BrowserStorageUsage {
        supported: true,
        cache_bytes: CACHE_FOLDERS
            .iter()
            .map(|folder| dir_bytes(&store.join(folder)))
            .sum(),
        total_bytes: dir_bytes(&store),
    }
}

#[cfg(target_os = "macos")]
mod native {
    use std::collections::HashMap;
    use std::ffi::{c_char, CStr};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use tauri::{AppHandle, Wry};
    use tokio::sync::oneshot;

    type Pending = HashMap<u64, oneshot::Sender<Result<(), String>>>;
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
    fn pending() -> &'static Mutex<Pending> {
        PENDING.get_or_init(|| Mutex::new(HashMap::new()))
    }

    unsafe extern "C" {
        fn dcc_browser_remove_data(
            identifier: *const u8,
            request: u64,
            kinds: i32,
            callback: extern "C" fn(u64, *const c_char),
        );
    }

    extern "C" fn completed(request: u64, error: *const c_char) {
        let sender = pending()
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request));
        if let Some(sender) = sender {
            let result = if error.is_null() {
                Ok(())
            } else {
                Err(unsafe { CStr::from_ptr(error) }
                    .to_string_lossy()
                    .into_owned())
            };
            let _ = sender.send(result);
        }
    }

    pub async fn remove(app: &AppHandle<Wry>, everything: bool) -> Result<(), String> {
        let request = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        pending()
            .lock()
            .map_err(|_| "Browser data lock failed".to_string())?
            .insert(request, sender);
        let kinds = if everything { 2 } else { 1 };
        app.run_on_main_thread(move || unsafe {
            dcc_browser_remove_data(
                crate::browser_popups::BROWSER_DATA_STORE_IDENTIFIER.as_ptr(),
                request,
                kinds,
                completed,
            )
        })
        .map_err(|error| error.to_string())?;
        let result = tokio::time::timeout(Duration::from_secs(60), receiver).await;
        if let Ok(mut pending) = pending().lock() {
            pending.remove(&request);
        }
        result
            .map_err(|_| "Clearing Browser data timed out".to_string())?
            .map_err(|_| "Clearing Browser data was cancelled".to_string())?
    }
}

pub async fn clear(app: &AppHandle<Wry>, everything: bool) -> Result<BrowserStorageUsage, String> {
    if !supported() {
        return Ok(usage(app));
    }
    #[cfg(target_os = "macos")]
    native::remove(app, everything).await?;
    Ok(usage(app))
}

pub fn spawn_cache_limit(app: AppHandle<Wry>) {
    if !supported() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("dcc-browser-cache-limit".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                let before = usage(&app).cache_bytes;
                if before > CACHE_LIMIT_BYTES {
                    match tauri::async_runtime::block_on(clear(&app, false)) {
                        Ok(after) => eprintln!(
                            "[DCC] Browser cache trimmed: {} MB -> {} MB",
                            before / (1024 * 1024),
                            after.cache_bytes / (1024 * 1024)
                        ),
                        Err(error) => eprintln!("[DCC] Browser cache left as is: {error}"),
                    }
                }
                std::thread::sleep(CHECK_INTERVAL);
            }
        });
    if let Err(error) = spawned {
        eprintln!("[DCC] Browser cache limit unavailable: {error}");
    }
}

#[tauri::command]
pub fn browser_storage_usage(app: AppHandle<Wry>) -> BrowserStorageUsage {
    usage(&app)
}

/// `everything: false` clears caches only; `true` also signs the Browser out
/// of every site (cookies and site storage).
#[tauri::command]
pub async fn browser_clear_data(
    app: AppHandle<Wry>,
    everything: bool,
) -> Result<BrowserStorageUsage, String> {
    clear(&app, everything).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_nested_cache_files() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("NetworkCache/Version 17/Records");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("a"), vec![0u8; 1000]).unwrap();
        std::fs::write(root.path().join("NetworkCache/b"), vec![0u8; 24]).unwrap();
        assert_eq!(dir_bytes(&root.path().join("NetworkCache")), 1024);
        assert_eq!(dir_bytes(&root.path().join("missing")), 0);
    }
}
