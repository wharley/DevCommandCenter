//! Transactional, content-only artifacts for applying a delegation worktree.
//!
//! This module deliberately has no journal/database dependency.  The command
//! layer can persist the returned digest and state around these operations,
//! while this engine keeps the filesystem part deterministic and fail-closed.

use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::git::{run_git_output_with_timeout, GIT_LOCAL_TIMEOUT};

pub(crate) const MAX_ARTIFACT_FILES: usize = 10_000;
pub(crate) const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApplyIdentity {
    pub head: String,
    pub branch: Option<String>,
    pub index_tree: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedApply {
    pub transaction_id: String,
    pub manifest_digest: String,
    pub file_count: usize,
    pub artifact_bytes: u64,
    pub changed_files: Vec<String>,
    pub source_identity: ApplyIdentity,
    pub destination_identity: ApplyIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApplyClassification {
    AllPre,
    AllPost,
    MixedKnown,
    Divergent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApplyOutput {
    pub changed_files: Vec<String>,
    pub artifact_bytes: u64,
}

/// A process-held advisory lock for a delegation operation.  The lock file is
/// intentionally retained after release; only the OS lock state is
/// authoritative, so a crash cannot leave a stale lock blocking recovery.
pub(crate) struct ApplyOperationLock {
    _file: File,
}

impl Drop for ApplyOperationLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

/// Attempts to acquire the cross-process operation lock.  `Ok(None)` means a
/// live process currently owns the operation; callers should treat that as a
/// conflict rather than waiting or taking over its artifacts.
pub(crate) fn try_lock_apply_operation(
    artifact_root: &Path,
    operation_id: &str,
) -> Result<Option<ApplyOperationLock>, String> {
    if operation_id.is_empty() {
        return Err("delegation operation ID must not be empty".to_string());
    }
    ensure_artifact_root(artifact_root)?;
    let locks = artifact_root.join(".locks");
    ensure_lock_directory(&locks)?;
    let lock_path = locks.join(sha256_hex(operation_id.as_bytes()));
    match fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err("delegation operation lock is not a regular file".to_string())
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect delegation operation lock: {error}"
            ))
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("failed to open delegation operation lock: {error}"))?;
    set_private_file_mode(&file)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(ApplyOperationLock { _file: file })),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(format!(
            "failed to acquire delegation operation lock: {error}"
        )),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    transaction_id: String,
    source: ApplyIdentity,
    destination: ApplyIdentity,
    files: Vec<FileArtifact>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FileArtifact {
    path: String,
    pre: Snapshot,
    post: Snapshot,
    pre_payload: Option<String>,
    post_payload: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    kind: SnapshotKind,
    digest: String,
    bytes: u64,
    file_mode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotKind {
    Missing,
    Regular,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitSide {
    Source,
    Destination,
}

/// Captures every changed path, including preimages, before any destination
/// write.  `artifact_root/transaction_id` is the only directory this module
/// creates; transaction IDs must be UUIDs so cleanup cannot escape the root.
pub(crate) fn prepare_apply_artifacts(
    transaction_id: &str,
    destination_root: &Path,
    source_root: &Path,
    artifact_root: &Path,
) -> Result<PreparedApply, String> {
    validate_transaction_id(transaction_id)?;
    ensure_git_root(destination_root, GitSide::Destination)?;
    ensure_git_root(source_root, GitSide::Source)?;
    ensure_artifact_root(artifact_root)?;

    let destination_identity = inspect_identity(destination_root, GitSide::Destination)?;
    let source_identity = inspect_identity(source_root, GitSide::Source)?;
    ensure_clean_repository(destination_root, GitSide::Destination)?;
    ensure_no_operation_in_progress(destination_root)?;
    ensure_no_operation_in_progress(source_root)?;
    if destination_identity.head != source_identity.head {
        return Err("destination and delegation HEAD differ from the same baseline".to_string());
    }

    let mut paths = tracked_changed_paths(source_root)?;
    paths.extend(untracked_paths(source_root)?);
    paths.sort();
    paths.dedup();
    validate_case_distinct_paths(&paths)?;
    if paths.is_empty() {
        return Err("delegation worktree has no changes to apply".to_string());
    }
    if paths.len() > MAX_ARTIFACT_FILES {
        return Err(format!(
            "delegation apply exceeds the {} file limit",
            MAX_ARTIFACT_FILES
        ));
    }

    let transaction_dir = transaction_dir(artifact_root, transaction_id)?;
    if transaction_dir.exists() {
        return Err("delegation apply transaction already exists".to_string());
    }
    create_private_dir(&transaction_dir)?;
    let pre_dir = transaction_dir.join("pre");
    let post_dir = transaction_dir.join("post");
    create_private_dir(&pre_dir)?;
    create_private_dir(&post_dir)?;

    let result = (|| {
        let mut files = Vec::with_capacity(paths.len());
        let mut artifact_bytes = 0_u64;
        for (ordinal, path) in paths.iter().enumerate() {
            validate_relative_path(path)?;
            reject_submodule(source_root, path)?;
            let (pre, pre_bytes) = capture_snapshot(destination_root, path)?;
            let (post, post_bytes) = capture_snapshot(source_root, path)?;
            artifact_bytes = artifact_bytes
                .checked_add(pre_bytes.as_ref().map_or(0, Vec::len) as u64)
                .and_then(|value| value.checked_add(post_bytes.as_ref().map_or(0, Vec::len) as u64))
                .ok_or_else(|| "delegation apply artifact size overflow".to_string())?;
            if artifact_bytes > MAX_ARTIFACT_BYTES {
                return Err(format!(
                    "delegation apply exceeds the {} MiB artifact limit",
                    MAX_ARTIFACT_BYTES / (1024 * 1024)
                ));
            }

            let pre_payload = pre_bytes.map(|bytes| {
                let name = format!("{ordinal:08}.bin");
                (name, bytes)
            });
            let post_payload = post_bytes.map(|bytes| {
                let name = format!("{ordinal:08}.bin");
                (name, bytes)
            });
            if let Some((name, bytes)) = pre_payload.as_ref() {
                write_atomic_private(&pre_dir.join(name), bytes)?;
            }
            if let Some((name, bytes)) = post_payload.as_ref() {
                write_atomic_private(&post_dir.join(name), bytes)?;
            }
            files.push(FileArtifact {
                path: path.clone(),
                pre,
                post,
                pre_payload: pre_payload.map(|(name, _)| format!("pre/{name}")),
                post_payload: post_payload.map(|(name, _)| format!("post/{name}")),
            });
        }

        let manifest = Manifest {
            version: 1,
            transaction_id: transaction_id.to_string(),
            source: source_identity.clone(),
            destination: destination_identity.clone(),
            files,
        };
        let bytes = serde_json::to_vec(&manifest)
            .map_err(|error| format!("failed to serialize apply manifest: {error}"))?;
        let digest = sha256_hex(&bytes);
        write_atomic_private(&transaction_dir.join("manifest.json"), &bytes)?;
        write_atomic_private(
            &transaction_dir.join("manifest.sha256"),
            format!("{digest}\n").as_bytes(),
        )?;
        sync_directory(&transaction_dir)?;

        let changed_files = manifest
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        Ok(PreparedApply {
            transaction_id: transaction_id.to_string(),
            manifest_digest: digest,
            file_count: manifest.files.len(),
            artifact_bytes,
            changed_files,
            source_identity,
            destination_identity,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&transaction_dir);
    }
    result
}

/// Applies frozen payloads only after revalidating both Git identities.  The
/// index is never written: destination changes are installed with same-parent
/// temp-file rename and executable bits are set on the temp file first.
pub(crate) fn apply_prepared_artifacts(
    transaction_id: &str,
    destination_root: &Path,
    source_root: &Path,
    artifact_root: &Path,
    expected_manifest_digest: &str,
) -> Result<ApplyOutput, String> {
    let (manifest, transaction_dir) =
        load_manifest(transaction_id, artifact_root, expected_manifest_digest)?;
    ensure_git_root(destination_root, GitSide::Destination)?;
    ensure_git_root(source_root, GitSide::Source)?;
    ensure_no_operation_in_progress(destination_root)?;
    ensure_no_operation_in_progress(source_root)?;
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    ensure_identity(source_root, &manifest.source, GitSide::Source)?;
    reconcile_destination_temporary_files(transaction_id, destination_root, &manifest)?;
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    ensure_clean_repository(destination_root, GitSide::Destination)?;

    match classify_manifest(&manifest, destination_root)? {
        ApplyClassification::AllPre => {}
        ApplyClassification::AllPost => {
            return Ok(ApplyOutput {
                changed_files: manifest
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect(),
                artifact_bytes: manifest_artifact_bytes(&manifest),
            })
        }
        ApplyClassification::MixedKnown => {
            return Err("destination contains a partially applied delegation".to_string())
        }
        ApplyClassification::Divergent => {
            return Err("destination diverged from the frozen delegation artifact".to_string())
        }
    }

    for file in &manifest.files {
        let payload = load_payload(&transaction_dir, file.post_payload.as_deref(), &file.post)?;
        install_snapshot(
            transaction_id,
            destination_root,
            &file.path,
            &file.post,
            payload.as_deref(),
        )?;
    }
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    if classify_manifest(&manifest, destination_root)? != ApplyClassification::AllPost {
        return Err("delegation apply did not produce the frozen postimage".to_string());
    }
    Ok(ApplyOutput {
        changed_files: manifest
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect(),
        artifact_bytes: manifest_artifact_bytes(&manifest),
    })
}

pub(crate) fn classify_apply_artifacts(
    transaction_id: &str,
    destination_root: &Path,
    artifact_root: &Path,
    expected_manifest_digest: &str,
) -> Result<ApplyClassification, String> {
    let (manifest, _) = load_manifest(transaction_id, artifact_root, expected_manifest_digest)?;
    ensure_git_root(destination_root, GitSide::Destination)?;
    ensure_no_operation_in_progress(destination_root)?;
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    reconcile_destination_temporary_files(transaction_id, destination_root, &manifest)?;
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    Ok(classify_manifest(&manifest, destination_root)?)
}

/// Restores only paths whose current state is a known pre/post/missing state.
/// Classification happens for every file before the first write, so a
/// divergent external edit prevents all rollback writes.
pub(crate) fn rollback_apply_artifacts(
    transaction_id: &str,
    destination_root: &Path,
    artifact_root: &Path,
    expected_manifest_digest: &str,
) -> Result<(), String> {
    let (manifest, transaction_dir) =
        load_manifest(transaction_id, artifact_root, expected_manifest_digest)?;
    ensure_git_root(destination_root, GitSide::Destination)?;
    ensure_no_operation_in_progress(destination_root)?;
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    reconcile_destination_temporary_files(transaction_id, destination_root, &manifest)?;
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    let classification = classify_manifest(&manifest, destination_root)?;
    if classification == ApplyClassification::Divergent {
        return Err("refusing rollback because destination diverged".to_string());
    }
    if classification == ApplyClassification::AllPre {
        return Ok(());
    }

    let mut states = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        let current = current_snapshot(destination_root, &file.path)?;
        if current != file.pre && current != file.post {
            return Err("refusing rollback because destination diverged".to_string());
        }
        states.push(current);
    }
    for (file, current) in manifest.files.iter().zip(states.iter()) {
        if current == &file.pre {
            continue;
        }
        let payload = load_payload(&transaction_dir, file.pre_payload.as_deref(), &file.pre)?;
        install_snapshot(
            transaction_id,
            destination_root,
            &file.path,
            &file.pre,
            payload.as_deref(),
        )?;
    }
    ensure_identity(
        destination_root,
        &manifest.destination,
        GitSide::Destination,
    )?;
    if classify_manifest(&manifest, destination_root)? != ApplyClassification::AllPre {
        return Err("delegation rollback did not restore the frozen preimage".to_string());
    }
    Ok(())
}

pub(crate) fn cleanup_apply_artifacts(
    transaction_id: &str,
    artifact_root: &Path,
) -> Result<(), String> {
    validate_transaction_id(transaction_id)?;
    ensure_artifact_root(artifact_root)?;
    let transaction_dir = transaction_dir(artifact_root, transaction_id)?;
    if transaction_dir.exists() {
        fs::remove_dir_all(&transaction_dir)
            .map_err(|error| format!("failed to remove delegation apply artifacts: {error}"))?;
    }
    Ok(())
}

fn classify_manifest(
    manifest: &Manifest,
    destination_root: &Path,
) -> Result<ApplyClassification, String> {
    let mut any_pre = false;
    let mut any_post = false;
    for file in &manifest.files {
        let current = match current_snapshot(destination_root, &file.path) {
            Ok(snapshot) => snapshot,
            Err(_) => return Ok(ApplyClassification::Divergent),
        };
        let is_pre = current == file.pre;
        let is_post = current == file.post;
        if !is_pre && !is_post {
            return Ok(ApplyClassification::Divergent);
        }
        any_pre |= is_pre && !is_post;
        any_post |= is_post && !is_pre;
    }
    match (any_pre, any_post) {
        (false, true) => Ok(ApplyClassification::AllPost),
        (true, false) => Ok(ApplyClassification::AllPre),
        (true, true) => Ok(ApplyClassification::MixedKnown),
        // Every file has identical pre/post content. Treat it as post so an
        // idempotent retry can complete its journal transition.
        (false, false) => Ok(ApplyClassification::AllPost),
    }
}

fn manifest_artifact_bytes(manifest: &Manifest) -> u64 {
    manifest
        .files
        .iter()
        .map(|file| file.pre.bytes.saturating_add(file.post.bytes))
        .fold(0, u64::saturating_add)
}

fn load_manifest(
    transaction_id: &str,
    artifact_root: &Path,
    expected_manifest_digest: &str,
) -> Result<(Manifest, PathBuf), String> {
    validate_transaction_id(transaction_id)?;
    ensure_artifact_root(artifact_root)?;
    let transaction_dir = transaction_dir(artifact_root, transaction_id)?;
    let manifest_path = transaction_dir.join("manifest.json");
    let bytes = fs::read(&manifest_path)
        .map_err(|error| format!("failed to read delegation apply manifest: {error}"))?;
    let expected = fs::read_to_string(transaction_dir.join("manifest.sha256"))
        .map_err(|error| format!("failed to read delegation manifest digest: {error}"))?;
    let sidecar_digest = expected.trim();
    let expected_manifest_digest = expected_manifest_digest.trim();
    let calculated_digest = sha256_hex(&bytes);
    if expected_manifest_digest.len() != 64
        || !is_hex(expected_manifest_digest)
        || sidecar_digest != calculated_digest
        || expected_manifest_digest != calculated_digest
    {
        return Err("delegation apply manifest digest mismatch".to_string());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("delegation apply manifest is corrupt: {error}"))?;
    if manifest.version != 1 || manifest.transaction_id != transaction_id {
        return Err("delegation apply manifest transaction mismatch".to_string());
    }
    validate_manifest(&manifest)?;
    Ok((manifest, transaction_dir))
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.files.is_empty() || manifest.files.len() > MAX_ARTIFACT_FILES {
        return Err("delegation apply manifest file count is invalid".to_string());
    }
    let manifest_paths: Vec<String> = manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    validate_case_distinct_paths(&manifest_paths)?;
    let mut paths = HashSet::with_capacity(manifest.files.len());
    let mut bytes = 0_u64;
    for file in &manifest.files {
        validate_relative_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err("delegation apply manifest contains duplicate paths".to_string());
        }
        validate_snapshot(&file.pre)?;
        validate_snapshot(&file.post)?;
        bytes = bytes
            .checked_add(file.pre.bytes)
            .and_then(|value| value.checked_add(file.post.bytes))
            .ok_or_else(|| "delegation apply manifest size overflow".to_string())?;
        if bytes > MAX_ARTIFACT_BYTES {
            return Err("delegation apply manifest exceeds size limit".to_string());
        }
        validate_payload_name(file.pre_payload.as_deref(), "pre", &file.pre)?;
        validate_payload_name(file.post_payload.as_deref(), "post", &file.post)?;
    }
    Ok(())
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    if !is_hex(&snapshot.digest) || snapshot.digest.len() != 64 {
        return Err("delegation apply snapshot digest is invalid".to_string());
    }
    if snapshot.kind == SnapshotKind::Missing && (snapshot.bytes != 0 || snapshot.file_mode != 0) {
        return Err("missing delegation snapshot has payload metadata".to_string());
    }
    Ok(())
}

fn validate_payload_name(
    payload: Option<&str>,
    prefix: &str,
    snapshot: &Snapshot,
) -> Result<(), String> {
    match (payload, &snapshot.kind) {
        (None, SnapshotKind::Missing) => Ok(()),
        (Some(value), SnapshotKind::Regular) => {
            let expected_prefix = format!("{prefix}/");
            let name = value.strip_prefix(&expected_prefix).unwrap_or_default();
            if name.len() != 12
                || !name.ends_with(".bin")
                || !name[..8].bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err("delegation apply payload path is invalid".to_string());
            }
            Ok(())
        }
        _ => Err("delegation apply payload does not match snapshot kind".to_string()),
    }
}

fn capture_snapshot(root: &Path, relative: &str) -> Result<(Snapshot, Option<Vec<u8>>), String> {
    validate_relative_path(relative)?;
    ensure_no_symlink_ancestors(root, Path::new(relative))?;
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((missing_snapshot(), None))
        }
        Err(error) => return Err(format!("failed to inspect {}: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "delegation artifact path is not a regular file: {}",
            path.display()
        ));
    }
    ensure_single_link(&metadata, &path)?;
    let bytes =
        fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let snapshot = regular_snapshot(&bytes, file_mode(&metadata));
    Ok((snapshot, Some(bytes)))
}

