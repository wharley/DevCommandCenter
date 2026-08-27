//! Synchronous, fail-closed capture-v2 orchestration for macOS.

#![cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration as StdDuration, Instant},
};

use chrono::{Duration, SecondsFormat, Utc};
use dcc_core::domain::{
    guarded_undo::{
        account_restore_files, canonical_restore_manifest_digest, CheckoutRefV1, GitIdentityV1,
        GuardedUndoReasonCode, IndexIdentityV1, OpaqueRepoPath, PhysicalRootId, RestoreFileStatus,
        RestoreSetId, RestoreSetState, Sha256Digest, TurnRestoreFile, TurnRestoreSet,
        CAPTURE_TIMEOUT_SECONDS, DEFAULT_RETENTION_DAYS, GIT_IDENTITY_SCHEMA_VERSION,
        MAX_BASELINE_PREIMAGE_BYTES, MAX_PREIMAGE_BYTES_PER_FILE, MAX_PREIMAGE_BYTES_PER_SET,
        MAX_RESTORE_FILES, RESTORE_CAPTURE_VERSION,
    },
    session::{SessionId, TurnId},
    workspace::WorkspaceId,
};
use uuid::Uuid;

use crate::db::SqliteSessionRepo;

use super::{
    coordinator::{
        CoordinatorError, TurnIntervalGuard, TurnOwner, TurnReceiptState,
        WorkspaceMutationCoordinator,
    },
    git_inspector::{
        CheckoutRef, GitInspection, GitInspector, GitInspectorLimits, TrustedGitBinary,
    },
    macos_git_bridge::{MacGitBridgeError, MacIndexFileReader},
    macos_root::{
        CapturePathClassification, IoErrorCategory, MacWorkspaceRoot, MacWorkspaceRootError,
    },
    macos_store::{
        MacArtifactStore, MacArtifactStoreError, MacArtifactStoreLease, OrphanRecoveryReport,
        PublishState, StagedArtifact, VerifiedArtifact,
    },
};

#[derive(Clone)]
pub struct CaptureV2Request {
    pub snapshot_id: String,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub workspace_id: WorkspaceId,
    pub workspace_absolute: PathBuf,
}

impl fmt::Debug for CaptureV2Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CaptureV2Request([redacted])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureV2Summary {
    pub restore_set_id: RestoreSetId,
    pub state: RestoreSetState,
    pub reason_code: Option<GuardedUndoReasonCode>,
    pub file_count: u32,
    pub artifact_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureV2Error {
    pub reason_code: GuardedUndoReasonCode,
}

impl fmt::Display for CaptureV2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("capture-v2 operation failed")
    }
}

impl std::error::Error for CaptureV2Error {}

pub struct CaptureV2Service {
    store_lease: Arc<MacArtifactStoreLease>,
    repo: SqliteSessionRepo,
    coordinator: Arc<WorkspaceMutationCoordinator>,
    git: TrustedGitBinary,
    clock: Arc<dyn EdgeClock>,
}

trait EdgeClock: Send + Sync {
    fn now(&self) -> Instant;
}

struct SystemEdgeClock;

impl EdgeClock for SystemEdgeClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

impl fmt::Debug for CaptureV2Service {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CaptureV2Service([redacted])")
    }
}

struct BaselineFile {
    path: OpaqueRepoPath,
    pre_size: u64,
    pre_sha256: Sha256Digest,
    metadata: dcc_core::domain::guarded_undo::RegularFileMetadataV1,
    artifact: VerifiedArtifact,
}

pub struct CaptureHandle {
    request: CaptureV2Request,
    collecting: TurnRestoreSet,
    repo: SqliteSessionRepo,
    root: Arc<MacWorkspaceRoot>,
    store: Arc<MacArtifactStore>,
    interval: Option<TurnIntervalGuard>,
    baseline: GitInspection,
    baseline_git: GitIdentityV1,
    generation: u64,
    files: Vec<BaselineFile>,
    baseline_reason: Option<GuardedUndoReasonCode>,
    terminalized: bool,
}

impl fmt::Debug for CaptureHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CaptureHandle")
            .field("restore_set", &"[redacted]")
            .field("baseline_files", &self.files.len())
            .finish_non_exhaustive()
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        if self.terminalized {
            return;
        }
        // Dropping the interval marks the receipt dirty. Abandonment cleanup
        // is intentionally best-effort, but the durable CAS is always tried.
        self.interval.take();
        let mut reason = GuardedUndoReasonCode::OperationInterrupted;
        for file in &self.files {
            if let Err(error) = self.store.cleanup_verified(&file.artifact) {
                reason = error.reason_code();
            }
        }
        let terminal = terminal_set(&self.collecting, RestoreSetState::Failed, reason);
        let _ = self.repo.finalize_turn_restore_set(&terminal, &[]);
    }
}

impl CaptureV2Service {
    pub fn new(
        app_data_absolute: PathBuf,
        repo: SqliteSessionRepo,
        coordinator: Arc<WorkspaceMutationCoordinator>,
        git: TrustedGitBinary,
    ) -> Result<Self, CaptureV2Error> {
        let store_lease = Arc::new(
            MacArtifactStoreLease::acquire(&app_data_absolute)
                .map_err(|error| capture_error(error.reason_code()))?,
        );
        Ok(Self {
            store_lease,
            repo,
            coordinator,
            git,
            clock: Arc::new(SystemEdgeClock),
        })
    }

    pub fn with_system_git(
        app_data_absolute: PathBuf,
        repo: SqliteSessionRepo,
        coordinator: Arc<WorkspaceMutationCoordinator>,
    ) -> Result<Self, CaptureV2Error> {
        let git = TrustedGitBinary::verify_absolute(Path::new("/usr/bin/git"))
            .map_err(|error| capture_error(error.reason_code()))?;
        Self::new(app_data_absolute, repo, coordinator, git)
    }

    pub fn recover_startup(
        &self,
        workspace_absolute: &Path,
    ) -> Result<OrphanRecoveryReport, CaptureV2Error> {
        let root = MacWorkspaceRoot::open_absolute(workspace_absolute)
            .map_err(|error| capture_error(error.reason_code()))?;
        let store = self
            .store_lease
            .bind_workspace(&root)
            .map_err(|error| capture_error(error.reason_code()))?;
        self.repo
            .recover_capture_v2_startup(&store, &now())
            .map_err(|_| capture_error(GuardedUndoReasonCode::IoError))
    }

