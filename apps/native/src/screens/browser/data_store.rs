//! "Dados do navegador" for the native app's own Browser store (the
//! desktop's `browser_data.rs`): what its WebKit store holds on disk, the
//! caches cleared on demand or past 500 MB (logins stay), or everything.

use std::path::{Path, PathBuf};

/// `CACHE_LIMIT_BYTES`
pub const CACHE_LIMIT_BYTES: u64 = 500 * 1024 * 1024;
/// Folders WebKit fills with cached responses (HTTP and Fetch caches).
const CACHE_FOLDERS: [&str; 2] = ["NetworkCache", "CacheStorage"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// False where the Browser keeps nothing on disk (before macOS 14).
    pub supported: bool,
    pub cache_bytes: u64,
    pub total_bytes: u64,
}

pub fn dir_bytes(path: &Path) -> u64 {
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

/// `~/Library/WebKit/<the app's bundle id, or its process name>/WebsiteDataStore/<id>`.
#[cfg(target_os = "macos")]
fn store_dir() -> Option<PathBuf> {
    use objc2_foundation::{NSBundle, NSProcessInfo};
    let owner = NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|id| id.to_string())
        .unwrap_or_else(|| NSProcessInfo::processInfo().processName().to_string());
    let identifier = uuid::Uuid::from_bytes(super::page::DATA_STORE_IDENTIFIER);
    Some(
        PathBuf::from(std::env::var_os("HOME")?)
            .join("Library/WebKit")
            .join(owner)
            .join("WebsiteDataStore")
            .join(identifier.to_string()),
    )
}

/// `usage`
pub fn usage() -> Usage {
    #[cfg(target_os = "macos")]
    if super::page::named_store_available()
        && let Some(store) = store_dir()
    {
        return Usage {
            supported: true,
            cache_bytes: CACHE_FOLDERS
                .iter()
                .map(|folder| dir_bytes(&store.join(folder)))
                .sum(),
            total_bytes: dir_bytes(&store),
        };
    }
    Usage::default()
}

/// Removes the caches (or everything: logins and site storage too), then
/// calls `done` on the main thread. Must run on the main thread.
pub fn clear(everything: bool, done: impl FnOnce(Result<(), String>) + 'static) {
    #[cfg(target_os = "macos")]
    {
        use std::cell::RefCell;

        use block2::RcBlock;
        use objc2::MainThreadMarker;
        use objc2_foundation::{NSDate, NSSet, NSString, NSUUID};
        use objc2_web_kit::WKWebsiteDataStore;

        let Some(main) = MainThreadMarker::new() else {
            done(Err("Browser data store is unavailable".into()));
            return;
        };
        if !super::page::named_store_available() {
            done(Ok(()));
            return;
        }
        let uuid = NSUUID::from_bytes(super::page::DATA_STORE_IDENTIFIER);
        let store = unsafe { WKWebsiteDataStore::dataStoreForIdentifier(&uuid, main) };
        let types = if everything {
            unsafe { WKWebsiteDataStore::allWebsiteDataTypes(main) }
        } else {
            let kinds: [&NSString; 4] = unsafe {
                [
                    objc2_web_kit::WKWebsiteDataTypeDiskCache,
                    objc2_web_kit::WKWebsiteDataTypeMemoryCache,
                    objc2_web_kit::WKWebsiteDataTypeFetchCache,
                    objc2_web_kit::WKWebsiteDataTypeOfflineWebApplicationCache,
                ]
            };
            NSSet::from_slice(&kinds)
        };
        let done = RefCell::new(Some(done));
        let block = RcBlock::new(move || {
            if let Some(done) = done.borrow_mut().take() {
                done(Ok(()));
            }
        });
        unsafe {
            store.removeDataOfTypes_modifiedSince_completionHandler(
                &types,
                &NSDate::distantPast(),
                &block,
            )
        };
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = everything;
        done(Ok(()));
    }
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