fn current_snapshot(root: &Path, relative: &str) -> Result<Snapshot, String> {
    capture_snapshot(root, relative).map(|(snapshot, _)| snapshot)
}

fn install_snapshot(
    transaction_id: &str,
    root: &Path,
    relative: &str,
    snapshot: &Snapshot,
    payload: Option<&[u8]>,
) -> Result<(), String> {
    validate_relative_path(relative)?;
    ensure_no_symlink_ancestors(root, Path::new(relative))?;
    let target = root.join(relative);
    match snapshot.kind {
        SnapshotKind::Missing => match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
                format!("refusing to remove non-regular path: {}", target.display()),
            ),
            Ok(_) => fs::remove_file(&target)
                .map_err(|error| format!("failed to remove {}: {error}", target.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("failed to inspect {}: {error}", target.display())),
        },
        SnapshotKind::Regular => {
            let payload =
                payload.ok_or_else(|| "regular delegation snapshot has no payload".to_string())?;
            if sha256_hex(payload) != snapshot.digest || payload.len() as u64 != snapshot.bytes {
                return Err("delegation apply payload digest mismatch".to_string());
            }
            let parent = target
                .parent()
                .ok_or_else(|| "delegation artifact path has no parent".to_string())?;
            create_destination_parent(root, parent)?;
            ensure_no_symlink_ancestors(root, Path::new(relative))?;
            let temp = destination_temp_path(&target, transaction_id, relative)?;
            write_destination_temp(&temp, payload, snapshot.file_mode)?;
            atomic_replace(&temp, &target)
                .map_err(|error| format!("failed to install {}: {error}", target.display()))?;
            sync_directory(parent)?;
            Ok(())
        }
    }
}

