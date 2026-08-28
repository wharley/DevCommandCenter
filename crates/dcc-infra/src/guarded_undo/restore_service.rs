//! Guarded Undo prepare/execute/recovery protocol.
//!
//! Filesystem authority is deliberately abstract. Platform implementations
//! must retain descriptor-rooted worktree/Git authority and implement a true
//! same-directory exchange that leaves the displaced target at the journaled
//! locator. This service never accepts paths, hashes, or bytes from a UI.

#![cfg(feature = "guarded-undo-capture-v2")]

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use dcc_core::domain::{
    guarded_undo::{
        ArtifactKey, GitIdentityV1, GuardedUndoReasonCode, OpaqueRepoPath, PhysicalRootId,
        PreparedIdentityV1, RecoveryDetailsV1, RegularFileMetadataV1, RestoreSetId,
        RestoreSetState, Sha256Digest, TurnRestoreFile, TurnRestoreSet, UndoOperation,
        UndoOperationFile, UndoOperationFileState, UndoOperationId, UndoOperationState,
        VerificationOutcome, PREPARED_IDENTITY_SCHEMA_VERSION, RECOVERY_DETAILS_SCHEMA_VERSION,
        UNDO_JOURNAL_SCHEMA_VERSION,
    },
    workspace::WorkspaceId,
};
use uuid::Uuid;

use crate::db::SqliteSessionRepo;

