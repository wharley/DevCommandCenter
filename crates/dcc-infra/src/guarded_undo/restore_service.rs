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
        chained_result_is_bound, ArtifactKey, GitIdentityV1, GuardedUndoReasonCode, OpaqueRepoPath,
        PhysicalRootId, PreparedIdentityV1, RecoveryDetailsV1, RegularFileMetadataV1, RestoreSetId,
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
    /// Inverse preview of `file` from the bytes `newer`'s Undo will install
    /// at the same path (its verified preimage, equal to `file`'s result).
    /// Read-only; used to plan a chain before any file is restored.
    fn chained_inverse_preview(
        &self,
        file: &TurnRestoreFile,
        newer: &TurnRestoreFile,
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

/// Which current file a restore set's target may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultBinding {
    /// Exactly the file the turn left (v1).
    TurnResult,
    /// The file the turn left or, when a later turn's completed Undo already
    /// replaced it with the same bytes, exactly the file that Undo installed.
    /// Used by a rewind that restores several turns newest first.
    AfterCompletedUndo,
}

/// One restore set of a chain, previewed without writing anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedRestore {
    pub snapshot_id: String,
    pub file_count: u32,
    pub total_bytes: u64,
    pub files: Vec<InversePreview>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanGuardedUndoChainResult {
    /// Every set can be restored in the given order (newest first).
    Ready(Vec<PlannedRestore>),
    Blocked {
        snapshot_id: String,
        reason_code: GuardedUndoReasonCode,
    },
    Unavailable {
        snapshot_id: String,
        reason_code: GuardedUndoReasonCode,
    },
}

/// The current file a target must be for prepare and execute.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExpectedResult {
    evidence: FileEvidence,
    /// The completed Undo that installed `evidence`, for a chained target.
    chained_from: Option<UndoOperationId>,
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
    binding: ResultBinding,
    /// Per restore file, in manifest order.
    expected: Vec<ExpectedResult>,
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

struct PreparableSet {
    restore: TurnRestoreSet,
    files: Vec<TurnRestoreFile>,
    root_id: PhysicalRootId,
    git: GitIdentityV1,
    manifest_digest: Sha256Digest,
}

enum SetRefusal {
    Blocked(GuardedUndoReasonCode),
    Unavailable(GuardedUndoReasonCode),
}

impl SetRefusal {
    fn into_prepare(self, snapshot_id: &str) -> PrepareGuardedUndoResult {
        let snapshot_id = snapshot_id.to_owned();
        match self {
            Self::Blocked(reason_code) => PrepareGuardedUndoResult::Blocked {
                snapshot_id,
                reason_code,
            },
            Self::Unavailable(reason_code) => PrepareGuardedUndoResult::Unavailable {
                snapshot_id,
                reason_code,
            },
        }
    }