/// Replaces a destination file atomically across supported platforms.  Unix
/// rename replaces an existing regular file; Windows requires the explicit
/// MoveFileExW replacement flag (and write-through keeps the rename durable).
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::{iter, os::windows::ffi::OsStrExt};
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        let source: Vec<u16> = source
            .as_os_str()
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        let result = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

/// Removes only a deterministic destination temp left by this transaction.
/// A complete pre/post temp is safe to discard and will be recreated or
/// ignored by the next operation.  Any other contents indicate an interrupted
/// write and must remain in place so recovery can surface it to the caller.
fn reconcile_destination_temporary_files(
    transaction_id: &str,
    root: &Path,
    manifest: &Manifest,
) -> Result<(), String> {
    for file in &manifest.files {
        validate_relative_path(&file.path)?;
        ensure_no_symlink_ancestors(root, Path::new(&file.path))?;
        let target = root.join(&file.path);
        let temp = destination_temp_path(&target, transaction_id, &file.path)?;
        let metadata = match fs::symlink_metadata(&temp) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("failed to inspect destination temp file: {error}")),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "destination temp file is not regular: {}",
                temp.display()
            ));
        }
        ensure_single_link(&metadata, &temp)?;
        let bytes = fs::read(&temp)
            .map_err(|error| format!("failed to read destination temp file: {error}"))?;
        let snapshot = regular_snapshot(&bytes, file_mode(&metadata));
        if snapshot != file.pre && snapshot != file.post {
            return Err(format!(
                "destination temp file diverged from frozen snapshots: {}",
                temp.display()
            ));
        }
        fs::remove_file(&temp)
            .map_err(|error| format!("failed to remove known destination temp file: {error}"))?;
        if let Some(parent) = temp.parent() {
            sync_directory(parent)?;
        }
    }
    Ok(())
}