    pub fn begin(&self, request: CaptureV2Request) -> Result<CaptureHandle, CaptureV2Error> {
        let started = self.clock.now();
        let Some(deadline) = started.checked_add(StdDuration::from_secs(CAPTURE_TIMEOUT_SECONDS))
        else {
            return self.preflight_error(
                &request,
                None,
                None,
                RestoreSetState::Failed,
                GuardedUndoReasonCode::CaptureTimeout,
            );
        };
        let root = match MacWorkspaceRoot::open_absolute(&request.workspace_absolute) {
            Ok(root) => Arc::new(root),
            Err(error) => {
                return self.preflight_error(
                    &request,
                    None,
                    None,
                    RestoreSetState::Failed,
                    error.reason_code(),
                )
            }
        };
        let root_id = root.physical_root_id();
        let store = match self.store_lease.bind_workspace(&root) {
            Ok(store) => Arc::new(store),
            Err(error) => {
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Failed,
                    error.reason_code(),
                )
            }
        };
        let interval = match self.coordinator.begin_turn_interval(
            root_id.clone(),
            TurnOwner::new(request.session_id.clone(), request.turn_id.clone()),
        ) {
            Ok(interval) => interval,
            Err(error) => {
                let reason = coordinator_error(error).reason_code;
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Ineligible,
                    reason,
                );
            }
        };
        let generation = match interval.receipt().state() {
            Ok(TurnReceiptState::Clean { generation }) => generation,
            Ok(TurnReceiptState::Ineligible { reason_code }) => {
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Ineligible,
                    reason_code,
                )
            }
            Err(error) => {
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Failed,
                    coordinator_error(error).reason_code,
                )
            }
        };
        let edge = match self.coordinator.try_acquire_capture_edge(&root_id) {
            Ok(edge) => edge,
            Err(error) => {
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Ineligible,
                    coordinator_error(error).reason_code,
                )
            }
        };
        let workspace_bytes = match path_bytes(&request.workspace_absolute) {
            Ok(bytes) => bytes,
            Err(error) => {
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Failed,
                    error.reason_code,
                )
            }
        };
        let inspection = match self.inspect(Arc::clone(&root), workspace_bytes.clone(), &request) {
            Ok(inspection) => inspection,
            Err(error) => {
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Failed,
                    error.reason_code,
                )
            }
        };
        if self.deadline_reached(deadline) {
            return self.preflight_error(
                &request,
                Some(root_id),
                None,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::CaptureTimeout,
            );
        }
        let git_identity = match git_identity(&root_id, &inspection) {
            Ok(identity) => identity,
            Err(error) => {
                return self.preflight_error(
                    &request,
                    Some(root_id),
                    None,
                    RestoreSetState::Failed,
                    error.reason_code,
                )
            }
        };
        let created_at = now();
        let collecting = TurnRestoreSet {
            restore_set_id: RestoreSetId(Uuid::new_v4().to_string()),
            snapshot_id: request.snapshot_id.clone(),
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            workspace_id: request.workspace_id.clone(),
            root_id: Some(root_id),
            capture_version: RESTORE_CAPTURE_VERSION,
            state: RestoreSetState::Collecting,
            reason_code: None,
            git_identity: Some(git_identity.clone()),
            artifact_bytes: 0,
            file_count: 0,
            manifest_digest: None,
            created_at,
            completed_at: None,
            expires_at: None,
        };
        self.repo
            .create_turn_restore_set(&collecting)
            .map_err(|_| capture_error(GuardedUndoReasonCode::InvalidPersistedRecord))?;

        let mut files = Vec::new();
        let mut baseline_reason = inspection.logical_ineligibility_reasons.first().cloned();
        if baseline_reason.is_none() {
            let mut total = 0_u64;
            for entry in &inspection.tracked {
                if self.deadline_reached(deadline) {
                    return self.fail_begin(
                        collecting,
                        store,
                        interval,
                        files,
                        GuardedUndoReasonCode::CaptureTimeout,
                    );
                }
                let capture =
                    match root.read_stable_twice(&entry.path, MAX_PREIMAGE_BYTES_PER_FILE, None) {
                        Ok(capture) => capture,
                        Err(
                            error @ (MacWorkspaceRootError::FileTooLarge
                            | MacWorkspaceRootError::AdapterUnsupported),
                        ) => {
                            let mut cleanup_reason = None;
                            for file in &files {
                                if let Err(cleanup_error) = store.cleanup_verified(&file.artifact) {
                                    cleanup_reason = Some(cleanup_error.reason_code());
                                }
                            }
                            if let Some(cleanup_reason) = cleanup_reason {
                                return self.fail_begin(
                                    collecting,
                                    store,
                                    interval,
                                    files,
                                    cleanup_reason,
                                );
                            }
                            files.clear();
                            baseline_reason =
                                Some(classify_physical_capture_error(&root, &entry.path, error));
                            break;
                        }
                        Err(error @ MacWorkspaceRootError::Io(_)) => {
                            let reason = classify_physical_capture_error(&root, &entry.path, error);
                            if matches!(
                                reason,
                                GuardedUndoReasonCode::SymlinkOrReparsePoint
                                    | GuardedUndoReasonCode::HardlinkUnsupported
                                    | GuardedUndoReasonCode::NonRegularFile
                            ) {
                                let mut cleanup_reason = None;
                                for file in &files {
                                    if let Err(cleanup_error) =
                                        store.cleanup_verified(&file.artifact)
                                    {
                                        cleanup_reason = Some(cleanup_error.reason_code());
                                    }
                                }
                                if let Some(cleanup_reason) = cleanup_reason {
                                    return self.fail_begin(
                                        collecting,
                                        store,
                                        interval,
                                        files,
                                        cleanup_reason,
                                    );
                                }
                                files.clear();
                                baseline_reason = Some(reason);
                                break;
                            }
                            return self.fail_begin(
                                collecting,
                                store,
                                interval,
                                files,
                                error.reason_code(),
                            );
                        }
                        Err(error) => {
                            return self.fail_begin(
                                collecting,
                                store,
                                interval,
                                files,
                                error.reason_code(),
                            )
                        }
                    };
                let size = capture.bytes.as_slice().len() as u64;
                total = match total.checked_add(size) {
                    Some(total) => total,
                    None => {
                        return self.fail_begin(
                            collecting,
                            store,
                            interval,
                            files,
                            GuardedUndoReasonCode::BaselineTooLarge,
                        )
                    }
                };
                if total > MAX_BASELINE_PREIMAGE_BYTES {
                    return self.fail_begin(
                        collecting,
                        store,
                        interval,
                        files,
                        GuardedUndoReasonCode::BaselineTooLarge,
                    );
                }
                let staged = match store.stage(&capture.bytes, MAX_PREIMAGE_BYTES_PER_FILE) {
                    Ok(staged) => staged,
                    Err(error) => {
                        return self.fail_begin(
                            collecting,
                            store,
                            interval,
                            files,
                            error.reason_code(),
                        )
                    }
                };
                let artifact = match publish_verified(&store, &staged) {
                    Ok(artifact) => artifact,
                    Err(error) => {
                        let reason = store
                            .cleanup_staged(&staged)
                            .err()
                            .map(|cleanup| cleanup.reason_code())
                            .unwrap_or_else(|| error.reason_code());
                        return self.fail_begin(collecting, store, interval, files, reason);
                    }
                };
                files.push(BaselineFile {
                    path: entry.path.clone(),
                    pre_size: size,
                    pre_sha256: capture.sha256,
                    metadata: capture.metadata,
                    artifact,
                });
            }
        }
        drop(edge);
        Ok(CaptureHandle {
            request,
            collecting,
            repo: self.repo.clone(),
            root,
            store,
            interval: Some(interval),
            baseline: inspection,
            baseline_git: git_identity,
            generation,
            files,
            baseline_reason,
            terminalized: false,
        })
    }

    pub fn finish(&self, handle: CaptureHandle) -> Result<CaptureV2Summary, CaptureV2Error> {
        self.finish_at(handle, self.clock.now())
    }

    fn finish_at(
        &self,
        mut handle: CaptureHandle,
        started: Instant,
    ) -> Result<CaptureV2Summary, CaptureV2Error> {
        let Some(deadline) = started.checked_add(StdDuration::from_secs(CAPTURE_TIMEOUT_SECONDS))
        else {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::CaptureTimeout,
            );
        };
        let root_id = handle.root.physical_root_id();
        let edge = match self.coordinator.try_acquire_capture_edge(&root_id) {
            Ok(edge) => edge,
            Err(error) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Ineligible,
                    coordinator_error(error).reason_code,
                )
            }
        };
        let reopened = match MacWorkspaceRoot::open_absolute(&handle.request.workspace_absolute) {
            Ok(root) => Arc::new(root),
            Err(error) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    error.reason_code(),
                );
            }
        };
        if reopened.physical_root_id() != root_id {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::RepositoryIdentityChanged,
            );
        }
        let workspace_bytes = match path_bytes(&handle.request.workspace_absolute) {
            Ok(bytes) => bytes,
            Err(error) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    error.reason_code,
                )
            }
        };
        let result = match self.inspect(Arc::clone(&reopened), workspace_bytes, &handle.request) {
            Ok(result) => result,
            Err(error) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    error.reason_code,
                );
            }
        };
        let Some(interval) = handle.interval.as_ref() else {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Failed,
                GuardedUndoReasonCode::CaptureRace,
            );
        };
        let receipt = interval.receipt();
        let receipt_state = match receipt.state() {
            Ok(state) => state,
            Err(error) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    coordinator_error(error).reason_code,
                )
            }
        };
        if let TurnReceiptState::Ineligible { reason_code } = receipt_state {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                reason_code,
            );
        }
        let generation = match self.coordinator.generation(&root_id) {
            Ok(generation) => generation,
            Err(error) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    coordinator_error(error).reason_code,
                )
            }
        };
        if generation != handle.generation {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::ConcurrentWorkspaceMutation,
            );
        }
        if let Some(reason) = compare_edges(&handle.baseline, &result)
            .or_else(|| handle.baseline_reason.clone())
            .or_else(|| result.logical_ineligibility_reasons.first().cloned())
        {
            return self.finalize_noneligible(&mut handle, RestoreSetState::Ineligible, reason);
        }
        if self.deadline_reached(deadline) {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::CaptureTimeout,
            );
        }

        let mut changed = Vec::new();
        let mut unchanged = Vec::new();
        for (index, baseline) in handle.files.iter().enumerate() {
            if self.deadline_reached(deadline) {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Ineligible,
                    GuardedUndoReasonCode::CaptureTimeout,
                );
            }
            let capture =
                match reopened.read_stable_twice(&baseline.path, MAX_PREIMAGE_BYTES_PER_FILE, None)
                {
                    Ok(capture) => capture,
                    Err(error) => {
                        let (state, reason) =
                            classify_result_file_error(&reopened, &baseline.path, error);
                        return self.finalize_noneligible(&mut handle, state, reason);
                    }
                };
            if !security_metadata_matches(&baseline.metadata, &capture.metadata) {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Ineligible,
                    GuardedUndoReasonCode::MetadataChanged,
                );
            }
            if capture.sha256 == baseline.pre_sha256 {
                unchanged.push(index);
            } else {
                changed.push((index, capture));
            }
        }
        if changed.is_empty() {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::NoTargetChanges,
            );
        }
        if changed.len() > MAX_RESTORE_FILES as usize {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::TooManyFiles,
            );
        }
        let changed_pre_bytes = changed.iter().try_fold(0_u64, |total, (index, _)| {
            total.checked_add(handle.files[*index].pre_size)
        });
        if changed_pre_bytes.is_none_or(|bytes| bytes > MAX_PREIMAGE_BYTES_PER_SET) {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::SetTooLarge,
            );
        }
        for (index, _) in &changed {
            let baseline = &handle.files[*index];
            if handle
                .store
                .verify(
                    baseline.artifact.key,
                    baseline.pre_size,
                    baseline.pre_sha256,
                )
                .is_err()
            {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    GuardedUndoReasonCode::ArtifactCorrupt,
                );
            }
        }
        let mut file_inputs = changed
            .into_iter()
            .map(|(index, result)| (handle.files[index].path.clone(), index, result))
            .collect::<Vec<_>>();
        file_inputs.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
        let mut restore_files = Vec::with_capacity(file_inputs.len());
        for (ordinal, (path, index, result)) in file_inputs.into_iter().enumerate() {
            let baseline = &handle.files[index];
            restore_files.push(TurnRestoreFile {
                restore_set_id: handle.collecting.restore_set_id.clone(),
                ordinal: ordinal as u32,
                path_bytes: path,
                status: RestoreFileStatus::Modified,
                pre_size: baseline.pre_size,
                pre_sha256: baseline.pre_sha256,
                pre_artifact_key: baseline.artifact.key,
                result_size: result.bytes.as_slice().len() as u64,
                result_sha256: result.sha256,
                metadata_fingerprint: result.metadata,
            });
        }
        let accounting = match account_restore_files(&restore_files) {
            Ok(accounting) => accounting,
            Err(_) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Ineligible,
                    GuardedUndoReasonCode::SetTooLarge,
                )
            }
        };
        let digest = match canonical_restore_manifest_digest(&restore_files) {
            Ok(digest) => digest,
            Err(_) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    GuardedUndoReasonCode::InvalidPersistedRecord,
                )
            }
        };
        let completed_at = now();
        let terminal = TurnRestoreSet {
            state: RestoreSetState::Eligible,
            reason_code: None,
            root_id: Some(root_id.clone()),
            git_identity: Some(handle.baseline_git.clone()),
            artifact_bytes: accounting.artifact_bytes,
            file_count: accounting.file_count,
            manifest_digest: Some(digest),
            completed_at: Some(completed_at),
            expires_at: Some(expires_at()),
            ..handle.collecting.clone()
        };
        // Finish and validate the interval while the capture edge is still
        // held. The edge prevents a mutation window between this final check
        // and the eligible database transaction.
        let Some(interval) = handle.interval.take() else {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Failed,
                GuardedUndoReasonCode::CaptureRace,
            );
        };
        let receipt = match interval.finish() {
            Ok(receipt) => receipt,
            Err(error) => {
                return self.finalize_noneligible(
                    &mut handle,
                    RestoreSetState::Failed,
                    coordinator_error(error).reason_code,
                )
            }
        };
        let receipt_clean = matches!(
            receipt.state(),
            Ok(TurnReceiptState::Clean { generation }) if generation == handle.generation
        );
        let generation_clean = self
            .coordinator
            .generation(&root_id)
            .is_ok_and(|generation| generation == handle.generation);
        if !receipt_clean || !generation_clean {
            return self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Ineligible,
                GuardedUndoReasonCode::ConcurrentWorkspaceMutation,
            );
        }
        if self
            .repo
            .finalize_turn_restore_set(&terminal, &restore_files)
            .is_err()
            && self
                .repo
                .finalize_turn_restore_set(&terminal, &restore_files)
                .is_err()
        {
            return match self.finalize_noneligible(
                &mut handle,
                RestoreSetState::Failed,
                GuardedUndoReasonCode::InvalidPersistedRecord,
            ) {
                Ok(_) => Err(capture_error(GuardedUndoReasonCode::InvalidPersistedRecord)),
                Err(error) => Err(error),
            };
        }
        handle.terminalized = true;
        drop(edge);
        // Eligible is already durable. Cleanup failure leaves only an
        // unreferenced object, which startup orphan recovery removes.
        for index in unchanged {
            let artifact = handle.files[index].artifact.clone();
            if handle.store.cleanup_verified(&artifact).is_err() {
                continue;
            }
        }
        Ok(summary(&terminal))
    }

    pub fn cancel(&self, mut handle: CaptureHandle) -> Result<CaptureV2Summary, CaptureV2Error> {
        handle.interval.take();
        self.finalize_noneligible(
            &mut handle,
            RestoreSetState::Ineligible,
            GuardedUndoReasonCode::OperationInterrupted,
        )
    }

    pub fn provider_failed(
        &self,
        mut handle: CaptureHandle,
    ) -> Result<CaptureV2Summary, CaptureV2Error> {
        handle.interval.take();
        self.finalize_noneligible(
            &mut handle,
            RestoreSetState::Failed,
            GuardedUndoReasonCode::OperationInterrupted,
        )
    }

    fn inspect(
        &self,
        root: Arc<MacWorkspaceRoot>,
        workspace_bytes: Vec<u8>,
        request: &CaptureV2Request,
    ) -> Result<GitInspection, CaptureV2Error> {
        let reader = MacIndexFileReader::new(root, workspace_bytes)
            .map_err(|error| capture_error(bridge_reason(error)))?;
        let inspector = GitInspector::with_index_reader(
            GitInspectorLimits::default(),
            self.git.clone(),
            reader,
        )
        .map_err(|error| capture_error(error.reason_code()))?;
        inspector
            .inspect(&request.workspace_absolute)
            .map_err(|error| capture_error(error.reason_code()))
    }

    fn deadline_reached(&self, deadline: Instant) -> bool {
        self.clock.now() >= deadline
    }

    fn preflight_error<T>(
        &self,
        request: &CaptureV2Request,
        root_id: Option<PhysicalRootId>,
        git_identity: Option<GitIdentityV1>,
        state: RestoreSetState,
        reason: GuardedUndoReasonCode,
    ) -> Result<T, CaptureV2Error> {
        let timestamp = now();
        let terminal = TurnRestoreSet {
            restore_set_id: RestoreSetId(Uuid::new_v4().to_string()),
            snapshot_id: request.snapshot_id.clone(),
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            workspace_id: request.workspace_id.clone(),
            root_id,
            capture_version: RESTORE_CAPTURE_VERSION,
            state,
            reason_code: Some(reason.clone()),
            git_identity,
            artifact_bytes: 0,
            file_count: 0,
            manifest_digest: None,
            created_at: timestamp.clone(),
            completed_at: Some(timestamp),
            expires_at: None,
        };
        self.repo
            .record_capture_v2_preflight_terminal(&terminal)
            .map_err(|_| capture_error(GuardedUndoReasonCode::InvalidPersistedRecord))?;
        Err(capture_error(reason))
    }

    fn fail_begin(
        &self,
        collecting: TurnRestoreSet,
        store: Arc<MacArtifactStore>,
        interval: TurnIntervalGuard,
        files: Vec<BaselineFile>,
        reason: GuardedUndoReasonCode,
    ) -> Result<CaptureHandle, CaptureV2Error> {
        let mut terminal_reason = reason;
        for file in &files {
            if let Err(error) = store.cleanup_verified(&file.artifact) {
                terminal_reason = error.reason_code();
            }
        }
        if let Err(error) = interval.finish() {
            terminal_reason = coordinator_error(error).reason_code;
        }
        let terminal = terminal_set(
            &collecting,
            RestoreSetState::Failed,
            terminal_reason.clone(),
        );
        self.repo
            .finalize_turn_restore_set(&terminal, &[])
            .map_err(|_| capture_error(GuardedUndoReasonCode::InvalidPersistedRecord))?;
        Err(capture_error(terminal_reason))
    }

    fn finalize_noneligible(
        &self,
        handle: &mut CaptureHandle,
        state: RestoreSetState,
        reason: GuardedUndoReasonCode,
    ) -> Result<CaptureV2Summary, CaptureV2Error> {
        handle.interval.take();
        let mut terminal_reason = reason;
        for file in &handle.files {
            if let Err(error) = handle.store.cleanup_verified(&file.artifact) {
                terminal_reason = error.reason_code();
            }
        }
        let terminal = terminal_set(&handle.collecting, state, terminal_reason);
        self.repo
            .finalize_turn_restore_set(&terminal, &[])
            .map_err(|_| capture_error(GuardedUndoReasonCode::InvalidPersistedRecord))?;
        handle.terminalized = true;
        Ok(summary(&terminal))
    }
}