const TOKEN_LIFETIME_SECONDS: i64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthorityMode {
    Shared,
    Exclusive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileEvidence {
    pub size: u64,
    pub sha256: Sha256Digest,
    pub metadata: RegularFileMetadataV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AuthorityGenerations {
    pub worktree: u64,
    pub git_dir: u64,
    pub common_dir: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InversePreview {
    pub display_path: String,
    pub size: u64,
    pub binary: bool,
    pub preview: Option<String>,
}

/// Descriptor-retaining authority supplied by the reviewed platform adapter.
/// Implementations must reject symlink/reparse traversal and hardlinks.
pub(crate) trait RestoreAuthority: Send {
    fn mode(&self) -> AuthorityMode;
    fn root_id(&self) -> PhysicalRootId;
    fn git_identity(&self) -> Result<GitIdentityV1, GuardedUndoReasonCode>;
    fn coordinator_generations(&self) -> AuthorityGenerations;
    fn inspect_target(&self, path: &OpaqueRepoPath) -> Result<FileEvidence, GuardedUndoReasonCode>;
    fn verify_preimage(
        &self,
        key: ArtifactKey,
        size: u64,
        sha256: Sha256Digest,
    ) -> Result<(), GuardedUndoReasonCode>;
    fn inverse_preview(
        &self,
        file: &TurnRestoreFile,
    ) -> Result<InversePreview, GuardedUndoReasonCode>;

    /// Creates and durably syncs a same-directory preimage file. It must not
    /// mutate the target. The returned evidence is the staged inode identity
    /// later expected at the target after exchange.
    fn stage_preimage(
        &self,
        file: &TurnRestoreFile,
        exchange_key: ArtifactKey,
    ) -> Result<FileEvidence, GuardedUndoReasonCode>;
    fn inspect_exchange(
        &self,
        path: &OpaqueRepoPath,
        exchange_key: ArtifactKey,
    ) -> Result<FileEvidence, GuardedUndoReasonCode>;
    /// Atomically swaps target and exchange locator, retaining the exact file
    /// displaced from target at that locator.
    fn exchange(
        &self,
        path: &OpaqueRepoPath,
        exchange_key: ArtifactKey,
        expected_target: &FileEvidence,
        expected_exchange: &FileEvidence,
    ) -> Result<(), GuardedUndoReasonCode>;
    fn cleanup_exchange(
        &self,
        path: &OpaqueRepoPath,
        exchange_key: ArtifactKey,
        expected_exchange: &FileEvidence,
    ) -> Result<(), GuardedUndoReasonCode>;
}

/// Platform admission boundary. `workspace_absolute` is durable backend
/// mapping, never UI input. The returned object owns all descriptors/leases.
pub(crate) trait RestoreAuthorityAdapter: Send + Sync {
    fn acquire(
        &self,
        workspace_absolute: &Path,
        expected_root: &PhysicalRootId,
        mode: AuthorityMode,
    ) -> Result<Box<dyn RestoreAuthority>, GuardedUndoReasonCode>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareReady {
    pub snapshot_id: String,
    pub preview_token: String,
    pub expires_at: String,
    pub file_count: u32,
    pub total_bytes: u64,
    pub files: Vec<InversePreview>,
    pub unrelated_paths_are_not_targets: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrepareGuardedUndoResult {
    Ready(PrepareReady),
    Blocked {
        snapshot_id: String,
        reason_code: GuardedUndoReasonCode,
    },
    Unavailable {
        snapshot_id: String,
        reason_code: GuardedUndoReasonCode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecuteGuardedUndoResult {
    Completed {
        operation_id: String,
    },
    Blocked(GuardedUndoReasonCode),
    RolledBack {
        operation_id: String,
    },
    RecoveryRequired {
        operation_id: String,
        reason_code: GuardedUndoReasonCode,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub completed: u32,
    pub rolled_back: u32,
    pub recovery_required: u32,
}

#[derive(Clone)]
struct PreparedToken {
    snapshot_id: String,
    workspace_id: WorkspaceId,
    workspace_absolute: PathBuf,
    restore_set_id: RestoreSetId,
    root_id: PhysicalRootId,
    git: GitIdentityV1,
    manifest_digest: Sha256Digest,
    coordinator_generations: AuthorityGenerations,
    expires_at: DateTime<Utc>,
}

impl fmt::Debug for PreparedToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedToken([redacted])")
    }
}

#[derive(Default)]
struct TokenState {
    by_digest: HashMap<Sha256Digest, PreparedToken>,
}

pub struct RestoreService {
    repo: SqliteSessionRepo,
    adapter: Arc<dyn RestoreAuthorityAdapter>,
    tokens: Mutex<TokenState>,
}

impl fmt::Debug for RestoreService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RestoreService([redacted])")
    }
}

impl RestoreService {
    pub(crate) fn new(repo: SqliteSessionRepo, adapter: Arc<dyn RestoreAuthorityAdapter>) -> Self {
        Self {
            repo,
            adapter,
            tokens: Mutex::new(TokenState::default()),
        }
    }

    pub fn prepare(
        &self,
        snapshot_id: &str,
        workspace_absolute: &Path,
    ) -> PrepareGuardedUndoResult {
        self.prepare_at(snapshot_id, workspace_absolute, Utc::now())
    }

    fn prepare_at(
        &self,
        snapshot_id: &str,
        workspace_absolute: &Path,
        now: DateTime<Utc>,
    ) -> PrepareGuardedUndoResult {
        let unavailable = |reason_code| PrepareGuardedUndoResult::Unavailable {
            snapshot_id: snapshot_id.to_owned(),
            reason_code,
        };
        let blocked = |reason_code| PrepareGuardedUndoResult::Blocked {
            snapshot_id: snapshot_id.to_owned(),
            reason_code,
        };
        let Some((restore, files)) = self
            .repo
            .get_turn_restore_set_by_snapshot(snapshot_id)
            .ok()
            .flatten()
        else {
            return unavailable(GuardedUndoReasonCode::CaptureV2Missing);
        };
        if self
            .repo
            .get_active_guarded_undo_summary(snapshot_id)
            .ok()
            .flatten()
            .is_some()
            || self
                .repo
                .has_guarded_undo_cleanup_pending(snapshot_id)
                .unwrap_or(true)
        {
            return blocked(GuardedUndoReasonCode::MutationInProgress);
        }
        if restore.state != RestoreSetState::Eligible {
            return unavailable(reason_for_set(&restore));
        }
        let Some(expires_at) = restore.expires_at.as_deref().and_then(parse_timestamp) else {
            return unavailable(GuardedUndoReasonCode::InvalidPersistedRecord);
        };
        if expires_at <= now {
            return unavailable(GuardedUndoReasonCode::RetentionExpired);
        }
        let Some(root_id) = restore.root_id.clone() else {
            return unavailable(GuardedUndoReasonCode::InvalidPersistedRecord);
        };
        let Some(git) = restore.git_identity.clone() else {
            return unavailable(GuardedUndoReasonCode::InvalidPersistedRecord);
        };
        let Some(manifest_digest) = restore.manifest_digest else {
            return unavailable(GuardedUndoReasonCode::InvalidPersistedRecord);
        };
        let authority =
            match self
                .adapter
                .acquire(workspace_absolute, &root_id, AuthorityMode::Shared)
            {
                Ok(authority) => authority,
                Err(reason) => return blocked(reason),
            };
        if authority.mode() != AuthorityMode::Shared || authority.root_id() != root_id {
            return blocked(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        if authority.git_identity().ok().as_ref() != Some(&git) {
            return blocked(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        let mut previews = Vec::with_capacity(files.len());
        for file in &files {
            if let Err(reason) =
                authority.verify_preimage(file.pre_artifact_key, file.pre_size, file.pre_sha256)
            {
                return blocked(reason);
            }
            let current = match authority.inspect_target(&file.path_bytes) {
                Ok(current) => current,
                Err(reason) => return blocked(reason),
            };
            if !matches_result(&current, file) {
                return blocked(GuardedUndoReasonCode::TargetResultMismatch);
            }
            match authority.inverse_preview(file) {
                Ok(preview) => previews.push(preview),
                Err(reason) => return blocked(reason),
            }
        }
        let expires_at = now + Duration::seconds(TOKEN_LIFETIME_SECONDS);
        let token = random_token();
        let token_digest = Sha256Digest::of(token.as_bytes());
        let prepared = PreparedToken {
            snapshot_id: snapshot_id.to_owned(),
            workspace_id: restore.workspace_id.clone(),
            workspace_absolute: workspace_absolute.to_path_buf(),
            restore_set_id: restore.restore_set_id,
            root_id,
            git,
            manifest_digest,
            coordinator_generations: authority.coordinator_generations(),
            expires_at,
        };
        let Ok(mut tokens) = self.tokens.lock() else {
            return blocked(GuardedUndoReasonCode::MutationInProgress);
        };
        tokens
            .by_digest
            .retain(|_, current| current.workspace_id != prepared.workspace_id);
        tokens.by_digest.insert(token_digest, prepared);
        PrepareGuardedUndoResult::Ready(PrepareReady {
            snapshot_id: snapshot_id.to_owned(),
            preview_token: token,
            expires_at: timestamp(expires_at),
            file_count: restore.file_count,
            total_bytes: restore.artifact_bytes,
            files: previews,
            unrelated_paths_are_not_targets: true,
        })
    }

    pub fn execute(&self, preview_token: &str, confirmed: bool) -> ExecuteGuardedUndoResult {
        self.execute_at(preview_token, confirmed, Utc::now())
    }

    fn execute_at(
        &self,
        preview_token: &str,
        confirmed: bool,
        now: DateTime<Utc>,
    ) -> ExecuteGuardedUndoResult {
        let digest = Sha256Digest::of(preview_token.as_bytes());
        let prepared = match self.tokens.lock() {
            Ok(mut tokens) => tokens.by_digest.remove(&digest),
            Err(_) => None,
        };
        let Some(prepared) = prepared else {
            return ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::PreviewConsumed);
        };
        if !confirmed {
            return ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::PreviewConsumed);
        }
        if prepared.expires_at <= now {
            return ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::PreviewExpired);
        }
        let Some((restore, files)) = self
            .repo
            .get_turn_restore_set(&prepared.restore_set_id)
            .ok()
            .flatten()
        else {
            return ExecuteGuardedUndoResult::Blocked(
                GuardedUndoReasonCode::InvalidPersistedRecord,
            );
        };
        if !prepared_matches_set(&prepared, &restore) {
            return ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::PreviewContextChanged);
        }
        if restore
            .expires_at
            .as_deref()
            .and_then(parse_timestamp)
            .is_none_or(|expires_at| expires_at <= now)
        {
            return ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::RetentionExpired);
        }
        let authority = match self.adapter.acquire(
            &prepared.workspace_absolute,
            &prepared.root_id,
            AuthorityMode::Exclusive,
        ) {
            Ok(authority) => authority,
            Err(reason) => return ExecuteGuardedUndoResult::Blocked(reason),
        };
        if let Err(reason) = validate_execute_authority(authority.as_ref(), &prepared, &files) {
            return ExecuteGuardedUndoResult::Blocked(reason);
        }

        let operation_id = UndoOperationId(Uuid::new_v4().to_string());
        let mut journal_files = files
            .iter()
            .map(|file| UndoOperationFile {
                operation_id: operation_id.clone(),
                restore_set_id: restore.restore_set_id.clone(),
                ordinal: file.ordinal,
                path_bytes: file.path_bytes.clone(),
                exchange_artifact_key: ArtifactKey(*Uuid::new_v4().as_bytes()),
                expected_result_size: file.result_size,
                expected_result_sha256: file.result_sha256,
                expected_metadata: file.metadata_fingerprint.clone(),
                pre_size: file.pre_size,
                pre_sha256: file.pre_sha256,
                staged_metadata: None,
                displaced_size: None,
                displaced_sha256: None,
                displaced_metadata: None,
                state: UndoOperationFileState::Planned,
                verification_outcome: VerificationOutcome::Pending,
                recovery_details: None,
                updated_at: timestamp(now),
            })
            .collect::<Vec<_>>();
        let operation = UndoOperation {
            operation_id: operation_id.clone(),
            restore_set_id: restore.restore_set_id.clone(),
            journal_version: UNDO_JOURNAL_SCHEMA_VERSION,
            state: UndoOperationState::Preparing,
            active: true,
            preview_token_digest: Some(digest),
            prepared_identity: PreparedIdentityV1 {
                schema_version: PREPARED_IDENTITY_SCHEMA_VERSION,
                root_id: prepared.root_id,
                git: prepared.git,
                manifest_digest: prepared.manifest_digest,
                coordinator_generation: prepared.coordinator_generations.worktree,
                git_dir_generation: prepared.coordinator_generations.git_dir,
                common_dir_generation: prepared.coordinator_generations.common_dir,
            },
            reason_code: None,
            recovery_details: None,
            created_at: timestamp(now),
            updated_at: timestamp(now),
            completed_at: None,
        };
        // Every future same-directory locator is durable before raw preimage
        // bytes can appear in the workspace. A crash during staging is thus
        // fully enumerable by startup recovery.
        if self
            .repo
            .create_undo_operation(&operation, &journal_files)
            .is_err()
        {
            return ExecuteGuardedUndoResult::Blocked(
                GuardedUndoReasonCode::InvalidPersistedRecord,
            );
        }
        for file in &files {
            let index = file.ordinal as usize;
            let exchange_key = journal_files[index].exchange_artifact_key;
            let staged = match authority.stage_preimage(file, exchange_key) {
                Ok(staged) => staged,
                Err(reason) => {
                    return self.abort_preparing(
                        authority.as_ref(),
                        &operation_id,
                        &journal_files,
                        reason,
                        now,
                    );
                }
            };
            if staged.size != file.pre_size || staged.sha256 != file.pre_sha256 {
                let _ = authority.cleanup_exchange(&file.path_bytes, exchange_key, &staged);
                return self.abort_preparing(
                    authority.as_ref(),
                    &operation_id,
                    &journal_files,
                    GuardedUndoReasonCode::ArtifactCorrupt,
                    now,
                );
            }
            let current = authority.inspect_target(&file.path_bytes);
            if !current
                .as_ref()
                .is_ok_and(|value| matches_result(value, file))
            {
                let _ = authority.cleanup_exchange(&file.path_bytes, exchange_key, &staged);
                return self.abort_preparing(
                    authority.as_ref(),
                    &operation_id,
                    &journal_files,
                    GuardedUndoReasonCode::TargetResultMismatch,
                    now,
                );
            }
            let mut staged_file = journal_files[index].clone();
            staged_file.staged_metadata = Some(staged.metadata);
            staged_file.state = UndoOperationFileState::Staged;
            if self
                .repo
                .transition_undo_operation_file(&UndoOperationFileState::Planned, &staged_file)
                .is_err()
            {
                return self.require_recovery(
                    &operation_id,
                    UndoOperationState::Preparing,
                    GuardedUndoReasonCode::OperationInterrupted,
                    "staged_journal_failed",
                    now,
                );
            }
            journal_files[index] = staged_file;
        }
        if self
            .repo
            .transition_undo_operation(
                &operation_id,
                &UndoOperationState::Preparing,
                &UndoOperationState::Prepared,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                &operation_id,
                UndoOperationState::Preparing,
                GuardedUndoReasonCode::OperationInterrupted,
                "prepared_transition_failed",
                now,
            );
        }
        if self
            .repo
            .transition_undo_operation(
                &operation_id,
                &UndoOperationState::Prepared,
                &UndoOperationState::Applying,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                &operation_id,
                UndoOperationState::Prepared,
                GuardedUndoReasonCode::OperationInterrupted,
                "applying_transition_failed",
                now,
            );
        }

        for journal_file in &journal_files {
            if apply_one(authority.as_ref(), journal_file).is_err()
                || self.record_applied_verified(journal_file, now).is_err()
            {
                return self.rollback_operation(
                    authority.as_ref(),
                    &operation_id,
                    UndoOperationState::Applying,
                    &journal_files,
                    now,
                );
            }
        }
        if authority.git_identity().ok().as_ref() != Some(&operation.prepared_identity.git) {
            return self.rollback_operation(
                authority.as_ref(),
                &operation_id,
                UndoOperationState::Applying,
                &journal_files,
                now,
            );
        }
        if self
            .repo
            .transition_undo_operation(
                &operation_id,
                &UndoOperationState::Applying,
                &UndoOperationState::Verifying,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                &operation_id,
                UndoOperationState::Applying,
                GuardedUndoReasonCode::OperationInterrupted,
                "verifying_transition_failed",
                now,
            );
        }
        if journal_files
            .iter()
            .any(|file| !pair_is_applied(authority.as_ref(), file))
            || authority.git_identity().ok().as_ref() != Some(&operation.prepared_identity.git)
        {
            return self.rollback_operation(
                authority.as_ref(),
                &operation_id,
                UndoOperationState::Verifying,
                &journal_files,
                now,
            );
        }
        if self
            .repo
            .transition_undo_operation(
                &operation_id,
                &UndoOperationState::Verifying,
                &UndoOperationState::Completed,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                &operation_id,
                UndoOperationState::Verifying,
                GuardedUndoReasonCode::OperationInterrupted,
                "completion_transition_failed",
                now,
            );
        }
        if self.cleanup_terminal(
            authority.as_ref(),
            &journal_files,
            UndoOperationState::Completed,
        ) {
            let _ = self.repo.finish_undo_operation_cleanup(&operation_id);
        }
        ExecuteGuardedUndoResult::Completed {
            operation_id: operation_id.0,
        }
    }

    /// Startup caller must hold the process-wide app-data lifetime lock and
    /// must not admit prepare/execute until this returns.
    pub fn recover_startup<F>(&self, mut resolve: F) -> RecoveryReport
    where
        F: FnMut(&WorkspaceId) -> Option<PathBuf>,
    {
        let mut report = RecoveryReport::default();
        let Ok(operations) = self.repo.list_active_undo_operations() else {
            return report;
        };
        for (operation, files) in operations {
            if operation.state == UndoOperationState::RecoveryRequired {
                report.recovery_required = report.recovery_required.saturating_add(1);
                continue;
            }
            let Some((restore, _)) = self
                .repo
                .get_turn_restore_set(&operation.restore_set_id)
                .ok()
                .flatten()
            else {
                let _ = self.require_recovery(
                    &operation.operation_id,
                    operation.state,
                    GuardedUndoReasonCode::InvalidPersistedRecord,
                    "restore_set_missing",
                    Utc::now(),
                );
                report.recovery_required = report.recovery_required.saturating_add(1);
                continue;
            };
            let Some(workspace_absolute) = resolve(&restore.workspace_id) else {
                let _ = self.require_recovery(
                    &operation.operation_id,
                    operation.state,
                    GuardedUndoReasonCode::WorkspaceMissing,
                    "workspace_missing",
                    Utc::now(),
                );
                report.recovery_required = report.recovery_required.saturating_add(1);
                continue;
            };
            let authority = match self.adapter.acquire(
                &workspace_absolute,
                &operation.prepared_identity.root_id,
                AuthorityMode::Exclusive,
            ) {
                Ok(authority) => authority,
                Err(reason) => {
                    let _ = self.require_recovery(
                        &operation.operation_id,
                        operation.state,
                        reason,
                        "recovery_authority_unavailable",
                        Utc::now(),
                    );
                    report.recovery_required = report.recovery_required.saturating_add(1);
                    continue;
                }
            };
            if authority.git_identity().ok().as_ref() != Some(&operation.prepared_identity.git) {
                let _ = self.require_recovery(
                    &operation.operation_id,
                    operation.state,
                    GuardedUndoReasonCode::RepositoryIdentityChanged,
                    "recovery_git_changed",
                    Utc::now(),
                );
                report.recovery_required = report.recovery_required.saturating_add(1);
                continue;
            }
            if operation.state == UndoOperationState::Preparing {
                match self.recover_preparing(
                    authority.as_ref(),
                    &operation.operation_id,
                    &files,
                    Utc::now(),
                ) {
                    ExecuteGuardedUndoResult::RolledBack { .. } => {
                        report.rolled_back = report.rolled_back.saturating_add(1)
                    }
                    _ => report.recovery_required = report.recovery_required.saturating_add(1),
                }
                continue;
            }
            let all_result = files
                .iter()
                .all(|file| pair_is_unapplied(authority.as_ref(), file));
            let all_pre = files
                .iter()
                .all(|file| pair_is_applied(authority.as_ref(), file));
            let now = Utc::now();
            if all_pre
                && matches!(
                    operation.state,
                    UndoOperationState::Applying | UndoOperationState::Verifying
                )
            {
                let recorded = files
                    .iter()
                    .all(|file| self.record_applied_verified(file, now).is_ok());
                let transitioned = if operation.state == UndoOperationState::Applying {
                    self.repo.transition_undo_operation(
                        &operation.operation_id,
                        &UndoOperationState::Applying,
                        &UndoOperationState::Verifying,
                        None,
                        None,
                        &timestamp(now),
                    )
                } else {
                    Ok(false)
                };
                if recorded
                    && transitioned.is_ok()
                    && self
                        .repo
                        .transition_undo_operation(
                            &operation.operation_id,
                            &UndoOperationState::Verifying,
                            &UndoOperationState::Completed,
                            None,
                            None,
                            &timestamp(now),
                        )
                        .is_ok()
                {
                    report.completed = report.completed.saturating_add(1);
                    continue;
                }
            }
            if all_result
                || matches!(
                    operation.state,
                    UndoOperationState::Applying
                        | UndoOperationState::Verifying
                        | UndoOperationState::RollingBack
                )
            {
                let outcome = self.rollback_operation(
                    authority.as_ref(),
                    &operation.operation_id,
                    operation.state,
                    &files,
                    now,
                );
                match outcome {
                    ExecuteGuardedUndoResult::RolledBack { .. } => {
                        report.rolled_back = report.rolled_back.saturating_add(1)
                    }
                    _ => report.recovery_required = report.recovery_required.saturating_add(1),
                }
                continue;
            }
            let _ = self.require_recovery(
                &operation.operation_id,
                operation.state,
                GuardedUndoReasonCode::ManualRecoveryRequired,
                "recovery_pair_diverged",
                now,
            );
            report.recovery_required = report.recovery_required.saturating_add(1);
        }
        if let Ok(cleanups) = self.repo.list_undo_operations_pending_cleanup() {
            for (operation, files) in cleanups {
                let Some((restore, _)) = self
                    .repo
                    .get_turn_restore_set(&operation.restore_set_id)
                    .ok()
                    .flatten()
                else {
                    continue;
                };
                let Some(workspace_absolute) = resolve(&restore.workspace_id) else {
                    continue;
                };
                let Ok(authority) = self.adapter.acquire(
                    &workspace_absolute,
                    &operation.prepared_identity.root_id,
                    AuthorityMode::Exclusive,
                ) else {
                    continue;
                };
                if self.cleanup_terminal(authority.as_ref(), &files, operation.state) {
                    let _ = self
                        .repo
                        .finish_undo_operation_cleanup(&operation.operation_id);
                }
            }
        }
        report
    }

    fn recover_preparing(
        &self,
        authority: &dyn RestoreAuthority,
        operation_id: &UndoOperationId,
        files: &[UndoOperationFile],
        now: DateTime<Utc>,
    ) -> ExecuteGuardedUndoResult {
        if self
            .repo
            .transition_undo_operation(
                operation_id,
                &UndoOperationState::Preparing,
                &UndoOperationState::RollingBack,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                operation_id,
                UndoOperationState::Preparing,
                GuardedUndoReasonCode::OperationInterrupted,
                "preparing_recovery_transition_failed",
                now,
            );
        }
        for file in files {
            match file.state {
                UndoOperationFileState::Planned => {
                    match authority.inspect_exchange(&file.path_bytes, file.exchange_artifact_key) {
                        Ok(observed)
                            if observed.size == file.pre_size
                                && observed.sha256 == file.pre_sha256 =>
                        {
                            if authority
                                .cleanup_exchange(
                                    &file.path_bytes,
                                    file.exchange_artifact_key,
                                    &observed,
                                )
                                .is_err()
                            {
                                return self.require_recovery(
                                    operation_id,
                                    UndoOperationState::RollingBack,
                                    GuardedUndoReasonCode::ManualRecoveryRequired,
                                    "planned_sibling_cleanup_failed",
                                    now,
                                );
                            }
                        }
                        Err(GuardedUndoReasonCode::DisplacedFileMissing)
                        | Err(GuardedUndoReasonCode::TargetMissing)
                        | Err(GuardedUndoReasonCode::ArtifactMissing) => {}
                        _ => {
                            let _ = self.mark_file_recovery(file, now);
                            return self.require_recovery(
                                operation_id,
                                UndoOperationState::RollingBack,
                                GuardedUndoReasonCode::ManualRecoveryRequired,
                                "planned_sibling_diverged",
                                now,
                            );
                        }
                    }
                }
                UndoOperationFileState::Staged if pair_is_unapplied(authority, file) => {
                    let expected = staged_evidence(file).expect("staged file has metadata");
                    if authority
                        .cleanup_exchange(&file.path_bytes, file.exchange_artifact_key, &expected)
                        .is_err()
                    {
                        return self.require_recovery(
                            operation_id,
                            UndoOperationState::RollingBack,
                            GuardedUndoReasonCode::ManualRecoveryRequired,
                            "staged_sibling_cleanup_failed",
                            now,
                        );
                    }
                }
                _ => {
                    let _ = self.mark_file_recovery(file, now);
                    return self.require_recovery(
                        operation_id,
                        UndoOperationState::RollingBack,
                        GuardedUndoReasonCode::ManualRecoveryRequired,
                        "preparing_state_diverged",
                        now,
                    );
                }
            }
            if self.mark_file_rolled_back(file, now).is_err() {
                return self.require_recovery(
                    operation_id,
                    UndoOperationState::RollingBack,
                    GuardedUndoReasonCode::ManualRecoveryRequired,
                    "preparing_file_finalize_failed",
                    now,
                );
            }
        }
        if self
            .repo
            .transition_undo_operation(
                operation_id,
                &UndoOperationState::RollingBack,
                &UndoOperationState::RolledBack,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                operation_id,
                UndoOperationState::RollingBack,
                GuardedUndoReasonCode::OperationInterrupted,
                "preparing_recovery_finalize_failed",
                now,
            );
        }
        let _ = self.repo.finish_undo_operation_cleanup(operation_id);
        ExecuteGuardedUndoResult::RolledBack {
            operation_id: operation_id.0.clone(),
        }
    }

    fn abort_preparing(
        &self,
        authority: &dyn RestoreAuthority,
        operation_id: &UndoOperationId,
        files: &[UndoOperationFile],
        _reason: GuardedUndoReasonCode,
        now: DateTime<Utc>,
    ) -> ExecuteGuardedUndoResult {
        if self
            .repo
            .transition_undo_operation(
                operation_id,
                &UndoOperationState::Preparing,
                &UndoOperationState::RollingBack,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                operation_id,
                UndoOperationState::Preparing,
                GuardedUndoReasonCode::OperationInterrupted,
                "preparing_abort_failed",
                now,
            );
        }
        for file in files {
            if let Some(expected) = staged_evidence(file) {
                let _ = authority.cleanup_exchange(
                    &file.path_bytes,
                    file.exchange_artifact_key,
                    &expected,
                );
            }
            if self.mark_file_rolled_back(file, now).is_err() {
                return self.require_recovery(
                    operation_id,
                    UndoOperationState::RollingBack,
                    GuardedUndoReasonCode::ManualRecoveryRequired,
                    "preparing_cleanup_failed",
                    now,
                );
            }
        }
        if self
            .repo
            .transition_undo_operation(
                operation_id,
                &UndoOperationState::RollingBack,
                &UndoOperationState::RolledBack,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                operation_id,
                UndoOperationState::RollingBack,
                GuardedUndoReasonCode::OperationInterrupted,
                "preparing_rollback_failed",
                now,
            );
        }
        if self.cleanup_terminal(authority, files, UndoOperationState::RolledBack) {
            let _ = self.repo.finish_undo_operation_cleanup(operation_id);
        }
        ExecuteGuardedUndoResult::RolledBack {
            operation_id: operation_id.0.clone(),
        }
    }

    fn record_applied_verified(
        &self,
        original: &UndoOperationFile,
        now: DateTime<Utc>,
    ) -> Result<(), ()> {
        let Some((_, files)) = self
            .repo
            .get_undo_operation(&original.operation_id)
            .ok()
            .flatten()
        else {
            return Err(());
        };
        let Some(mut current) = files
            .into_iter()
            .find(|file| file.ordinal == original.ordinal)
        else {
            return Err(());
        };
        if current.state == UndoOperationFileState::Staged {
            current.displaced_size = Some(current.expected_result_size);
            current.displaced_sha256 = Some(current.expected_result_sha256);
            current.displaced_metadata = Some(current.expected_metadata.clone());
            current.state = UndoOperationFileState::Applied;
            current.updated_at = timestamp(now);
            self.repo
                .transition_undo_operation_file(&UndoOperationFileState::Staged, &current)
                .map_err(|_| ())?;
        }
        if current.state == UndoOperationFileState::Applied {
            current.state = UndoOperationFileState::Verified;
            current.verification_outcome = VerificationOutcome::Verified;
            current.updated_at = timestamp(now);
            self.repo
                .transition_undo_operation_file(&UndoOperationFileState::Applied, &current)
                .map_err(|_| ())?;
        }
        (current.state == UndoOperationFileState::Verified)
            .then_some(())
            .ok_or(())
    }

    fn rollback_operation(
        &self,
        authority: &dyn RestoreAuthority,
        operation_id: &UndoOperationId,
        mut operation_state: UndoOperationState,
        files: &[UndoOperationFile],
        now: DateTime<Utc>,
    ) -> ExecuteGuardedUndoResult {
        if operation_state != UndoOperationState::RollingBack {
            if self
                .repo
                .transition_undo_operation(
                    operation_id,
                    &operation_state,
                    &UndoOperationState::RollingBack,
                    None,
                    None,
                    &timestamp(now),
                )
                .is_err()
            {
                return self.require_recovery(
                    operation_id,
                    operation_state,
                    GuardedUndoReasonCode::ExchangeRollbackFailed,
                    "rollback_transition_failed",
                    now,
                );
            }
            operation_state = UndoOperationState::RollingBack;
        }
        for original in files.iter().rev() {
            if pair_is_applied(authority, original)
                && authority
                    .exchange(
                        &original.path_bytes,
                        original.exchange_artifact_key,
                        &staged_evidence(original).expect("applied pair has staged evidence"),
                        &result_evidence(original),
                    )
                    .is_err()
            {
                return self.require_recovery(
                    operation_id,
                    operation_state,
                    GuardedUndoReasonCode::ExchangeRollbackFailed,
                    "rollback_exchange_failed",
                    now,
                );
            }
            if !pair_is_unapplied(authority, original) {
                let _ = self.mark_file_recovery(original, now);
                return self.require_recovery(
                    operation_id,
                    operation_state,
                    GuardedUndoReasonCode::RecoveryTargetChanged,
                    "rollback_pair_diverged",
                    now,
                );
            }
            if self.mark_file_rolled_back(original, now).is_err() {
                return self.require_recovery(
                    operation_id,
                    operation_state,
                    GuardedUndoReasonCode::ExchangeRollbackFailed,
                    "rollback_journal_failed",
                    now,
                );
            }
        }
        if self
            .repo
            .transition_undo_operation(
                operation_id,
                &UndoOperationState::RollingBack,
                &UndoOperationState::RolledBack,
                None,
                None,
                &timestamp(now),
            )
            .is_err()
        {
            return self.require_recovery(
                operation_id,
                UndoOperationState::RollingBack,
                GuardedUndoReasonCode::ExchangeRollbackFailed,
                "rollback_completion_failed",
                now,
            );
        }
        if self.cleanup_terminal(authority, files, UndoOperationState::RolledBack) {
            let _ = self.repo.finish_undo_operation_cleanup(operation_id);
        }
        ExecuteGuardedUndoResult::RolledBack {
            operation_id: operation_id.0.clone(),
        }
    }

    fn mark_file_rolled_back(
        &self,
        original: &UndoOperationFile,
        now: DateTime<Utc>,
    ) -> Result<(), ()> {
        let Some((_, files)) = self
            .repo
            .get_undo_operation(&original.operation_id)
            .ok()
            .flatten()
        else {
            return Err(());
        };
        let Some(mut current) = files
            .into_iter()
            .find(|file| file.ordinal == original.ordinal)
        else {
            return Err(());
        };
        if current.state == UndoOperationFileState::RolledBack {
            return Ok(());
        }
        let expected = current.state.clone();
        if matches!(
            current.state,
            UndoOperationFileState::Applied | UndoOperationFileState::Verified
        ) {
            current.displaced_size = Some(current.expected_result_size);
            current.displaced_sha256 = Some(current.expected_result_sha256);
            current.displaced_metadata = Some(current.expected_metadata.clone());
        }
        current.state = UndoOperationFileState::RolledBack;
        current.verification_outcome = VerificationOutcome::Verified;
        current.recovery_details = None;
        current.updated_at = timestamp(now);
        self.repo
            .transition_undo_operation_file(&expected, &current)
            .map(|_| ())
            .map_err(|_| ())
    }

    fn mark_file_recovery(
        &self,
        original: &UndoOperationFile,
        now: DateTime<Utc>,
    ) -> Result<(), ()> {
        let Some((_, files)) = self
            .repo
            .get_undo_operation(&original.operation_id)
            .ok()
            .flatten()
        else {
            return Err(());
        };
        let Some(mut current) = files
            .into_iter()
            .find(|file| file.ordinal == original.ordinal)
        else {
            return Err(());
        };
        if current.state == UndoOperationFileState::RecoveryRequired {
            return Ok(());
        }
        let expected = current.state.clone();
        let details = recovery_details(
            GuardedUndoReasonCode::ManualRecoveryRequired,
            "file_pair_diverged",
        );
        current.state = UndoOperationFileState::RecoveryRequired;
        current.verification_outcome = VerificationOutcome::Failed;
        current.recovery_details = Some(details);
        current.updated_at = timestamp(now);
        self.repo
            .transition_undo_operation_file(&expected, &current)
            .map(|_| ())
            .map_err(|_| ())
    }

    fn require_recovery(
        &self,
        operation_id: &UndoOperationId,
        expected: UndoOperationState,
        reason: GuardedUndoReasonCode,
        label: &str,
        now: DateTime<Utc>,
    ) -> ExecuteGuardedUndoResult {
        let details = recovery_details(reason.clone(), label);
        let _ = self.repo.transition_undo_operation(
            operation_id,
            &expected,
            &UndoOperationState::RecoveryRequired,
            Some(&reason),
            Some(&details),
            &timestamp(now),
        );
        ExecuteGuardedUndoResult::RecoveryRequired {
            operation_id: operation_id.0.clone(),
            reason_code: reason,
        }
    }

    fn cleanup_terminal(
        &self,
        authority: &dyn RestoreAuthority,
        files: &[UndoOperationFile],
        state: UndoOperationState,
    ) -> bool {
        files.iter().all(|file| {
            let expected = match state {
                UndoOperationState::Completed => Some(result_evidence(file)),
                UndoOperationState::RolledBack | UndoOperationState::Blocked => {
                    staged_evidence(file)
                }
                _ => return false,
            };
            let Some(expected) = expected else {
                return matches!(
                    authority.inspect_exchange(&file.path_bytes, file.exchange_artifact_key),
                    Err(GuardedUndoReasonCode::DisplacedFileMissing)
                        | Err(GuardedUndoReasonCode::TargetMissing)
                        | Err(GuardedUndoReasonCode::ArtifactMissing)
                );
            };
            match authority.inspect_exchange(&file.path_bytes, file.exchange_artifact_key) {
                Ok(observed) if observed == expected => authority
                    .cleanup_exchange(&file.path_bytes, file.exchange_artifact_key, &expected)
                    .is_ok(),
                Err(GuardedUndoReasonCode::DisplacedFileMissing)
                | Err(GuardedUndoReasonCode::TargetMissing)
                | Err(GuardedUndoReasonCode::ArtifactMissing) => true,
                _ => false,
            }
        })
    }
}