fn load_payload(
    transaction_dir: &Path,
    relative: Option<&str>,
    snapshot: &Snapshot,
) -> Result<Option<Vec<u8>>, String> {
    if snapshot.kind == SnapshotKind::Missing {
        return Ok(None);
    }
    let relative =
        relative.ok_or_else(|| "regular delegation snapshot has no payload".to_string())?;
    let path = transaction_dir.join(relative);
    let bytes =
        fs::read(&path).map_err(|error| format!("failed to read artifact payload: {error}"))?;
    if bytes.len() as u64 != snapshot.bytes || sha256_hex(&bytes) != snapshot.digest {
        return Err("delegation artifact payload is corrupt".to_string());
    }
    Ok(Some(bytes))
}

fn inspect_identity(root: &Path, side: GitSide) -> Result<ApplyIdentity, String> {
    let head = git_stdout(root, &["rev-parse", "HEAD"])?;
    let branch = git_stdout_optional(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let index_tree = git_stdout(root, &["write-tree"])?;
    if head.is_empty() || index_tree.is_empty() {
        return Err(format!("{} Git identity is empty", side_label(side)));
    }
    Ok(ApplyIdentity {
        head,
        branch,
        index_tree,
    })
}

fn ensure_identity(root: &Path, expected: &ApplyIdentity, side: GitSide) -> Result<(), String> {
    let current = inspect_identity(root, side)?;
    if &current != expected {
        return Err(format!(
            "{} Git HEAD, ref, or index changed",
            side_label(side)
        ));
    }
    Ok(())
}

fn ensure_git_root(root: &Path, side: GitSide) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("{} root is unavailable: {error}", side_label(side)))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{} root must be a real directory",
            side_label(side)
        ));
    }
    let root_str = root
        .to_str()
        .ok_or_else(|| format!("{} Git root path must be UTF-8", side_label(side)))?;
    let result = run_git_output_with_timeout(
        root_str,
        ["rev-parse", "--show-toplevel"],
        GIT_LOCAL_TIMEOUT,
    )
    .map_err(|error| format!("failed to inspect {} Git root: {error}", side_label(side)))?;
    if !result.status.success() {
        return Err(format!("{} root is not a Git worktree", side_label(side)));
    }
    Ok(())
}