fn compare_edges(
    baseline: &GitInspection,
    result: &GitInspection,
) -> Option<GuardedUndoReasonCode> {
    if baseline.head_oid != result.head_oid {
        Some(GuardedUndoReasonCode::HeadChanged)
    } else if baseline.checkout_ref != result.checkout_ref {
        Some(GuardedUndoReasonCode::RefChanged)
    } else if baseline.index != result.index {
        Some(GuardedUndoReasonCode::IndexChanged)
    } else if baseline.attributes_sha256 != result.attributes_sha256 {
        Some(GuardedUndoReasonCode::GitAttributesChanged)
    } else if baseline.tracked_manifest_sha256 != result.tracked_manifest_sha256 {
        Some(GuardedUndoReasonCode::TrackedManifestChanged)
    } else if !baseline.untracked.is_empty() || !result.untracked.is_empty() {
        Some(GuardedUndoReasonCode::UntrackedPath)
    } else {
        None
    }
}

fn security_metadata_matches(
    baseline: &dcc_core::domain::guarded_undo::RegularFileMetadataV1,
    result: &dcc_core::domain::guarded_undo::RegularFileMetadataV1,
) -> bool {
    // macOS metadata intentionally excludes size, mtime and ctime. Content
    // edits may change those values; only owner/mode/link/file identity and
    // other security invariants participate in this comparison.
    baseline == result
}