fn validate_execute_authority(
    authority: &dyn RestoreAuthority,
    prepared: &PreparedToken,
    files: &[TurnRestoreFile],
) -> Result<(), GuardedUndoReasonCode> {
    if authority.mode() != AuthorityMode::Exclusive || authority.root_id() != prepared.root_id {
        return Err(GuardedUndoReasonCode::RepositoryIdentityChanged);
    }
    let expected_generations = AuthorityGenerations {
        worktree: prepared
            .coordinator_generations
            .worktree
            .checked_add(1)
            .ok_or(GuardedUndoReasonCode::PreviewContextChanged)?,
        git_dir: prepared
            .coordinator_generations
            .git_dir
            .checked_add(1)
            .ok_or(GuardedUndoReasonCode::PreviewContextChanged)?,
        common_dir: prepared
            .coordinator_generations
            .common_dir
            .checked_add(1)
            .ok_or(GuardedUndoReasonCode::PreviewContextChanged)?,
    };
    if authority.coordinator_generations() != expected_generations {
        return Err(GuardedUndoReasonCode::PreviewContextChanged);
    }
    if authority.git_identity()? != prepared.git {
        return Err(GuardedUndoReasonCode::RepositoryIdentityChanged);
    }
    for file in files {
        authority.verify_preimage(file.pre_artifact_key, file.pre_size, file.pre_sha256)?;
        if !matches_result(&authority.inspect_target(&file.path_bytes)?, file) {
            return Err(GuardedUndoReasonCode::TargetResultMismatch);
        }
    }
    Ok(())
}