fn ensure_clean_repository(root: &Path, side: GitSide) -> Result<(), String> {
    let output = git_output(
        root,
        &["status", "--porcelain=v1", "--untracked-files=all", "-z"],
    )?;
    if !output.is_empty() {
        return Err(format!("{} Git worktree is not clean", side_label(side)));
    }
    Ok(())
}

fn ensure_no_operation_in_progress(root: &Path) -> Result<(), String> {
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = PathBuf::from(git_stdout(root, &["rev-parse", "--git-path", marker])?);
        // `git rev-parse --git-path` commonly returns a path relative to the
        // process working directory.  Resolve it against the worktree before
        // checking it; otherwise an in-progress operation could be missed
        // whenever the command is run outside the repository.
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        if path.exists() {
            return Err(format!("Git operation is in progress: {marker}"));
        }
    }
    Ok(())
}

fn tracked_changed_paths(root: &Path) -> Result<Vec<String>, String> {
    // Keep both sides of a rename.  The artifact manifest is path-based, so
    // collapsing a rename to its new name would leave the old destination
    // file behind during apply (and make rollback unable to restore it).
    let output = git_output(root, &["diff", "HEAD", "--no-renames", "--name-only", "-z"])?;
    parse_nul_paths(&output)
}

fn untracked_paths(root: &Path) -> Result<Vec<String>, String> {
    let output = git_output(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    parse_nul_paths(&output)
}

fn reject_submodule(root: &Path, relative: &str) -> Result<(), String> {
    let output = git_output(root, &["ls-files", "--stage", "--", relative])?;
    for record in output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let mode = record
            .split(|byte| *byte == b' ')
            .next()
            .and_then(|value| std::str::from_utf8(value).ok())
            .unwrap_or_default();
        if mode == "160000" {
            return Err(format!("submodule path is not supported: {relative}"));
        }
    }
    Ok(())
}

fn parse_nul_paths(bytes: &[u8]) -> Result<Vec<String>, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let path = std::str::from_utf8(path)
                .map_err(|_| "Git returned a non-UTF-8 path".to_string())?;
            validate_relative_path(path)?;
            Ok(path.to_string())
        })
        .collect()
}

fn validate_relative_path(value: &str) -> Result<(), String> {
    if value.is_empty() || value.as_bytes().contains(&0) {
        return Err("delegation artifact path is empty or contains NUL".to_string());
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || path
            .components()
            .all(|component| matches!(component, Component::CurDir))
    {
        return Err(format!("delegation artifact path is unsafe: {value}"));
    }
    if path.to_str() != Some(value) {
        return Err("delegation artifact path must be UTF-8".to_string());
    }
    Ok(())
}

fn validate_case_distinct_paths(paths: &[String]) -> Result<(), String> {
    let mut folded = HashSet::with_capacity(paths.len());
    for path in paths {
        if !folded.insert(path.to_lowercase()) {
            return Err(format!(
                "delegation paths differing only by case are unsupported: {path}"
            ));
        }
    }
    Ok(())
}

fn ensure_no_symlink_ancestors(root: &Path, relative: &Path) -> Result<(), String> {
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("failed to inspect artifact root: {error}"))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("artifact root must be a real directory".to_string());
    }
    let mut current = root.to_path_buf();
    let components: Vec<_> = relative.components().collect();
    for component in components {
        let Component::Normal(name) = component else {
            return Err("delegation artifact path contains an unsafe component".to_string());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "symlink ancestor is not supported: {}",
                    current.display()
                ))
            }
            Ok(metadata) if !metadata.is_dir() && current != root.join(relative) => {
                return Err(format!(
                    "path ancestor is not a directory: {}",
                    current.display()
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(format!("failed to inspect {}: {error}", current.display())),
        }
    }
    Ok(())
}

