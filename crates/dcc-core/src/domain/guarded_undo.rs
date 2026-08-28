//! Versioned, fail-closed persistence contract for Guarded Undo.
//!
//! Phase 0 contains no workspace mutation, raw preimage capture, artifact file
//! I/O, prepare endpoint, or execute endpoint.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{session::SessionId, session::TurnId, workspace::WorkspaceId};

pub const RESTORE_CAPTURE_VERSION: u32 = 2;
pub const RESTORE_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const GIT_IDENTITY_SCHEMA_VERSION: u32 = 2;
pub const FILE_METADATA_SCHEMA_VERSION: u32 = 1;
pub const PREPARED_IDENTITY_SCHEMA_VERSION: u32 = 2;
pub const RECOVERY_DETAILS_SCHEMA_VERSION: u32 = 1;
pub const UNDO_JOURNAL_SCHEMA_VERSION: u32 = 1;
pub const MAX_RESTORE_FILES: u32 = 256;
pub const MAX_PREIMAGE_BYTES_PER_FILE: u64 = 8 * 1024 * 1024;
pub const MAX_PREIMAGE_BYTES_PER_SET: u64 = 32 * 1024 * 1024;
pub const MAX_BASELINE_FILES: u32 = 20_000;
pub const MAX_BASELINE_PREIMAGE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;
pub const CAPTURE_TIMEOUT_SECONDS: u64 = 10;
pub const DEFAULT_RETENTION_DAYS: u32 = 7;
pub const MAX_ELIGIBLE_SETS_PER_WORKSPACE: u32 = 20;
pub const GLOBAL_ARTIFACT_BUDGET_BYTES: u64 = 500 * 1024 * 1024;
pub const MAX_OPAQUE_PATH_BYTES: usize = 4096;
pub const MAX_IDENTITY_BYTES: usize = 1024;
pub const MAX_METADATA_FIELDS: usize = 8;
pub const MAX_METADATA_VALUE_BYTES: usize = 64;

const PATH_ENCODING_VERSION: u8 = 1;
const PATH_ENCODING_UNIX_BYTES: u8 = 1;
const PATH_ENCODING_WINDOWS_WTF16_LE: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RestoreSetId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UndoOperationId(pub String);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha256Digest(pub [u8; 32]);

impl Sha256Digest {
    pub fn of(bytes: impl AsRef<[u8]>) -> Self {
        Self(Sha256::digest(bytes.as_ref()).into())
    }

    pub fn to_hex(self) -> String {
        hex_lower(&self.0)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, GuardedUndoSchemaError> {
        bytes.try_into().map(Self).map_err(|_| {
            GuardedUndoSchemaError::InvalidField("SHA-256 must be exactly 32 bytes".to_owned())
        })
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Sha256Digest([redacted])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactKey(pub [u8; 16]);

impl ArtifactKey {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, GuardedUndoSchemaError> {
        bytes.try_into().map(Self).map_err(|_| {
            GuardedUndoSchemaError::InvalidField(
                "artifact key must be exactly 16 opaque bytes".to_owned(),
            )
        })
    }
}

impl fmt::Debug for ArtifactKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ArtifactKey([redacted])")
    }
}

/// Self-identifying SQLite BLOB: version, encoding, then Unix bytes or Windows
/// WTF-16LE. It is never a UI path or mutation input supplied by the frontend.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaqueRepoPath(pub Vec<u8>);

impl fmt::Debug for OpaqueRepoPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OpaqueRepoPath([redacted])")
    }
}

impl OpaqueRepoPath {
    pub fn unix(path: &[u8]) -> Result<Self, GuardedUndoSchemaError> {
        let mut encoded = vec![PATH_ENCODING_VERSION, PATH_ENCODING_UNIX_BYTES];
        encoded.extend_from_slice(path);
        let value = Self(encoded);
        value.validate()?;
        Ok(value)
    }

    pub fn windows_wtf16(code_units: &[u16]) -> Result<Self, GuardedUndoSchemaError> {
        let mut encoded = vec![PATH_ENCODING_VERSION, PATH_ENCODING_WINDOWS_WTF16_LE];
        for unit in code_units {
            encoded.extend_from_slice(&unit.to_le_bytes());
        }
        let value = Self(encoded);
        value.validate()?;
        Ok(value)
    }

    pub fn from_persisted(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_persisted_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        if self.0.len() < 3
            || self.0.len() > MAX_OPAQUE_PATH_BYTES
            || self.0[0] != PATH_ENCODING_VERSION
        {
            return Err(GuardedUndoSchemaError::UnsupportedPathEncoding);
        }
        match self.0[1] {
            PATH_ENCODING_UNIX_BYTES => validate_unix_relative_path(&self.0[2..]),
            PATH_ENCODING_WINDOWS_WTF16_LE => validate_windows_relative_path(&self.0[2..]),
            _ => Err(GuardedUndoSchemaError::UnsupportedPathEncoding),
        }
    }
}

/// Adapter-owned OS physical identity, not a path or a hash of a path.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PhysicalRootId(pub Vec<u8>);

impl fmt::Debug for PhysicalRootId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PhysicalRootId([redacted])")
    }
}

impl PhysicalRootId {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        if self.0.len() < 3
            || self.0.len() > MAX_IDENTITY_BYTES
            || self.0[0] != 1
            || !matches!(self.0[1], 1 | 2)
        {
            return Err(GuardedUndoSchemaError::InvalidField(
                "unsupported physical root identity".to_owned(),
            ));
        }
        Ok(())
    }
}