fn git_identity(
    root_id: &PhysicalRootId,
    inspection: &GitInspection,
) -> Result<GitIdentityV1, CaptureV2Error> {
    let checkout_ref = match &inspection.checkout_ref {
        CheckoutRef::Symbolic { full_name } => CheckoutRefV1::Symbolic {
            full_name: std::str::from_utf8(full_name)
                .map_err(|_| capture_error(GuardedUndoReasonCode::RefChanged))?
                .to_owned(),
        },
        CheckoutRef::Detached => CheckoutRefV1::Detached,
    };
    let identity = GitIdentityV1 {
        schema_version: GIT_IDENTITY_SCHEMA_VERSION,
        worktree_identity: root_id.0.clone(),
        head_oid: inspection.head_oid.clone(),
        checkout_ref,
        index: IndexIdentityV1 {
            sha256: inspection.index.sha256,
            size: inspection.index.size,
            stat_identity: inspection.index.stat_identity.clone(),
        },
    };
    identity
        .validate()
        .map_err(|_| capture_error(GuardedUndoReasonCode::InvalidPersistedRecord))?;
    Ok(identity)
}

fn publish_verified(
    store: &MacArtifactStore,
    staged: &StagedArtifact,
) -> Result<VerifiedArtifact, MacArtifactStoreError> {
    let state = match store.publish(staged) {
        Ok(state) => state,
        Err(_) => store.reconcile_publish(staged)?,
    };
    match state {
        PublishState::Published(artifact) => Ok(artifact),
        PublishState::PublishedCleanupPending(staged) | PublishState::StagedOnly(staged) => {
            match store.reconcile_publish(&staged)? {
                PublishState::Published(artifact) => Ok(artifact),
                _ => Err(MacArtifactStoreError::ArtifactStoreUnsafe),
            }
        }
    }
}

