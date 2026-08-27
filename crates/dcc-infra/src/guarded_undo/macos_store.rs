//! macOS private artifact store for capture-v2.
//!
//! This is storage-only.  It has no database, Git, lifecycle, or execute
//! integration.  All names are generated opaque keys and all filesystem
//! traversal is descriptor-relative.
//!
//! `stage` intentionally accepts only `CapturedBytes`: the workspace adapter
//! has already materialized and bounded the preimage at
//! `MAX_PREIMAGE_BYTES_PER_FILE`.  The store repeats that central check at its
//! boundary and never accepts a generic reader.

#![cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]

use std::{
    collections::BTreeMap,
    ffi::CString,
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    os::fd::{AsRawFd, FromRawFd, RawFd},
    os::unix::ffi::OsStrExt,
    path::Path,
};

use dcc_core::domain::guarded_undo::{
    ArtifactKey, GuardedUndoReasonCode, PhysicalRootId, RegularFileMetadataV1, Sha256Digest,
    MAX_PREIMAGE_BYTES_PER_FILE,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::macos_root::{CapturedBytes, MacWorkspaceRoot};

const STORE_DIR: &[u8] = b".dcc-guarded-undo";
const STAGING_DIR: &[u8] = b"staging";
const OBJECTS_DIR: &[u8] = b"objects";
const LOCK_FILE: &[u8] = b".dcc-guarded-undo.lock";
const MODE_DIR: libc::mode_t = 0o700;
const MODE_FILE: libc::mode_t = 0o600;
const ACL_TYPE_EXTENDED: libc::c_int = 0x0000_0100;
const ACL_FIRST_ENTRY: libc::c_int = 0;
const ENOENT_MACOS: i32 = 2;
const ENOATTR_MACOS: i32 = 93;

#[derive(Clone, PartialEq, Eq)]
pub enum MacArtifactStoreError {
    InvalidPath,
    Io(IoCategory),
    AdapterUnsupported,
    LockUnavailable,
    Collision,
    Integrity,
    NotFound,
    FileChanged,
    LimitExceeded,
    ExtendedMetadataUnsupported,
    ArtifactStoreUnsafe,
    InsufficientDiskSpace,
    PublishReconciliationRequired(StagedArtifact),
}

impl fmt::Debug for MacArtifactStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::InvalidPath => "InvalidPath",
            Self::Io(_) => "Io([category])",
            Self::AdapterUnsupported => "AdapterUnsupported",
            Self::LockUnavailable => "LockUnavailable",
            Self::Collision => "Collision",
            Self::Integrity => "Integrity",
            Self::NotFound => "NotFound",
            Self::FileChanged => "FileChanged",
            Self::LimitExceeded => "LimitExceeded",
            Self::ExtendedMetadataUnsupported => "ExtendedMetadataUnsupported",
            Self::ArtifactStoreUnsafe => "ArtifactStoreUnsafe",
            Self::InsufficientDiskSpace => "InsufficientDiskSpace",
            Self::PublishReconciliationRequired(_) => "PublishReconciliationRequired([redacted])",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoCategory {
    PermissionDenied,
    Busy,
    Other,
}

impl fmt::Display for MacArtifactStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPath => "invalid artifact key or app-data path",
            Self::Io(_) => "artifact store I/O failed",
            Self::AdapterUnsupported => "artifact store filesystem is unsupported",
            Self::LockUnavailable => "artifact store instance lock unavailable",
            Self::Collision => "artifact key collision",
            Self::Integrity => "artifact integrity mismatch",
            Self::NotFound => "artifact not found",
            Self::FileChanged => "artifact changed during operation",
            Self::LimitExceeded => "artifact exceeds the central size limit",
            Self::ExtendedMetadataUnsupported => "extended metadata is not supported",
            Self::ArtifactStoreUnsafe => "artifact store failed safety validation",
            Self::InsufficientDiskSpace => "insufficient disk space",
            Self::PublishReconciliationRequired(_) => "publish reconciliation required",
        })
    }
}

impl std::error::Error for MacArtifactStoreError {}

impl MacArtifactStoreError {
    pub fn reason_code(&self) -> GuardedUndoReasonCode {
        match self {
            Self::LockUnavailable => GuardedUndoReasonCode::AppInstanceConflict,
            Self::Integrity | Self::FileChanged => GuardedUndoReasonCode::ArtifactCorrupt,
            Self::LimitExceeded => GuardedUndoReasonCode::FileTooLarge,
            Self::ExtendedMetadataUnsupported => GuardedUndoReasonCode::ExtendedMetadataUnsupported,
            Self::ArtifactStoreUnsafe => GuardedUndoReasonCode::ArtifactStoreUnsafe,
            Self::InsufficientDiskSpace => GuardedUndoReasonCode::InsufficientDiskSpace,
            Self::PublishReconciliationRequired(_) => GuardedUndoReasonCode::ArtifactStoreUnsafe,
            Self::AdapterUnsupported => GuardedUndoReasonCode::AdapterUnsupported,
            Self::NotFound => GuardedUndoReasonCode::ArtifactMissing,
            Self::InvalidPath | Self::Io(_) | Self::Collision => GuardedUndoReasonCode::IoError,
        }
    }
}