fn create_destination_parent(root: &Path, parent: &Path) -> Result<(), String> {
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| "destination parent escaped root".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("destination parent contains an unsafe component".to_string());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "destination parent is not a directory: {}",
                    current.display()
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| format!("failed to create {}: {error}", current.display()))?;
            }
            Err(error) => return Err(format!("failed to inspect {}: {error}", current.display())),
        }
    }
    Ok(())
}

fn transaction_dir(artifact_root: &Path, transaction_id: &str) -> Result<PathBuf, String> {
    validate_transaction_id(transaction_id)?;
    let path = artifact_root.join(transaction_id);
    if path.parent() != Some(artifact_root) {
        return Err("delegation artifact transaction escaped its root".to_string());
    }
    Ok(path)
}

fn validate_transaction_id(value: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map_err(|_| "delegation apply transaction ID must be a UUID".to_string())?;
    Ok(())
}

fn ensure_artifact_root(root: &Path) -> Result<(), String> {
    if root.exists() {
        let metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("delegation artifact root must be a real directory".to_string());
        }
    } else {
        fs::create_dir_all(root)
            .map_err(|error| format!("failed to create artifact root: {error}"))?;
    }
    set_private_dir_mode(root)?;
    Ok(())
}

fn ensure_lock_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err("delegation operation lock directory must be a real directory".to_string())
        }
        Ok(_) => set_private_dir_mode(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .map_err(|error| format!("failed to create operation lock directory: {error}"))?;
            set_private_dir_mode(path)
        }
        Err(error) => Err(format!(
            "failed to inspect operation lock directory: {error}"
        )),
    }
}

fn set_private_dir_mode(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to protect artifact directory: {error}"))?;
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir(path)
        .map_err(|error| format!("failed to create artifact directory: {error}"))?;
    set_private_dir_mode(path)?;
    Ok(())
}

fn write_atomic_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = temporary_sibling(path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|error| format!("failed to create artifact temp file: {error}"))?;
    set_private_file_mode(&file)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist artifact: {error}"))?;
    drop(file);
    fs::rename(&temp, path).map_err(|error| format!("failed to install artifact: {error}"))?;
    sync_directory(path.parent().unwrap_or(Path::new(".")))?;
    Ok(())
}

fn write_destination_temp(path: &Path, bytes: &[u8], file_mode: u32) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("failed to create destination temp file: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(file_mode & 0o777))
            .map_err(|error| format!("failed to set destination mode: {error}"))?;
    }
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist destination temp file: {error}"))?;
    Ok(())
}

fn temporary_sibling(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "artifact path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before Unix epoch".to_string())?
        .as_nanos();
    Ok(parent.join(format!(".{name}.dcc-tmp-{nonce}")))
}

fn destination_temp_path(
    target: &Path,
    transaction_id: &str,
    relative: &str,
) -> Result<PathBuf, String> {
    validate_transaction_id(transaction_id)?;
    validate_relative_path(relative)?;
    let parent = target
        .parent()
        .ok_or_else(|| "destination path has no parent".to_string())?;
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "destination path must be UTF-8".to_string())?;
    let path_digest = sha256_hex(relative.as_bytes());
    Ok(parent.join(format!(
        ".{name}.dcc-tmp-{transaction_id}-{}",
        &path_digest[..16]
    )))
}

fn set_private_file_mode(file: &File) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("failed to protect artifact file: {error}"))?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("failed to sync artifact directory: {error}"))?;
    }
    Ok(())
}

fn regular_snapshot(bytes: &[u8], file_mode: u32) -> Snapshot {
    Snapshot {
        kind: SnapshotKind::Regular,
        digest: sha256_hex(bytes),
        bytes: bytes.len() as u64,
        file_mode,
    }
}