fn terminal_set(
    collecting: &TurnRestoreSet,
    state: RestoreSetState,
    reason: GuardedUndoReasonCode,
) -> TurnRestoreSet {
    TurnRestoreSet {
        state,
        reason_code: Some(reason),
        artifact_bytes: 0,
        file_count: 0,
        manifest_digest: None,
        completed_at: Some(now()),
        expires_at: None,
        ..collecting.clone()
    }
}

fn classify_result_file_error(
    root: &MacWorkspaceRoot,
    path: &OpaqueRepoPath,
    error: MacWorkspaceRootError,
) -> (RestoreSetState, GuardedUndoReasonCode) {
    match error {
        error @ MacWorkspaceRootError::Io(category) => {
            let physical = classify_physical_capture_error(root, path, error);
            if matches!(
                physical,
                GuardedUndoReasonCode::SymlinkOrReparsePoint
                    | GuardedUndoReasonCode::HardlinkUnsupported
                    | GuardedUndoReasonCode::NonRegularFile
            ) {
                return (RestoreSetState::Ineligible, physical);
            }
            match category {
                IoErrorCategory::NotFound => (
                    RestoreSetState::Ineligible,
                    GuardedUndoReasonCode::UnsupportedStatus,
                ),
                IoErrorCategory::PermissionDenied => (
                    RestoreSetState::Failed,
                    GuardedUndoReasonCode::PermissionDenied,
                ),
                _ => (RestoreSetState::Failed, GuardedUndoReasonCode::IoError),
            }
        }
        MacWorkspaceRootError::FileTooLarge => (
            RestoreSetState::Ineligible,
            GuardedUndoReasonCode::FileTooLarge,
        ),
        MacWorkspaceRootError::AdapterUnsupported => (
            RestoreSetState::Ineligible,
            classify_physical_capture_error(root, path, MacWorkspaceRootError::AdapterUnsupported),
        ),
        other => (RestoreSetState::Failed, other.reason_code()),
    }
}

fn classify_physical_capture_error(
    root: &MacWorkspaceRoot,
    path: &OpaqueRepoPath,
    fallback: MacWorkspaceRootError,
) -> GuardedUndoReasonCode {
    match root.classify_capture_path(path) {
        Ok(CapturePathClassification::Symlink) => GuardedUndoReasonCode::SymlinkOrReparsePoint,
        Ok(CapturePathClassification::Hardlink) => GuardedUndoReasonCode::HardlinkUnsupported,
        Ok(CapturePathClassification::NonRegular) => GuardedUndoReasonCode::NonRegularFile,
        Ok(CapturePathClassification::RegularSingleLink) => {
            GuardedUndoReasonCode::ExtendedMetadataUnsupported
        }
        Err(_) => fallback.reason_code(),
    }
}

fn bridge_reason(error: MacGitBridgeError) -> GuardedUndoReasonCode {
    match error {
        MacGitBridgeError::IndexTooLarge => GuardedUndoReasonCode::IndexTooLarge,
        MacGitBridgeError::IndexChanged => GuardedUndoReasonCode::CaptureRace,
        MacGitBridgeError::IndexUnreadable => GuardedUndoReasonCode::IndexUnreadable,
        _ => GuardedUndoReasonCode::AdapterUnsupported,
    }
}

fn coordinator_error(error: CoordinatorError) -> CaptureV2Error {
    let reason = match error {
        CoordinatorError::MutationInProgress => GuardedUndoReasonCode::MutationInProgress,
        CoordinatorError::DuplicateOwner
        | CoordinatorError::CaptureEdgeActive
        | CoordinatorError::Unavailable
        | CoordinatorError::ReceiptIdExhausted
        | CoordinatorError::GenerationExhausted
        | CoordinatorError::RootGenerationCapacityExhausted
        | CoordinatorError::InvalidPhysicalRoot => {
            GuardedUndoReasonCode::ConcurrentWorkspaceMutation
        }
    };
    capture_error(reason)
}