macro_rules! extensible_string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, PartialEq, Eq, Hash)]
        pub enum $name { $($variant,)+ Unknown(String) }

        impl $name {
            pub fn as_str(&self) -> &str {
                match self { $(Self::$variant => $value,)+ Self::Unknown(value) => value }
            }
            pub const fn is_known(&self) -> bool { !matches!(self, Self::Unknown(_)) }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }


        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Unknown(_) => f.write_str(concat!(stringify!($name), "::Unknown([redacted])")),
                    known => f.debug_tuple(stringify!($name)).field(&known.as_str()).finish(),
                }
            }
        }

        impl FromStr for $name {
            type Err = GuardedUndoSchemaError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    other if valid_stable_label(other) => Ok(Self::Unknown(other.to_owned())),
                    _ => Err(GuardedUndoSchemaError::InvalidStableValue),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                Self::from_str(&raw).map_err(de::Error::custom)
            }
        }
    };
}

extensible_string_enum!(RestoreSetState {
    Collecting => "collecting", Eligible => "eligible", Ineligible => "ineligible",
    Failed => "failed", Expired => "expired", Consumed => "consumed",
});

impl RestoreSetState {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Collecting,
                    Self::Eligible | Self::Ineligible | Self::Failed
                ) | (
                    Self::Eligible,
                    Self::Expired | Self::Consumed | Self::Failed
                )
            )
    }
}

extensible_string_enum!(UndoOperationState {
    Preparing => "preparing", Prepared => "prepared", Applying => "applying",
    Verifying => "verifying", Completed => "completed", RollingBack => "rolling_back",
    RolledBack => "rolled_back", Blocked => "blocked", RecoveryRequired => "recovery_required",
});

impl UndoOperationState {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Preparing,
                    Self::Prepared | Self::RollingBack | Self::Blocked | Self::RecoveryRequired
                ) | (
                    Self::Prepared,
                    Self::Applying | Self::RollingBack | Self::Blocked | Self::RecoveryRequired
                ) | (
                    Self::Applying,
                    Self::Verifying | Self::RollingBack | Self::RecoveryRequired
                ) | (
                    Self::Verifying,
                    Self::Completed | Self::RollingBack | Self::RecoveryRequired
                ) | (Self::RollingBack, Self::RolledBack | Self::RecoveryRequired)
            )
    }
}

extensible_string_enum!(UndoOperationFileState {
    Planned => "planned", Staged => "staged", Applied => "applied", Verified => "verified",
    RolledBack => "rolled_back", RecoveryRequired => "recovery_required",
});

impl UndoOperationFileState {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Planned,
                    Self::Staged | Self::RolledBack | Self::RecoveryRequired
                ) | (
                    Self::Staged,
                    Self::Applied | Self::RolledBack | Self::RecoveryRequired
                ) | (
                    Self::Applied,
                    Self::Verified | Self::RolledBack | Self::RecoveryRequired
                ) | (Self::Verified, Self::RolledBack | Self::RecoveryRequired)
            )
    }
}
extensible_string_enum!(VerificationOutcome {
    Pending => "pending", Verified => "verified", Failed => "failed",
});
extensible_string_enum!(RestoreFileStatus { Modified => "M" });