fn apply_one(
    authority: &dyn RestoreAuthority,
    file: &UndoOperationFile,
) -> Result<(), GuardedUndoReasonCode> {
    if !pair_is_unapplied(authority, file) {
        return Err(GuardedUndoReasonCode::TargetResultMismatch);
    }
    authority.exchange(
        &file.path_bytes,
        file.exchange_artifact_key,
        &result_evidence(file),
        &staged_evidence(file).ok_or(GuardedUndoReasonCode::InvalidPersistedRecord)?,
    )?;
    if pair_is_applied(authority, file) {
        Ok(())
    } else {
        Err(GuardedUndoReasonCode::DisplacedTargetMismatch)
    }
}

fn pair_is_unapplied(authority: &dyn RestoreAuthority, file: &UndoOperationFile) -> bool {
    let target = authority.inspect_target(&file.path_bytes);
    let exchange = authority.inspect_exchange(&file.path_bytes, file.exchange_artifact_key);
    target.as_ref().is_ok_and(|value| {
        value.size == file.expected_result_size
            && value.sha256 == file.expected_result_sha256
            && value.metadata == file.expected_metadata
    }) && exchange.as_ref().is_ok_and(|value| {
        value.size == file.pre_size
            && value.sha256 == file.pre_sha256
            && file.staged_metadata.as_ref() == Some(&value.metadata)
    })
}