fn missing_snapshot() -> Snapshot {
    Snapshot {
        kind: SnapshotKind::Missing,
        digest: sha256_hex(&[]),
        bytes: 0,
        file_mode: 0,
    }
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

fn ensure_single_link(metadata: &fs::Metadata, path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(format!(
                "hard-linked delegation path is not supported: {}",
                path.display()
            ));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.number_of_links() != 1 {
            return Err(format!(
                "hard-linked delegation path is not supported: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> u32 {
    0
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_hex(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn side_label(side: GitSide) -> &'static str {
    match side {
        GitSide::Source => "source",
        GitSide::Destination => "destination",
    }
}

fn git_output(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let root = root
        .to_str()
        .ok_or_else(|| format!("Git root path must be UTF-8 for git {}", args.join(" ")))?;
    let output = run_git_output_with_timeout(root, args.iter().copied(), GIT_LOCAL_TIMEOUT)
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn git_stdout(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_output(root, args)?;
    String::from_utf8(output)
        .map(|value| value.trim().to_string())
        .map_err(|_| format!("git {} returned non-UTF-8 output", args.join(" ")))
}

fn git_stdout_optional(root: &Path, args: &[&str]) -> Result<Option<String>, String> {
    let root = root
        .to_str()
        .ok_or_else(|| format!("Git root path must be UTF-8 for git {}", args.join(" ")))?;
    let output = run_git_output_with_timeout(root, args.iter().copied(), GIT_LOCAL_TIMEOUT)
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8(output.stdout)
                .map_err(|_| "git returned non-UTF-8 branch name".to_string())?
                .trim()
                .to_string(),
        ));
    }
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    Err(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path, process::Command};
    use tempfile::TempDir;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("git runs");
        assert!(output.status.success(), "git failed: {:?}", output);
    }

    fn repo() -> (TempDir, TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        let artifacts = temp.path().join("artifacts");
        fs::create_dir_all(&source).expect("source");
        fs::create_dir_all(&destination).expect("destination");
        git(&source, &["init", "-q"]);
        git(&source, &["config", "user.email", "test@example.com"]);
        git(&source, &["config", "user.name", "Test"]);
        fs::write(source.join("tracked.txt"), b"before\n").expect("tracked");
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "base"]);
        git(
            &source,
            &[
                "worktree",
                "add",
                "-q",
                &destination.to_string_lossy(),
                "HEAD",
            ],
        );
        (
            temp,
            tempfile::tempdir().expect("keep artifacts"),
            source,
            destination,
            artifacts,
        )
    }

    fn id() -> String {
        Uuid::new_v4().to_string()
    }

    #[test]
    fn prepare_freezes_binary_delete_nested_untracked_and_mode() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::write(source.join("tracked.txt"), [0, 1, 2, 255]).expect("binary");
        fs::remove_file(source.join("tracked.txt")).expect("delete");
        fs::create_dir_all(source.join("nested/dir")).expect("nested");
        fs::write(source.join("nested/dir/new.bin"), [3, 4, 5]).expect("untracked");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                source.join("nested/dir/new.bin"),
                fs::Permissions::from_mode(0o755),
            )
            .expect("mode");
        }
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        assert_eq!(prepared.file_count, 2);
        assert!(prepared.artifact_bytes > 0);
        assert_eq!(
            classify_apply_artifacts(
                &prepared.transaction_id,
                &destination,
                &artifacts,
                &prepared.manifest_digest,
            )
            .unwrap(),
            ApplyClassification::AllPre
        );
    }

    #[test]
    fn atomic_replace_replaces_existing_regular_file() {
        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::write(&source, b"new").expect("source");
        fs::write(&destination, b"old").expect("destination");
        atomic_replace(&source, &destination).expect("replace");
        assert_eq!(fs::read(&destination).expect("read destination"), b"new");
        assert!(!source.exists());
    }

    #[test]
    fn operation_lock_is_exclusive_and_reacquirable_after_drop() {
        let temp = tempfile::tempdir().expect("temp");
        let first = try_lock_apply_operation(temp.path(), "operation-1")
            .expect("first lock")
            .expect("first owner");
        assert!(try_lock_apply_operation(temp.path(), "operation-1")
            .expect("second lock")
            .is_none());
        drop(first);
        assert!(try_lock_apply_operation(temp.path(), "operation-1")
            .expect("reacquire")
            .is_some());
    }

    #[cfg(unix)]
    #[test]
    fn hard_linked_source_file_is_rejected() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::hard_link(source.join("tracked.txt"), source.join("alias.txt")).expect("hard link");
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        assert!(prepare_apply_artifacts(&id(), &destination, &source, &artifacts).is_err());
    }

    #[test]
    fn case_colliding_paths_are_rejected() {
        let paths = vec!["old.txt".to_string(), "Old.txt".to_string()];
        let error = validate_case_distinct_paths(&paths).expect_err("case collision");
        assert!(error.contains("differing only by case"));
    }

    #[test]
    fn rename_freezes_old_and_new_paths_for_apply_and_rollback() {
        let (_keep, _other, source, destination, artifacts) = repo();
        git(&source, &["mv", "tracked.txt", "new.txt"]);
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        assert_eq!(prepared.file_count, 2);
        apply_prepared_artifacts(
            &prepared.transaction_id,
            &destination,
            &source,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("apply");
        assert!(!destination.join("tracked.txt").exists());
        assert_eq!(
            fs::read(destination.join("new.txt")).expect("new destination"),
            b"before\n"
        );
        rollback_apply_artifacts(
            &prepared.transaction_id,
            &destination,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("rollback");
        assert_eq!(
            fs::read(destination.join("tracked.txt")).expect("old destination"),
            b"before\n"
        );
        assert!(!destination.join("new.txt").exists());
    }

    #[test]
    fn apply_and_rollback_preserve_executable_mode() {
        let (_keep, _other, source, destination, artifacts) = repo();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                destination.join("tracked.txt"),
                fs::Permissions::from_mode(0o644),
            )
            .expect("baseline mode");
        }
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                source.join("tracked.txt"),
                fs::Permissions::from_mode(0o755),
            )
            .expect("post mode");
        }
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        apply_prepared_artifacts(
            &prepared.transaction_id,
            &destination,
            &source,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("apply");
        assert_eq!(
            fs::read(destination.join("tracked.txt")).unwrap(),
            b"after\n"
        );
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &fs::metadata(destination.join("tracked.txt"))
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o755
        );
        assert_eq!(
            classify_apply_artifacts(
                &prepared.transaction_id,
                &destination,
                &artifacts,
                &prepared.manifest_digest,
            )
            .unwrap(),
            ApplyClassification::AllPost
        );
        rollback_apply_artifacts(
            &prepared.transaction_id,
            &destination,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("rollback");
        assert_eq!(
            fs::read(destination.join("tracked.txt")).unwrap(),
            b"before\n"
        );
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &fs::metadata(destination.join("tracked.txt"))
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o644
        );
    }

    #[test]
    fn known_destination_temp_is_removed_before_retry() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        let (manifest, _) = load_manifest(
            &prepared.transaction_id,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("manifest");
        let file = &manifest.files[0];
        let temp = destination_temp_path(
            &destination.join(&file.path),
            &prepared.transaction_id,
            &file.path,
        )
        .expect("temp path");
        write_destination_temp(&temp, b"after\n", file.post.file_mode).expect("temp");
        assert!(temp.exists());
        assert_eq!(
            classify_apply_artifacts(
                &prepared.transaction_id,
                &destination,
                &artifacts,
                &prepared.manifest_digest,
            )
            .expect("classify"),
            ApplyClassification::AllPre
        );
        assert!(!temp.exists());
    }

    #[test]
    fn divergent_destination_temp_is_preserved_and_fails_closed() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        let (manifest, _) = load_manifest(
            &prepared.transaction_id,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("manifest");
        let file = &manifest.files[0];
        let temp = destination_temp_path(
            &destination.join(&file.path),
            &prepared.transaction_id,
            &file.path,
        )
        .expect("temp path");
        write_destination_temp(&temp, b"external partial\n", file.pre.file_mode).expect("temp");
        assert!(classify_apply_artifacts(
            &prepared.transaction_id,
            &destination,
            &artifacts,
            &prepared.manifest_digest,
        )
        .is_err());
        assert!(temp.exists());
        assert_eq!(
            fs::read(&temp).expect("preserved temp"),
            b"external partial\n"
        );
    }

    #[test]
    fn mixed_known_rollback_restores_only_post_paths() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        fs::write(source.join("second.txt"), b"new\n").expect("new");
        git(&source, &["add", "second.txt"]);
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        let (manifest, dir) = load_manifest(
            &prepared.transaction_id,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("manifest");
        let file = &manifest.files[0];
        let payload = load_payload(&dir, file.post_payload.as_deref(), &file.post).unwrap();
        install_snapshot(
            &prepared.transaction_id,
            &destination,
            &file.path,
            &file.post,
            payload.as_deref(),
        )
        .unwrap();
        assert_eq!(
            classify_apply_artifacts(
                &prepared.transaction_id,
                &destination,
                &artifacts,
                &prepared.manifest_digest,
            )
            .unwrap(),
            ApplyClassification::MixedKnown
        );
        rollback_apply_artifacts(
            &prepared.transaction_id,
            &destination,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("rollback");
        assert_eq!(
            fs::read(destination.join("tracked.txt")).unwrap(),
            b"before\n"
        );
        assert!(!destination.join("second.txt").exists());
    }

    #[test]
    fn divergent_external_change_is_preserved() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        apply_prepared_artifacts(
            &prepared.transaction_id,
            &destination,
            &source,
            &artifacts,
            &prepared.manifest_digest,
        )
        .expect("apply");
        fs::write(destination.join("tracked.txt"), b"external\n").expect("external");
        assert_eq!(
            classify_apply_artifacts(
                &prepared.transaction_id,
                &destination,
                &artifacts,
                &prepared.manifest_digest,
            )
            .unwrap(),
            ApplyClassification::Divergent
        );
        assert!(rollback_apply_artifacts(
            &prepared.transaction_id,
            &destination,
            &artifacts,
            &prepared.manifest_digest,
        )
        .is_err());
        assert_eq!(
            fs::read(destination.join("tracked.txt")).unwrap(),
            b"external\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_ancestor_is_rejected() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::create_dir_all(source.join("escape")).unwrap();
        fs::write(source.join("escape/file"), b"x").unwrap();
        std::os::unix::fs::symlink(source.join("escape"), destination.join("escape")).unwrap();
        assert!(prepare_apply_artifacts(&id(), &destination, &source, &artifacts).is_err());
    }

    #[test]
    fn corrupt_manifest_is_rejected() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        fs::write(
            artifacts
                .join(&prepared.transaction_id)
                .join("manifest.json"),
            b"{}",
        )
        .expect("corrupt");
        assert!(classify_apply_artifacts(
            &prepared.transaction_id,
            &destination,
            &artifacts,
            &prepared.manifest_digest,
        )
        .is_err());
    }

    #[test]
    fn durable_expected_digest_rejects_manifest_and_sidecar_rewrite() {
        let (_keep, _other, source, destination, artifacts) = repo();
        fs::write(source.join("tracked.txt"), b"after\n").expect("modify");
        let prepared =
            prepare_apply_artifacts(&id(), &destination, &source, &artifacts).expect("prepare");
        let transaction_dir = artifacts.join(&prepared.transaction_id);
        let manifest_path = transaction_dir.join("manifest.json");
        let mut manifest = fs::read(&manifest_path).expect("manifest");
        manifest.push(b' ');
        fs::write(&manifest_path, &manifest).expect("rewrite manifest");
        let rewritten_digest = sha256_hex(&manifest);
        fs::write(transaction_dir.join("manifest.sha256"), &rewritten_digest).expect("sidecar");
        // A caller must bind the artifact bytes to the digest persisted in
        // SQLite; accepting a freshly rewritten pair would defeat that bind.
        assert!(classify_apply_artifacts(
            &prepared.transaction_id,
            &destination,
            &artifacts,
            &prepared.manifest_digest,
        )
        .is_err());
    }
}