impl From<io::Error> for MacArtifactStoreError {
    fn from(error: io::Error) -> Self {
        if error.raw_os_error() == Some(libc::ENOSPC) {
            return Self::InsufficientDiskSpace;
        }
        let category = match error.kind() {
            io::ErrorKind::PermissionDenied => IoCategory::PermissionDenied,
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => IoCategory::Busy,
            _ => IoCategory::Other,
        };
        Self::Io(category)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct StagedArtifact {
    pub key: ArtifactKey,
    pub size: u64,
    pub sha256: Sha256Digest,
    pub metadata: RegularFileMetadataV1,
}

impl fmt::Debug for StagedArtifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StagedArtifact")
            .field("key", &"[redacted]")
            .field("size", &self.size)
            .field("sha256", &"[redacted]")
            .field("metadata", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedArtifact {
    pub key: ArtifactKey,
    pub size: u64,
    pub sha256: Sha256Digest,
}

impl fmt::Debug for VerifiedArtifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedArtifact")
            .field("key", &"[redacted]")
            .field("size", &self.size)
            .field("sha256", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishState {
    Published(VerifiedArtifact),
    PublishedCleanupPending(StagedArtifact),
    StagedOnly(StagedArtifact),
}

pub struct MacArtifactStore {
    staging: File,
    objects: File,
    _lock: File,
}

impl fmt::Debug for MacArtifactStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacArtifactStore").finish_non_exhaustive()
    }
}

impl MacArtifactStore {
    pub fn open(
        app_data_abs: &Path,
        workspace: &MacWorkspaceRoot,
    ) -> Result<Self, MacArtifactStoreError> {
        let (app_data, ancestry) = walk_absolute_directory(app_data_abs)?;
        if overlaps_excluding_root(&ancestry, &workspace.ancestry_ids()) {
            return Err(MacArtifactStoreError::InvalidPath);
        }
        ensure_fs(&app_data)?;
        let lock = open_lock(&app_data)?;
        let store = mkdir_validated(&app_data, STORE_DIR)?;
        fsync_dir(&app_data)?;
        let staging = mkdir_validated(&store, STAGING_DIR)?;
        let objects = mkdir_validated(&store, OBJECTS_DIR)?;
        fsync_dir(&store)?;
        fsync_dir(&app_data)?;
        Ok(Self {
            staging,
            objects,
            _lock: lock,
        })
    }

    pub fn stage(
        &self,
        bytes: &CapturedBytes,
        maximum_bytes: u64,
    ) -> Result<StagedArtifact, MacArtifactStoreError> {
        self.stage_impl(
            bytes,
            maximum_bytes,
            ArtifactKey(Uuid::new_v4().into_bytes()),
            None,
        )
    }

    #[cfg(test)]
    fn stage_with_key(
        &self,
        bytes: &CapturedBytes,
        maximum_bytes: u64,
        key: ArtifactKey,
    ) -> Result<StagedArtifact, MacArtifactStoreError> {
        self.stage_impl(bytes, maximum_bytes, key, None)
    }

    #[cfg(test)]
    fn stage_with_fault(
        &self,
        bytes: &CapturedBytes,
        maximum_bytes: u64,
        key: ArtifactKey,
        fault: FaultPoint,
    ) -> Result<StagedArtifact, MacArtifactStoreError> {
        self.stage_impl(bytes, maximum_bytes, key, Some(fault))
    }

    fn stage_impl(
        &self,
        bytes: &CapturedBytes,
        maximum_bytes: u64,
        key: ArtifactKey,
        #[cfg(test)] fault: Option<FaultPoint>,
        #[cfg(not(test))] _fault: Option<()>,
    ) -> Result<StagedArtifact, MacArtifactStoreError> {
        let size = bytes.as_slice().len() as u64;
        if size > maximum_bytes || size > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(MacArtifactStoreError::LimitExceeded);
        }
        let name = key_name(key);
        let fd = unsafe {
            libc::openat(
                self.staging.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                MODE_FILE as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        #[cfg(test)]
        assert_cloexec(fd);
        let mut file = unsafe { File::from_raw_fd(fd) };
        #[cfg(test)]
        if fault == Some(FaultPoint::Write) {
            return Err(cleanup_failed_stage(
                &self.staging,
                name.as_bytes(),
                MacArtifactStoreError::Io(IoCategory::Other),
            ));
        }
        if let Err(error) = file
            .write_all(bytes.as_slice())
            .and_then(|_| file.sync_all())
        {
            let mapped = MacArtifactStoreError::from(error);
            return Err(cleanup_failed_stage(&self.staging, name.as_bytes(), mapped));
        }
        #[cfg(test)]
        if fault == Some(FaultPoint::FullSync) {
            return Err(cleanup_failed_stage(
                &self.staging,
                name.as_bytes(),
                MacArtifactStoreError::Io(IoCategory::Other),
            ));
        }
        if let Err(error) = full_sync(&file).and_then(|_| fsync_dir(&self.staging)) {
            return Err(cleanup_failed_stage(&self.staging, name.as_bytes(), error));
        }
        let expected_sha256 = Sha256Digest::of(bytes.as_slice());
        let verification_file = match self.open_named(&self.staging, name.as_bytes()) {
            Ok(file) => file,
            Err(error) => return Err(cleanup_failed_stage(&self.staging, name.as_bytes(), error)),
        };
        let snapshot = match inspect_artifact(&verification_file, 1) {
            Ok(snapshot) if snapshot.size == size && snapshot.sha256 == expected_sha256 => snapshot,
            Ok(_) => {
                return Err(cleanup_failed_stage(
                    &self.staging,
                    name.as_bytes(),
                    MacArtifactStoreError::Integrity,
                ));
            }
            Err(error) => {
                return Err(cleanup_failed_stage(&self.staging, name.as_bytes(), error));
            }
        };
        Ok(StagedArtifact {
            key,
            size,
            sha256: expected_sha256,
            metadata: snapshot.metadata,
        })
    }

    pub fn publish(&self, staged: &StagedArtifact) -> Result<PublishState, MacArtifactStoreError> {
        self.publish_impl(staged, None)
    }

    #[cfg(test)]
    fn publish_with_fault(
        &self,
        staged: &StagedArtifact,
        fault: FaultPoint,
    ) -> Result<PublishState, MacArtifactStoreError> {
        self.publish_impl(staged, Some(fault))
    }

    fn publish_impl(
        &self,
        staged: &StagedArtifact,
        #[cfg(test)] fault: Option<FaultPoint>,
        #[cfg(not(test))] fault: Option<()>,
    ) -> Result<PublishState, MacArtifactStoreError> {
        let name = key_name(staged.key);
        let staged_file = self.open_named(&self.staging, name.as_bytes())?;
        let snapshot = inspect_artifact(&staged_file, 1)?;
        if snapshot.size != staged.size
            || snapshot.sha256 != staged.sha256
            || snapshot.metadata != staged.metadata
        {
            return Err(MacArtifactStoreError::Integrity);
        }
        #[cfg(test)]
        if fault == Some(FaultPoint::Link) {
            return Err(MacArtifactStoreError::Io(IoCategory::Other));
        }
        link_staged(&self.staging, &self.objects, name.as_bytes())?;
        self.complete_linked_publish_impl(staged, name.as_bytes(), fault)
    }

    pub fn reconcile_publish(
        &self,
        staged: &StagedArtifact,
    ) -> Result<PublishState, MacArtifactStoreError> {
        let name = key_name(staged.key);
        let staging_exists = exists(&self.staging, name.as_bytes())?;
        // A completed publish has one link.  A crash between link and unlink
        // has two, and is only valid while the staging name is still present.
        if !staging_exists {
            let final_artifact = self.verify_fd(staged.key, staged.size, staged.sha256, false)?;
            return Ok(PublishState::Published(final_artifact));
        }
        let final_result = self.verify_fd(staged.key, staged.size, staged.sha256, true);
        match (final_result, staging_exists) {
            (Ok(_), false) => Err(MacArtifactStoreError::ArtifactStoreUnsafe),
            (Ok(_), true) => {
                let staging = self.open_named(&self.staging, name.as_bytes())?;
                let final_file = self.open_named(&self.objects, name.as_bytes())?;
                if !same_inode(&staging, &final_file)? {
                    return Err(MacArtifactStoreError::Integrity);
                }
                unlink(&self.staging, name.as_bytes()).map_err(|_| {
                    MacArtifactStoreError::PublishReconciliationRequired(staged.clone())
                })?;
                fsync_dir(&self.staging).map_err(|_| {
                    MacArtifactStoreError::PublishReconciliationRequired(staged.clone())
                })?;
                let final_artifact = self
                    .verify_fd(staged.key, staged.size, staged.sha256, false)
                    .map_err(|_| {
                        MacArtifactStoreError::PublishReconciliationRequired(staged.clone())
                    })?;
                Ok(PublishState::Published(final_artifact))
            }
            (Err(MacArtifactStoreError::NotFound), true) => {
                // A staging-only object is never reported as cleanup-pending.  Retry
                // the no-replace hard link while holding the instance lock.
                match link_staged(&self.staging, &self.objects, name.as_bytes()) {
                    Ok(()) => self.complete_linked_publish(staged, name.as_bytes()),
                    Err(MacArtifactStoreError::Collision) => {
                        let final_artifact =
                            self.verify_fd(staged.key, staged.size, staged.sha256, true)?;
                        let staging = self.open_named(&self.staging, name.as_bytes())?;
                        let final_file = self.open_named(&self.objects, name.as_bytes())?;
                        if !same_inode(&staging, &final_file)? {
                            return Err(MacArtifactStoreError::Collision);
                        }
                        self.complete_linked_publish(staged, name.as_bytes())
                            .map(|_| PublishState::Published(final_artifact))
                    }
                    Err(error) => Err(error),
                }
            }
            (Err(error), _) => Err(error),
        }
    }

    pub fn verify(
        &self,
        key: ArtifactKey,
        expected_size: u64,
        expected_sha256: Sha256Digest,
    ) -> Result<VerifiedArtifact, MacArtifactStoreError> {
        if expected_size > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(MacArtifactStoreError::LimitExceeded);
        }
        self.verify_fd(key, expected_size, expected_sha256, false)
    }

    pub fn cleanup_verified(
        &self,
        artifact: &VerifiedArtifact,
    ) -> Result<(), MacArtifactStoreError> {
        let _ = self.verify(artifact.key, artifact.size, artifact.sha256)?;
        unlink(&self.objects, key_name(artifact.key).as_bytes())?;
        fsync_dir(&self.objects)?;
        Ok(())
    }

    pub fn cleanup_staged(&self, artifact: &StagedArtifact) -> Result<(), MacArtifactStoreError> {
        let name = key_name(artifact.key);
        let file = self.open_named(&self.staging, name.as_bytes())?;
        let snapshot = inspect_artifact(&file, 1)?;
        if snapshot.size != artifact.size
            || snapshot.sha256 != artifact.sha256
            || snapshot.metadata != artifact.metadata
        {
            return Err(MacArtifactStoreError::Integrity);
        }
        unlink(&self.staging, name.as_bytes())?;
        fsync_dir(&self.staging)?;
        Ok(())
    }

    fn complete_linked_publish(
        &self,
        staged: &StagedArtifact,
        name: &[u8],
    ) -> Result<PublishState, MacArtifactStoreError> {
        self.complete_linked_publish_impl(staged, name, None)
    }

    fn complete_linked_publish_impl(
        &self,
        staged: &StagedArtifact,
        name: &[u8],
        #[cfg(test)] fault: Option<FaultPoint>,
        #[cfg(not(test))] _fault: Option<()>,
    ) -> Result<PublishState, MacArtifactStoreError> {
        let reconciliation =
            || MacArtifactStoreError::PublishReconciliationRequired(staged.clone());
        #[cfg(test)]
        if matches!(
            fault,
            Some(FaultPoint::AfterLink | FaultPoint::ObjectsFsync)
        ) {
            return Err(reconciliation());
        }
        fsync_dir(&self.objects).map_err(|_| reconciliation())?;
        #[cfg(test)]
        if fault == Some(FaultPoint::Verify) {
            return Err(reconciliation());
        }
        self.verify_fd(staged.key, staged.size, staged.sha256, true)
            .map_err(|_| reconciliation())?;
        #[cfg(test)]
        if fault == Some(FaultPoint::Unlink) {
            return Err(reconciliation());
        }
        unlink(&self.staging, name).map_err(|_| reconciliation())?;
        fsync_dir(&self.staging).map_err(|_| reconciliation())?;
        let final_artifact = self
            .verify(staged.key, staged.size, staged.sha256)
            .map_err(|_| reconciliation())?;
        Ok(PublishState::Published(final_artifact))
    }

    fn open_named(&self, directory: &File, name: &[u8]) -> Result<File, MacArtifactStoreError> {
        let name = CString::new(name).map_err(|_| MacArtifactStoreError::InvalidPath)?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::NotFound {
                Err(MacArtifactStoreError::NotFound)
            } else {
                Err(error.into())
            };
        }
        #[cfg(test)]
        assert_cloexec(fd);
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn verify_fd(
        &self,
        key: ArtifactKey,
        expected_size: u64,
        expected_sha256: Sha256Digest,
        allow_linked: bool,
    ) -> Result<VerifiedArtifact, MacArtifactStoreError> {
        if expected_size > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(MacArtifactStoreError::LimitExceeded);
        }
        let file = self.open_named(&self.objects, key_name(key).as_bytes())?;
        let snapshot = inspect_artifact(&file, if allow_linked { 2 } else { 1 })?;
        if snapshot.size != expected_size || snapshot.sha256 != expected_sha256 {
            return Err(MacArtifactStoreError::Integrity);
        }
        Ok(VerifiedArtifact {
            key,
            size: expected_size,
            sha256: expected_sha256,
        })
    }
}

fn cleanup_failed_stage(
    staging: &File,
    name: &[u8],
    error: MacArtifactStoreError,
) -> MacArtifactStoreError {
    if unlink(staging, name).is_ok() && fsync_dir(staging).is_ok() {
        error
    } else {
        MacArtifactStoreError::ArtifactStoreUnsafe
    }
}

fn link_staged(staging: &File, objects: &File, name: &[u8]) -> Result<(), MacArtifactStoreError> {
    let name = CString::new(name).map_err(|_| MacArtifactStoreError::InvalidPath)?;
    let result = unsafe {
        libc::linkat(
            staging.as_raw_fd(),
            name.as_ptr(),
            objects.as_raw_fd(),
            name.as_ptr(),
            0,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::AlreadyExists {
            Err(MacArtifactStoreError::Collision)
        } else {
            Err(error.into())
        }
    }
}

struct ArtifactSnapshot {
    size: u64,
    sha256: Sha256Digest,
    metadata: RegularFileMetadataV1,
}

fn inspect_artifact(
    file: &File,
    expected_links: libc::nlink_t,
) -> Result<ArtifactSnapshot, MacArtifactStoreError> {
    let stat = fstat(file.as_raw_fd())?;
    if !is_regular(stat.st_mode)
        || stat.st_uid != unsafe { libc::geteuid() }
        || stat.st_nlink != expected_links
        || stat.st_mode & 0o777 != MODE_FILE
        || stat.st_flags != 0
        || stat.st_mode & (libc::S_ISUID | libc::S_ISGID) != 0
    {
        return Err(MacArtifactStoreError::Integrity);
    }
    reject_xattrs(file.as_raw_fd())?;
    reject_acl(file.as_raw_fd())?;
    let size = u64::try_from(stat.st_size).map_err(|_| MacArtifactStoreError::Integrity)?;
    if size > MAX_PREIMAGE_BYTES_PER_FILE {
        return Err(MacArtifactStoreError::LimitExceeded);
    }
    let mut identity = Vec::with_capacity(17);
    identity.push(1);
    identity.extend_from_slice(&(stat.st_dev as i64).to_le_bytes());
    identity.extend_from_slice(&(stat.st_ino as u64).to_le_bytes());
    let mut fields = BTreeMap::new();
    fields.insert(
        "mode".to_owned(),
        (stat.st_mode as u32).to_le_bytes().to_vec(),
    );
    fields.insert(
        "uid".to_owned(),
        (stat.st_uid as u32).to_le_bytes().to_vec(),
    );
    fields.insert(
        "gid".to_owned(),
        (stat.st_gid as u32).to_le_bytes().to_vec(),
    );
    fields.insert(
        "file_id".to_owned(),
        (stat.st_ino as u64).to_le_bytes().to_vec(),
    );
    let metadata = RegularFileMetadataV1 {
        schema_version: 1,
        adapter: "macos".to_owned(),
        file_identity: identity,
        link_count: 1,
        fields,
    };
    metadata
        .validate()
        .map_err(|_| MacArtifactStoreError::ArtifactStoreUnsafe)?;

    let mut reader = file.try_clone().map_err(MacArtifactStoreError::from)?;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(MacArtifactStoreError::from)?;
    let mut reader = reader.take(MAX_PREIMAGE_BYTES_PER_FILE.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(MacArtifactStoreError::from)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or(MacArtifactStoreError::LimitExceeded)?;
        if total > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(MacArtifactStoreError::LimitExceeded);
        }
        hasher.update(&buffer[..count]);
    }
    if total != size {
        return Err(MacArtifactStoreError::Integrity);
    }
    Ok(ArtifactSnapshot {
        size,
        sha256: Sha256Digest(hasher.finalize().into()),
        metadata,
    })
}

fn walk_absolute_directory(
    path: &Path,
) -> Result<(File, Vec<PhysicalRootId>), MacArtifactStoreError> {
    if !path.is_absolute() {
        return Err(MacArtifactStoreError::InvalidPath);
    }
    let root = open_dir(-1, b"/")?;
    let mut current = root;
    let mut ids = vec![physical_id(&fstat(current.as_raw_fd())?)];
    for component in path.components() {
        let name = match component {
            std::path::Component::RootDir => continue,
            std::path::Component::Normal(name) => name.as_bytes(),
            _ => return Err(MacArtifactStoreError::InvalidPath),
        };
        current = open_dir(current.as_raw_fd(), name)?;
        ids.push(physical_id(&fstat(current.as_raw_fd())?));
    }
    Ok((current, ids))
}

fn overlaps_excluding_root(a: &[PhysicalRootId], b: &[PhysicalRootId]) -> bool {
    let a = &a[1..];
    let b = &b[1..];
    is_prefix(a, b) || is_prefix(b, a)
}

fn is_prefix(prefix: &[PhysicalRootId], value: &[PhysicalRootId]) -> bool {
    prefix.len() <= value.len() && prefix.iter().zip(value).all(|(left, right)| left == right)
}

fn open_dir(parent: RawFd, name: &[u8]) -> Result<File, MacArtifactStoreError> {
    let name = CString::new(name).map_err(|_| MacArtifactStoreError::InvalidPath)?;
    let fd = if parent < 0 {
        unsafe {
            libc::open(
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        }
    } else {
        unsafe {
            libc::openat(
                parent,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        }
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    #[cfg(test)]
    assert_cloexec(fd);
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(test)]
fn assert_cloexec(fd: RawFd) {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0 && flags & libc::FD_CLOEXEC != 0);
}

fn open_lock(app_data: &File) -> Result<File, MacArtifactStoreError> {
    let name = CString::new(LOCK_FILE).unwrap();
    let fd = unsafe {
        libc::openat(
            app_data.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            MODE_FILE as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(MacArtifactStoreError::LockUnavailable);
    }
    #[cfg(test)]
    assert_cloexec(fd);
    let lock = unsafe { File::from_raw_fd(fd) };
    validate_file(&lock, MODE_FILE)?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(MacArtifactStoreError::LockUnavailable);
    }
    Ok(lock)
}

fn mkdir_validated(parent: &File, name: &[u8]) -> Result<File, MacArtifactStoreError> {
    let name = CString::new(name).unwrap();
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), MODE_DIR) };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error.into());
        }
    }
    // Directory creation is durable before its descriptor is exposed.
    fsync_dir(parent)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let dir = unsafe { File::from_raw_fd(fd) };
    validate_dir(&dir)?;
    Ok(dir)
}