fn pair_is_applied(authority: &dyn RestoreAuthority, file: &UndoOperationFile) -> bool {
    let target = authority.inspect_target(&file.path_bytes);
    let exchange = authority.inspect_exchange(&file.path_bytes, file.exchange_artifact_key);
    target.as_ref().is_ok_and(|value| {
        value.size == file.pre_size
            && value.sha256 == file.pre_sha256
            && file.staged_metadata.as_ref() == Some(&value.metadata)
    }) && exchange.as_ref().is_ok_and(|value| {
        value.size == file.expected_result_size
            && value.sha256 == file.expected_result_sha256
            && value.metadata == file.expected_metadata
    })
}

fn matches_result(evidence: &FileEvidence, file: &TurnRestoreFile) -> bool {
    evidence.size == file.result_size
        && evidence.sha256 == file.result_sha256
        && evidence.metadata == file.metadata_fingerprint
}

fn result_evidence(file: &UndoOperationFile) -> FileEvidence {
    FileEvidence {
        size: file.expected_result_size,
        sha256: file.expected_result_sha256,
        metadata: file.expected_metadata.clone(),
    }
}

fn staged_evidence(file: &UndoOperationFile) -> Option<FileEvidence> {
    Some(FileEvidence {
        size: file.pre_size,
        sha256: file.pre_sha256,
        metadata: file.staged_metadata.clone()?,
    })
}