extensible_string_enum!(GuardedUndoReasonCode {
    CaptureV1EvidenceOnly => "capture_v1_evidence_only",
    CaptureV2Missing => "capture_v2_missing",
    CaptureInterrupted => "capture_interrupted",
    UnknownCaptureVersion => "unknown_capture_version",
    SchemaUnsupported => "schema_unsupported",
    InvalidPersistedRecord => "invalid_persisted_record",
    NoTargetChanges => "no_target_changes",
    RetentionExpired => "retention_expired",
    MultipleRoots => "multiple_roots", DetachedHead => "detached_head",
    BareRepository => "bare_repository", HeadChanged => "head_changed",
    RefChanged => "ref_changed", IndexChanged => "index_changed",
    IndexUnreadable => "index_unreadable", RepositoryIdentityChanged => "repository_identity_changed",
    UnsupportedStatus => "unsupported_status", UnmergedPath => "unmerged_path",
    UntrackedPath => "untracked_path", SymlinkOrReparsePoint => "symlink_or_reparse_point",
    HardlinkUnsupported => "hardlink_unsupported", Submodule => "submodule",
    NonRegularFile => "non_regular_file", MetadataChanged => "metadata_changed",
    GitFilterPresent => "git_filter_present", WorkingTreeEncodingPresent => "working_tree_encoding_present",
    SparseOrSkipWorktree => "sparse_or_skip_worktree", TooManyBaselineFiles => "too_many_baseline_files",
    BaselineTooLarge => "baseline_too_large", TooManyFiles => "too_many_files",
    FileTooLarge => "file_too_large", SetTooLarge => "set_too_large",
    IndexTooLarge => "index_too_large", CaptureTimeout => "capture_timeout",
    CaptureRace => "capture_race", ArtifactMissing => "artifact_missing",
    ArtifactCorrupt => "artifact_corrupt", PermissionDenied => "permission_denied",
    IoError => "io_error", WorkspaceMissing => "workspace_missing",
    TargetMissing => "target_missing", TargetResultMismatch => "target_result_mismatch",
    PreviewExpired => "preview_expired", PreviewConsumed => "preview_consumed",
    PreviewContextChanged => "preview_context_changed", MutationInProgress => "mutation_in_progress",
    AdapterUnsupported => "adapter_unsupported", OperationInterrupted => "operation_interrupted",
    ConcurrentWorkspaceMutation => "concurrent_workspace_mutation",
    AppInstanceConflict => "app_instance_conflict",
    ArtifactStoreUnsafe => "artifact_store_unsafe",
    FilesystemUnsupported => "filesystem_unsupported",
    InsufficientDiskSpace => "insufficient_disk_space",
    PathAliasCollision => "path_alias_collision",
    ExtendedMetadataUnsupported => "extended_metadata_unsupported",
    GitAttributesChanged => "git_attributes_changed",
    TrackedManifestChanged => "tracked_manifest_changed",
    AssumeUnchanged => "assume_unchanged",
    DisplacedTargetMismatch => "displaced_target_mismatch", DisplacedFileMissing => "displaced_file_missing",
    DisplacedFileCorrupt => "displaced_file_corrupt", RecoveryTargetChanged => "recovery_target_changed",
    ExchangeRollbackFailed => "exchange_rollback_failed", ManualRecoveryRequired => "manual_recovery_required",
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureVersionDecision {
    EvidenceOnly(GuardedUndoReasonCode),
    RestorationContractV2,
}

pub fn classify_capture_version(
    capture_version: u32,
) -> Result<CaptureVersionDecision, GuardedUndoSchemaError> {
    match capture_version {
        1 => Ok(CaptureVersionDecision::EvidenceOnly(
            GuardedUndoReasonCode::CaptureV1EvidenceOnly,
        )),
        RESTORE_CAPTURE_VERSION => Ok(CaptureVersionDecision::RestorationContractV2),
        version => Err(GuardedUndoSchemaError::UnsupportedCaptureVersion(version)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutRefV1 {
    Symbolic { full_name: String },
    Detached,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexIdentityV1 {
    pub sha256: Sha256Digest,
    pub size: u64,
    pub stat_identity: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitIdentityV1 {
    pub schema_version: u32,
    pub worktree_identity: Vec<u8>,
    pub git_dir_identity: Vec<u8>,
    pub common_dir_identity: Vec<u8>,
    pub head_oid: Vec<u8>,
    pub checkout_ref: CheckoutRefV1,
    pub index: IndexIdentityV1,
}

impl fmt::Debug for GitIdentityV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitIdentityV1")
            .field("schema_version", &self.schema_version)
            .field("checkout_ref", &"[redacted]")
            .field("identities", &"[redacted]")
            .finish()
    }
}

impl GitIdentityV1 {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        require_schema(
            "git_identity",
            self.schema_version,
            GIT_IDENTITY_SCHEMA_VERSION,
        )?;
        require_bounded_bytes(
            "worktree_identity",
            &self.worktree_identity,
            MAX_IDENTITY_BYTES,
        )?;
        require_bounded_bytes(
            "git_dir_identity",
            &self.git_dir_identity,
            MAX_IDENTITY_BYTES,
        )?;
        require_bounded_bytes(
            "common_dir_identity",
            &self.common_dir_identity,
            MAX_IDENTITY_BYTES,
        )?;
        if !matches!(self.head_oid.len(), 20 | 32) {
            return Err(GuardedUndoSchemaError::InvalidField(
                "head_oid must be a raw SHA-1 or SHA-256 object id".to_owned(),
            ));
        }
        require_bounded_bytes(
            "index.stat_identity",
            &self.index.stat_identity,
            MAX_IDENTITY_BYTES,
        )?;
        if self.index.size > MAX_INDEX_BYTES {
            return Err(GuardedUndoSchemaError::AccountingLimit(
                GuardedUndoReasonCode::IndexTooLarge,
            ));
        }
        if let CheckoutRefV1::Symbolic { full_name } = &self.checkout_ref {
            require_bounded_text("checkout_ref.full_name", full_name, 1_024)?;
            if full_name.contains('\0') {
                return Err(GuardedUndoSchemaError::InvalidField(
                    "checkout ref contains NUL".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegularFileMetadataV1 {
    pub schema_version: u32,
    pub adapter: String,
    pub file_identity: Vec<u8>,
    pub link_count: u64,
    pub fields: BTreeMap<String, Vec<u8>>,
}

impl fmt::Debug for RegularFileMetadataV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegularFileMetadataV1")
            .field("schema_version", &self.schema_version)
            .field("adapter", &self.adapter)
            .field("identity", &"[redacted]")
            .finish()
    }
}

impl RegularFileMetadataV1 {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        require_schema(
            "regular_file_metadata",
            self.schema_version,
            FILE_METADATA_SCHEMA_VERSION,
        )?;
        if !valid_stable_label(&self.adapter) {
            return Err(GuardedUndoSchemaError::InvalidField(
                "metadata.adapter must be a bounded stable label".to_owned(),
            ));
        }
        require_bounded_bytes(
            "metadata.file_identity",
            &self.file_identity,
            MAX_IDENTITY_BYTES,
        )?;
        if self.link_count != 1 {
            return Err(GuardedUndoSchemaError::InvalidField(
                "metadata.link_count must equal one".to_owned(),
            ));
        }
        if self.fields.len() > MAX_METADATA_FIELDS {
            return Err(GuardedUndoSchemaError::InvalidField(
                "too many metadata fields".to_owned(),
            ));
        }
        const ALLOWED: &[&str] = &[
            "mode",
            "readonly",
            "uid",
            "gid",
            "volume_serial",
            "file_id",
            "attributes",
        ];
        for (key, value) in &self.fields {
            if !ALLOWED.contains(&key.as_str()) || value.len() > MAX_METADATA_VALUE_BYTES {
                return Err(GuardedUndoSchemaError::InvalidField(
                    "metadata field is unsupported or oversized".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn write_canonical(&self, output: &mut Vec<u8>) -> Result<(), GuardedUndoSchemaError> {
        self.validate()?;
        push_u32(output, self.schema_version);
        push_bytes(output, self.adapter.as_bytes())?;
        push_bytes(output, &self.file_identity)?;
        push_u64(output, self.link_count);
        push_u32(
            output,
            u32::try_from(self.fields.len()).map_err(|_| {
                GuardedUndoSchemaError::InvalidField("too many metadata fields".to_owned())
            })?,
        );
        for (key, value) in &self.fields {
            push_bytes(output, key.as_bytes())?;
            push_bytes(output, value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedIdentityV1 {
    pub schema_version: u32,
    pub root_id: PhysicalRootId,
    pub git: GitIdentityV1,
    pub manifest_digest: Sha256Digest,
    pub coordinator_generation: u64,
    pub git_dir_generation: u64,
    pub common_dir_generation: u64,
}

impl PreparedIdentityV1 {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        require_schema(
            "prepared_identity",
            self.schema_version,
            PREPARED_IDENTITY_SCHEMA_VERSION,
        )?;
        self.root_id.validate()?;
        self.git.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryDetailsV1 {
    pub schema_version: u32,
    pub reason_code: GuardedUndoReasonCode,
    /// A bounded enum-like diagnostic label, not arbitrary text.
    pub diagnostic_label: String,
}

impl RecoveryDetailsV1 {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        require_schema(
            "recovery_details",
            self.schema_version,
            RECOVERY_DETAILS_SCHEMA_VERSION,
        )?;
        require_known_reason(&self.reason_code)?;
        if self.diagnostic_label.is_empty()
            || self.diagnostic_label.len() > 64
            || !self
                .diagnostic_label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(GuardedUndoSchemaError::InvalidField(
                "recovery diagnostic label is not content-free".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnRestoreSet {
    pub restore_set_id: RestoreSetId,
    pub snapshot_id: String,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub workspace_id: WorkspaceId,
    pub root_id: Option<PhysicalRootId>,
    pub capture_version: u32,
    pub state: RestoreSetState,
    pub reason_code: Option<GuardedUndoReasonCode>,
    pub git_identity: Option<GitIdentityV1>,
    pub artifact_bytes: u64,
    pub file_count: u32,
    pub manifest_digest: Option<Sha256Digest>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub expires_at: Option<String>,
}

impl TurnRestoreSet {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        if self.capture_version != RESTORE_CAPTURE_VERSION {
            return Err(GuardedUndoSchemaError::UnsupportedCaptureVersion(
                self.capture_version,
            ));
        }
        require_text("restore_set_id", &self.restore_set_id.0)?;
        require_text("snapshot_id", &self.snapshot_id)?;
        require_text("session_id", &self.session_id.0)?;
        require_text("turn_id", &self.turn_id.0)?;
        require_text("workspace_id", &self.workspace_id.0)?;
        if let Some(root_id) = &self.root_id {
            root_id.validate()?;
        }
        if let Some(git_identity) = &self.git_identity {
            git_identity.validate()?;
        }
        require_known_state(&self.state)?;
        if let Some(reason) = &self.reason_code {
            require_known_reason(reason)?;
        }
        if self.file_count > MAX_RESTORE_FILES {
            return Err(GuardedUndoSchemaError::AccountingLimit(
                GuardedUndoReasonCode::TooManyFiles,
            ));
        }
        if self.artifact_bytes > MAX_PREIMAGE_BYTES_PER_SET {
            return Err(GuardedUndoSchemaError::AccountingLimit(
                GuardedUndoReasonCode::SetTooLarge,
            ));
        }
        match self.state {
            RestoreSetState::Collecting
                if self.completed_at.is_none() && self.reason_code.is_none() => {}
            RestoreSetState::Eligible
                if self.reason_code.is_none()
                    && self.manifest_digest.is_some()
                    && self.completed_at.is_some()
                    && self.expires_at.is_some()
                    && self.file_count > 0
                    && self.root_id.is_some()
                    && self.git_identity.as_ref().is_some_and(|git| {
                        matches!(
                            &git.checkout_ref,
                            CheckoutRefV1::Symbolic { full_name }
                                if full_name.starts_with("refs/heads/")
                        )
                    }) => {}
            RestoreSetState::Ineligible | RestoreSetState::Failed | RestoreSetState::Expired
                if self.reason_code.is_some() && self.completed_at.is_some() => {}
            RestoreSetState::Consumed
                if self.reason_code.is_none()
                    && self.completed_at.is_some()
                    && self.manifest_digest.is_some()
                    && self.file_count > 0
                    && self.root_id.is_some()
                    && self.git_identity.is_some() => {}
            RestoreSetState::Unknown(_) => unreachable!("unknown rejected above"),
            _ => {
                return Err(GuardedUndoSchemaError::InvalidField(
                    "restore set state fields are inconsistent".to_owned(),
                ))
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnRestoreFile {
    pub restore_set_id: RestoreSetId,
    pub ordinal: u32,
    pub path_bytes: OpaqueRepoPath,
    pub status: RestoreFileStatus,
    pub pre_size: u64,
    pub pre_sha256: Sha256Digest,
    pub pre_artifact_key: ArtifactKey,
    pub result_size: u64,
    pub result_sha256: Sha256Digest,
    pub metadata_fingerprint: RegularFileMetadataV1,
}

impl TurnRestoreFile {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        require_text("restore_set_id", &self.restore_set_id.0)?;
        self.path_bytes.validate()?;
        if self.status != RestoreFileStatus::Modified {
            return Err(GuardedUndoSchemaError::InvalidField(
                "capture v2 only supports status M".to_owned(),
            ));
        }
        if self.pre_size > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(GuardedUndoSchemaError::AccountingLimit(
                GuardedUndoReasonCode::FileTooLarge,
            ));
        }
        if self.result_size > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(GuardedUndoSchemaError::AccountingLimit(
                GuardedUndoReasonCode::FileTooLarge,
            ));
        }
        self.metadata_fingerprint.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoOperation {
    pub operation_id: UndoOperationId,
    pub restore_set_id: RestoreSetId,
    pub journal_version: u32,
    pub state: UndoOperationState,
    pub active: bool,
    pub preview_token_digest: Option<Sha256Digest>,
    pub prepared_identity: PreparedIdentityV1,
    pub reason_code: Option<GuardedUndoReasonCode>,
    pub recovery_details: Option<RecoveryDetailsV1>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

impl UndoOperation {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        require_schema(
            "undo_journal",
            self.journal_version,
            UNDO_JOURNAL_SCHEMA_VERSION,
        )?;
        require_text("operation_id", &self.operation_id.0)?;
        require_text("restore_set_id", &self.restore_set_id.0)?;
        if !self.state.is_known() {
            return Err(GuardedUndoSchemaError::UnknownPersistedValue);
        }
        if let Some(reason) = &self.reason_code {
            require_known_reason(reason)?;
        }
        self.prepared_identity.validate()?;
        if let Some(details) = &self.recovery_details {
            details.validate()?;
        }
        if self
            .recovery_details
            .as_ref()
            .is_some_and(|details| Some(&details.reason_code) != self.reason_code.as_ref())
        {
            return Err(GuardedUndoSchemaError::InvalidField(
                "recovery details reason does not match operation reason".to_owned(),
            ));
        }
        match self.state {
            UndoOperationState::Preparing
            | UndoOperationState::Prepared
            | UndoOperationState::Applying
            | UndoOperationState::Verifying
            | UndoOperationState::RollingBack
                if self.active
                    && self.completed_at.is_none()
                    && self.reason_code.is_none()
                    && self.recovery_details.is_none() => {}
            UndoOperationState::Completed | UndoOperationState::RolledBack
                if !self.active
                    && self.completed_at.is_some()
                    && self.reason_code.is_none()
                    && self.recovery_details.is_none() => {}
            UndoOperationState::Blocked
                if !self.active
                    && self.completed_at.is_some()
                    && self.reason_code.is_some()
                    && self.recovery_details.is_none() => {}
            UndoOperationState::RecoveryRequired
                if self.active
                    && self.completed_at.is_none()
                    && self.reason_code.is_some()
                    && self.recovery_details.is_some() => {}
            UndoOperationState::Unknown(_) => unreachable!("unknown rejected above"),
            _ => {
                return Err(GuardedUndoSchemaError::InvalidField(
                    "undo operation state fields are inconsistent".to_owned(),
                ))
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoOperationFile {
    pub operation_id: UndoOperationId,
    pub restore_set_id: RestoreSetId,
    pub ordinal: u32,
    pub path_bytes: OpaqueRepoPath,
    pub exchange_artifact_key: ArtifactKey,
    pub expected_result_size: u64,
    pub expected_result_sha256: Sha256Digest,
    pub expected_metadata: RegularFileMetadataV1,
    pub pre_size: u64,
    pub pre_sha256: Sha256Digest,
    /// Physical identity and supported metadata of the same-directory staged
    /// preimage before the first exchange. After a successful exchange this
    /// exact identity must be observed at the target path. A content digest
    /// alone cannot prove that the target was installed by this operation.
    pub staged_metadata: Option<RegularFileMetadataV1>,
    pub displaced_size: Option<u64>,
    pub displaced_sha256: Option<Sha256Digest>,
    pub displaced_metadata: Option<RegularFileMetadataV1>,
    pub state: UndoOperationFileState,
    pub verification_outcome: VerificationOutcome,
    pub recovery_details: Option<RecoveryDetailsV1>,
    pub updated_at: String,
}

impl UndoOperationFile {
    pub fn validate(&self) -> Result<(), GuardedUndoSchemaError> {
        require_text("operation_id", &self.operation_id.0)?;
        require_text("restore_set_id", &self.restore_set_id.0)?;
        self.path_bytes.validate()?;
        if self.pre_size > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(GuardedUndoSchemaError::AccountingLimit(
                GuardedUndoReasonCode::FileTooLarge,
            ));
        }
        if self.expected_result_size > MAX_PREIMAGE_BYTES_PER_FILE
            || self
                .displaced_size
                .is_some_and(|size| size > MAX_PREIMAGE_BYTES_PER_FILE)
        {
            return Err(GuardedUndoSchemaError::AccountingLimit(
                GuardedUndoReasonCode::FileTooLarge,
            ));
        }
        if !self.state.is_known() || !self.verification_outcome.is_known() {
            return Err(GuardedUndoSchemaError::UnknownPersistedValue);
        }
        self.expected_metadata.validate()?;
        if let Some(metadata) = &self.staged_metadata {
            metadata.validate()?;
        }
        match (
            &self.displaced_size,
            &self.displaced_sha256,
            &self.displaced_metadata,
        ) {
            (None, None, None) | (Some(_), Some(_), Some(_)) => {}
            _ => {
                return Err(GuardedUndoSchemaError::InvalidField(
                    "displaced identity must be entirely absent or present".to_owned(),
                ))
            }
        }
        if let Some(metadata) = &self.displaced_metadata {
            metadata.validate()?;
        }
        if let Some(details) = &self.recovery_details {
            details.validate()?;
        }
        let displaced_present = self.displaced_size.is_some();
        match self.state {
            UndoOperationFileState::Planned
                if self.staged_metadata.is_none()
                    && !displaced_present
                    && self.verification_outcome == VerificationOutcome::Pending
                    && self.recovery_details.is_none() => {}
            UndoOperationFileState::Staged
                if self.staged_metadata.is_some()
                    && !displaced_present
                    && self.verification_outcome == VerificationOutcome::Pending
                    && self.recovery_details.is_none() => {}
            UndoOperationFileState::Applied
                if self.staged_metadata.is_some()
                    && displaced_present
                    && self.verification_outcome == VerificationOutcome::Pending => {}
            UndoOperationFileState::Verified
                if self.staged_metadata.is_some()
                    && displaced_present
                    && self.verification_outcome == VerificationOutcome::Verified
                    && self.recovery_details.is_none() => {}
            UndoOperationFileState::RolledBack
                if self.verification_outcome == VerificationOutcome::Verified
                    && self.recovery_details.is_none()
                    && (self.staged_metadata.is_some() || !displaced_present) => {}
            UndoOperationFileState::RecoveryRequired
                if self.verification_outcome == VerificationOutcome::Failed
                    && self.recovery_details.is_some() => {}
            UndoOperationFileState::Unknown(_) => unreachable!("unknown rejected above"),
            _ => {
                return Err(GuardedUndoSchemaError::InvalidField(
                    "undo operation file state fields are inconsistent".to_owned(),
                ))
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreAccounting {
    pub file_count: u32,
    pub artifact_bytes: u64,
}

pub fn account_restore_files(
    files: &[TurnRestoreFile],
) -> Result<RestoreAccounting, GuardedUndoSchemaError> {
    let file_count = u32::try_from(files.len()).map_err(|_| {
        GuardedUndoSchemaError::AccountingLimit(GuardedUndoReasonCode::TooManyFiles)
    })?;
    if file_count > MAX_RESTORE_FILES {
        return Err(GuardedUndoSchemaError::AccountingLimit(
            GuardedUndoReasonCode::TooManyFiles,
        ));
    }
    let mut artifact_bytes = 0_u64;
    for file in files {
        file.validate()?;
        artifact_bytes = artifact_bytes.checked_add(file.pre_size).ok_or(
            GuardedUndoSchemaError::AccountingLimit(GuardedUndoReasonCode::SetTooLarge),
        )?;
    }
    if artifact_bytes > MAX_PREIMAGE_BYTES_PER_SET {
        return Err(GuardedUndoSchemaError::AccountingLimit(
            GuardedUndoReasonCode::SetTooLarge,
        ));
    }
    Ok(RestoreAccounting {
        file_count,
        artifact_bytes,
    })
}

/// Canonical binary SHA-256; independent of caller order and JSON encoders.
pub fn canonical_restore_manifest_digest(
    files: &[TurnRestoreFile],
) -> Result<Sha256Digest, GuardedUndoSchemaError> {
    account_restore_files(files)?;
    let mut ordered = files.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.path_bytes.0.cmp(&b.path_bytes.0));
    let mut artifact_keys = BTreeSet::new();
    for (expected, file) in ordered.iter().enumerate() {
        if usize::try_from(file.ordinal).ok() != Some(expected) {
            return Err(GuardedUndoSchemaError::InvalidField(
                "ordinals must match canonical path order".to_owned(),
            ));
        }
        if expected > 0 && ordered[expected - 1].path_bytes == file.path_bytes {
            return Err(GuardedUndoSchemaError::InvalidField(
                "duplicate restore path".to_owned(),
            ));
        }
        if !artifact_keys.insert(file.pre_artifact_key.0) {
            return Err(GuardedUndoSchemaError::InvalidField(
                "duplicate preimage artifact key".to_owned(),
            ));
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"DCC_GUARDED_UNDO_MANIFEST\0");
    push_u32(&mut out, RESTORE_MANIFEST_SCHEMA_VERSION);
    push_u32(&mut out, RESTORE_CAPTURE_VERSION);
    push_u32(
        &mut out,
        u32::try_from(ordered.len()).map_err(|_| {
            GuardedUndoSchemaError::AccountingLimit(GuardedUndoReasonCode::TooManyFiles)
        })?,
    );
    for file in ordered {
        push_u32(&mut out, file.ordinal);
        push_bytes(&mut out, &file.path_bytes.0)?;
        push_bytes(&mut out, file.status.as_str().as_bytes())?;
        push_u64(&mut out, file.pre_size);
        out.extend_from_slice(&file.pre_sha256.0);
        out.extend_from_slice(&file.pre_artifact_key.0);
        push_u64(&mut out, file.result_size);
        out.extend_from_slice(&file.result_sha256.0);
        file.metadata_fingerprint.write_canonical(&mut out)?;
    }
    Ok(Sha256Digest::of(out))
}

pub fn validate_restore_set_manifest(
    set: &TurnRestoreSet,
    files: &[TurnRestoreFile],
) -> Result<(), GuardedUndoSchemaError> {
    set.validate()?;
    if files
        .iter()
        .any(|file| file.restore_set_id != set.restore_set_id)
    {
        return Err(GuardedUndoSchemaError::InvalidField(
            "file belongs to another set".to_owned(),
        ));
    }
    let accounting = account_restore_files(files)?;
    if accounting.file_count != set.file_count || accounting.artifact_bytes != set.artifact_bytes {
        return Err(GuardedUndoSchemaError::InvalidField(
            "accounting mismatch".to_owned(),
        ));
    }
    match set.manifest_digest {
        Some(expected) if canonical_restore_manifest_digest(files)? == expected => Ok(()),
        Some(_) => Err(GuardedUndoSchemaError::ManifestDigestMismatch),
        None if files.is_empty() && set.state == RestoreSetState::Collecting => Ok(()),
        None => Err(GuardedUndoSchemaError::InvalidField(
            "files require manifest digest".to_owned(),
        )),
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GuardedUndoSchemaError {
    #[error("unsupported guarded undo capture version {0}")]
    UnsupportedCaptureVersion(u32),
    #[error("unsupported {kind} schema {actual}; expected {expected}")]
    UnsupportedSchemaVersion {
        kind: &'static str,
        actual: u32,
        expected: u32,
    },
    #[error("unknown persisted enum value")]
    UnknownPersistedValue,
    #[error("invalid stable enum/reason value")]
    InvalidStableValue,
    #[error("unsupported opaque repository path encoding")]
    UnsupportedPathEncoding,
    #[error("invalid guarded undo field: {0}")]
    InvalidField(String),
    #[error("guarded undo accounting limit: {0}")]
    AccountingLimit(GuardedUndoReasonCode),
    #[error("manifest digest mismatch")]
    ManifestDigestMismatch,
}

fn require_schema(
    kind: &'static str,
    actual: u32,
    expected: u32,
) -> Result<(), GuardedUndoSchemaError> {
    if actual == expected {
        Ok(())
    } else {
        Err(GuardedUndoSchemaError::UnsupportedSchemaVersion {
            kind,
            actual,
            expected,
        })
    }
}
fn require_text(field: &str, value: &str) -> Result<(), GuardedUndoSchemaError> {
    if value.trim().is_empty() {
        Err(GuardedUndoSchemaError::InvalidField(format!(
            "{field} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn require_bounded_text(
    field: &str,
    value: &str,
    maximum: usize,
) -> Result<(), GuardedUndoSchemaError> {
    require_text(field, value)?;
    if value.len() > maximum {
        Err(GuardedUndoSchemaError::InvalidField(format!(
            "{field} is oversized"
        )))
    } else {
        Ok(())
    }
}
fn require_bounded_bytes(
    field: &str,
    value: &[u8],
    maximum: usize,
) -> Result<(), GuardedUndoSchemaError> {
    if value.is_empty() || value.len() > maximum {
        Err(GuardedUndoSchemaError::InvalidField(format!(
            "{field} is empty or oversized"
        )))
    } else {
        Ok(())
    }
}
fn require_known_reason(reason: &GuardedUndoReasonCode) -> Result<(), GuardedUndoSchemaError> {
    if reason.is_known() {
        Ok(())
    } else {
        Err(GuardedUndoSchemaError::UnknownPersistedValue)
    }
}
fn require_known_state(state: &RestoreSetState) -> Result<(), GuardedUndoSchemaError> {
    if state.is_known() {
        Ok(())
    } else {
        Err(GuardedUndoSchemaError::UnknownPersistedValue)
    }
}

fn valid_stable_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_unix_relative_path(path: &[u8]) -> Result<(), GuardedUndoSchemaError> {
    if path.is_empty() || path.contains(&0) || path.starts_with(b"/") {
        return Err(GuardedUndoSchemaError::InvalidField(
            "Unix path is not relative".to_owned(),
        ));
    }
    if path
        .split(|b| *b == b'/')
        .any(|c| c.is_empty() || c == b"." || c == b"..")
    {
        return Err(GuardedUndoSchemaError::InvalidField(
            "unsafe Unix path component".to_owned(),
        ));
    }
    Ok(())
}

fn validate_windows_relative_path(bytes: &[u8]) -> Result<(), GuardedUndoSchemaError> {
    if bytes.is_empty() || bytes.len() % 2 != 0 {
        return Err(GuardedUndoSchemaError::InvalidField(
            "invalid WTF-16LE path".to_owned(),
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect::<Vec<_>>();
    if units.contains(&0)
        || units.contains(&0x3a)
        || matches!(units.first(), Some(0x2f | 0x5c))
        || (units.len() >= 3
            && (units[0] as u8).is_ascii_alphabetic()
            && units[1] == 0x3a
            && matches!(units[2], 0x2f | 0x5c))
    {
        return Err(GuardedUndoSchemaError::InvalidField(
            "Windows path is not relative".to_owned(),
        ));
    }
    if units
        .split(|u| matches!(*u, 0x2f | 0x5c))
        .any(|c| c.is_empty() || c == [0x2e] || c == [0x2e, 0x2e])
    {
        return Err(GuardedUndoSchemaError::InvalidField(
            "unsafe Windows path component".to_owned(),
        ));
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
    out
}
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn push_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), GuardedUndoSchemaError> {
    push_u64(
        out,
        u64::try_from(value.len()).map_err(|_| {
            GuardedUndoSchemaError::InvalidField("canonical field too large".to_owned())
        })?,
    );
    out.extend_from_slice(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> RegularFileMetadataV1 {
        RegularFileMetadataV1 {
            schema_version: 1,
            adapter: "fixture".into(),
            file_identity: b"dev1ino2".to_vec(),
            link_count: 1,
            fields: BTreeMap::from([
                ("mode".into(), b"100644".to_vec()),
                ("readonly".into(), b"false".to_vec()),
            ]),
        }
    }
    fn file(ordinal: u32, path: &[u8], size: u64) -> TurnRestoreFile {
        TurnRestoreFile {
            restore_set_id: RestoreSetId("restore-1".into()),
            ordinal,
            path_bytes: OpaqueRepoPath::unix(path).unwrap(),
            status: RestoreFileStatus::Modified,
            pre_size: size,
            pre_sha256: Sha256Digest([0x11; 32]),
            pre_artifact_key: ArtifactKey([ordinal as u8; 16]),
            result_size: size + 1,
            result_sha256: Sha256Digest([0x22; 32]),
            metadata_fingerprint: metadata(),
        }
    }

    #[test]
    fn capture_v1_is_evidence_only_and_unknown_version_fails_closed() {
        assert_eq!(
            classify_capture_version(1),
            Ok(CaptureVersionDecision::EvidenceOnly(
                GuardedUndoReasonCode::CaptureV1EvidenceOnly
            ))
        );
        assert_eq!(
            classify_capture_version(2),
            Ok(CaptureVersionDecision::RestorationContractV2)
        );
        assert!(matches!(
            classify_capture_version(99),
            Err(GuardedUndoSchemaError::UnsupportedCaptureVersion(99))
        ));
    }

    #[test]
    fn reason_codes_roundtrip_and_unknown_is_preserved_but_unsafe() {
        let reason = GuardedUndoReasonCode::CaptureV1EvidenceOnly;
        assert_eq!(reason.to_string().parse(), Ok(reason));
        let future: GuardedUndoReasonCode = "future_reason".parse().unwrap();
        assert_eq!(future.as_str(), "future_reason");
        assert!(!future.is_known());
    }

    #[test]
    fn phase_1a_reason_codes_are_stable_and_known() {
        for raw in [
            "concurrent_workspace_mutation",
            "app_instance_conflict",
            "artifact_store_unsafe",
            "filesystem_unsupported",
            "insufficient_disk_space",
            "path_alias_collision",
            "extended_metadata_unsupported",
            "git_attributes_changed",
            "tracked_manifest_changed",
            "assume_unchanged",
        ] {
            let reason: GuardedUndoReasonCode = raw.parse().unwrap();
            assert_eq!(reason.as_str(), raw);
            assert!(reason.is_known());
        }
    }

    #[test]
    fn canonical_digest_is_order_independent_and_handles_non_utf8() {
        let first = file(0, b"src/a-\xff.rs", 8);
        let second = file(1, b"src/lib.rs", 12);
        assert_eq!(
            canonical_restore_manifest_digest(&[first.clone(), second.clone()]).unwrap(),
            canonical_restore_manifest_digest(&[second, first]).unwrap()
        );
    }

    #[test]
    fn canonical_digest_fixture_is_frozen() {
        let digest = canonical_restore_manifest_digest(&[
            file(0, b"src/a.rs", 8),
            file(1, b"src/lib.rs", 12),
        ])
        .unwrap();
        assert_eq!(
            digest.to_hex(),
            "1a82267c0dde11cf95bc1a4c396379c197e90ce12e0b17715c43558e26199cf7"
        );
    }

    #[test]
    fn corrupt_schema_nul_and_parent_paths_fail_closed() {
        assert!(OpaqueRepoPath::unix(b"../secret").is_err());
        assert!(OpaqueRepoPath::unix(b"src/a\0b").is_err());
        assert!(OpaqueRepoPath::windows_wtf16(
            &"foo\\..\\secret".encode_utf16().collect::<Vec<_>>()
        )
        .is_err());
        let mut bad = metadata();
        bad.schema_version = 99;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn lifecycle_transitions_are_narrow_and_idempotent() {
        assert!(RestoreSetState::Collecting.can_transition_to(&RestoreSetState::Eligible));
        assert!(RestoreSetState::Eligible.can_transition_to(&RestoreSetState::Eligible));
        assert!(!RestoreSetState::Consumed.can_transition_to(&RestoreSetState::Eligible));
        assert!(UndoOperationState::Applying.can_transition_to(&UndoOperationState::RollingBack));
        assert!(UndoOperationState::Completed.can_transition_to(&UndoOperationState::Completed));
        assert!(!UndoOperationState::Completed.can_transition_to(&UndoOperationState::Applying));
    }
}