fn validate_dir(file: &File) -> Result<(), MacArtifactStoreError> {
    let stat = fstat(file.as_raw_fd())?;
    if !is_directory(stat.st_mode)
        || stat.st_uid != unsafe { libc::geteuid() }
        || stat.st_nlink < 2
        || stat.st_flags != 0
        || stat.st_mode & 0o777 != MODE_DIR
    {
        return Err(MacArtifactStoreError::Integrity);
    }
    reject_xattrs(file.as_raw_fd())?;
    reject_acl(file.as_raw_fd())?;
    Ok(())
}

fn validate_file(file: &File, mode: libc::mode_t) -> Result<(), MacArtifactStoreError> {
    let stat = fstat(file.as_raw_fd())?;
    if !is_regular(stat.st_mode)
        || stat.st_uid != unsafe { libc::geteuid() }
        || stat.st_nlink != 1
        || stat.st_flags != 0
        || stat.st_mode & 0o777 != mode
    {
        return Err(MacArtifactStoreError::Integrity);
    }
    reject_xattrs(file.as_raw_fd())?;
    reject_acl(file.as_raw_fd())?;
    Ok(())
}

fn ensure_fs(file: &File) -> Result<(), MacArtifactStoreError> {
    let mut statfs = unsafe { std::mem::zeroed::<libc::statfs>() };
    if unsafe { libc::fstatfs(file.as_raw_fd(), &mut statfs) } != 0 {
        return Err(MacArtifactStoreError::AdapterUnsupported);
    }
    let name = unsafe { std::ffi::CStr::from_ptr(statfs.f_fstypename.as_ptr()) }.to_bytes();
    if name == b"apfs" || name == b"hfs" {
        Ok(())
    } else {
        Err(MacArtifactStoreError::AdapterUnsupported)
    }
}