fn prepared_matches_set(prepared: &PreparedToken, restore: &TurnRestoreSet) -> bool {
    restore.state == RestoreSetState::Eligible
        && restore.snapshot_id == prepared.snapshot_id
        && restore.workspace_id == prepared.workspace_id
        && restore.restore_set_id == prepared.restore_set_id
        && restore.root_id.as_ref() == Some(&prepared.root_id)
        && restore.git_identity.as_ref() == Some(&prepared.git)
        && restore.manifest_digest == Some(prepared.manifest_digest)
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    let mut token = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(token, "{byte:02x}");
    }
    token
}

fn recovery_details(reason_code: GuardedUndoReasonCode, label: &str) -> RecoveryDetailsV1 {
    RecoveryDetailsV1 {
        schema_version: RECOVERY_DETAILS_SCHEMA_VERSION,
        reason_code,
        diagnostic_label: label.to_owned(),
    }
}

fn reason_for_set(set: &TurnRestoreSet) -> GuardedUndoReasonCode {
    match set.state {
        RestoreSetState::Expired => GuardedUndoReasonCode::RetentionExpired,
        RestoreSetState::Consumed => GuardedUndoReasonCode::PreviewConsumed,
        _ => set
            .reason_code
            .clone()
            .unwrap_or(GuardedUndoReasonCode::CaptureV2Missing),
    }
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use dcc_core::domain::{
        guarded_undo::{
            canonical_restore_manifest_digest, CheckoutRefV1, IndexIdentityV1, RestoreFileStatus,
            GIT_IDENTITY_SCHEMA_VERSION, RESTORE_CAPTURE_VERSION,
        },
        session::{SessionId, TurnId},
    };
    use rusqlite::Connection;

    #[derive(Default)]
    struct FakeFs {
        generation: u64,
        targets: HashMap<Vec<u8>, FileEvidence>,
        siblings: HashMap<(Vec<u8>, [u8; 16]), FileEvidence>,
        artifacts: HashMap<[u8; 16], FileEvidence>,
        exchange_calls: usize,
        fail_exchange_call: Option<usize>,
        cleanup_calls: usize,
        fail_cleanup_call: Option<usize>,
    }

    struct FakeAdapter {
        root: PhysicalRootId,
        git: GitIdentityV1,
        fs: Arc<Mutex<FakeFs>>,
    }

    struct FakeAuthority {
        mode: AuthorityMode,
        root: PhysicalRootId,
        git: GitIdentityV1,
        generation: u64,
        fs: Arc<Mutex<FakeFs>>,
    }

    impl RestoreAuthorityAdapter for FakeAdapter {
        fn acquire(
            &self,
            _workspace_absolute: &Path,
            expected_root: &PhysicalRootId,
            mode: AuthorityMode,
        ) -> Result<Box<dyn RestoreAuthority>, GuardedUndoReasonCode> {
            if expected_root != &self.root {
                return Err(GuardedUndoReasonCode::RepositoryIdentityChanged);
            }
            let generation = {
                let mut fs = self.fs.lock().unwrap();
                if mode == AuthorityMode::Exclusive {
                    fs.generation = fs
                        .generation
                        .checked_add(1)
                        .ok_or(GuardedUndoReasonCode::MutationInProgress)?;
                }
                fs.generation
            };
            Ok(Box::new(FakeAuthority {
                mode,
                root: self.root.clone(),
                git: self.git.clone(),
                generation,
                fs: Arc::clone(&self.fs),
            }))
        }
    }

    impl RestoreAuthority for FakeAuthority {
        fn mode(&self) -> AuthorityMode {
            self.mode
        }

        fn root_id(&self) -> PhysicalRootId {
            self.root.clone()
        }

        fn git_identity(&self) -> Result<GitIdentityV1, GuardedUndoReasonCode> {
            Ok(self.git.clone())
        }

        fn coordinator_generations(&self) -> AuthorityGenerations {
            AuthorityGenerations {
                worktree: self.generation,
                git_dir: self.generation,
                common_dir: self.generation,
            }
        }

        fn inspect_target(
            &self,
            path: &OpaqueRepoPath,
        ) -> Result<FileEvidence, GuardedUndoReasonCode> {
            self.fs
                .lock()
                .unwrap()
                .targets
                .get(path.as_persisted_bytes())
                .cloned()
                .ok_or(GuardedUndoReasonCode::TargetMissing)
        }

        fn verify_preimage(
            &self,
            key: ArtifactKey,
            size: u64,
            sha256: Sha256Digest,
        ) -> Result<(), GuardedUndoReasonCode> {
            self.fs
                .lock()
                .unwrap()
                .artifacts
                .get(&key.0)
                .is_some_and(|value| value.size == size && value.sha256 == sha256)
                .then_some(())
                .ok_or(GuardedUndoReasonCode::ArtifactCorrupt)
        }

        fn inverse_preview(
            &self,
            file: &TurnRestoreFile,
        ) -> Result<InversePreview, GuardedUndoReasonCode> {
            Ok(InversePreview {
                display_path: format!("file-{}", file.ordinal),
                size: file.pre_size,
                binary: false,
                preview: Some("verified inverse preview".to_owned()),
            })
        }

        fn stage_preimage(
            &self,
            file: &TurnRestoreFile,
            exchange_key: ArtifactKey,
        ) -> Result<FileEvidence, GuardedUndoReasonCode> {
            let mut fs = self.fs.lock().unwrap();
            let artifact = fs
                .artifacts
                .get(&file.pre_artifact_key.0)
                .cloned()
                .ok_or(GuardedUndoReasonCode::ArtifactMissing)?;
            let staged = FileEvidence {
                metadata: metadata(100 + u64::from(file.ordinal)),
                ..artifact
            };
            fs.siblings
                .insert((path_key(&file.path_bytes), exchange_key.0), staged.clone());
            Ok(staged)
        }

        fn inspect_exchange(
            &self,
            path: &OpaqueRepoPath,
            exchange_key: ArtifactKey,
        ) -> Result<FileEvidence, GuardedUndoReasonCode> {
            self.fs
                .lock()
                .unwrap()
                .siblings
                .get(&(path_key(path), exchange_key.0))
                .cloned()
                .ok_or(GuardedUndoReasonCode::DisplacedFileMissing)
        }

        fn exchange(
            &self,
            path: &OpaqueRepoPath,
            exchange_key: ArtifactKey,
            expected_target: &FileEvidence,
            expected_exchange: &FileEvidence,
        ) -> Result<(), GuardedUndoReasonCode> {
            let mut fs = self.fs.lock().unwrap();
            fs.exchange_calls += 1;
            if fs.fail_exchange_call == Some(fs.exchange_calls) {
                fs.fail_exchange_call = None;
                return Err(GuardedUndoReasonCode::IoError);
            }
            let path_key = path_key(path);
            let sibling_key = (path_key.clone(), exchange_key.0);
            if fs.targets.get(&path_key) != Some(expected_target)
                || fs.siblings.get(&sibling_key) != Some(expected_exchange)
            {
                return Err(GuardedUndoReasonCode::DisplacedTargetMismatch);
            }
            let target = fs.targets.remove(&path_key).unwrap();
            let sibling = fs.siblings.remove(&sibling_key).unwrap();
            fs.targets.insert(path_key, sibling);
            fs.siblings.insert(sibling_key, target);
            Ok(())
        }

        fn cleanup_exchange(
            &self,
            path: &OpaqueRepoPath,
            exchange_key: ArtifactKey,
            expected_exchange: &FileEvidence,
        ) -> Result<(), GuardedUndoReasonCode> {
            let mut fs = self.fs.lock().unwrap();
            fs.cleanup_calls += 1;
            if fs.fail_cleanup_call == Some(fs.cleanup_calls) {
                fs.fail_cleanup_call = None;
                return Err(GuardedUndoReasonCode::IoError);
            }
            let key = (path_key(path), exchange_key.0);
            if fs.siblings.get(&key) != Some(expected_exchange) {
                return Err(GuardedUndoReasonCode::DisplacedTargetMismatch);
            }
            fs.siblings.remove(&key);
            Ok(())
        }
    }

    struct Fixture {
        repo: SqliteSessionRepo,
        service: RestoreService,
        adapter: Arc<FakeAdapter>,
        restore: TurnRestoreSet,
        files: Vec<TurnRestoreFile>,
        now: DateTime<Utc>,
    }

    impl Fixture {
        fn new(file_count: u32) -> Self {
            let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
            let repo = SqliteSessionRepo::from_connection(Arc::clone(&connection)).unwrap();
            connection
                .lock()
                .unwrap()
                .execute(
                    r#"INSERT INTO dcc_workspaces
                        (id, project_id, root_path, base_branch, state, created_at, updated_at)
                       VALUES ('workspace', 'project', '/workspace', 'main', 'ready', 't0', 't0')"#,
                    [],
                )
                .unwrap();
            connection
                .lock()
                .unwrap()
                .execute(
                    r#"INSERT INTO dcc_sessions
                        (id, project_id, workspace_id, provider_id, state, created_at, updated_at)
                       VALUES ('session', 'project', 'workspace', 'provider', 'idle', 't0', 't0')"#,
                    [],
                )
                .unwrap();
            connection
                .lock()
                .unwrap()
                .execute(
                    r#"INSERT INTO dcc_turn_change_sets
                        (snapshot_id, session_id, turn_id, workspace_id, capture_version,
                         state, created_at, completed_at)
                       VALUES ('snapshot', 'session', 'turn', 'workspace', 1,
                               'available', 't0', 't1')"#,
                    [],
                )
                .unwrap();

            let now = Utc::now();
            let root = root_id(1);
            let git = git_identity(&root);
            let collecting = TurnRestoreSet {
                restore_set_id: RestoreSetId("restore".to_owned()),
                snapshot_id: "snapshot".to_owned(),
                session_id: SessionId("session".to_owned()),
                turn_id: TurnId("turn".to_owned()),
                workspace_id: WorkspaceId("workspace".to_owned()),
                root_id: Some(root.clone()),
                capture_version: RESTORE_CAPTURE_VERSION,
                state: RestoreSetState::Collecting,
                reason_code: None,
                git_identity: Some(git.clone()),
                artifact_bytes: 0,
                file_count: 0,
                manifest_digest: None,
                created_at: timestamp(now),
                completed_at: None,
                expires_at: None,
            };
            repo.create_turn_restore_set(&collecting).unwrap();
            let files = (0..file_count).map(restore_file).collect::<Vec<_>>();
            let artifact_bytes = files.iter().map(|file| file.pre_size).sum();
            let restore = TurnRestoreSet {
                state: RestoreSetState::Eligible,
                artifact_bytes,
                file_count,
                manifest_digest: Some(canonical_restore_manifest_digest(&files).unwrap()),
                completed_at: Some(timestamp(now)),
                expires_at: Some(timestamp(now + Duration::days(1))),
                ..collecting
            };
            repo.finalize_turn_restore_set(&restore, &files).unwrap();
            let mut fs = FakeFs::default();
            for file in &files {
                fs.targets.insert(
                    path_key(&file.path_bytes),
                    FileEvidence {
                        size: file.result_size,
                        sha256: file.result_sha256,
                        metadata: file.metadata_fingerprint.clone(),
                    },
                );
                fs.artifacts.insert(
                    file.pre_artifact_key.0,
                    FileEvidence {
                        size: file.pre_size,
                        sha256: file.pre_sha256,
                        metadata: metadata(200 + u64::from(file.ordinal)),
                    },
                );
            }
            let adapter = Arc::new(FakeAdapter {
                root,
                git,
                fs: Arc::new(Mutex::new(fs)),
            });
            let service = RestoreService::new(
                repo.clone(),
                Arc::clone(&adapter) as Arc<dyn RestoreAuthorityAdapter>,
            );
            Self {
                repo,
                service,
                adapter,
                restore,
                files,
                now,
            }
        }

        fn prepare(&self) -> PrepareReady {
            match self
                .service
                .prepare_at("snapshot", Path::new("/workspace"), self.now)
            {
                PrepareGuardedUndoResult::Ready(ready) => ready,
                result => panic!("unexpected prepare result: {result:?}"),
            }
        }

        fn planned_operation(&self, suffix: &str) -> (UndoOperation, Vec<UndoOperationFile>) {
            let operation_id = UndoOperationId(format!("operation-{suffix}"));
            let operation = UndoOperation {
                operation_id: operation_id.clone(),
                restore_set_id: self.restore.restore_set_id.clone(),
                journal_version: UNDO_JOURNAL_SCHEMA_VERSION,
                state: UndoOperationState::Preparing,
                active: true,
                preview_token_digest: Some(Sha256Digest::of(suffix)),
                prepared_identity: PreparedIdentityV1 {
                    schema_version: PREPARED_IDENTITY_SCHEMA_VERSION,
                    root_id: self.restore.root_id.clone().unwrap(),
                    git: self.restore.git_identity.clone().unwrap(),
                    manifest_digest: self.restore.manifest_digest.unwrap(),
                    coordinator_generation: 0,
                    git_dir_generation: 0,
                    common_dir_generation: 0,
                },
                reason_code: None,
                recovery_details: None,
                created_at: timestamp(self.now),
                updated_at: timestamp(self.now),
                completed_at: None,
            };
            let files = self
                .files
                .iter()
                .map(|file| UndoOperationFile {
                    operation_id: operation_id.clone(),
                    restore_set_id: self.restore.restore_set_id.clone(),
                    ordinal: file.ordinal,
                    path_bytes: file.path_bytes.clone(),
                    exchange_artifact_key: ArtifactKey([50 + file.ordinal as u8; 16]),
                    expected_result_size: file.result_size,
                    expected_result_sha256: file.result_sha256,
                    expected_metadata: file.metadata_fingerprint.clone(),
                    pre_size: file.pre_size,
                    pre_sha256: file.pre_sha256,
                    staged_metadata: None,
                    displaced_size: None,
                    displaced_sha256: None,
                    displaced_metadata: None,
                    state: UndoOperationFileState::Planned,
                    verification_outcome: VerificationOutcome::Pending,
                    recovery_details: None,
                    updated_at: timestamp(self.now),
                })
                .collect();
            (operation, files)
        }
    }

    #[test]
    fn token_is_single_use_expiring_and_second_prepare_invalidates_first() {
        let fixture = Fixture::new(1);
        let first = fixture.prepare();
        let second = fixture.prepare();
        assert!(matches!(
            fixture
                .service
                .execute_at(&first.preview_token, true, fixture.now),
            ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::PreviewConsumed)
        ));
        assert!(matches!(
            fixture.service.execute_at(
                &second.preview_token,
                true,
                fixture.now + Duration::seconds(TOKEN_LIFETIME_SECONDS + 1),
            ),
            ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::PreviewExpired)
        ));
    }

    #[test]
    fn generation_change_blocks_execute_before_journal_or_mutation() {
        let fixture = Fixture::new(1);
        let ready = fixture.prepare();
        fixture.adapter.fs.lock().unwrap().generation += 1;
        assert!(matches!(
            fixture
                .service
                .execute_at(&ready.preview_token, true, fixture.now),
            ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::PreviewContextChanged)
        ));
        assert!(fixture
            .repo
            .list_active_undo_operations()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn execute_restores_multiple_files_and_completes_journal() {
        let fixture = Fixture::new(2);
        let ready = fixture.prepare();
        let operation_id = match fixture
            .service
            .execute_at(&ready.preview_token, true, fixture.now)
        {
            ExecuteGuardedUndoResult::Completed { operation_id } => operation_id,
            result => panic!("unexpected execute result: {result:?}"),
        };
        let (operation, journal) = fixture
            .repo
            .get_undo_operation(&UndoOperationId(operation_id))
            .unwrap()
            .unwrap();
        assert_eq!(operation.state, UndoOperationState::Completed);
        assert!(journal
            .iter()
            .all(|file| file.state == UndoOperationFileState::Verified));
        let fs = fixture.adapter.fs.lock().unwrap();
        for file in &fixture.files {
            let target = fs.targets.get(&path_key(&file.path_bytes)).unwrap();
            assert_eq!(
                (target.size, target.sha256),
                (file.pre_size, file.pre_sha256)
            );
        }
    }

    #[test]
    fn exchange_failure_rolls_back_exact_displaced_files() {
        let fixture = Fixture::new(2);
        fixture.adapter.fs.lock().unwrap().fail_exchange_call = Some(2);
        let ready = fixture.prepare();
        let operation_id = match fixture
            .service
            .execute_at(&ready.preview_token, true, fixture.now)
        {
            ExecuteGuardedUndoResult::RolledBack { operation_id } => operation_id,
            result => panic!("unexpected execute result: {result:?}"),
        };
        let (operation, journal) = fixture
            .repo
            .get_undo_operation(&UndoOperationId(operation_id))
            .unwrap()
            .unwrap();
        assert_eq!(operation.state, UndoOperationState::RolledBack);
        assert!(journal
            .iter()
            .all(|file| file.state == UndoOperationFileState::RolledBack));
        let fs = fixture.adapter.fs.lock().unwrap();
        for file in &fixture.files {
            let target = fs.targets.get(&path_key(&file.path_bytes)).unwrap();
            assert_eq!(
                (target.size, target.sha256),
                (file.result_size, file.result_sha256)
            );
        }
    }

    #[test]
    fn startup_recovers_preparing_journal_before_first_stage() {
        let fixture = Fixture::new(2);
        let (operation, files) = fixture.planned_operation("planned");
        fixture
            .repo
            .create_undo_operation(&operation, &files)
            .unwrap();
        let report = fixture.service.recover_startup(|workspace| {
            (workspace == &WorkspaceId("workspace".to_owned())).then(|| PathBuf::from("/workspace"))
        });
        assert_eq!(report.rolled_back, 1);
        let (operation, journal) = fixture
            .repo
            .get_undo_operation(&operation.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(operation.state, UndoOperationState::RolledBack);
        assert!(journal
            .iter()
            .all(|file| file.state == UndoOperationFileState::RolledBack));
    }

    #[test]
    fn startup_finishes_partial_terminal_cleanup_after_completed_is_durable() {
        let fixture = Fixture::new(2);
        fixture.adapter.fs.lock().unwrap().fail_cleanup_call = Some(2);
        let ready = fixture.prepare();
        assert!(matches!(
            fixture
                .service
                .execute_at(&ready.preview_token, true, fixture.now),
            ExecuteGuardedUndoResult::Completed { .. }
        ));
        assert_eq!(
            fixture
                .repo
                .list_undo_operations_pending_cleanup()
                .unwrap()
                .len(),
            1
        );
        let report = fixture
            .service
            .recover_startup(|_| Some(PathBuf::from("/workspace")));
        assert_eq!(report, RecoveryReport::default());
        assert!(fixture
            .repo
            .list_undo_operations_pending_cleanup()
            .unwrap()
            .is_empty());
        assert!(fixture.adapter.fs.lock().unwrap().siblings.is_empty());
    }

    #[test]
    fn startup_preserves_divergent_pair_as_recovery_required() {
        let fixture = Fixture::new(1);
        let (operation, mut files) = fixture.planned_operation("divergent");
        fixture
            .repo
            .create_undo_operation(&operation, &files)
            .unwrap();
        let authority = fixture
            .adapter
            .acquire(
                Path::new("/workspace"),
                fixture.restore.root_id.as_ref().unwrap(),
                AuthorityMode::Shared,
            )
            .unwrap();
        let staged = authority
            .stage_preimage(&fixture.files[0], files[0].exchange_artifact_key)
            .unwrap();
        files[0].staged_metadata = Some(staged.metadata);
        files[0].state = UndoOperationFileState::Staged;
        fixture
            .repo
            .transition_undo_operation_file(&UndoOperationFileState::Planned, &files[0])
            .unwrap();
        fixture
            .repo
            .transition_undo_operation(
                &operation.operation_id,
                &UndoOperationState::Preparing,
                &UndoOperationState::Prepared,
                None,
                None,
                &timestamp(fixture.now),
            )
            .unwrap();
        fixture
            .repo
            .transition_undo_operation(
                &operation.operation_id,
                &UndoOperationState::Prepared,
                &UndoOperationState::Applying,
                None,
                None,
                &timestamp(fixture.now),
            )
            .unwrap();
        fixture.adapter.fs.lock().unwrap().targets.insert(
            path_key(&fixture.files[0].path_bytes),
            FileEvidence {
                size: 99,
                sha256: Sha256Digest::of(b"external"),
                metadata: metadata(999),
            },
        );
        let report = fixture
            .service
            .recover_startup(|_| Some(PathBuf::from("/workspace")));
        assert_eq!(report.recovery_required, 1);
        let (operation, _) = fixture
            .repo
            .get_undo_operation(&operation.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(operation.state, UndoOperationState::RecoveryRequired);
    }

    fn restore_file(ordinal: u32) -> TurnRestoreFile {
        let pre = format!("pre-{ordinal}");
        let result = format!("result-{ordinal}");
        TurnRestoreFile {
            restore_set_id: RestoreSetId("restore".to_owned()),
            ordinal,
            path_bytes: OpaqueRepoPath::unix(format!("file-{ordinal}.txt").as_bytes()).unwrap(),
            status: RestoreFileStatus::Modified,
            pre_size: pre.len() as u64,
            pre_sha256: Sha256Digest::of(pre.as_bytes()),
            pre_artifact_key: ArtifactKey([ordinal as u8 + 1; 16]),
            result_size: result.len() as u64,
            result_sha256: Sha256Digest::of(result.as_bytes()),
            metadata_fingerprint: metadata(u64::from(ordinal) + 1),
        }
    }

    fn metadata(identity: u64) -> RegularFileMetadataV1 {
        RegularFileMetadataV1 {
            schema_version: 1,
            adapter: "fixture".to_owned(),
            file_identity: identity.to_le_bytes().to_vec(),
            link_count: 1,
            fields: BTreeMap::from([("mode".to_owned(), 0o100644_u32.to_le_bytes().to_vec())]),
        }
    }

    fn root_id(identity: u8) -> PhysicalRootId {
        PhysicalRootId(vec![1, 1, identity])
    }

    fn git_identity(root: &PhysicalRootId) -> GitIdentityV1 {
        GitIdentityV1 {
            schema_version: GIT_IDENTITY_SCHEMA_VERSION,
            worktree_identity: root.0.clone(),
            git_dir_identity: root_id(2).0,
            common_dir_identity: root_id(3).0,
            head_oid: vec![0x42; 20],
            checkout_ref: CheckoutRefV1::Symbolic {
                full_name: "refs/heads/main".to_owned(),
            },
            index: IndexIdentityV1 {
                sha256: Sha256Digest::of(b"index"),
                size: 5,
                stat_identity: b"index-stat".to_vec(),
            },
        }
    }

    fn path_key(path: &OpaqueRepoPath) -> Vec<u8> {
        path.as_persisted_bytes().to_vec()
    }
}
