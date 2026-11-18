//! Secrets in a file only this user can read, for a machine with no system
//! store: a Linux server has no Secret Service (no D-Bus session, or one with
//! no keyring daemon), so the connectors' sign-ins had nowhere to go.
//!
//! The file is not encrypted on purpose. With no system store there is no
//! place to keep a key that the file's own owner could not read too, so a key
//! beside it would protect nothing; the protection is the file's mode (0600 in
//! a 0700 folder), as the Claude CLI keeps its own sign-in on Linux. Writes go
//! through a temporary file and a rename, under a lock, so a crash or a second
//! process never leaves half a file.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};

use dcc_core::ports::{CredentialStoreError, CredentialStoreResult, SecretValue};

/// One service's secrets, by reference, in `<dir>/<service>.json`.
#[derive(Clone, Debug)]
pub struct FileCredentialStore {
    path: PathBuf,
    lock_path: PathBuf,
}

impl FileCredentialStore {
    pub fn new(dir: &Path, service: &str) -> Self {
        let name: String = service
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Self {
            path: dir.join(format!("{name}.json")),
            lock_path: dir.join(format!("{name}.lock")),
        }
    }

    /// `DCC_CREDENTIALS_DIR`, else `~/.dcc/credentials`.
    pub fn default_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("DCC_CREDENTIALS_DIR") {
            return Some(PathBuf::from(dir));
        }
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".dcc").join("credentials"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn store(&self, reference: &str, secret: &SecretValue) -> CredentialStoreResult<()> {
        self.update(|entries| {
            entries.insert(
                reference.to_string(),
                STANDARD.encode(secret.expose_secret()),
            );
            true
        })
        .map(|_| ())
    }

    pub fn resolve(&self, reference: &str) -> CredentialStoreResult<Option<SecretValue>> {
        let _lock = self.lock()?;
        let entries = self.read()?;
        match entries.get(reference) {
            Some(encoded) => {
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|_| CredentialStoreError::CorruptEntry)?;
                SecretValue::new(bytes).map(Some)
            }
            None => Ok(None),
        }
    }

    pub fn delete(&self, reference: &str) -> CredentialStoreResult<bool> {
        self.update(|entries| entries.remove(reference).is_some())
    }

    /// Reads, changes and writes back under the lock; writes only when
    /// `change` says something changed.
    fn update(
        &self,
        change: impl FnOnce(&mut BTreeMap<String, String>) -> bool,
    ) -> CredentialStoreResult<bool> {
        let _lock = self.lock()?;
        let mut entries = self.read()?;
        let changed = change(&mut entries);
        if changed {
            self.write(&entries)?;
        }
        Ok(changed)
    }

    fn lock(&self) -> CredentialStoreResult<File> {
        let dir = self
            .path
            .parent()
            .ok_or(CredentialStoreError::Unavailable)?;
        create_private_dir(dir).map_err(io_error)?;
        let file = private_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)
            .map_err(io_error)?;
        file.lock().map_err(io_error)?;
        // Released when the handle drops.
        Ok(file)
    }

    fn read(&self) -> CredentialStoreResult<BTreeMap<String, String>> {
        match fs::read(&self.path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| CredentialStoreError::CorruptEntry)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(io_error(error)),
        }
    }

    fn write(&self, entries: &BTreeMap<String, String>) -> CredentialStoreResult<()> {
        let bytes = serde_json::to_vec_pretty(entries)
            .map_err(|_| CredentialStoreError::OperationFailed)?;
        let temporary = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        let written = (|| {
            let mut file = private_options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        written.map_err(io_error)
    }
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn io_error(error: io::Error) -> CredentialStoreError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => CredentialStoreError::AccessDenied,
        _ => CredentialStoreError::OperationFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(value: &str) -> SecretValue {
        SecretValue::new(value.as_bytes().to_vec()).unwrap()
    }

    #[test]
    fn stores_replaces_resolves_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::new(&dir.path().join("credentials"), "com.dcc.mcp");
        assert!(store.resolve("credential:figma").unwrap().is_none());
        store.store("credential:figma", &secret("one")).unwrap();
        store.store("credential:figma", &secret("two")).unwrap();
        store.store("credential:slack", &secret("three")).unwrap();
        let figma = store.resolve("credential:figma").unwrap().unwrap();
        assert_eq!(figma.expose_secret(), b"two");
        assert!(store.delete("credential:figma").unwrap());
        assert!(!store.delete("credential:figma").unwrap());
        assert!(store.resolve("credential:figma").unwrap().is_none());
        let slack = store.resolve("credential:slack").unwrap().unwrap();
        assert_eq!(slack.expose_secret(), b"three");
    }

    #[test]
    fn a_second_store_on_the_same_file_sees_the_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let first = FileCredentialStore::new(dir.path(), "svc");
        first.store("a", &secret("x")).unwrap();
        let second = FileCredentialStore::new(dir.path(), "svc");
        assert_eq!(second.resolve("a").unwrap().unwrap().expose_secret(), b"x");
    }

    #[test]
    fn a_broken_file_is_reported_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::new(dir.path(), "svc");
        fs::write(store.path(), b"not json").unwrap();
        assert_eq!(
            store.store("a", &secret("x")).unwrap_err(),
            CredentialStoreError::CorruptEntry
        );
        assert_eq!(fs::read(store.path()).unwrap(), b"not json");
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_can_read_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("credentials");
        let store = FileCredentialStore::new(&folder, "svc");
        store.store("a", &secret("x")).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(store.path()), 0o600);
        assert_eq!(mode(&folder), 0o700);
    }
}