    fn into_plan(self, snapshot_id: &str) -> PlanGuardedUndoChainResult {
        let snapshot_id = snapshot_id.to_owned();
        match self {
            Self::Blocked(reason_code) => PlanGuardedUndoChainResult::Blocked {
                snapshot_id,
                reason_code,
            },
            Self::Unavailable(reason_code) => PlanGuardedUndoChainResult::Unavailable {
                snapshot_id,
                reason_code,
            },
        }
    }
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
        self.prepare_at(
            snapshot_id,
            workspace_absolute,
            ResultBinding::TurnResult,
            Utc::now(),
        )
    }

    /// Prepare that also accepts a target a later turn's completed Undo
    /// installed with exactly this turn's result bytes.
    pub fn prepare_after_completed_undo(
        &self,
        snapshot_id: &str,
        workspace_absolute: &Path,
    ) -> PrepareGuardedUndoResult {
        self.prepare_at(
            snapshot_id,
            workspace_absolute,
            ResultBinding::AfterCompletedUndo,
            Utc::now(),
        )
    }

    fn prepare_at(
        &self,
        snapshot_id: &str,
        workspace_absolute: &Path,
        binding: ResultBinding,
        now: DateTime<Utc>,
    ) -> PrepareGuardedUndoResult {
        let blocked = |reason_code| PrepareGuardedUndoResult::Blocked {
            snapshot_id: snapshot_id.to_owned(),
            reason_code,
        };
        let set = match self.load_preparable(snapshot_id, now) {
            Ok(set) => set,
            Err(refusal) => return refusal.into_prepare(snapshot_id),
        };
        let authority =
            match self
                .adapter
                .acquire(workspace_absolute, &set.root_id, AuthorityMode::Shared)
            {
                Ok(authority) => authority,
                Err(reason) => return blocked(reason),
            };
        if authority.mode() != AuthorityMode::Shared || authority.root_id() != set.root_id {
            return blocked(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        if !same_repository_state(authority.as_ref(), &set.git) {
            return blocked(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        let mut previews = Vec::with_capacity(set.files.len());
        let mut expected = Vec::with_capacity(set.files.len());
        for file in &set.files {
            if let Err(reason) =
                authority.verify_preimage(file.pre_artifact_key, file.pre_size, file.pre_sha256)
            {
                return blocked(reason);
            }
            let current = match authority.inspect_target(&file.path_bytes) {
                Ok(current) => current,
                Err(reason) => return blocked(reason),
            };
            match self.expected_result(binding, &set.restore, file, &current) {
                Ok(result) => expected.push(result),
                Err(reason) => return blocked(reason),
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
            workspace_id: set.restore.workspace_id.clone(),
            workspace_absolute: workspace_absolute.to_path_buf(),
            restore_set_id: set.restore.restore_set_id.clone(),
            root_id: set.root_id,
            git: set.git,
            manifest_digest: set.manifest_digest,
            coordinator_generations: authority.coordinator_generations(),
            expires_at,
            binding,
            expected,
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
            file_count: set.restore.file_count,
            total_bytes: set.restore.artifact_bytes,
            files: previews,
            unrelated_paths_are_not_targets: true,
        })
    }

    /// Read-only check that the given sets, newest first, can each be
    /// restored after the ones before it. A path a newer set restores is
    /// checked against the bytes that set will install (its preimage must be
    /// exactly this set's result, with the same attributes); every other path
    /// is checked against the disk as `prepare_after_completed_undo` would.
    /// Issues no token: each set is prepared again, against the disk, right
    /// before it is executed.
    pub fn plan_chain(
        &self,
        snapshot_ids_newest_first: &[String],
        workspace_absolute: &Path,
    ) -> PlanGuardedUndoChainResult {
        self.plan_chain_at(snapshot_ids_newest_first, workspace_absolute, Utc::now())
    }

    fn plan_chain_at(
        &self,
        snapshot_ids_newest_first: &[String],
        workspace_absolute: &Path,
        now: DateTime<Utc>,
    ) -> PlanGuardedUndoChainResult {
        let mut sets = Vec::with_capacity(snapshot_ids_newest_first.len());
        for snapshot_id in snapshot_ids_newest_first {
            match self.load_preparable(snapshot_id, now) {
                Ok(set) => sets.push((snapshot_id, set)),
                Err(refusal) => return refusal.into_plan(snapshot_id),
            }
        }
        let Some((_, first)) = sets.first() else {
            return PlanGuardedUndoChainResult::Ready(Vec::new());
        };
        let blocked = |snapshot_id: &String, reason_code| PlanGuardedUndoChainResult::Blocked {
            snapshot_id: snapshot_id.clone(),
            reason_code,
        };
        let (first_snapshot, workspace_id, root_id) = (
            sets[0].0,
            first.restore.workspace_id.clone(),
            first.root_id.clone(),
        );
        if let Some((snapshot_id, _)) = sets
            .iter()
            .find(|(_, set)| set.restore.workspace_id != workspace_id || set.root_id != root_id)
        {
            return blocked(
                snapshot_id,
                GuardedUndoReasonCode::RepositoryIdentityChanged,
            );
        }
        let authority =
            match self
                .adapter
                .acquire(workspace_absolute, &root_id, AuthorityMode::Shared)
            {
                Ok(authority) => authority,
                Err(reason) => return blocked(first_snapshot, reason),
            };
        if authority.mode() != AuthorityMode::Shared || authority.root_id() != root_id {
            return blocked(
                first_snapshot,
                GuardedUndoReasonCode::RepositoryIdentityChanged,
            );
        }
        let Ok(git) = authority.git_identity() else {
            return blocked(
                first_snapshot,
                GuardedUndoReasonCode::RepositoryIdentityChanged,
            );
        };
        // The newest planned restore of each path: what that path will hold
        // when an older set's turn comes.
        let mut pending: HashMap<Vec<u8>, &TurnRestoreFile> = HashMap::new();
        let mut planned = Vec::with_capacity(sets.len());
        for (snapshot_id, set) in &sets {
            // Undo never changes HEAD, ref or index, so every set must match
            // the repository as it is now, exactly as a lone prepare would.
            if !set.git.same_repository_state(&git) {
                return blocked(
                    snapshot_id,
                    GuardedUndoReasonCode::RepositoryIdentityChanged,
                );
            }
            let mut previews = Vec::with_capacity(set.files.len());
            for file in &set.files {
                if let Err(reason) =
                    authority.verify_preimage(file.pre_artifact_key, file.pre_size, file.pre_sha256)
                {
                    return blocked(snapshot_id, reason);
                }
                let preview = match pending.get(file.path_bytes.as_persisted_bytes()) {
                    Some(newer) => {
                        // The staged file a newer Undo installs carries its
                        // preimage bytes and its result's attributes.
                        if newer.pre_size != file.result_size
                            || newer.pre_sha256 != file.result_sha256
                            || !newer
                                .metadata_fingerprint
                                .same_attributes(&file.metadata_fingerprint)
                        {
                            return blocked(
                                snapshot_id,
                                GuardedUndoReasonCode::TargetResultMismatch,
                            );
                        }
                        authority.chained_inverse_preview(file, newer)
                    }
                    None => {
                        let current = match authority.inspect_target(&file.path_bytes) {
                            Ok(current) => current,
                            Err(reason) => return blocked(snapshot_id, reason),
                        };
                        if let Err(reason) = self.expected_result(
                            ResultBinding::AfterCompletedUndo,
                            &set.restore,
                            file,
                            &current,
                        ) {
                            return blocked(snapshot_id, reason);
                        }
                        authority.inverse_preview(file)
                    }
                };
                match preview {
                    Ok(preview) => previews.push(preview),
                    Err(reason) => return blocked(snapshot_id, reason),
                }
            }
            for file in &set.files {
                pending.insert(file.path_bytes.as_persisted_bytes().to_vec(), file);
            }
            planned.push(PlannedRestore {
                snapshot_id: (*snapshot_id).clone(),
                file_count: set.restore.file_count,
                total_bytes: set.restore.artifact_bytes,
                files: previews,
            });
        }
        PlanGuardedUndoChainResult::Ready(planned)
    }

    /// The eligibility gates every prepare applies before touching the disk.
    fn load_preparable(
        &self,
        snapshot_id: &str,
        now: DateTime<Utc>,
    ) -> Result<PreparableSet, SetRefusal> {
        let Some((restore, files)) = self
            .repo
            .get_turn_restore_set_by_snapshot(snapshot_id)
            .ok()
            .flatten()
        else {
            return Err(SetRefusal::Unavailable(
                GuardedUndoReasonCode::CaptureV2Missing,
            ));
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
            return Err(SetRefusal::Blocked(
                GuardedUndoReasonCode::MutationInProgress,
            ));
        }
        if restore.state != RestoreSetState::Eligible {
            return Err(SetRefusal::Unavailable(reason_for_set(&restore)));
        }
        let invalid = SetRefusal::Unavailable(GuardedUndoReasonCode::InvalidPersistedRecord);
        let Some(expires_at) = restore.expires_at.as_deref().and_then(parse_timestamp) else {
            return Err(invalid);
        };
        if expires_at <= now {
            return Err(SetRefusal::Unavailable(
                GuardedUndoReasonCode::RetentionExpired,
            ));
        }
        let (Some(root_id), Some(git), Some(manifest_digest)) = (
            restore.root_id.clone(),
            restore.git_identity.clone(),
            restore.manifest_digest,
        ) else {
            return Err(invalid);
        };
        Ok(PreparableSet {
            restore,
            files,
            root_id,
            git,
            manifest_digest,
        })
    }

    /// The file `current` must be for `file` to be restored. Under
    /// `AfterCompletedUndo`, a target whose bytes are still exactly the
    /// turn's result but whose physical identity is not the one the turn
    /// left is accepted only when it is exactly the file a completed Undo of
    /// another set in the same workspace, root and Git identity installed at
    /// this path, carrying this turn's result bytes and attributes. Anything
    /// else is a mismatch: nothing is inferred from content alone.
    fn expected_result(
        &self,
        binding: ResultBinding,
        restore: &TurnRestoreSet,
        file: &TurnRestoreFile,
        current: &FileEvidence,
    ) -> Result<ExpectedResult, GuardedUndoReasonCode> {
        if matches_result(current, file) {
            return Ok(ExpectedResult {
                evidence: current.clone(),
                chained_from: None,
            });
        }
        if binding == ResultBinding::TurnResult
            || current.size != file.result_size
            || current.sha256 != file.result_sha256
        {
            return Err(GuardedUndoReasonCode::TargetResultMismatch);
        }
        let Some(root_id) = restore.root_id.as_ref() else {
            return Err(GuardedUndoReasonCode::InvalidPersistedRecord);
        };
        let sources = self
            .repo
            .list_undo_chain_sources(&restore.workspace_id, root_id, &file.path_bytes)
            .map_err(|_| GuardedUndoReasonCode::InvalidPersistedRecord)?;
        sources
            .into_iter()
            .find(|source| chained_result_is_bound(restore, file, source, &current.metadata))
            .map(|source| ExpectedResult {
                evidence: current.clone(),
                chained_from: Some(source.operation_id),
            })
            .ok_or(GuardedUndoReasonCode::TargetResultMismatch)
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
        if let Err(reason) =
            self.validate_execute_authority(authority.as_ref(), &prepared, &restore, &files)
        {
            return ExecuteGuardedUndoResult::Blocked(reason);
        }

        let operation_id = UndoOperationId(Uuid::new_v4().to_string());
        let mut journal_files = files
            .iter()
            .zip(&prepared.expected)
            .map(|(file, expected)| UndoOperationFile {
                operation_id: operation_id.clone(),
                restore_set_id: restore.restore_set_id.clone(),
                ordinal: file.ordinal,
                path_bytes: file.path_bytes.clone(),
                exchange_artifact_key: ArtifactKey(*Uuid::new_v4().as_bytes()),
                expected_result_size: expected.evidence.size,
                expected_result_sha256: expected.evidence.sha256,
                expected_metadata: expected.evidence.metadata.clone(),
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
                chained_from_operation_id: expected.chained_from.clone(),
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
            if current.as_ref().ok() != Some(&prepared.expected[index].evidence) {
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
        if !same_repository_state(authority.as_ref(), &operation.prepared_identity.git) {
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
            || !same_repository_state(authority.as_ref(), &operation.prepared_identity.git)
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
            if !same_repository_state(authority.as_ref(), &operation.prepared_identity.git) {
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

    fn validate_execute_authority(
        &self,
        authority: &dyn RestoreAuthority,
        prepared: &PreparedToken,
        restore: &TurnRestoreSet,
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
        if !authority
            .git_identity()?
            .same_repository_state(&prepared.git)
        {
            return Err(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        if files.len() != prepared.expected.len() {
            return Err(GuardedUndoReasonCode::PreviewContextChanged);
        }
        // The chain's provenance is derived again under the exclusive lease;
        // it must name the very files the person previewed.
        for (file, prepared_expected) in files.iter().zip(&prepared.expected) {
            authority.verify_preimage(file.pre_artifact_key, file.pre_size, file.pre_sha256)?;
            let current = authority.inspect_target(&file.path_bytes)?;
            if self.expected_result(prepared.binding, restore, file, &current)?
                != *prepared_expected
            {
                return Err(GuardedUndoReasonCode::PreviewContextChanged);
            }
        }
        Ok(())
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

/// The repository as `authority` observes it now is in the state `captured`
/// recorded. An index Git only rewrote with identical bytes since then does
/// not count as a change (see [`GitIdentityV1::same_repository_state`]).
fn same_repository_state(authority: &dyn RestoreAuthority, captured: &GitIdentityV1) -> bool {
    authority
        .git_identity()
        .is_ok_and(|current| current.same_repository_state(captured))
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
        /// The repository as the authority observes it now.
        git: Option<GitIdentityV1>,
        /// What the repository becomes while the first exchange runs.
        git_on_exchange: Option<GitIdentityV1>,
        targets: HashMap<Vec<u8>, FileEvidence>,
        siblings: HashMap<(Vec<u8>, [u8; 16]), FileEvidence>,
        artifacts: HashMap<[u8; 16], FileEvidence>,
        exchange_calls: usize,
        fail_exchange_call: Option<usize>,
        cleanup_calls: usize,
        fail_cleanup_call: Option<usize>,
        staged_count: u64,
    }

    struct FakeAdapter {
        root: PhysicalRootId,
        fs: Arc<Mutex<FakeFs>>,
    }

    struct FakeAuthority {
        mode: AuthorityMode,
        root: PhysicalRootId,
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
            self.fs
                .lock()
                .unwrap()
                .git
                .clone()
                .ok_or(GuardedUndoReasonCode::RepositoryIdentityChanged)
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

        fn chained_inverse_preview(
            &self,
            file: &TurnRestoreFile,
            newer: &TurnRestoreFile,
        ) -> Result<InversePreview, GuardedUndoReasonCode> {
            assert_eq!(newer.pre_sha256, file.result_sha256);
            Ok(InversePreview {
                display_path: format!("file-{}", file.ordinal),
                size: file.pre_size,
                binary: false,
                preview: Some("chained inverse preview".to_owned()),
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
            // Every staged file is a new physical file that carries the
            // attributes of the result it replaces, as the macOS adapter's
            // fchown/fchmod do.
            fs.staged_count += 1;
            let staged = FileEvidence {
                metadata: RegularFileMetadataV1 {
                    fields: file.metadata_fingerprint.fields.clone(),
                    ..metadata(1_000 + fs.staged_count)
                },
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
            if let Some(git) = fs.git_on_exchange.take() {
                fs.git = Some(git);
            }
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
        connection: Arc<Mutex<Connection>>,
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
            let mut fs = FakeFs {
                git: Some(git),
                ..FakeFs::default()
            };
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
                fs: Arc::new(Mutex::new(fs)),
            });
            let service = RestoreService::new(
                repo.clone(),
                Arc::clone(&adapter) as Arc<dyn RestoreAuthorityAdapter>,
            );
            Self {
                connection,
                repo,
                service,
                adapter,
                restore,
                files,
                now,
            }
        }

        /// A later turn's eligible set that changed `file-0.txt` from
        /// `pre` to `result`, leaving `result` on disk.
        fn add_later_turn(&self, pre: &str, result: &str, mode: u32) -> TurnRestoreFile {
            let git = self.restore.git_identity.clone().unwrap();
            self.add_later_turn_with_git(pre, result, mode, git)
        }

        /// [`Self::add_later_turn`], captured with the repository as `git`.
        fn add_later_turn_with_git(
            &self,
            pre: &str,
            result: &str,
            mode: u32,
            git: GitIdentityV1,
        ) -> TurnRestoreFile {
            self.connection
                .lock()
                .unwrap()
                .execute(
                    r#"INSERT INTO dcc_turn_change_sets
                        (snapshot_id, session_id, turn_id, workspace_id, capture_version,
                         state, created_at, completed_at)
                       VALUES ('snapshot-later', 'session', 'turn-later', 'workspace', 1,
                               'available', 't2', 't3')"#,
                    [],
                )
                .unwrap();
            let collecting = TurnRestoreSet {
                restore_set_id: RestoreSetId("restore-later".to_owned()),
                snapshot_id: "snapshot-later".to_owned(),
                turn_id: TurnId("turn-later".to_owned()),
                git_identity: Some(git),
                state: RestoreSetState::Collecting,
                artifact_bytes: 0,
                file_count: 0,
                manifest_digest: None,
                completed_at: None,
                expires_at: None,
                ..self.restore.clone()
            };
            self.repo.create_turn_restore_set(&collecting).unwrap();
            let mut result_metadata = metadata(77);
            result_metadata
                .fields
                .insert("mode".to_owned(), mode.to_le_bytes().to_vec());
            let file = TurnRestoreFile {
                restore_set_id: collecting.restore_set_id.clone(),
                pre_size: pre.len() as u64,
                pre_sha256: Sha256Digest::of(pre.as_bytes()),
                pre_artifact_key: ArtifactKey([0x77; 16]),
                result_size: result.len() as u64,
                result_sha256: Sha256Digest::of(result.as_bytes()),
                metadata_fingerprint: result_metadata,
                ..restore_file(0)
            };
            let files = vec![file.clone()];
            self.repo
                .finalize_turn_restore_set(
                    &TurnRestoreSet {
                        state: RestoreSetState::Eligible,
                        artifact_bytes: file.pre_size,
                        file_count: 1,
                        manifest_digest: Some(canonical_restore_manifest_digest(&files).unwrap()),
                        completed_at: Some(timestamp(self.now)),
                        expires_at: Some(timestamp(self.now + Duration::days(1))),
                        ..collecting
                    },
                    &files,
                )
                .unwrap();
            let mut fs = self.adapter.fs.lock().unwrap();
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
                    metadata: metadata(300),
                },
            );
            file
        }

        fn prepare_snapshot(
            &self,
            snapshot_id: &str,
            binding: ResultBinding,
        ) -> PrepareGuardedUndoResult {
            self.service
                .prepare_at(snapshot_id, Path::new("/workspace"), binding, self.now)
        }

        fn undo(&self, snapshot_id: &str, binding: ResultBinding) -> UndoOperationId {
            let PrepareGuardedUndoResult::Ready(ready) =
                self.prepare_snapshot(snapshot_id, binding)
            else {
                panic!("expected {snapshot_id} to prepare");
            };
            match self
                .service
                .execute_at(&ready.preview_token, true, self.now)
            {
                ExecuteGuardedUndoResult::Completed { operation_id } => {
                    UndoOperationId(operation_id)
                }
                result => panic!("unexpected execute result: {result:?}"),
            }
        }

        fn plan(&self, snapshot_ids: &[&str]) -> PlanGuardedUndoChainResult {
            let snapshot_ids = snapshot_ids
                .iter()
                .map(|id| (*id).to_owned())
                .collect::<Vec<_>>();
            self.service
                .plan_chain_at(&snapshot_ids, Path::new("/workspace"), self.now)
        }

        fn target(&self) -> FileEvidence {
            self.adapter.fs.lock().unwrap().targets[&path_key(&self.files[0].path_bytes)].clone()
        }

        fn prepare(&self) -> PrepareReady {
            match self.service.prepare_at(
                "snapshot",
                Path::new("/workspace"),
                ResultBinding::TurnResult,
                self.now,
            ) {
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
                    chained_from_operation_id: None,
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

    #[test]
    fn chained_restore_binds_only_the_file_a_completed_undo_installed() {
        let fixture = Fixture::new(1);
        let later = fixture.add_later_turn("result-0", "later-0", 0o100644);
        assert!(matches!(
            fixture.plan(&["snapshot-later", "snapshot"]),
            PlanGuardedUndoChainResult::Ready(planned) if planned.len() == 2
                && planned[1].files[0].preview.as_deref() == Some("chained inverse preview")
        ));
        let later_operation = fixture.undo("snapshot-later", ResultBinding::TurnResult);
        let installed = fixture.target();
        assert_eq!(
            (installed.size, installed.sha256),
            (later.pre_size, later.pre_sha256)
        );

        // v1 still binds the identity the turn left.
        assert!(matches!(
            fixture.prepare_snapshot("snapshot", ResultBinding::TurnResult),
            PrepareGuardedUndoResult::Blocked {
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
                ..
            }
        ));
        let operation_id = fixture.undo("snapshot", ResultBinding::AfterCompletedUndo);
        let target = fixture.target();
        assert_eq!(
            (target.size, target.sha256),
            (fixture.files[0].pre_size, fixture.files[0].pre_sha256)
        );
        let (_, journal) = fixture
            .repo
            .get_undo_operation(&operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(journal[0].chained_from_operation_id, Some(later_operation));
        assert_eq!(journal[0].expected_metadata, installed.metadata);
        assert_eq!(journal[0].displaced_metadata, Some(installed.metadata));
    }

    #[test]
    fn a_same_bytes_file_that_no_undo_installed_is_not_chained() {
        let fixture = Fixture::new(1);
        fixture.add_later_turn("result-0", "later-0", 0o100644);
        fixture.undo("snapshot-later", ResultBinding::TurnResult);
        // Someone replaces the installed file with the very same bytes.
        let replaced = FileEvidence {
            metadata: metadata(555),
            ..fixture.target()
        };
        fixture
            .adapter
            .fs
            .lock()
            .unwrap()
            .targets
            .insert(path_key(&fixture.files[0].path_bytes), replaced.clone());
        assert!(matches!(
            fixture.prepare_snapshot("snapshot", ResultBinding::AfterCompletedUndo),
            PrepareGuardedUndoResult::Blocked {
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
                ..
            }
        ));
        assert!(matches!(
            fixture.plan(&["snapshot"]),
            PlanGuardedUndoChainResult::Blocked {
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
                ..
            }
        ));
        assert_eq!(fixture.target(), replaced);
    }

    #[test]
    fn a_change_between_the_turns_breaks_the_chain_before_anything_is_written() {
        // The person edited the file between the turns: the later turn's
        // preimage is not what the earlier turn left.
        let fixture = Fixture::new(1);
        fixture.add_later_turn("edited-by-person", "later-0", 0o100644);
        assert_eq!(
            fixture.plan(&["snapshot-later", "snapshot"]),
            PlanGuardedUndoChainResult::Blocked {
                snapshot_id: "snapshot".to_owned(),
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
            }
        );
        fixture.undo("snapshot-later", ResultBinding::TurnResult);
        assert!(matches!(
            fixture.prepare_snapshot("snapshot", ResultBinding::AfterCompletedUndo),
            PrepareGuardedUndoResult::Blocked {
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
                ..
            }
        ));
    }

    #[test]
    fn a_permission_change_between_the_turns_breaks_the_chain() {
        let fixture = Fixture::new(1);
        fixture.add_later_turn("result-0", "later-0", 0o100755);
        assert!(matches!(
            fixture.plan(&["snapshot-later", "snapshot"]),
            PlanGuardedUndoChainResult::Blocked {
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
                ..
            }
        ));
        fixture.undo("snapshot-later", ResultBinding::TurnResult);
        assert!(matches!(
            fixture.prepare_snapshot("snapshot", ResultBinding::AfterCompletedUndo),
            PrepareGuardedUndoResult::Blocked {
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
                ..
            }
        ));
    }

    #[test]
    fn journal_refuses_a_chained_identity_without_a_completed_source() {
        let fixture = Fixture::new(1);
        let (operation, mut files) = fixture.planned_operation("ghost");
        files[0].expected_metadata = metadata(4242);
        files[0].chained_from_operation_id = Some(UndoOperationId("ghost-source".to_owned()));
        assert!(fixture
            .repo
            .create_undo_operation(&operation, &files)
            .is_err());
        files[0].chained_from_operation_id = None;
        assert!(fixture
            .repo
            .create_undo_operation(&operation, &files)
            .is_err());
    }

    #[test]
    fn startup_rolls_back_an_interrupted_chained_restore() {
        let fixture = Fixture::new(1);
        fixture.add_later_turn("result-0", "later-0", 0o100644);
        let later_operation = fixture.undo("snapshot-later", ResultBinding::TurnResult);
        let (operation, mut files) = fixture.planned_operation("chained");
        files[0].expected_metadata = fixture.target().metadata;
        files[0].chained_from_operation_id = Some(later_operation);
        fixture
            .repo
            .create_undo_operation(&operation, &files)
            .unwrap();
        let report = fixture
            .service
            .recover_startup(|_| Some(PathBuf::from("/workspace")));
        assert_eq!(report.rolled_back, 1);
        let (operation, _) = fixture
            .repo
            .get_undo_operation(&operation.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(operation.state, UndoOperationState::RolledBack);
    }

    /// The captured identity after Git rewrote the index with the same bytes:
    /// new inode and times, same staged state.
    fn rewritten_index(git: &GitIdentityV1, stat_identity: &[u8]) -> GitIdentityV1 {
        let mut rewritten = git.clone();
        rewritten.index.stat_identity = stat_identity.to_vec();
        rewritten
    }

    #[test]
    fn an_index_rewritten_with_the_same_bytes_does_not_block_undo() {
        let fixture = Fixture::new(2);
        let captured = fixture.restore.git_identity.clone().unwrap();
        // `git status` in a terminal refreshes the index after the turn...
        fixture.adapter.fs.lock().unwrap().git = Some(rewritten_index(&captured, b"refresh-1"));
        let ready = fixture.prepare();
        // ...again between the preview and the confirmation, and once more
        // while the files are being exchanged.
        {
            let mut fs = fixture.adapter.fs.lock().unwrap();
            fs.git = Some(rewritten_index(&captured, b"refresh-2"));
            fs.git_on_exchange = Some(rewritten_index(&captured, b"refresh-3"));
        }
        let operation_id = match fixture
            .service
            .execute_at(&ready.preview_token, true, fixture.now)
        {
            ExecuteGuardedUndoResult::Completed { operation_id } => operation_id,
            result => panic!("unexpected execute result: {result:?}"),
        };
        let (operation, _) = fixture
            .repo
            .get_undo_operation(&UndoOperationId(operation_id))
            .unwrap()
            .unwrap();
        assert_eq!(operation.state, UndoOperationState::Completed);
        // The journal still binds exactly the identity the set captured.
        assert_eq!(operation.prepared_identity.git, captured);
        let fs = fixture.adapter.fs.lock().unwrap();
        assert!(fs.git_on_exchange.is_none());
        for file in &fixture.files {
            let target = &fs.targets[&path_key(&file.path_bytes)];
            assert_eq!(
                (target.size, target.sha256),
                (file.pre_size, file.pre_sha256)
            );
        }
    }

    #[test]
    fn an_index_with_other_bytes_still_blocks_undo() {
        let changes: [fn(&mut GitIdentityV1); 3] = [
            |git| git.index.sha256 = Sha256Digest::of(b"staged"),
            |git| git.index.size += 1,
            |git| git.head_oid = vec![0x43; 20],
        ];
        for change in changes {
            let fixture = Fixture::new(1);
            let mut current =
                rewritten_index(fixture.restore.git_identity.as_ref().unwrap(), b"refresh");
            change(&mut current);
            fixture.adapter.fs.lock().unwrap().git = Some(current.clone());
            assert!(matches!(
                fixture.prepare_snapshot("snapshot", ResultBinding::TurnResult),
                PrepareGuardedUndoResult::Blocked {
                    reason_code: GuardedUndoReasonCode::RepositoryIdentityChanged,
                    ..
                }
            ));
            assert!(matches!(
                fixture.plan(&["snapshot"]),
                PlanGuardedUndoChainResult::Blocked {
                    reason_code: GuardedUndoReasonCode::RepositoryIdentityChanged,
                    ..
                }
            ));

            // A change between the preview and the confirmation blocks
            // execute before anything is journaled or written.
            fixture.adapter.fs.lock().unwrap().git = fixture.restore.git_identity.clone();
            let ready = fixture.prepare();
            fixture.adapter.fs.lock().unwrap().git = Some(current.clone());
            assert!(matches!(
                fixture
                    .service
                    .execute_at(&ready.preview_token, true, fixture.now),
                ExecuteGuardedUndoResult::Blocked(GuardedUndoReasonCode::RepositoryIdentityChanged)
            ));
            assert!(fixture
                .repo
                .list_active_undo_operations()
                .unwrap()
                .is_empty());
            assert_eq!(fixture.target().sha256, fixture.files[0].result_sha256);
        }
    }

    #[test]
    fn staging_while_files_are_exchanged_rolls_the_undo_back() {
        let fixture = Fixture::new(2);
        let ready = fixture.prepare();
        let mut staged = rewritten_index(fixture.restore.git_identity.as_ref().unwrap(), b"staged");
        staged.index.sha256 = Sha256Digest::of(b"staged");
        fixture.adapter.fs.lock().unwrap().git_on_exchange = Some(staged);
        assert!(matches!(
            fixture
                .service
                .execute_at(&ready.preview_token, true, fixture.now),
            ExecuteGuardedUndoResult::RolledBack { .. }
        ));
        let fs = fixture.adapter.fs.lock().unwrap();
        for file in &fixture.files {
            assert_eq!(
                fs.targets[&path_key(&file.path_bytes)].sha256,
                file.result_sha256
            );
        }
    }

    #[test]
    fn a_chain_survives_an_index_rewritten_between_the_turns() {
        let fixture = Fixture::new(1);
        let captured = fixture.restore.git_identity.clone().unwrap();
        // The index was refreshed between the two turns, so the later turn
        // captured another stat identity for the very same bytes.
        let later_git = rewritten_index(&captured, b"between-turns");
        fixture.add_later_turn_with_git("result-0", "later-0", 0o100644, later_git);
        fixture.adapter.fs.lock().unwrap().git = Some(rewritten_index(&captured, b"before-rewind"));
        assert!(matches!(
            fixture.plan(&["snapshot-later", "snapshot"]),
            PlanGuardedUndoChainResult::Ready(planned) if planned.len() == 2
        ));
        let later_operation = fixture.undo("snapshot-later", ResultBinding::TurnResult);
        let operation_id = fixture.undo("snapshot", ResultBinding::AfterCompletedUndo);
        let (operation, journal) = fixture
            .repo
            .get_undo_operation(&operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(operation.prepared_identity.git, captured);
        assert_eq!(journal[0].chained_from_operation_id, Some(later_operation));
        let target = fixture.target();
        assert_eq!(
            (target.size, target.sha256),
            (fixture.files[0].pre_size, fixture.files[0].pre_sha256)
        );
    }

    #[test]
    fn a_chain_breaks_when_the_index_bytes_changed_between_the_turns() {
        let fixture = Fixture::new(1);
        let mut later_git = fixture.restore.git_identity.clone().unwrap();
        later_git.index.sha256 = Sha256Digest::of(b"staged-between-turns");
        fixture.add_later_turn_with_git("result-0", "later-0", 0o100644, later_git.clone());
        fixture.adapter.fs.lock().unwrap().git = Some(later_git);
        assert_eq!(
            fixture.plan(&["snapshot-later", "snapshot"]),
            PlanGuardedUndoChainResult::Blocked {
                snapshot_id: "snapshot".to_owned(),
                reason_code: GuardedUndoReasonCode::RepositoryIdentityChanged,
            }
        );
        let later_operation = fixture.undo("snapshot-later", ResultBinding::TurnResult);
        // Even with the index bytes back to what the older turn captured, the
        // installed file came from an Undo bound to another index.
        fixture.adapter.fs.lock().unwrap().git = fixture.restore.git_identity.clone();
        assert!(matches!(
            fixture.prepare_snapshot("snapshot", ResultBinding::AfterCompletedUndo),
            PrepareGuardedUndoResult::Blocked {
                reason_code: GuardedUndoReasonCode::TargetResultMismatch,
                ..
            }
        ));
        let (operation, mut files) = fixture.planned_operation("cross-index");
        files[0].expected_metadata = fixture.target().metadata;
        files[0].chained_from_operation_id = Some(later_operation);
        assert!(fixture
            .repo
            .create_undo_operation(&operation, &files)
            .is_err());
    }

    #[test]
    fn startup_recovery_tolerates_an_index_rewritten_with_the_same_bytes() {
        let fixture = Fixture::new(2);
        let (operation, files) = fixture.planned_operation("planned");
        fixture
            .repo
            .create_undo_operation(&operation, &files)
            .unwrap();
        let captured = fixture.restore.git_identity.clone().unwrap();
        fixture.adapter.fs.lock().unwrap().git = Some(rewritten_index(&captured, b"after-crash"));
        let report = fixture
            .service
            .recover_startup(|_| Some(PathBuf::from("/workspace")));
        assert_eq!(report.rolled_back, 1);

        let fixture = Fixture::new(2);
        let (operation, files) = fixture.planned_operation("planned");
        fixture
            .repo
            .create_undo_operation(&operation, &files)
            .unwrap();
        let mut staged = rewritten_index(&captured, b"after-crash");
        staged.index.sha256 = Sha256Digest::of(b"staged");
        fixture.adapter.fs.lock().unwrap().git = Some(staged);
        let report = fixture
            .service
            .recover_startup(|_| Some(PathBuf::from("/workspace")));
        assert_eq!(report.recovery_required, 1);
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