fn fsync_dir(file: &File) -> Result<(), MacArtifactStoreError> {
    if unsafe { libc::fsync(file.as_raw_fd()) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}
fn full_sync(file: &File) -> Result<(), MacArtifactStoreError> {
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } != 0 {
        #[cfg(test)]
        eprintln!("fullsync errno={:?}", io::Error::last_os_error());
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

fn reject_xattrs(fd: RawFd) -> Result<(), MacArtifactStoreError> {
    let size = unsafe { libc::flistxattr(fd, std::ptr::null_mut(), 0, 0) };
    if size == 0 {
        return Ok(());
    }
    if size < 0 || size > 64 * 1024 {
        return Err(MacArtifactStoreError::ExtendedMetadataUnsupported);
    }
    let mut names = vec![0_u8; size as usize];
    let actual =
        unsafe { libc::flistxattr(fd, names.as_mut_ptr() as *mut libc::c_char, names.len(), 0) };
    #[cfg(test)]
    if actual > 0
        && names[..actual as usize]
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .eq([b"com.apple.provenance".as_slice()].into_iter())
    {
        return Ok(());
    }
    #[cfg(not(test))]
    let _ = actual;
    Err(MacArtifactStoreError::ExtendedMetadataUnsupported)
}

fn reject_acl(fd: RawFd) -> Result<(), MacArtifactStoreError> {
    let acl = unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        let errno = unsafe { *libc::__error() };
        return if errno == ENOENT_MACOS || errno == ENOATTR_MACOS {
            Ok(())
        } else {
            Err(MacArtifactStoreError::ExtendedMetadataUnsupported)
        };
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &mut entry) };
    let errno = unsafe { *libc::__error() };
    unsafe { acl_free(acl) };
    if result == 0 || !entry.is_null() {
        Err(MacArtifactStoreError::ExtendedMetadataUnsupported)
    } else if errno == ENOENT_MACOS || errno == ENOATTR_MACOS {
        Ok(())
    } else {
        Err(MacArtifactStoreError::ExtendedMetadataUnsupported)
    }
}
fn exists(parent: &File, name: &[u8]) -> Result<bool, MacArtifactStoreError> {
    let name = CString::new(name).map_err(|_| MacArtifactStoreError::InvalidPath)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ENOENT_MACOS) {
            Ok(false)
        } else {
            Err(error.into())
        };
    }
    unsafe { libc::close(fd) };
    Ok(true)
}
fn unlink(parent: &File, name: &[u8]) -> Result<(), MacArtifactStoreError> {
    let name = CString::new(name).map_err(|_| MacArtifactStoreError::InvalidPath)?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::NotFound {
            return Err(MacArtifactStoreError::NotFound);
        }
        return Err(e.into());
    }
    Ok(())
}
fn same_inode(a: &File, b: &File) -> Result<bool, MacArtifactStoreError> {
    let left = fstat(a.as_raw_fd())?;
    let right = fstat(b.as_raw_fd())?;
    Ok(left.st_dev == right.st_dev && left.st_ino == right.st_ino)
}
fn fstat(fd: RawFd) -> Result<libc::stat, MacArtifactStoreError> {
    let mut stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut stat) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(stat)
}
fn physical_id(stat: &libc::stat) -> PhysicalRootId {
    let mut out = vec![1, 1];
    out.extend_from_slice(&(stat.st_dev as i64).to_le_bytes());
    out.extend_from_slice(&(stat.st_ino as u64).to_le_bytes());
    PhysicalRootId(out)
}
fn is_regular(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFREG
}
fn is_directory(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFDIR
}
fn key_name(key: ArtifactKey) -> CString {
    let mut text = String::with_capacity(32);
    for byte in key.0 {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
    }
    CString::new(text).unwrap()
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum FaultPoint {
    Write,
    FullSync,
    Link,
    AfterLink,
    ObjectsFsync,
    Verify,
    Unlink,
}

extern "C" {
    fn acl_get_fd_np(fd: libc::c_int, acl_type: libc::c_int) -> *mut libc::c_void;
    fn acl_get_entry(
        acl: *mut libc::c_void,
        entry_id: libc::c_int,
        entry: *mut *mut libc::c_void,
    ) -> libc::c_int;
    fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir_in;

    fn setup() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        MacWorkspaceRoot,
        MacArtifactStore,
    ) {
        let workspace_dir = tempfile::tempdir_in("/private/tmp").unwrap();
        let app_data_dir = tempfile::tempdir_in("/private/tmp").unwrap();
        let workspace_path = fs::canonicalize(workspace_dir.path()).unwrap();
        let workspace = MacWorkspaceRoot::open_absolute(&workspace_path).unwrap();
        let store = MacArtifactStore::open(app_data_dir.path(), &workspace).unwrap();
        (workspace_dir, app_data_dir, workspace, store)
    }

    #[test]
    fn stages_publishes_verifies_and_cleans_opaque_bytes() {
        let (_workspace_dir, _app_data_dir, _workspace, store) = setup();
        let bytes = CapturedBytes::from_slice(b"fixture-secret");
        let staged = store.stage(&bytes, 1024).unwrap();
        let state = store.publish(&staged).unwrap();
        let published = match state {
            PublishState::Published(value) => value,
            PublishState::PublishedCleanupPending(_) => panic!("cleanup should complete"),
            PublishState::StagedOnly(_) => panic!("publish must not leave staging-only state"),
        };
        assert_eq!(published.size, bytes.as_slice().len() as u64);
        assert_eq!(
            store
                .verify(published.key, published.size, published.sha256)
                .unwrap(),
            published
        );
        store.cleanup_verified(&published).unwrap();
        assert_eq!(
            store.verify(published.key, published.size, published.sha256),
            Err(MacArtifactStoreError::NotFound)
        );
    }

    #[test]
    fn rejects_workspace_app_data_containment() {
        let parent = tempdir_in("/private/tmp").unwrap();
        let workspace_path = parent.path().join("workspace");
        let app_path = workspace_path.join("app-data");
        fs::create_dir(&workspace_path).unwrap();
        fs::create_dir(&app_path).unwrap();
        let workspace = MacWorkspaceRoot::open_absolute(&workspace_path).unwrap();
        assert!(matches!(
            MacArtifactStore::open(&app_path, &workspace),
            Err(MacArtifactStoreError::InvalidPath)
        ));
    }

    #[test]
    fn rejects_reverse_containment_and_symlink_aliases() {
        let parent = tempdir_in("/private/tmp").unwrap();
        let app_path = parent.path().join("app-data");
        let workspace_path = app_path.join("workspace");
        fs::create_dir(&app_path).unwrap();
        fs::create_dir(&workspace_path).unwrap();
        let workspace = MacWorkspaceRoot::open_absolute(&workspace_path).unwrap();
        assert!(matches!(
            MacArtifactStore::open(&app_path, &workspace),
            Err(MacArtifactStoreError::InvalidPath)
        ));

        let alias = parent.path().join("alias");
        std::os::unix::fs::symlink(&app_path, &alias).unwrap();
        assert!(MacArtifactStore::open(&alias, &workspace).is_err());
    }

    #[test]
    fn size_limit_is_central_and_staged_cleanup_validates_provenance() {
        let (_workspace_dir, _app_data_dir, _workspace, store) = setup();
        let bytes = CapturedBytes::from_slice(b"bounded");
        let staged = store.stage(&bytes, MAX_PREIMAGE_BYTES_PER_FILE).unwrap();
        assert_eq!(
            store.stage(
                &CapturedBytes::from_slice(&vec![0; (MAX_PREIMAGE_BYTES_PER_FILE + 1) as usize]),
                u64::MAX
            ),
            Err(MacArtifactStoreError::LimitExceeded)
        );
        let mut tampered = staged.clone();
        tampered.size += 1;
        assert_eq!(
            store.cleanup_staged(&tampered),
            Err(MacArtifactStoreError::Integrity)
        );
        store.cleanup_staged(&staged).unwrap();
    }

    #[test]
    fn reconciliation_never_reports_staging_only_as_cleanup_pending() {
        let (_workspace_dir, _app_data_dir, _workspace, store) = setup();
        let staged = store
            .stage(&CapturedBytes::from_slice(b"recover"), 1024)
            .unwrap();
        match store.reconcile_publish(&staged).unwrap() {
            PublishState::Published(_) => {}
            PublishState::PublishedCleanupPending(_) => panic!("staging-only is not pending"),
            PublishState::StagedOnly(_) => panic!("reconciliation retries the link"),
        }
    }

    #[test]
    fn diagnostics_redact_all_artifact_locators_hashes_and_metadata() {
        let (_workspace_dir, _app_data_dir, _workspace, store) = setup();
        let staged = store
            .stage(&CapturedBytes::from_slice(b"private"), 1024)
            .unwrap();
        let staged_debug = format!("{:?}", staged);
        assert!(staged_debug.contains("redacted"));
        assert!(!staged_debug.contains(&format!("{:?}", staged.key)));
        assert!(!staged_debug.contains(&staged.sha256.to_hex()));
        let error = MacArtifactStoreError::PublishReconciliationRequired(staged);
        let debug = format!("{:?}", error);
        assert!(debug.contains("redacted"));
        assert!(!debug.contains("PublishReconciliationRequired(StagedArtifact"));
    }

    #[test]
    fn instance_lock_conflicts_and_releases_and_store_descriptors_are_safe() {
        let (workspace_dir, app_data_dir, workspace, store) = setup();
        assert_eq!(
            fstat(store.staging.as_raw_fd()).unwrap().st_mode & 0o777,
            MODE_DIR
        );
        assert_eq!(
            fstat(store.objects.as_raw_fd()).unwrap().st_mode & 0o777,
            MODE_DIR
        );
        assert!(matches!(
            MacArtifactStore::open(app_data_dir.path(), &workspace),
            Err(MacArtifactStoreError::LockUnavailable)
        ));
        #[cfg(test)]
        assert_cloexec(store.staging.as_raw_fd());
        drop(store);
        drop(workspace);
        drop(workspace_dir);
    }

    #[test]
    fn instance_lock_is_exclusive_across_processes() {
        let (workspace_dir, app_data_dir, workspace, store) = setup();
        let child = |expect_conflict: bool| {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "guarded_undo::macos_store::tests::lock_child",
                    "--nocapture",
                ])
                .env("DCC_LOCK_APP", app_data_dir.path())
                .env("DCC_LOCK_WORKSPACE", workspace_dir.path())
                .env(
                    "DCC_LOCK_EXPECT_CONFLICT",
                    if expect_conflict { "1" } else { "0" },
                )
                .status()
                .unwrap();
            assert!(status.success());
        };
        child(true);
        drop(store);
        drop(workspace);
        child(false);
        drop(workspace_dir);
    }

    #[test]
    fn lock_child() {
        let (Ok(app), Ok(workspace), Ok(expect)) = (
            std::env::var("DCC_LOCK_APP"),
            std::env::var("DCC_LOCK_WORKSPACE"),
            std::env::var("DCC_LOCK_EXPECT_CONFLICT"),
        ) else {
            return;
        };
        let workspace = MacWorkspaceRoot::open_absolute(Path::new(&workspace)).unwrap();
        let result = MacArtifactStore::open(Path::new(&app), &workspace);
        if expect == "1" {
            assert!(matches!(
                result,
                Err(MacArtifactStoreError::LockUnavailable)
            ));
        } else {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn symlink_intermediate_is_rejected_and_hardlink_tamper_is_not_cleaned() {
        let parent = tempdir_in("/private/tmp").unwrap();
        let real = parent.path().join("real");
        let app = real.join("app");
        fs::create_dir(&real).unwrap();
        fs::create_dir(&app).unwrap();
        let alias = parent.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let workspace_dir = tempfile::tempdir_in("/private/tmp").unwrap();
        let workspace = MacWorkspaceRoot::open_absolute(workspace_dir.path()).unwrap();
        assert!(MacArtifactStore::open(&alias.join("app"), &workspace).is_err());

        let store = MacArtifactStore::open(&app, &workspace).unwrap();
        let staged = store
            .stage(&CapturedBytes::from_slice(b"hardlink"), 1024)
            .unwrap();
        let name = key_name(staged.key);
        let extra = CString::new("extra-hardlink").unwrap();
        assert_eq!(
            unsafe {
                libc::linkat(
                    store.staging.as_raw_fd(),
                    name.as_ptr(),
                    store.staging.as_raw_fd(),
                    extra.as_ptr(),
                    0,
                )
            },
            0
        );
        assert_eq!(
            store.cleanup_staged(&staged),
            Err(MacArtifactStoreError::Integrity)
        );
        unsafe { libc::unlinkat(store.staging.as_raw_fd(), extra.as_ptr(), 0) };
        store.cleanup_staged(&staged).unwrap();
    }

    #[test]
    fn published_bytes_are_rechecked_after_tamper_and_mode_change() {
        let (_workspace_dir, _app_data_dir, _workspace, store) = setup();
        let staged = store
            .stage(&CapturedBytes::from_slice(b"tamper-me"), 1024)
            .unwrap();
        let name = key_name(staged.key);
        let fd = unsafe {
            libc::openat(
                store.staging.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CLOEXEC,
            )
        };
        assert!(fd >= 0);
        assert_eq!(unsafe { libc::fchmod(fd, 0o644) }, 0);
        unsafe { libc::close(fd) };
        assert_eq!(
            store.publish(&staged),
            Err(MacArtifactStoreError::Integrity)
        );
        unsafe { libc::unlinkat(store.staging.as_raw_fd(), name.as_ptr(), 0) };
        fsync_dir(&store.staging).unwrap();

        let staged = store
            .stage(&CapturedBytes::from_slice(b"tamper-me"), 1024)
            .unwrap();
        let published = match store.publish(&staged).unwrap() {
            PublishState::Published(value) => value,
            _ => panic!("unexpected publish state"),
        };
        let name = key_name(published.key);
        let fd = unsafe {
            libc::openat(
                store.objects.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CLOEXEC,
            )
        };
        assert!(fd >= 0);
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.write_all(b"X").unwrap();
        file.sync_all().unwrap();
        assert_eq!(
            store.verify(published.key, published.size, published.sha256),
            Err(MacArtifactStoreError::Integrity)
        );
        unsafe { libc::unlinkat(store.objects.as_raw_fd(), name.as_ptr(), 0) };
        fsync_dir(&store.objects).unwrap();
    }

    #[test]
    fn reconcile_rejects_an_external_hardlink_on_final_only_object() {
        let (_workspace_dir, _app_data_dir, _workspace, store) = setup();
        let staged = store
            .stage(&CapturedBytes::from_slice(b"final-link"), 1024)
            .unwrap();
        let published = match store.publish(&staged).unwrap() {
            PublishState::Published(value) => value,
            _ => panic!("unexpected publish state"),
        };
        let name = key_name(published.key);
        let extra = CString::new("external-final-hardlink").unwrap();
        assert_eq!(
            unsafe {
                libc::linkat(
                    store.objects.as_raw_fd(),
                    name.as_ptr(),
                    store.objects.as_raw_fd(),
                    extra.as_ptr(),
                    0,
                )
            },
            0
        );
        assert_eq!(
            store.reconcile_publish(&staged),
            Err(MacArtifactStoreError::Integrity)
        );
        unsafe { libc::unlinkat(store.objects.as_raw_fd(), extra.as_ptr(), 0) };
        unsafe { libc::unlinkat(store.objects.as_raw_fd(), name.as_ptr(), 0) };
        fsync_dir(&store.objects).unwrap();
    }

    #[test]
    fn deterministic_key_collision_and_fault_boundaries_are_recoverable() {
        let (_workspace_dir, _app_data_dir, _workspace, store) = setup();
        let bytes = CapturedBytes::from_slice(b"fault-boundary");
        let key = ArtifactKey([9; 16]);
        let staged = store.stage_with_key(&bytes, 1024, key).unwrap();
        let name = key_name(key);
        let fd = unsafe {
            libc::openat(
                store.objects.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
                MODE_FILE as libc::c_uint,
            )
        };
        assert!(fd >= 0);
        unsafe { libc::close(fd) };
        assert_eq!(
            store.publish(&staged),
            Err(MacArtifactStoreError::Collision)
        );
        unsafe { libc::unlinkat(store.objects.as_raw_fd(), name.as_ptr(), 0) };
        store.cleanup_staged(&staged).unwrap();

        for (key, fault) in [
            (ArtifactKey([10; 16]), FaultPoint::Write),
            (ArtifactKey([11; 16]), FaultPoint::FullSync),
        ] {
            assert!(store.stage_with_fault(&bytes, 1024, key, fault).is_err());
            assert!(!exists(&store.staging, key_name(key).as_bytes()).unwrap());
        }

        let staged = store
            .stage_with_key(&bytes, 1024, ArtifactKey([12; 16]))
            .unwrap();
        assert!(matches!(
            store.publish_with_fault(&staged, FaultPoint::Link),
            Err(MacArtifactStoreError::Io(_))
        ));
        assert!(!exists(&store.objects, key_name(staged.key).as_bytes()).unwrap());
        let error = store
            .publish_with_fault(&staged, FaultPoint::AfterLink)
            .unwrap_err();
        assert!(matches!(
            error,
            MacArtifactStoreError::PublishReconciliationRequired(_)
        ));
        let recovered = match store.reconcile_publish(&staged).unwrap() {
            PublishState::Published(value) => value,
            _ => panic!("reconciliation must finish with a single final link"),
        };
        assert_eq!(
            store.verify(recovered.key, recovered.size, recovered.sha256),
            Ok(recovered)
        );

        for (key, fault) in [
            (ArtifactKey([13; 16]), FaultPoint::ObjectsFsync),
            (ArtifactKey([14; 16]), FaultPoint::Verify),
            (ArtifactKey([15; 16]), FaultPoint::Unlink),
        ] {
            let staged = store.stage_with_key(&bytes, 1024, key).unwrap();
            assert!(matches!(
                store.publish_with_fault(&staged, fault),
                Err(MacArtifactStoreError::PublishReconciliationRequired(_))
            ));
            assert!(matches!(
                store.reconcile_publish(&staged),
                Ok(PublishState::Published(_))
            ));
        }
    }
}