fn capture_error(reason_code: GuardedUndoReasonCode) -> CaptureV2Error {
    CaptureV2Error { reason_code }
}

fn summary(set: &TurnRestoreSet) -> CaptureV2Summary {
    CaptureV2Summary {
        restore_set_id: set.restore_set_id.clone(),
        state: set.state.clone(),
        reason_code: set.reason_code.clone(),
        file_count: set.file_count,
        artifact_bytes: set.artifact_bytes,
    }
}

fn path_bytes(path: &Path) -> Result<Vec<u8>, CaptureV2Error> {
    use std::os::unix::ffi::OsStrExt;
    if !path.is_absolute() {
        return Err(capture_error(GuardedUndoReasonCode::WorkspaceMissing));
    }
    Ok(path.as_os_str().as_bytes().to_vec())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn expires_at() -> String {
    (Utc::now() + Duration::days(i64::from(DEFAULT_RETENTION_DAYS)))
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::{ffi::CString, fs, os::unix::ffi::OsStrExt, process::Command, sync::Mutex};

    struct ManualClock(Mutex<Instant>);

    impl ManualClock {
        fn new(now: Instant) -> Self {
            Self(Mutex::new(now))
        }

        fn advance(&self, duration: StdDuration) {
            let mut now = self.0.lock().unwrap();
            *now = now.checked_add(duration).unwrap();
        }
    }

    impl EdgeClock for ManualClock {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
    }

    struct BoundaryClock {
        edge_start: Instant,
        calls: AtomicUsize,
    }

    impl BoundaryClock {
        fn new(edge_start: Instant) -> Self {
            Self {
                edge_start,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl EdgeClock for BoundaryClock {
        fn now(&self) -> Instant {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.edge_start
            } else {
                self.edge_start
                    .checked_add(StdDuration::from_secs(CAPTURE_TIMEOUT_SECONDS))
                    .unwrap()
            }
        }
    }

    struct Fixture {
        _workspace_dir: tempfile::TempDir,
        _app_data_dir: tempfile::TempDir,
        service: CaptureV2Service,
        repo: SqliteSessionRepo,
        request: CaptureV2Request,
        app_data: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let workspace_dir = tempfile::tempdir_in("/private/tmp").unwrap();
            let app_data_dir = tempfile::tempdir_in("/private/tmp").unwrap();
            run_git(workspace_dir.path(), &["init", "-q"]);
            fs::write(workspace_dir.path().join("tracked.txt"), b"before\n").unwrap();
            run_git(workspace_dir.path(), &["add", "--", "tracked.txt"]);
            run_git(
                workspace_dir.path(),
                &[
                    "-c",
                    "user.name=DCC Test",
                    "-c",
                    "user.email=dcc@example.invalid",
                    "commit",
                    "--no-gpg-sign",
                    "--no-verify",
                    "-qm",
                    "initial",
                ],
            );
            for path in [
                workspace_dir.path(),
                &workspace_dir.path().join(".git"),
                &workspace_dir.path().join(".git/index"),
                &workspace_dir.path().join("tracked.txt"),
                app_data_dir.path(),
            ] {
                remove_xattrs(path);
            }
            let workspace = fs::canonicalize(workspace_dir.path()).unwrap();
            let app_data = fs::canonicalize(app_data_dir.path()).unwrap();
            let db_path = app_data.join("sessions.sqlite");
            let repo = SqliteSessionRepo::open(&db_path).unwrap();
            seed_m3(&db_path, &workspace);
            let service = CaptureV2Service::with_system_git(
                app_data.clone(),
                repo.clone(),
                Arc::new(WorkspaceMutationCoordinator::new()),
            )
            .unwrap();
            let request = CaptureV2Request {
                snapshot_id: "snapshot-1".to_owned(),
                session_id: SessionId("session-1".to_owned()),
                turn_id: TurnId("turn-1".to_owned()),
                workspace_id: WorkspaceId("workspace-1".to_owned()),
                workspace_absolute: workspace,
            };
            Self {
                _workspace_dir: workspace_dir,
                _app_data_dir: app_data_dir,
                service,
                repo,
                request,
                app_data,
            }
        }

        fn write_tracked(&self, bytes: &[u8]) {
            let path = self.request.workspace_absolute.join("tracked.txt");
            fs::write(&path, bytes).unwrap();
            remove_xattrs(&path);
        }

        fn second_request(&self) -> CaptureV2Request {
            CaptureV2Request {
                snapshot_id: "snapshot-2".to_owned(),
                turn_id: TurnId("turn-2".to_owned()),
                ..self.request.clone()
            }
        }
    }

    fn seed_m3(db_path: &Path, workspace: &Path) {
        let conn = Connection::open(db_path).unwrap();
        conn.execute(
            r#"INSERT INTO dcc_workspaces
               (id, project_id, name, root_path, base_branch, state, created_at, updated_at)
               VALUES ('workspace-1', 'project-1', 'workspace', ?1, 'main', 'active', 't0', 't0')"#,
            params![workspace.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO dcc_sessions
               (id, project_id, workspace_id, provider_id, state, created_at, updated_at)
               VALUES ('session-1', 'project-1', 'workspace-1', 'test', 'active', 't0', 't0')"#,
            [],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO dcc_turn_change_sets
               (snapshot_id, session_id, turn_id, workspace_id, capture_version, state, created_at)
               VALUES ('snapshot-2', 'session-1', 'turn-2', 'workspace-1', 1, 'complete', 't0')"#,
            [],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO dcc_turn_change_sets
               (snapshot_id, session_id, turn_id, workspace_id, capture_version, state, created_at)
               VALUES ('snapshot-1', 'session-1', 'turn-1', 'workspace-1', 1, 'complete', 't0')"#,
            [],
        )
        .unwrap();
    }

    fn run_git(workspace: &Path, arguments: &[&str]) {
        assert!(Command::new("/usr/bin/git")
            .current_dir(workspace)
            .args(arguments)
            .status()
            .unwrap()
            .success());
    }

    fn remove_xattrs(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let size = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0, 0) };
        if size <= 0 {
            return;
        }
        let mut names = vec![0_u8; size as usize];
        let actual = unsafe {
            libc::listxattr(
                path.as_ptr(),
                names.as_mut_ptr() as *mut libc::c_char,
                names.len(),
                0,
            )
        };
        if actual <= 0 {
            return;
        }
        for name in names[..actual as usize]
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = CString::new(name).unwrap();
            unsafe { libc::removexattr(path.as_ptr(), name.as_ptr(), 0) };
        }
    }

    #[test]
    fn modified_only_turn_is_eligible_and_persists_verified_artifact() {
        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        fixture.write_tracked(b"after\n");
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(summary.state, RestoreSetState::Eligible);
        assert_eq!(summary.file_count, 1);
        let (set, files) = fixture
            .repo
            .get_turn_restore_set(&summary.restore_set_id)
            .unwrap()
            .unwrap();
        assert_eq!(set.state, RestoreSetState::Eligible);
        assert_eq!(files.len(), 1);
        let root = MacWorkspaceRoot::open_absolute(&fixture.request.workspace_absolute).unwrap();
        fixture
            .service
            .store_lease
            .bind_workspace(&root)
            .unwrap()
            .verify(
                files[0].pre_artifact_key,
                files[0].pre_size,
                files[0].pre_sha256,
            )
            .unwrap();
    }

    #[test]
    fn clean_turn_is_terminal_ineligible_with_zero_files() {
        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(summary.state, RestoreSetState::Ineligible);
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::NoTargetChanges)
        );
        assert_eq!((summary.file_count, summary.artifact_bytes), (0, 0));
    }

    #[test]
    fn untracked_and_deleted_result_edges_are_ineligible() {
        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        fs::write(
            fixture.request.workspace_absolute.join("untracked.txt"),
            b"never read",
        )
        .unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::UntrackedPath)
        );

        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        fs::remove_file(fixture.request.workspace_absolute.join("tracked.txt")).unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(summary.state, RestoreSetState::Ineligible);
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::UnsupportedStatus)
        );
    }

    #[test]
    fn head_or_index_change_is_ineligible() {
        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        fs::write(
            fixture.request.workspace_absolute.join("added.txt"),
            b"added\n",
        )
        .unwrap();
        run_git(
            &fixture.request.workspace_absolute,
            &["add", "--", "added.txt"],
        );
        remove_xattrs(&fixture.request.workspace_absolute.join(".git/index"));
        let summary = fixture.service.finish(handle).unwrap();
        assert!(matches!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::IndexChanged | GuardedUndoReasonCode::CaptureRace)
        ));

        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        fixture.write_tracked(b"committed\n");
        run_git(
            &fixture.request.workspace_absolute,
            &["add", "--", "tracked.txt"],
        );
        run_git(
            &fixture.request.workspace_absolute,
            &[
                "-c",
                "user.name=DCC Test",
                "-c",
                "user.email=dcc@example.invalid",
                "commit",
                "--no-gpg-sign",
                "--no-verify",
                "-qm",
                "second",
            ],
        );
        remove_xattrs(&fixture.request.workspace_absolute.join(".git/index"));
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::HeadChanged)
        );
    }

    #[test]
    fn cancellation_provider_failure_and_attribution_mismatch_are_terminal() {
        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let cancelled = fixture.service.cancel(handle).unwrap();
        assert_eq!(cancelled.state, RestoreSetState::Ineligible);
        assert_eq!(
            cancelled.reason_code,
            Some(GuardedUndoReasonCode::OperationInterrupted)
        );
        assert_eq!(cancelled.file_count, 0);

        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let failed = fixture.service.provider_failed(handle).unwrap();
        assert_eq!(failed.state, RestoreSetState::Failed);
        assert_eq!(
            failed.reason_code,
            Some(GuardedUndoReasonCode::OperationInterrupted)
        );
        assert_eq!(failed.file_count, 0);

        let fixture = Fixture::new();
        let mut mismatched = fixture.request.clone();
        mismatched.turn_id = TurnId("other-turn".to_owned());
        assert_eq!(
            fixture.service.begin(mismatched).unwrap_err().reason_code,
            GuardedUndoReasonCode::InvalidPersistedRecord
        );
    }

    #[test]
    fn dropped_handle_is_terminal_and_cleans_artifacts_immediately() {
        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let restore_set_id = handle.collecting.restore_set_id.clone();
        drop(handle);
        let (set, files) = fixture
            .repo
            .get_turn_restore_set(&restore_set_id)
            .unwrap()
            .unwrap();
        assert_eq!(set.state, RestoreSetState::Failed);
        assert_eq!(
            set.reason_code,
            Some(GuardedUndoReasonCode::OperationInterrupted)
        );
        assert!(files.is_empty());
        assert_eq!(
            fixture
                .service
                .recover_startup(&fixture.request.workspace_absolute)
                .unwrap()
                .objects_removed,
            0
        );
    }

    #[test]
    fn service_lifetime_lease_allows_own_captures_and_rejects_second_store() {
        let fixture = Fixture::new();
        assert!(matches!(
            MacArtifactStoreLease::acquire(&fixture.app_data),
            Err(MacArtifactStoreError::LockUnavailable)
        ));

        let first = fixture.service.begin(fixture.request.clone()).unwrap();
        fixture.service.cancel(first).unwrap();
        let second = fixture.service.begin(fixture.second_request()).unwrap();
        fixture.service.cancel(second).unwrap();

        let workspace_two = tempfile::tempdir_in("/private/tmp").unwrap();
        run_git(workspace_two.path(), &["init", "-q"]);
        fs::write(workspace_two.path().join("tracked.txt"), b"second\n").unwrap();
        run_git(workspace_two.path(), &["add", "--", "tracked.txt"]);
        run_git(
            workspace_two.path(),
            &[
                "-c",
                "user.name=DCC Test",
                "-c",
                "user.email=dcc@example.invalid",
                "commit",
                "--no-gpg-sign",
                "--no-verify",
                "-qm",
                "initial",
            ],
        );
        for path in [
            workspace_two.path(),
            &workspace_two.path().join(".git"),
            &workspace_two.path().join(".git/index"),
            &workspace_two.path().join("tracked.txt"),
        ] {
            remove_xattrs(path);
        }
        let workspace_two_path = fs::canonicalize(workspace_two.path()).unwrap();
        let conn = Connection::open(fixture.app_data.join("sessions.sqlite")).unwrap();
        conn.execute(
            r#"INSERT INTO dcc_workspaces
               (id, project_id, name, root_path, base_branch, state, created_at, updated_at)
               VALUES ('workspace-2', 'project-1', 'workspace-2', ?1, 'main', 'active', 't0', 't0')"#,
            params![workspace_two_path.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO dcc_sessions
               (id, project_id, workspace_id, provider_id, state, created_at, updated_at)
               VALUES ('session-2', 'project-1', 'workspace-2', 'test', 'active', 't0', 't0')"#,
            [],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO dcc_turn_change_sets
               (snapshot_id, session_id, turn_id, workspace_id, capture_version, state, created_at)
               VALUES ('snapshot-3', 'session-2', 'turn-3', 'workspace-2', 1, 'complete', 't0')"#,
            [],
        )
        .unwrap();
        let other_workspace = fixture
            .service
            .begin(CaptureV2Request {
                snapshot_id: "snapshot-3".to_owned(),
                session_id: SessionId("session-2".to_owned()),
                turn_id: TurnId("turn-3".to_owned()),
                workspace_id: WorkspaceId("workspace-2".to_owned()),
                workspace_absolute: workspace_two_path,
            })
            .unwrap();
        fixture.service.cancel(other_workspace).unwrap();

        let fixture = Fixture::new();
        let first = fixture.service.begin(fixture.request.clone()).unwrap();
        let error = fixture.service.begin(fixture.second_request()).unwrap_err();
        assert_eq!(
            error.reason_code,
            GuardedUndoReasonCode::ConcurrentWorkspaceMutation
        );
        let first = fixture.service.finish(first).unwrap();
        assert_eq!(
            first.reason_code,
            Some(GuardedUndoReasonCode::ConcurrentWorkspaceMutation)
        );
    }

    #[test]
    fn physical_replacements_are_classified_exactly() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let outside = tempfile::tempdir_in("/private/tmp").unwrap();
        fs::hard_link(
            fixture.request.workspace_absolute.join("tracked.txt"),
            outside.path().join("hard"),
        )
        .unwrap();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::HardlinkUnsupported)
        );

        let fixture = Fixture::new();
        let tracked = fixture.request.workspace_absolute.join("tracked.txt");
        fs::remove_file(&tracked).unwrap();
        let outside = tempfile::NamedTempFile::new_in("/private/tmp").unwrap();
        symlink(outside.path(), &tracked).unwrap();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::SymlinkOrReparsePoint)
        );

        let fixture = Fixture::new();
        let tracked = fixture.request.workspace_absolute.join("tracked.txt");
        fs::remove_file(&tracked).unwrap();
        let tracked = CString::new(tracked.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(tracked.as_ptr(), 0o600) }, 0);
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::NonRegularFile)
        );
    }

    #[test]
    fn result_edge_physical_replacements_are_classified_exactly() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let tracked = fixture.request.workspace_absolute.join("tracked.txt");
        fs::remove_file(&tracked).unwrap();
        let outside = tempfile::NamedTempFile::new_in("/private/tmp").unwrap();
        symlink(outside.path(), &tracked).unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::SymlinkOrReparsePoint)
        );

        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let tracked = fixture.request.workspace_absolute.join("tracked.txt");
        let outside = tempfile::NamedTempFile::new_in("/private/tmp").unwrap();
        fs::remove_file(&tracked).unwrap();
        fs::hard_link(outside.path(), &tracked).unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::HardlinkUnsupported)
        );

        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let tracked = fixture.request.workspace_absolute.join("tracked.txt");
        fs::remove_file(&tracked).unwrap();
        let tracked = CString::new(tracked.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(tracked.as_ptr(), 0o600) }, 0);
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::NonRegularFile)
        );
    }

    #[test]
    fn result_reopens_workspace_and_rejects_replaced_root_before_git() {
        let fixture = Fixture::new();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let workspace = fixture.request.workspace_absolute.clone();
        let displaced = workspace.with_extension("dcc-displaced");
        fs::rename(&workspace, &displaced).unwrap();
        fs::create_dir(&workspace).unwrap();
        remove_xattrs(&workspace);
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::RepositoryIdentityChanged)
        );
        fs::remove_dir(&workspace).unwrap();
        fs::rename(displaced, workspace).unwrap();
    }

    #[test]
    fn edge_deadlines_exclude_provider_time_and_git_preflight_is_terminal() {
        let mut fixture = Fixture::new();
        let clock = Arc::new(ManualClock::new(Instant::now()));
        fixture.service.clock = clock.clone();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        clock.advance(StdDuration::from_secs(CAPTURE_TIMEOUT_SECONDS + 1));
        fs::write(
            fixture.request.workspace_absolute.join("tracked.txt"),
            b"provider result\n",
        )
        .unwrap();
        let summary = fixture.service.finish(handle).unwrap();
        assert_eq!(summary.state, RestoreSetState::Eligible);

        let mut fixture = Fixture::new();
        let clock = Arc::new(ManualClock::new(Instant::now()));
        fixture.service.clock = clock.clone();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        let expired_result_edge = clock
            .now()
            .checked_sub(StdDuration::from_secs(CAPTURE_TIMEOUT_SECONDS + 1))
            .unwrap();
        let summary = fixture
            .service
            .finish_at(handle, expired_result_edge)
            .unwrap();
        assert_eq!(
            summary.reason_code,
            Some(GuardedUndoReasonCode::CaptureTimeout)
        );

        let fixture = Fixture::new();
        fs::remove_file(fixture.request.workspace_absolute.join(".git/index")).unwrap();
        assert!(fixture.service.begin(fixture.request.clone()).is_err());
        let summary = fixture
            .repo
            .get_guarded_undo_capture_summary("snapshot-1")
            .unwrap()
            .unwrap();
        assert_eq!(summary.state, "failed");
        assert_eq!(summary.file_count, 0);
    }

    #[test]
    fn exact_baseline_and_result_deadline_are_terminal_timeouts() {
        let mut fixture = Fixture::new();
        fixture.service.clock = Arc::new(BoundaryClock::new(Instant::now()));
        let error = fixture.service.begin(fixture.request.clone()).unwrap_err();
        assert_eq!(error.reason_code, GuardedUndoReasonCode::CaptureTimeout);
        let baseline = fixture
            .repo
            .get_guarded_undo_capture_summary("snapshot-1")
            .unwrap()
            .unwrap();
        assert_eq!(baseline.state, "ineligible");
        assert_eq!(baseline.reason_code.as_deref(), Some("capture_timeout"));

        let mut fixture = Fixture::new();
        let edge_start = Instant::now();
        let clock = Arc::new(ManualClock::new(edge_start));
        fixture.service.clock = clock.clone();
        let handle = fixture.service.begin(fixture.request.clone()).unwrap();
        clock.advance(StdDuration::from_secs(CAPTURE_TIMEOUT_SECONDS));
        let result = fixture.service.finish_at(handle, edge_start).unwrap();
        assert_eq!(result.state, RestoreSetState::Ineligible);
        assert_eq!(
            result.reason_code,
            Some(GuardedUndoReasonCode::CaptureTimeout)
        );
        let result = fixture
            .repo
            .get_guarded_undo_capture_summary("snapshot-1")
            .unwrap()
            .unwrap();
        assert_eq!(result.state, "ineligible");
        assert_eq!(result.reason_code.as_deref(), Some("capture_timeout"));
    }
}
