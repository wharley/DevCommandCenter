//! Process-wide, cancellation-safe ownership of guarded-undo capture handles.
//!
//! This module deliberately does not start provider turns. It owns the
//! lifecycle primitives used by the integrated provider flows to capture and
//! terminalize guarded-undo state.

// Feature-off builds retain the compatibility/no-op facade, while the active
// macOS implementation remains fully checked for dead production code.
#![cfg_attr(
    not(all(target_os = "macos", feature = "guarded-undo-capture-v2")),
    allow(dead_code)
)]

use std::{
    any::Any,
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
use dcc_core::domain::guarded_undo::PhysicalRootId;
use dcc_core::domain::{
    session::{SessionId, TurnId},
    workspace::WorkspaceId,
};
use dcc_infra::db::SqliteSessionRepo;
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
use dcc_infra::guarded_undo::{
    coordinator::{
        CoordinatorError, MultiMutationGuard, MutationGuard, WorkspaceMutationCoordinator,
    },
    macos_git_bridge::MacGitMutationAuthority,
    macos_root::MacWorkspaceRoot,
};

use crate::terminal_arbiter::TerminalKey;

const MAX_TRACKED_TURNS: usize = 1_024;
// Capture v2/M4 v1 is deliberately single-root. Multi-root turns require a
// separate attribution and lock-ordering design extension.
const MAX_CAPTURE_ROOTS_PER_TURN: usize = 1;
// Startup recovery is global to the app-data store, not a single provider
// turn. Keep its bounded admission independent from the stricter per-turn
// capture fan-out limit.
const MAX_RECOVERY_ROOTS: usize = 1_024;

/// A fail-closed result from the process-local workspace mutation gate.
///
/// The operation payload is intentionally never rendered by `Debug` or
/// `Display`; callers that need to surface their own domain error must
/// destructure `Operation` explicitly.
pub(crate) enum WorkspaceMutationRunError<E> {
    Busy,
    PhysicalRootUnavailable,
    CoordinatorUnavailable,
    WorkerUnavailable,
    Operation(E),
}

impl<E> fmt::Debug for WorkspaceMutationRunError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Busy => "WorkspaceMutationRunError::Busy",
            Self::PhysicalRootUnavailable => "WorkspaceMutationRunError::PhysicalRootUnavailable",
            Self::CoordinatorUnavailable => "WorkspaceMutationRunError::CoordinatorUnavailable",
            Self::WorkerUnavailable => "WorkspaceMutationRunError::WorkerUnavailable",
            Self::Operation(_) => "WorkspaceMutationRunError::Operation([redacted])",
        })
    }
}

impl<E> fmt::Display for WorkspaceMutationRunError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Busy => "workspace mutation is busy",
            Self::PhysicalRootUnavailable => "workspace physical root is unavailable",
            Self::CoordinatorUnavailable => "workspace mutation coordination is unavailable",
            Self::WorkerUnavailable => "workspace mutation worker is unavailable",
            Self::Operation(_) => "workspace mutation operation failed",
        })
    }
}

impl<E: std::error::Error + 'static> std::error::Error for WorkspaceMutationRunError<E> {}

/// Retains both the descriptor-rooted identity and the mutation guard for the
/// complete synchronous operation. This type never crosses the public API and
/// cannot be held across an async suspension point by command handlers.
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
struct PhysicalMutationLease {
    // Field drop order is declaration order: release coordinator admission
    // before closing the retained root descriptor.
    _guard: MutationGuard,
    _root: MacWorkspaceRoot,
    workspace_absolute: PathBuf,
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
impl fmt::Debug for PhysicalMutationLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PhysicalMutationLease([redacted])")
    }
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
impl PhysicalMutationLease {
    fn acquire(
        coordinator: &Arc<WorkspaceMutationCoordinator>,
        workspace_absolute: PathBuf,
    ) -> Result<Self, WorkspaceMutationRunError<std::convert::Infallible>> {
        let root = MacWorkspaceRoot::open_absolute(&workspace_absolute)
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        let root_id = root.physical_root_id();
        let guard = coordinator
            .try_acquire_mutation(&root_id)
            .map_err(map_coordinator_error)?;

        // Close the ordinary rename/replacement window between the first
        // physical observation and admission. The retained root remains alive
        // with the guard for the whole operation.
        let reopened = MacWorkspaceRoot::open_absolute(&workspace_absolute)
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        if reopened.physical_root_id() != root_id {
            return Err(WorkspaceMutationRunError::PhysicalRootUnavailable);
        }

        Ok(Self {
            _guard: guard,
            _root: root,
            workspace_absolute,
        })
    }

    fn path(&self) -> &Path {
        &self.workspace_absolute
    }
}

/// Retains the descriptor-proven worktree/common-dir authority and their
/// atomic coordinator guard for the complete synchronous Git operation.
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
struct PhysicalGitMutationLease {
    // Drop the guard before closing authority descriptors.
    _guard: MultiMutationGuard,
    authority: MacGitMutationAuthority,
}

/// Retains two descriptor-proven linked worktrees and their shared Git
/// common directory for the complete synchronous operation. This is used by
/// delegation apply/remove flows where both the destination and an isolated
/// child worktree must remain stable while the closure compares or transfers
/// data between them.
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
struct PhysicalGitPairMutationLease {
    // Drop the guard before closing either authority's descriptors.
    _guard: MultiMutationGuard,
    primary: MacGitMutationAuthority,
    secondary: MacGitMutationAuthority,
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
impl fmt::Debug for PhysicalGitPairMutationLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PhysicalGitPairMutationLease([redacted])")
    }
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
impl PhysicalGitPairMutationLease {
    fn acquire(
        coordinator: &Arc<WorkspaceMutationCoordinator>,
        primary_absolute: PathBuf,
        secondary_absolute: PathBuf,
    ) -> Result<Self, WorkspaceMutationRunError<std::convert::Infallible>> {
        let primary = MacGitMutationAuthority::open(&primary_absolute)
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        let secondary = MacGitMutationAuthority::open(&secondary_absolute)
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        if primary.common_dir_id() != secondary.common_dir_id()
            || primary.worktree_root_id() == secondary.worktree_root_id()
        {
            return Err(WorkspaceMutationRunError::PhysicalRootUnavailable);
        }

        // Coordinator admission is all-or-none and normalizes/deduplicates
        // the common directory identity. No user-controlled path is retained
        // by the coordinator; the authorities own the descriptor lifetime.
        let guard = coordinator
            .try_acquire_mutations(vec![
                primary.worktree_root_id(),
                secondary.worktree_root_id(),
                primary.common_dir_id(),
            ])
            .map_err(map_coordinator_error)?;
        primary
            .revalidate()
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        secondary
            .revalidate()
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        if primary.common_dir_id() != secondary.common_dir_id() {
            return Err(WorkspaceMutationRunError::PhysicalRootUnavailable);
        }

        Ok(Self {
            _guard: guard,
            primary,
            secondary,
        })
    }

    fn paths(&self) -> (&Path, &Path) {
        (
            self.primary.workspace_path(),
            self.secondary.workspace_path(),
        )
    }
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
impl fmt::Debug for PhysicalGitMutationLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PhysicalGitMutationLease([redacted])")
    }
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
impl PhysicalGitMutationLease {
    fn acquire(
        coordinator: &Arc<WorkspaceMutationCoordinator>,
        workspace_absolute: PathBuf,
    ) -> Result<Self, WorkspaceMutationRunError<std::convert::Infallible>> {
        let authority = MacGitMutationAuthority::open(&workspace_absolute)
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        let guard = coordinator
            .try_acquire_mutations(vec![
                authority.worktree_root_id(),
                authority.common_dir_id(),
            ])
            .map_err(map_coordinator_error)?;
        authority
            .revalidate()
            .map_err(|_| WorkspaceMutationRunError::PhysicalRootUnavailable)?;
        Ok(Self {
            _guard: guard,
            authority,
        })
    }

    fn path(&self) -> &Path {
        self.authority.workspace_path()
    }
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
fn map_coordinator_error(
    error: CoordinatorError,
) -> WorkspaceMutationRunError<std::convert::Infallible> {
    match error {
        CoordinatorError::MutationInProgress | CoordinatorError::CaptureEdgeActive => {
            WorkspaceMutationRunError::Busy
        }
        _ => WorkspaceMutationRunError::CoordinatorUnavailable,
    }
}

/// Ephemeral binding between one durable M3 snapshot and its physical
/// workspace path. The path is consumed by the platform adapter and is never
/// formatted by this module.
#[derive(Clone)]
pub(crate) struct GuardedUndoCaptureRequest {
    pub snapshot_id: String,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub workspace_id: WorkspaceId,
    pub workspace_absolute: PathBuf,
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub expected_physical_root_id: PhysicalRootId,
}

impl fmt::Debug for GuardedUndoCaptureRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GuardedUndoCaptureRequest([redacted])")
    }
}

fn capture_request_fingerprint(
    requests: &[GuardedUndoCaptureRequest],
) -> Result<CaptureRequestFingerprint, GuardedUndoRuntimeError> {
    let mut entries = Vec::with_capacity(requests.len());
    for request in requests {
        let path = platform_path_bytes(&request.workspace_absolute)?;
        let mut entry = Vec::new();
        append_fingerprint_field(&mut entry, request.snapshot_id.as_bytes())?;
        append_fingerprint_field(&mut entry, request.workspace_id.0.as_bytes())?;
        append_fingerprint_field(&mut entry, &path)?;
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        append_fingerprint_field(&mut entry, &request.expected_physical_root_id.0)?;
        entries.push(entry);
    }
    entries.sort_unstable();

    let mut hasher = Sha256::new();
    hasher.update(b"dcc-guarded-undo-request-set:v1\0");
    hasher.update(
        u64::try_from(entries.len())
            .map_err(|_| GuardedUndoRuntimeError::CaptureRootLimitExceeded)?
            .to_be_bytes(),
    );
    for entry in entries {
        hasher.update(
            u64::try_from(entry.len())
                .map_err(|_| GuardedUndoRuntimeError::InvalidAttribution)?
                .to_be_bytes(),
        );
        hasher.update(entry);
    }
    Ok(CaptureRequestFingerprint(hasher.finalize().into()))
}

fn configuration_fingerprint(scope_identity: &[u8]) -> ConfigurationFingerprint {
    let mut hasher = Sha256::new();
    hasher.update(b"dcc-guarded-undo-physical-scope:v1\0");
    hasher.update(scope_identity);
    ConfigurationFingerprint(hasher.finalize().into())
}

fn recovery_roots_fingerprint(
    roots: &[PathBuf],
) -> Result<RecoveryRootsFingerprint, GuardedUndoRuntimeError> {
    if roots.is_empty() {
        return Err(GuardedUndoRuntimeError::EmptyRecoverySet);
    }
    if roots.len() > MAX_RECOVERY_ROOTS {
        return Err(GuardedUndoRuntimeError::RecoveryRootLimitExceeded);
    }
    let mut encoded = Vec::with_capacity(roots.len());
    let mut distinct = HashSet::with_capacity(roots.len());
    for root in roots {
        let path = platform_path_bytes(root)?;
        if !distinct.insert(path.clone()) {
            return Err(GuardedUndoRuntimeError::DuplicateRecoveryRoot);
        }
        encoded.push(path);
    }
    encoded.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(b"dcc-guarded-undo-recovery-roots:v1\0");
    hasher.update(
        u64::try_from(encoded.len())
            .map_err(|_| GuardedUndoRuntimeError::RecoveryRootLimitExceeded)?
            .to_be_bytes(),
    );
    for path in encoded {
        append_fingerprint_field_to_hasher(&mut hasher, &path)?;
    }
    Ok(RecoveryRootsFingerprint(hasher.finalize().into()))
}

fn append_fingerprint_field_to_hasher(
    hasher: &mut Sha256,
    value: &[u8],
) -> Result<(), GuardedUndoRuntimeError> {
    hasher.update(
        u64::try_from(value.len())
            .map_err(|_| GuardedUndoRuntimeError::InvalidAttribution)?
            .to_be_bytes(),
    );
    hasher.update(value);
    Ok(())
}

fn append_fingerprint_field(
    output: &mut Vec<u8>,
    value: &[u8],
) -> Result<(), GuardedUndoRuntimeError> {
    output.extend_from_slice(
        &u64::try_from(value.len())
            .map_err(|_| GuardedUndoRuntimeError::InvalidAttribution)?
            .to_be_bytes(),
    );
    output.extend_from_slice(value);
    Ok(())
}

#[cfg(unix)]
fn platform_path_bytes(path: &std::path::Path) -> Result<Vec<u8>, GuardedUndoRuntimeError> {
    use std::os::unix::ffi::OsStrExt;

    let mut encoded = b"unix\0".to_vec();
    encoded.extend_from_slice(path.as_os_str().as_bytes());
    Ok(encoded)
}

#[cfg(windows)]
fn platform_path_bytes(path: &std::path::Path) -> Result<Vec<u8>, GuardedUndoRuntimeError> {
    use std::os::windows::ffi::OsStrExt;

    let mut encoded = b"windows-utf16le\0".to_vec();
    for unit in path.as_os_str().encode_wide() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(encoded)
}

#[cfg(all(not(unix), not(windows)))]
fn platform_path_bytes(path: &std::path::Path) -> Result<Vec<u8>, GuardedUndoRuntimeError> {
    let value = path
        .as_os_str()
        .to_str()
        .ok_or(GuardedUndoRuntimeError::InvalidAttribution)?;
    let mut encoded = b"utf8-fallback\0".to_vec();
    encoded.extend_from_slice(value.as_bytes());
    Ok(encoded)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureTerminalMode {
    Completed,
    Cancelled,
    ProviderFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BeginDisposition {
    Disabled,
    Started,
    Replayed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BeginTurnReport {
    pub generation: Option<u64>,
    pub disposition: BeginDisposition,
    pub active_captures: u32,
    pub failed_captures: u32,
}

impl BeginTurnReport {
    fn disabled() -> Self {
        Self {
            generation: None,
            disposition: BeginDisposition::Disabled,
            active_captures: 0,
            failed_captures: 0,
        }
    }

    fn replayed(mut self) -> Self {
        self.disposition = BeginDisposition::Replayed;
        self
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GuardedUndoCaptureSummary {
    pub state: String,
    pub reason_code: Option<String>,
    pub file_count: u32,
    pub artifact_bytes: u64,
}

impl fmt::Debug for GuardedUndoCaptureSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GuardedUndoCaptureSummary")
            .field("state", &"[redacted]")
            .field(
                "reason_code",
                &self.reason_code.as_ref().map(|_| "[redacted]"),
            )
            .field("file_count", &self.file_count)
            .field("artifact_bytes", &self.artifact_bytes)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FinalizationReport {
    pub generation: u64,
    pub mode: CaptureTerminalMode,
    pub attempted_captures: u32,
    pub succeeded_captures: u32,
    pub failed_captures: u32,
    pub summaries: Vec<GuardedUndoCaptureSummary>,
}

impl fmt::Debug for FinalizationReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FinalizationReport")
            .field("generation", &self.generation)
            .field("mode", &self.mode)
            .field("attempted_captures", &self.attempted_captures)
            .field("succeeded_captures", &self.succeeded_captures)
            .field("failed_captures", &self.failed_captures)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FinalizeTurnOutcome {
    NotTracked,
    Finalized(Arc<FinalizationReport>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuardedUndoRuntimeError {
    CapacityExhausted,
    CaptureRootLimitExceeded,
    RecoveryRootLimitExceeded,
    DuplicateSnapshot,
    DuplicateWorkspace,
    DuplicateRecoveryRoot,
    EmptyCaptureSet,
    EmptyRecoverySet,
    GenerationExhausted,
    InvalidAttribution,
    Poisoned,
    ConfigurationMismatch,
    RecoveryRootsMismatch,
    ReplayRequestMismatch,
    #[cfg(test)]
    AlreadyConfigured,
}

impl fmt::Display for GuardedUndoRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CapacityExhausted => "guarded undo runtime capacity exhausted",
            Self::CaptureRootLimitExceeded => "guarded undo capture root limit exceeded",
            Self::RecoveryRootLimitExceeded => "guarded undo recovery root limit exceeded",
            Self::DuplicateSnapshot => "guarded undo capture has a duplicate snapshot",
            Self::DuplicateWorkspace => "guarded undo capture has a duplicate workspace",
            Self::DuplicateRecoveryRoot => "guarded undo recovery has a duplicate root",
            Self::EmptyCaptureSet => "guarded undo capture set is empty",
            Self::EmptyRecoverySet => "guarded undo recovery root set is empty",
            Self::GenerationExhausted => "guarded undo runtime generation exhausted",
            Self::InvalidAttribution => "guarded undo capture attribution is invalid",
            Self::Poisoned => "guarded undo runtime unavailable",
            Self::ConfigurationMismatch => "guarded undo runtime configuration does not match",
            Self::RecoveryRootsMismatch => "guarded undo recovery roots do not match",
            Self::ReplayRequestMismatch => "guarded undo capture replay does not match",
            #[cfg(test)]
            Self::AlreadyConfigured => "guarded undo runtime already configured",
        })
    }
}

impl std::error::Error for GuardedUndoRuntimeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigureOutcome {
    #[allow(dead_code)] // Constructed by the feature-off implementation.
    Disabled,
    Configured,
    AlreadyConfigured,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryOutcome {
    #[allow(dead_code)] // Constructed by the feature-off implementation.
    Disabled,
    Recovered,
    AlreadyRecovered,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuardedUndoPreview {
    pub display_path: String,
    pub size: u64,
    pub binary: bool,
    pub preview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GuardedUndoPrepareResult {
    Ready {
        snapshot_id: String,
        preview_token: String,
        expires_at: String,
        file_count: u32,
        total_bytes: u64,
        files: Vec<GuardedUndoPreview>,
        unrelated_paths_are_not_targets: bool,
    },
    Blocked {
        snapshot_id: String,
        reason_code: String,
    },
    Unavailable {
        snapshot_id: String,
        reason_code: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GuardedUndoExecuteResult {
    Completed {
        operation_id: String,
    },
    Blocked {
        reason_code: String,
    },
    RolledBack {
        operation_id: String,
    },
    RecoveryRequired {
        operation_id: String,
        reason_code: String,
    },
}

trait CaptureLease: Send {
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send>;
}

impl<T> CaptureLease for T
where
    T: Any + Send,
{
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send> {
        self
    }
}

type DynCaptureLease = Box<dyn CaptureLease>;

struct DriverFailure;

impl fmt::Debug for DriverFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DriverFailure([redacted])")
    }
}

trait CaptureDriver: Send + Sync {
    fn begin(&self, request: GuardedUndoCaptureRequest) -> Result<DynCaptureLease, DriverFailure>;

    fn finalize(
        &self,
        lease: DynCaptureLease,
        mode: CaptureTerminalMode,
    ) -> Result<GuardedUndoCaptureSummary, DriverFailure>;

    fn recover_all(&self, roots: Vec<PathBuf>) -> Result<(), DriverFailure>;

    fn prepare_guarded_undo(
        &self,
        snapshot_id: String,
        _workspace_absolute: PathBuf,
    ) -> GuardedUndoPrepareResult {
        GuardedUndoPrepareResult::Unavailable {
            snapshot_id,
            reason_code: "adapter_unsupported".to_owned(),
        }
    }

    fn execute_guarded_undo(
        &self,
        _preview_token: String,
        _confirmed: bool,
    ) -> GuardedUndoExecuteResult {
        GuardedUndoExecuteResult::Blocked {
            reason_code: "adapter_unsupported".to_owned(),
        }
    }
}

struct ActiveTurn {
    begin_report: BeginTurnReport,
    fingerprint: CaptureRequestFingerprint,
    leases: Vec<DynCaptureLease>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CaptureRequestFingerprint([u8; 32]);

impl fmt::Debug for CaptureRequestFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CaptureRequestFingerprint([redacted])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ConfigurationFingerprint([u8; 32]);

impl fmt::Debug for ConfigurationFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConfigurationFingerprint([redacted])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RecoveryRootsFingerprint([u8; 32]);

impl fmt::Debug for RecoveryRootsFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RecoveryRootsFingerprint([redacted])")
    }
}

enum TurnState {
    Beginning {
        generation: u64,
        fingerprint: CaptureRequestFingerprint,
        notify: Arc<Notify>,
    },
    Active(ActiveTurn),
    Finalizing {
        generation: u64,
        begin_report: BeginTurnReport,
        fingerprint: CaptureRequestFingerprint,
        mode: CaptureTerminalMode,
        notify: Arc<Notify>,
    },
    Finalized {
        begin_report: BeginTurnReport,
        fingerprint: CaptureRequestFingerprint,
        report: Arc<FinalizationReport>,
    },
}

impl TurnState {
    fn begin_report(&self) -> Option<&BeginTurnReport> {
        match self {
            Self::Beginning { .. } => None,
            Self::Active(active) => Some(&active.begin_report),
            Self::Finalizing { begin_report, .. } | Self::Finalized { begin_report, .. } => {
                Some(begin_report)
            }
        }
    }

    fn fingerprint(&self) -> &CaptureRequestFingerprint {
        match self {
            Self::Beginning { fingerprint, .. }
            | Self::Finalizing { fingerprint, .. }
            | Self::Finalized { fingerprint, .. } => fingerprint,
            Self::Active(active) => &active.fingerprint,
        }
    }
}

struct Inner {
    driver: Mutex<Option<Arc<dyn CaptureDriver>>>,
    configuration: Mutex<ConfigurationState>,
    recovery: Mutex<RecoveryState>,
    registry: Mutex<TurnRegistry>,
    next_generation: AtomicU64,
    next_attempt: AtomicU64,
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    coordinator: Arc<dcc_infra::guarded_undo::coordinator::WorkspaceMutationCoordinator>,
}

enum ConfigurationState {
    Unconfigured,
    Configuring {
        attempt: u64,
        fingerprint: ConfigurationFingerprint,
        notify: Arc<Notify>,
    },
    Configured {
        fingerprint: ConfigurationFingerprint,
    },
    Failed {
        attempt: u64,
        fingerprint: ConfigurationFingerprint,
    },
}

enum RecoveryState {
    Unrecovered,
    Recovering {
        attempt: u64,
        fingerprint: RecoveryRootsFingerprint,
        notify: Arc<Notify>,
    },
    Recovered {
        fingerprint: RecoveryRootsFingerprint,
    },
    Failed {
        attempt: u64,
        fingerprint: RecoveryRootsFingerprint,
    },
}

#[derive(Default)]
struct TurnRegistry {
    turns: HashMap<TerminalKey, TurnState>,
    finalized_order: VecDeque<(TerminalKey, u64)>,
}

/// Makes room without ever dropping a live capture lease. Finalized entries
/// contain reports only; their durable M4 rows remain the authority if an
/// integration caller failed to forget the in-memory result after append.
fn evict_finalized_for_capacity(registry: &mut TurnRegistry) {
    while registry.turns.len() >= MAX_TRACKED_TURNS {
        let Some((candidate, generation)) = registry.finalized_order.pop_front() else {
            break;
        };
        let removable = registry.turns.get(&candidate).is_some_and(|state| {
            matches!(state, TurnState::Finalized { report, .. } if report.generation == generation)
        });
        if removable {
            registry.turns.remove(&candidate);
        }
    }
}

/// Shared process-level lifecycle for capture-v2 leases.
///
/// Blocking capture work always owns its handles. The map retains a
/// `Finalizing` marker and notification independently of any async waiter, so
/// cancelling a waiter cannot release the leases or permit duplicate work.
#[derive(Clone)]
pub(crate) struct GuardedUndoRuntime {
    inner: Arc<Inner>,
}

impl Default for GuardedUndoRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for GuardedUndoRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GuardedUndoRuntime([redacted])")
    }
}

impl GuardedUndoRuntime {
    pub(crate) fn new() -> Self {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        {
            return Self::new_with_coordinator(Arc::new(WorkspaceMutationCoordinator::new()));
        }

        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        Self::new_without_coordinator()
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) fn new_with_coordinator(coordinator: Arc<WorkspaceMutationCoordinator>) -> Self {
        Self {
            inner: Arc::new(Inner {
                driver: Mutex::new(None),
                configuration: Mutex::new(ConfigurationState::Unconfigured),
                recovery: Mutex::new(RecoveryState::Unrecovered),
                registry: Mutex::new(TurnRegistry::default()),
                next_generation: AtomicU64::new(0),
                next_attempt: AtomicU64::new(0),
                coordinator,
            }),
        }
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    fn new_without_coordinator() -> Self {
        Self {
            inner: Arc::new(Inner {
                driver: Mutex::new(None),
                configuration: Mutex::new(ConfigurationState::Unconfigured),
                recovery: Mutex::new(RecoveryState::Unrecovered),
                registry: Mutex::new(TurnRegistry::default()),
                next_generation: AtomicU64::new(0),
                next_attempt: AtomicU64::new(0),
            }),
        }
    }

    /// Executes one synchronous workspace mutation while the descriptor-rooted
    /// identity and coordinator guard have the same lifetime as the operation.
    /// Cancelling the async waiter never cancels a running blocking worker.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn run_workspace_mutation<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        let coordinator = Arc::clone(&self.inner.coordinator);
        tokio::task::spawn_blocking(move || {
            let lease = PhysicalMutationLease::acquire(&coordinator, workspace_absolute).map_err(
                |error| match error {
                    WorkspaceMutationRunError::Busy => WorkspaceMutationRunError::Busy,
                    WorkspaceMutationRunError::PhysicalRootUnavailable => {
                        WorkspaceMutationRunError::PhysicalRootUnavailable
                    }
                    WorkspaceMutationRunError::CoordinatorUnavailable => {
                        WorkspaceMutationRunError::CoordinatorUnavailable
                    }
                    WorkspaceMutationRunError::WorkerUnavailable
                    | WorkspaceMutationRunError::Operation(_) => {
                        WorkspaceMutationRunError::CoordinatorUnavailable
                    }
                },
            )?;
            operation(lease.path()).map_err(WorkspaceMutationRunError::Operation)
        })
        .await
        .map_err(|_| WorkspaceMutationRunError::WorkerUnavailable)?
    }

    /// The blocking variant has the same physical-lease semantics as the
    /// ordinary mutation runner.  Keep this separate from the feature-off
    /// no-op gate so existing command handlers that launch child processes do
    /// not start running those processes on Tokio's async workers.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn run_workspace_mutation_blocking<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        self.run_workspace_mutation(workspace_absolute, operation)
            .await
    }

    /// Runs a Git mutation while atomically holding both the authorized
    /// worktree root and its descriptor-proven shared common directory.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn run_git_workspace_mutation<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        let coordinator = Arc::clone(&self.inner.coordinator);
        tokio::task::spawn_blocking(move || {
            let lease = PhysicalGitMutationLease::acquire(&coordinator, workspace_absolute)
                .map_err(|error| match error {
                    WorkspaceMutationRunError::Busy => WorkspaceMutationRunError::Busy,
                    WorkspaceMutationRunError::PhysicalRootUnavailable => {
                        WorkspaceMutationRunError::PhysicalRootUnavailable
                    }
                    WorkspaceMutationRunError::CoordinatorUnavailable => {
                        WorkspaceMutationRunError::CoordinatorUnavailable
                    }
                    WorkspaceMutationRunError::WorkerUnavailable
                    | WorkspaceMutationRunError::Operation(_) => {
                        WorkspaceMutationRunError::CoordinatorUnavailable
                    }
                })?;
            operation(lease.path()).map_err(WorkspaceMutationRunError::Operation)
        })
        .await
        .map_err(|_| WorkspaceMutationRunError::WorkerUnavailable)?
    }

    /// Child-process-capable Git operations use the same blocking worker and
    /// two-root lease; cancelling the async waiter cannot release either root.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn run_git_workspace_mutation_blocking<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        self.run_git_workspace_mutation(workspace_absolute, operation)
            .await
    }

    /// Runs a synchronous operation with both linked worktrees and their
    /// shared Git common directory admitted atomically. The secondary path is
    /// supplied by a command layer only after it has scoped the path to DCC's
    /// delegation directory. This runtime proves the physical Git relationship
    /// before invoking the closure; a durable operation binding is a later
    /// lifecycle-journal requirement.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn run_git_workspace_pair_mutation<T, E, F>(
        &self,
        primary_absolute: PathBuf,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path, &Path) -> Result<T, E> + Send + 'static,
    {
        let coordinator = Arc::clone(&self.inner.coordinator);
        tokio::task::spawn_blocking(move || {
            let lease = PhysicalGitPairMutationLease::acquire(
                &coordinator,
                primary_absolute,
                secondary_absolute,
            )
            .map_err(|error| match error {
                WorkspaceMutationRunError::Busy => WorkspaceMutationRunError::Busy,
                WorkspaceMutationRunError::PhysicalRootUnavailable => {
                    WorkspaceMutationRunError::PhysicalRootUnavailable
                }
                WorkspaceMutationRunError::CoordinatorUnavailable => {
                    WorkspaceMutationRunError::CoordinatorUnavailable
                }
                WorkspaceMutationRunError::WorkerUnavailable
                | WorkspaceMutationRunError::Operation(_) => {
                    WorkspaceMutationRunError::CoordinatorUnavailable
                }
            })?;
            let (primary, secondary) = lease.paths();
            operation(primary, secondary).map_err(WorkspaceMutationRunError::Operation)
        })
        .await
        .map_err(|_| WorkspaceMutationRunError::WorkerUnavailable)?
    }

    /// Child-process-capable pair operations use the same blocking worker.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn run_git_workspace_pair_mutation_blocking<T, E, F>(
        &self,
        primary_absolute: PathBuf,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path, &Path) -> Result<T, E> + Send + 'static,
    {
        self.run_git_workspace_pair_mutation(primary_absolute, secondary_absolute, operation)
            .await
    }

    /// Feature-off is deliberately a direct call: no platform inspection,
    /// coordinator allocation, blocking worker, or filesystem I/O is added.
    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(crate) async fn run_workspace_mutation<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        F: FnOnce(&Path) -> Result<T, E>,
    {
        operation(&workspace_absolute).map_err(WorkspaceMutationRunError::Operation)
    }

    /// With capture v2 disabled, retain the old executor boundary for command
    /// operations that may synchronously run a child process.  This adds no
    /// filesystem/coordinator work before the user operation itself.
    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(crate) async fn run_workspace_mutation_blocking<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        tokio::task::spawn_blocking(move || {
            operation(&workspace_absolute).map_err(WorkspaceMutationRunError::Operation)
        })
        .await
        .map_err(|_| WorkspaceMutationRunError::WorkerUnavailable)?
    }

    /// Feature-off preserves the ordinary direct mutation runner exactly: the
    /// path is neither opened nor inspected as a Git repository.
    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(crate) async fn run_git_workspace_mutation<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        F: FnOnce(&Path) -> Result<T, E>,
    {
        self.run_workspace_mutation(workspace_absolute, operation)
            .await
    }

    /// Feature-off preserves the existing blocking executor boundary and does
    /// no SQLite, Git-layout, descriptor, or coordinator work of its own.
    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(crate) async fn run_git_workspace_mutation_blocking<T, E, F>(
        &self,
        workspace_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        self.run_workspace_mutation_blocking(workspace_absolute, operation)
            .await
    }

    /// Feature-off remains a direct path handoff. In particular, this path
    /// does not open SQLite, inspect Git layout, acquire a coordinator, or
    /// touch either filesystem root before the caller's closure runs.
    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(crate) async fn run_git_workspace_pair_mutation<T, E, F>(
        &self,
        primary_absolute: PathBuf,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        F: FnOnce(&Path, &Path) -> Result<T, E>,
    {
        operation(&primary_absolute, &secondary_absolute)
            .map_err(WorkspaceMutationRunError::Operation)
    }

    /// Feature-off preserves the existing blocking executor boundary without
    /// adding any authority or filesystem work of its own.
    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(crate) async fn run_git_workspace_pair_mutation_blocking<T, E, F>(
        &self,
        primary_absolute: PathBuf,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path, &Path) -> Result<T, E> + Send + 'static,
    {
        tokio::task::spawn_blocking(move || {
            operation(&primary_absolute, &secondary_absolute)
                .map_err(WorkspaceMutationRunError::Operation)
        })
        .await
        .map_err(|_| WorkspaceMutationRunError::WorkerUnavailable)?
    }

    /// Lazily creates the one capture-v2 service (and therefore the one
    /// app-data lifetime lease) owned by this process runtime.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(super) async fn configure_capture_v2_scoped(
        &self,
        app_data_absolute: PathBuf,
        repo: SqliteSessionRepo,
        physical_scope_identity: [u8; 32],
    ) -> Result<ConfigureOutcome, GuardedUndoRuntimeError> {
        let fingerprint = configuration_fingerprint(&physical_scope_identity);
        let coordinator = Arc::clone(&self.inner.coordinator);
        self.configure_with_factory(
            fingerprint,
            Box::new(move || {
                dcc_infra::guarded_undo::capture_v2_service::CaptureV2Service::with_system_git(
                    app_data_absolute,
                    repo,
                    coordinator,
                )
                .map(|service| {
                    Arc::new(CaptureV2Driver {
                        service: Arc::new(service),
                    }) as Arc<dyn CaptureDriver>
                })
                .map_err(|_| DriverFailure)
            }),
        )
        .await
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(super) async fn configure_capture_v2_scoped(
        &self,
        _app_data_absolute: PathBuf,
        _repo: SqliteSessionRepo,
        _physical_scope_identity: [u8; 32],
    ) -> Result<ConfigureOutcome, GuardedUndoRuntimeError> {
        Ok(ConfigureOutcome::Disabled)
    }

    /// Runs global startup recovery once for the canonical authorized root
    /// set. A failed attempt is retryable; cancellation of an async waiter does
    /// not cancel or duplicate the blocking worker.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn recovery_all(
        &self,
        roots: Vec<PathBuf>,
    ) -> Result<RecoveryOutcome, GuardedUndoRuntimeError> {
        self.recovery_all_with_driver(roots).await
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub(crate) async fn recovery_all(
        &self,
        _roots: Vec<PathBuf>,
    ) -> Result<RecoveryOutcome, GuardedUndoRuntimeError> {
        Ok(RecoveryOutcome::Disabled)
    }

    pub(crate) async fn prepare_guarded_undo(
        &self,
        snapshot_id: String,
        workspace_absolute: PathBuf,
    ) -> GuardedUndoPrepareResult {
        if snapshot_id.trim().is_empty() || !workspace_absolute.is_absolute() {
            return GuardedUndoPrepareResult::Unavailable {
                snapshot_id,
                reason_code: "invalid_persisted_record".to_owned(),
            };
        }
        let Ok(Some(driver)) = self.driver() else {
            return GuardedUndoPrepareResult::Unavailable {
                snapshot_id,
                reason_code: "adapter_unsupported".to_owned(),
            };
        };
        if !self.recovery_admits_capture().unwrap_or(false) {
            return GuardedUndoPrepareResult::Unavailable {
                snapshot_id,
                reason_code: "operation_interrupted".to_owned(),
            };
        }
        let fallback_snapshot = snapshot_id.clone();
        tokio::task::spawn_blocking(move || {
            catch_unwind(AssertUnwindSafe(|| {
                driver.prepare_guarded_undo(snapshot_id, workspace_absolute)
            }))
            .unwrap_or(GuardedUndoPrepareResult::Unavailable {
                snapshot_id: fallback_snapshot,
                reason_code: "operation_interrupted".to_owned(),
            })
        })
        .await
        .unwrap_or(GuardedUndoPrepareResult::Unavailable {
            snapshot_id: String::new(),
            reason_code: "operation_interrupted".to_owned(),
        })
    }

    pub(crate) async fn execute_guarded_undo(
        &self,
        preview_token: String,
        confirmed: bool,
    ) -> GuardedUndoExecuteResult {
        if !confirmed || preview_token.len() != 64 || !preview_token.is_ascii() {
            return GuardedUndoExecuteResult::Blocked {
                reason_code: "preview_consumed".to_owned(),
            };
        }
        let Ok(Some(driver)) = self.driver() else {
            return GuardedUndoExecuteResult::Blocked {
                reason_code: "adapter_unsupported".to_owned(),
            };
        };
        if !self.recovery_admits_capture().unwrap_or(false) {
            return GuardedUndoExecuteResult::Blocked {
                reason_code: "operation_interrupted".to_owned(),
            };
        }
        tokio::task::spawn_blocking(move || {
            catch_unwind(AssertUnwindSafe(|| {
                driver.execute_guarded_undo(preview_token, confirmed)
            }))
            .unwrap_or(GuardedUndoExecuteResult::Blocked {
                reason_code: "operation_interrupted".to_owned(),
            })
        })
        .await
        .unwrap_or(GuardedUndoExecuteResult::Blocked {
            reason_code: "operation_interrupted".to_owned(),
        })
    }

    async fn configure_with_factory(
        &self,
        fingerprint: ConfigurationFingerprint,
        factory: Box<
            dyn FnOnce() -> Result<Arc<dyn CaptureDriver>, DriverFailure> + Send + 'static,
        >,
    ) -> Result<ConfigureOutcome, GuardedUndoRuntimeError> {
        enum Action {
            Start { attempt: u64, notify: Arc<Notify> },
            Wait { attempt: u64, notify: Arc<Notify> },
        }

        let action = {
            let mut state = self
                .inner
                .configuration
                .lock()
                .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
            match &*state {
                ConfigurationState::Configured {
                    fingerprint: current,
                } => {
                    return if current == &fingerprint {
                        Ok(ConfigureOutcome::AlreadyConfigured)
                    } else {
                        Err(GuardedUndoRuntimeError::ConfigurationMismatch)
                    };
                }
                ConfigurationState::Configuring {
                    attempt,
                    fingerprint: current,
                    notify,
                } => {
                    if current != &fingerprint {
                        return Err(GuardedUndoRuntimeError::ConfigurationMismatch);
                    }
                    Action::Wait {
                        attempt: *attempt,
                        notify: Arc::clone(notify),
                    }
                }
                ConfigurationState::Failed {
                    fingerprint: current,
                    ..
                } if current != &fingerprint => {
                    return Err(GuardedUndoRuntimeError::ConfigurationMismatch);
                }
                ConfigurationState::Unconfigured | ConfigurationState::Failed { .. } => {
                    let attempt = self.allocate_attempt()?;
                    let notify = Arc::new(Notify::new());
                    *state = ConfigurationState::Configuring {
                        attempt,
                        fingerprint,
                        notify: Arc::clone(&notify),
                    };
                    Action::Start { attempt, notify }
                }
            }
        };

        let started = matches!(&action, Action::Start { .. });
        let (attempt, notify) = match action {
            Action::Start { attempt, notify } => {
                let inner = Arc::clone(&self.inner);
                let worker_notify = Arc::clone(&notify);
                tokio::task::spawn_blocking(move || {
                    complete_configuration(inner, attempt, fingerprint, factory, worker_notify)
                });
                (attempt, notify)
            }
            Action::Wait { attempt, notify } => (attempt, notify),
        };
        let available = wait_for_configuration(&self.inner, attempt, fingerprint, &notify).await?;
        Ok(if available {
            if started {
                ConfigureOutcome::Configured
            } else {
                ConfigureOutcome::AlreadyConfigured
            }
        } else {
            ConfigureOutcome::Unavailable
        })
    }

    async fn recovery_all_with_driver(
        &self,
        roots: Vec<PathBuf>,
    ) -> Result<RecoveryOutcome, GuardedUndoRuntimeError> {
        let fingerprint = recovery_roots_fingerprint(&roots)?;
        let Some(driver) = self.driver()? else {
            return Ok(RecoveryOutcome::Unavailable);
        };
        enum Action {
            Start { attempt: u64, notify: Arc<Notify> },
            Wait { attempt: u64, notify: Arc<Notify> },
        }
        let action = {
            let mut state = self
                .inner
                .recovery
                .lock()
                .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
            match &*state {
                // Recovery is a one-time global maintenance operation. Once
                // it completed, a later session may have a different current
                // root set (for example after a workspace was added); do not
                // rerun global cleanup or reject capture solely for that.
                RecoveryState::Recovered { .. } => {
                    return Ok(RecoveryOutcome::AlreadyRecovered);
                }
                RecoveryState::Recovering {
                    attempt,
                    fingerprint: current,
                    notify,
                } => {
                    if current != &fingerprint {
                        return Err(GuardedUndoRuntimeError::RecoveryRootsMismatch);
                    }
                    Action::Wait {
                        attempt: *attempt,
                        notify: Arc::clone(notify),
                    }
                }
                RecoveryState::Unrecovered | RecoveryState::Failed { .. } => {
                    let attempt = self.allocate_attempt()?;
                    let notify = Arc::new(Notify::new());
                    *state = RecoveryState::Recovering {
                        attempt,
                        fingerprint,
                        notify: Arc::clone(&notify),
                    };
                    Action::Start { attempt, notify }
                }
            }
        };
        let started = matches!(&action, Action::Start { .. });
        let (attempt, notify) = match action {
            Action::Start { attempt, notify } => {
                let inner = Arc::clone(&self.inner);
                let worker_notify = Arc::clone(&notify);
                tokio::task::spawn_blocking(move || {
                    complete_recovery(inner, driver, attempt, fingerprint, roots, worker_notify)
                });
                (attempt, notify)
            }
            Action::Wait { attempt, notify } => (attempt, notify),
        };
        let recovered = wait_for_recovery(&self.inner, attempt, fingerprint, &notify).await?;
        Ok(if recovered {
            if started {
                RecoveryOutcome::Recovered
            } else {
                RecoveryOutcome::AlreadyRecovered
            }
        } else {
            RecoveryOutcome::Unavailable
        })
    }

    #[cfg(test)]
    fn install_driver(
        &self,
        driver: Arc<dyn CaptureDriver>,
    ) -> Result<(), GuardedUndoRuntimeError> {
        let mut configured = self
            .inner
            .driver
            .lock()
            .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
        if configured.is_some() {
            return Err(GuardedUndoRuntimeError::AlreadyConfigured);
        }
        *configured = Some(driver);
        Ok(())
    }

    pub(crate) async fn begin_turn(
        &self,
        key: TerminalKey,
        requests: Vec<GuardedUndoCaptureRequest>,
    ) -> Result<BeginTurnReport, GuardedUndoRuntimeError> {
        if requests.is_empty() {
            return Err(GuardedUndoRuntimeError::EmptyCaptureSet);
        }
        if requests.len() > MAX_CAPTURE_ROOTS_PER_TURN {
            return Err(GuardedUndoRuntimeError::CaptureRootLimitExceeded);
        }
        let mut snapshots = HashSet::with_capacity(requests.len());
        let mut workspaces = HashSet::with_capacity(requests.len());
        for request in &requests {
            if request.session_id != key.session_id
                || request.turn_id != key.turn_id
                || request.snapshot_id.trim().is_empty()
            {
                return Err(GuardedUndoRuntimeError::InvalidAttribution);
            }
            if !snapshots.insert(request.snapshot_id.clone()) {
                return Err(GuardedUndoRuntimeError::DuplicateSnapshot);
            }
            if !workspaces.insert(request.workspace_id.clone()) {
                return Err(GuardedUndoRuntimeError::DuplicateWorkspace);
            }
        }
        let fingerprint = capture_request_fingerprint(&requests)?;
        if !self.recovery_admits_capture()? {
            return Ok(BeginTurnReport::disabled());
        }
        let Some(driver) = self.driver()? else {
            return Ok(BeginTurnReport::disabled());
        };

        enum Reservation {
            Start {
                generation: u64,
                notify: Arc<Notify>,
            },
            Wait,
        }

        let reservation = {
            let mut registry = self
                .inner
                .registry
                .lock()
                .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
            if let Some(existing) = registry.turns.get(&key) {
                if existing.fingerprint() != &fingerprint {
                    return Err(GuardedUndoRuntimeError::ReplayRequestMismatch);
                }
                if let Some(report) = existing.begin_report() {
                    return Ok(report.clone().replayed());
                }
                Reservation::Wait
            } else {
                evict_finalized_for_capacity(&mut registry);
                if registry.turns.len() >= MAX_TRACKED_TURNS {
                    return Err(GuardedUndoRuntimeError::CapacityExhausted);
                }
                let generation = self.allocate_generation()?;
                let notify = Arc::new(Notify::new());
                registry.turns.insert(
                    key.clone(),
                    TurnState::Beginning {
                        generation,
                        fingerprint,
                        notify: Arc::clone(&notify),
                    },
                );
                Reservation::Start { generation, notify }
            }
        };

        let replayed = matches!(&reservation, Reservation::Wait);
        if let Reservation::Start { generation, notify } = reservation {
            let inner = Arc::clone(&self.inner);
            let worker_key = key.clone();
            tokio::task::spawn_blocking(move || {
                complete_begin(inner, driver, worker_key, generation, requests, notify)
            });
        }

        let report = self.wait_for_begin(&key).await?;
        Ok(if replayed { report.replayed() } else { report })
    }

    pub(crate) async fn finalize_turn(
        &self,
        key: &TerminalKey,
        mode: CaptureTerminalMode,
    ) -> Result<FinalizeTurnOutcome, GuardedUndoRuntimeError> {
        let Some(driver) = self.driver()? else {
            return Ok(FinalizeTurnOutcome::NotTracked);
        };

        loop {
            enum Action {
                Wait {
                    generation: u64,
                    phase: WaitPhase,
                    notify: Arc<Notify>,
                },
                Start {
                    generation: u64,
                    notify: Arc<Notify>,
                    leases: Vec<DynCaptureLease>,
                },
                Done(Arc<FinalizationReport>),
            }

            let action = {
                let mut registry = self
                    .inner
                    .registry
                    .lock()
                    .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
                let Some(state) = registry.turns.remove(key) else {
                    return Ok(FinalizeTurnOutcome::NotTracked);
                };
                match state {
                    TurnState::Beginning {
                        generation,
                        fingerprint,
                        notify,
                    } => {
                        registry.turns.insert(
                            key.clone(),
                            TurnState::Beginning {
                                generation,
                                fingerprint,
                                notify: Arc::clone(&notify),
                            },
                        );
                        Action::Wait {
                            generation,
                            phase: WaitPhase::Beginning,
                            notify,
                        }
                    }
                    TurnState::Active(active) => {
                        let generation = active
                            .begin_report
                            .generation
                            .ok_or(GuardedUndoRuntimeError::GenerationExhausted)?;
                        let notify = Arc::new(Notify::new());
                        let leases = active.leases;
                        registry.turns.insert(
                            key.clone(),
                            TurnState::Finalizing {
                                generation,
                                begin_report: active.begin_report,
                                fingerprint: active.fingerprint,
                                mode,
                                notify: Arc::clone(&notify),
                            },
                        );
                        Action::Start {
                            generation,
                            notify,
                            leases,
                        }
                    }
                    TurnState::Finalizing {
                        generation,
                        begin_report,
                        fingerprint,
                        mode,
                        notify,
                    } => {
                        registry.turns.insert(
                            key.clone(),
                            TurnState::Finalizing {
                                generation,
                                begin_report,
                                fingerprint,
                                mode,
                                notify: Arc::clone(&notify),
                            },
                        );
                        Action::Wait {
                            generation,
                            phase: WaitPhase::Finalizing,
                            notify,
                        }
                    }
                    TurnState::Finalized {
                        begin_report,
                        fingerprint,
                        report,
                    } => {
                        registry.turns.insert(
                            key.clone(),
                            TurnState::Finalized {
                                begin_report,
                                fingerprint,
                                report: Arc::clone(&report),
                            },
                        );
                        Action::Done(report)
                    }
                }
            };

            match action {
                Action::Wait {
                    generation,
                    phase,
                    notify,
                } => {
                    wait_for_transition(&self.inner, key, generation, phase, &notify).await?;
                }
                Action::Start {
                    generation,
                    notify,
                    leases,
                } => {
                    let inner = Arc::clone(&self.inner);
                    let key = key.clone();
                    let driver = Arc::clone(&driver);
                    tokio::task::spawn_blocking(move || {
                        complete_finalization(inner, driver, key, generation, mode, leases, notify)
                    });
                }
                Action::Done(report) => {
                    return Ok(FinalizeTurnOutcome::Finalized(report));
                }
            }
        }
    }

    /// Forgets the exact finalized result only after the caller has durably
    /// appended (or found) the canonical terminal event.
    ///
    /// Integration MUST NOT call this before the SQLite terminal append. A
    /// delayed caller cannot erase a successor because the report generation
    /// participates in the compare-and-remove operation.
    pub(crate) fn forget_finalized_after_terminal_append(
        &self,
        key: &TerminalKey,
        report: &FinalizationReport,
    ) -> Result<bool, GuardedUndoRuntimeError> {
        let generation = report.generation;
        let mut registry = self
            .inner
            .registry
            .lock()
            .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
        let matches = registry.turns.get(key).is_some_and(|state| {
            matches!(state, TurnState::Finalized { report, .. } if report.generation == generation)
        });
        if matches {
            registry.turns.remove(key);
            registry
                .finalized_order
                .retain(|(candidate, candidate_generation)| {
                    candidate != key || *candidate_generation != generation
                });
        }
        Ok(matches)
    }

    fn driver(&self) -> Result<Option<Arc<dyn CaptureDriver>>, GuardedUndoRuntimeError> {
        self.inner
            .driver
            .lock()
            .map(|driver| driver.clone())
            .map_err(|_| GuardedUndoRuntimeError::Poisoned)
    }

    fn recovery_admits_capture(&self) -> Result<bool, GuardedUndoRuntimeError> {
        self.inner
            .recovery
            .lock()
            .map(|state| matches!(*state, RecoveryState::Recovered { .. }))
            .map_err(|_| GuardedUndoRuntimeError::Poisoned)
    }

    fn allocate_generation(&self) -> Result<u64, GuardedUndoRuntimeError> {
        self.inner
            .next_generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| GuardedUndoRuntimeError::GenerationExhausted)
    }

    fn allocate_attempt(&self) -> Result<u64, GuardedUndoRuntimeError> {
        self.inner
            .next_attempt
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| GuardedUndoRuntimeError::GenerationExhausted)
    }

    async fn wait_for_begin(
        &self,
        key: &TerminalKey,
    ) -> Result<BeginTurnReport, GuardedUndoRuntimeError> {
        loop {
            let wait = {
                let registry = self
                    .inner
                    .registry
                    .lock()
                    .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
                match registry.turns.get(key) {
                    Some(TurnState::Beginning {
                        generation, notify, ..
                    }) => Some((*generation, Arc::clone(notify))),
                    Some(state) => {
                        let report = state
                            .begin_report()
                            .ok_or(GuardedUndoRuntimeError::Poisoned)?;
                        return Ok(report.clone());
                    }
                    None => return Err(GuardedUndoRuntimeError::Poisoned),
                }
            };
            if let Some((generation, notify)) = wait {
                wait_for_transition(&self.inner, key, generation, WaitPhase::Beginning, &notify)
                    .await?;
            }
        }
    }

    #[cfg(test)]
    fn with_test_driver(driver: Arc<dyn CaptureDriver>) -> Self {
        let runtime = Self::new();
        runtime.install_driver(driver).expect("install test driver");
        *runtime.inner.recovery.lock().expect("test recovery state") = RecoveryState::Recovered {
            fingerprint: RecoveryRootsFingerprint([0; 32]),
        };
        runtime
    }

    #[cfg(test)]
    fn with_unrecovered_test_driver(driver: Arc<dyn CaptureDriver>) -> Self {
        let runtime = Self::new();
        runtime.install_driver(driver).expect("install test driver");
        runtime
    }

    #[cfg(test)]
    fn phase(&self, key: &TerminalKey) -> Option<&'static str> {
        let registry = self.inner.registry.lock().ok()?;
        Some(match registry.turns.get(key)? {
            TurnState::Beginning { .. } => "beginning",
            TurnState::Active(_) => "active",
            TurnState::Finalizing { .. } => "finalizing",
            TurnState::Finalized { .. } => "finalized",
        })
    }

    #[cfg(test)]
    fn tracked_count(&self) -> usize {
        self.inner
            .registry
            .lock()
            .map(|registry| registry.turns.len())
            .unwrap_or(usize::MAX)
    }
}

fn complete_configuration(
    inner: Arc<Inner>,
    attempt: u64,
    fingerprint: ConfigurationFingerprint,
    factory: Box<dyn FnOnce() -> Result<Arc<dyn CaptureDriver>, DriverFailure> + Send + 'static>,
    notify: Arc<Notify>,
) {
    let created = catch_unwind(AssertUnwindSafe(factory));
    if let Ok(mut state) = inner.configuration.lock() {
        let owns_attempt = matches!(
            &*state,
            ConfigurationState::Configuring {
                attempt: current_attempt,
                fingerprint: current_fingerprint,
                ..
            } if *current_attempt == attempt && *current_fingerprint == fingerprint
        );
        if owns_attempt {
            match created {
                Ok(Ok(driver)) => match inner.driver.lock() {
                    Ok(mut slot) if slot.is_none() => {
                        *slot = Some(driver);
                        *state = ConfigurationState::Configured { fingerprint };
                    }
                    _ => {
                        *state = ConfigurationState::Failed {
                            attempt,
                            fingerprint,
                        };
                    }
                },
                Ok(Err(_)) | Err(_) => {
                    *state = ConfigurationState::Failed {
                        attempt,
                        fingerprint,
                    };
                }
            }
        }
    }
    notify.notify_waiters();
}

async fn wait_for_configuration(
    inner: &Arc<Inner>,
    attempt: u64,
    fingerprint: ConfigurationFingerprint,
    notify: &Arc<Notify>,
) -> Result<bool, GuardedUndoRuntimeError> {
    loop {
        let notified = notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let should_wait = {
            let state = inner
                .configuration
                .lock()
                .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
            match &*state {
                ConfigurationState::Configured {
                    fingerprint: current,
                } => {
                    return if *current == fingerprint {
                        Ok(true)
                    } else {
                        Err(GuardedUndoRuntimeError::ConfigurationMismatch)
                    };
                }
                ConfigurationState::Failed {
                    attempt: current_attempt,
                    fingerprint: current,
                } => {
                    if *current != fingerprint {
                        return Err(GuardedUndoRuntimeError::ConfigurationMismatch);
                    }
                    if *current_attempt == attempt {
                        return Ok(false);
                    }
                    return Ok(false);
                }
                ConfigurationState::Configuring {
                    attempt: current_attempt,
                    fingerprint: current,
                    notify: current_notify,
                } => {
                    if *current != fingerprint {
                        return Err(GuardedUndoRuntimeError::ConfigurationMismatch);
                    }
                    *current_attempt == attempt && Arc::ptr_eq(current_notify, notify)
                }
                ConfigurationState::Unconfigured => return Ok(false),
            }
        };
        if !should_wait {
            return Ok(false);
        }
        notified.await;
    }
}

fn complete_recovery(
    inner: Arc<Inner>,
    driver: Arc<dyn CaptureDriver>,
    attempt: u64,
    fingerprint: RecoveryRootsFingerprint,
    roots: Vec<PathBuf>,
    notify: Arc<Notify>,
) {
    let result = catch_unwind(AssertUnwindSafe(|| driver.recover_all(roots)));
    if let Ok(mut state) = inner.recovery.lock() {
        let owns_attempt = matches!(
            &*state,
            RecoveryState::Recovering {
                attempt: current_attempt,
                fingerprint: current_fingerprint,
                ..
            } if *current_attempt == attempt && *current_fingerprint == fingerprint
        );
        if owns_attempt {
            *state = if matches!(result, Ok(Ok(()))) {
                RecoveryState::Recovered { fingerprint }
            } else {
                RecoveryState::Failed {
                    attempt,
                    fingerprint,
                }
            };
        }
    }
    notify.notify_waiters();
}

async fn wait_for_recovery(
    inner: &Arc<Inner>,
    attempt: u64,
    fingerprint: RecoveryRootsFingerprint,
    notify: &Arc<Notify>,
) -> Result<bool, GuardedUndoRuntimeError> {
    loop {
        let notified = notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let should_wait = {
            let state = inner
                .recovery
                .lock()
                .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
            match &*state {
                RecoveryState::Recovered {
                    fingerprint: current,
                } => {
                    return if *current == fingerprint {
                        Ok(true)
                    } else {
                        Err(GuardedUndoRuntimeError::RecoveryRootsMismatch)
                    };
                }
                RecoveryState::Failed {
                    attempt: current_attempt,
                    fingerprint: current,
                } => {
                    if *current != fingerprint {
                        return Err(GuardedUndoRuntimeError::RecoveryRootsMismatch);
                    }
                    if *current_attempt == attempt {
                        return Ok(false);
                    }
                    return Ok(false);
                }
                RecoveryState::Recovering {
                    attempt: current_attempt,
                    fingerprint: current,
                    notify: current_notify,
                } => {
                    if *current != fingerprint {
                        return Err(GuardedUndoRuntimeError::RecoveryRootsMismatch);
                    }
                    *current_attempt == attempt && Arc::ptr_eq(current_notify, notify)
                }
                RecoveryState::Unrecovered => return Ok(false),
            }
        };
        if !should_wait {
            return Ok(false);
        }
        notified.await;
    }
}

#[derive(Clone, Copy)]
enum WaitPhase {
    Beginning,
    Finalizing,
}

async fn wait_for_transition(
    inner: &Arc<Inner>,
    key: &TerminalKey,
    generation: u64,
    phase: WaitPhase,
    notify: &Arc<Notify>,
) -> Result<(), GuardedUndoRuntimeError> {
    let notified = notify.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    let unchanged = {
        let registry = inner
            .registry
            .lock()
            .map_err(|_| GuardedUndoRuntimeError::Poisoned)?;
        match (phase, registry.turns.get(key)) {
            (
                WaitPhase::Beginning,
                Some(TurnState::Beginning {
                    generation: current,
                    notify: current_notify,
                    ..
                }),
            ) => *current == generation && Arc::ptr_eq(current_notify, notify),
            (
                WaitPhase::Finalizing,
                Some(TurnState::Finalizing {
                    generation: current,
                    notify: current_notify,
                    ..
                }),
            ) => *current == generation && Arc::ptr_eq(current_notify, notify),
            _ => false,
        }
    };
    if unchanged {
        notified.await;
    }
    Ok(())
}

fn complete_begin(
    inner: Arc<Inner>,
    driver: Arc<dyn CaptureDriver>,
    key: TerminalKey,
    generation: u64,
    requests: Vec<GuardedUndoCaptureRequest>,
    notify: Arc<Notify>,
) {
    let mut leases = Vec::new();
    let mut failures = 0_u32;
    for request in requests {
        match catch_unwind(AssertUnwindSafe(|| driver.begin(request))) {
            Ok(Ok(lease)) => leases.push(lease),
            Ok(Err(_)) | Err(_) => failures = failures.saturating_add(1),
        }
    }
    let active_captures = u32::try_from(leases.len()).unwrap_or(u32::MAX);
    let report = BeginTurnReport {
        generation: Some(generation),
        disposition: BeginDisposition::Started,
        active_captures,
        failed_captures: failures,
    };

    if let Ok(mut registry) = inner.registry.lock() {
        let current = registry.turns.remove(&key);
        match current {
            Some(TurnState::Beginning {
                generation: current_generation,
                fingerprint,
                ..
            }) if current_generation == generation => {
                registry.turns.insert(
                    key,
                    TurnState::Active(ActiveTurn {
                        begin_report: report,
                        fingerprint,
                        leases,
                    }),
                );
            }
            Some(other) => {
                registry.turns.insert(key, other);
            }
            None => {}
        }
    }
    // Notify even when the map was poisoned or ownership changed. In those
    // cases local leases drop here and preserve the adapter's RAII guarantee.
    notify.notify_waiters();
}

fn complete_finalization(
    inner: Arc<Inner>,
    driver: Arc<dyn CaptureDriver>,
    key: TerminalKey,
    generation: u64,
    mode: CaptureTerminalMode,
    leases: Vec<DynCaptureLease>,
    notify: Arc<Notify>,
) {
    let attempted_captures = u32::try_from(leases.len()).unwrap_or(u32::MAX);
    let mut summaries = Vec::with_capacity(leases.len());
    let mut failed_captures = 0_u32;
    for lease in leases {
        match catch_unwind(AssertUnwindSafe(|| driver.finalize(lease, mode))) {
            Ok(Ok(summary)) => summaries.push(summary),
            Ok(Err(_)) | Err(_) => failed_captures = failed_captures.saturating_add(1),
        }
    }
    let succeeded_captures = u32::try_from(summaries.len()).unwrap_or(u32::MAX);
    let report = Arc::new(FinalizationReport {
        generation,
        mode,
        attempted_captures,
        succeeded_captures,
        failed_captures,
        summaries,
    });

    if let Ok(mut registry) = inner.registry.lock() {
        let current = registry.turns.remove(&key);
        match current {
            Some(TurnState::Finalizing {
                generation: current_generation,
                begin_report,
                fingerprint,
                mode: current_mode,
                ..
            }) if current_generation == generation && current_mode == mode => {
                registry.turns.insert(
                    key.clone(),
                    TurnState::Finalized {
                        begin_report,
                        fingerprint,
                        report,
                    },
                );
                registry.finalized_order.push_back((key, generation));
            }
            Some(other) => {
                registry.turns.insert(key, other);
            }
            None => {}
        }
    }
    notify.notify_waiters();
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
struct CaptureV2Driver {
    service: Arc<dcc_infra::guarded_undo::capture_v2_service::CaptureV2Service>,
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
impl CaptureDriver for CaptureV2Driver {
    fn begin(&self, request: GuardedUndoCaptureRequest) -> Result<DynCaptureLease, DriverFailure> {
        use dcc_infra::guarded_undo::capture_v2_service::CaptureV2Request;

        self.service
            .begin(CaptureV2Request {
                snapshot_id: request.snapshot_id,
                session_id: request.session_id,
                turn_id: request.turn_id,
                workspace_id: request.workspace_id,
                workspace_absolute: request.workspace_absolute,
                expected_physical_root_id: request.expected_physical_root_id,
            })
            .map(|handle| Box::new(handle) as DynCaptureLease)
            .map_err(|_| DriverFailure)
    }

    fn finalize(
        &self,
        lease: DynCaptureLease,
        mode: CaptureTerminalMode,
    ) -> Result<GuardedUndoCaptureSummary, DriverFailure> {
        use dcc_infra::guarded_undo::capture_v2_service::CaptureHandle;

        let handle = lease
            .into_any()
            .downcast::<CaptureHandle>()
            .map_err(|_| DriverFailure)?;
        let summary = match mode {
            // Workspace, editor, Git, delegation, and delivery mutations now
            // share this runtime's physical-root coordinator. The capture
            // service still performs its complete result-edge revalidation;
            // only that reviewed path may persist an Eligible restore set.
            CaptureTerminalMode::Completed => self.service.finish(*handle),
            CaptureTerminalMode::Cancelled => self.service.cancel(*handle),
            CaptureTerminalMode::ProviderFailed => self.service.provider_failed(*handle),
        }
        .map_err(|_| DriverFailure)?;
        Ok(GuardedUndoCaptureSummary {
            state: summary.state.as_str().to_owned(),
            reason_code: summary.reason_code.map(|reason| reason.as_str().to_owned()),
            file_count: summary.file_count,
            artifact_bytes: summary.artifact_bytes,
        })
    }

    fn recover_all(&self, roots: Vec<PathBuf>) -> Result<(), DriverFailure> {
        self.service
            .recover_startup_all(roots.iter())
            .map_err(|_| DriverFailure)?;
        let _ = self.service.recover_guarded_undo_startup_all(roots.iter());
        Ok(())
    }

    fn prepare_guarded_undo(
        &self,
        snapshot_id: String,
        workspace_absolute: PathBuf,
    ) -> GuardedUndoPrepareResult {
        use dcc_infra::guarded_undo::restore_service::PrepareGuardedUndoResult;

        match self
            .service
            .prepare_guarded_undo(&snapshot_id, &workspace_absolute)
        {
            PrepareGuardedUndoResult::Ready(ready) => GuardedUndoPrepareResult::Ready {
                snapshot_id: ready.snapshot_id,
                preview_token: ready.preview_token,
                expires_at: ready.expires_at,
                file_count: ready.file_count,
                total_bytes: ready.total_bytes,
                files: ready
                    .files
                    .into_iter()
                    .map(|file| GuardedUndoPreview {
                        display_path: file.display_path,
                        size: file.size,
                        binary: file.binary,
                        preview: file.preview,
                    })
                    .collect(),
                unrelated_paths_are_not_targets: ready.unrelated_paths_are_not_targets,
            },
            PrepareGuardedUndoResult::Blocked {
                snapshot_id,
                reason_code,
            } => GuardedUndoPrepareResult::Blocked {
                snapshot_id,
                reason_code: reason_code.as_str().to_owned(),
            },
            PrepareGuardedUndoResult::Unavailable {
                snapshot_id,
                reason_code,
            } => GuardedUndoPrepareResult::Unavailable {
                snapshot_id,
                reason_code: reason_code.as_str().to_owned(),
            },
        }
    }

    fn execute_guarded_undo(
        &self,
        preview_token: String,
        confirmed: bool,
    ) -> GuardedUndoExecuteResult {
        use dcc_infra::guarded_undo::restore_service::ExecuteGuardedUndoResult;

        match self.service.execute_guarded_undo(&preview_token, confirmed) {
            ExecuteGuardedUndoResult::Completed { operation_id } => {
                GuardedUndoExecuteResult::Completed { operation_id }
            }
            ExecuteGuardedUndoResult::Blocked(reason_code) => GuardedUndoExecuteResult::Blocked {
                reason_code: reason_code.as_str().to_owned(),
            },
            ExecuteGuardedUndoResult::RolledBack { operation_id } => {
                GuardedUndoExecuteResult::RolledBack { operation_id }
            }
            ExecuteGuardedUndoResult::RecoveryRequired {
                operation_id,
                reason_code,
            } => GuardedUndoExecuteResult::RecoveryRequired {
                operation_id,
                reason_code: reason_code.as_str().to_owned(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    use std::process::Command;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Condvar,
    };

    use super::*;

    struct Gate {
        open: Mutex<bool>,
        changed: Condvar,
    }

    impl Gate {
        fn closed() -> Self {
            Self {
                open: Mutex::new(false),
                changed: Condvar::new(),
            }
        }

        fn wait(&self) {
            let mut open = self.open.lock().expect("gate");
            while !*open {
                open = self.changed.wait(open).expect("gate wait");
            }
        }

        fn release(&self) {
            *self.open.lock().expect("gate") = true;
            self.changed.notify_all();
        }
    }

    struct FakeLease {
        drops: Arc<AtomicUsize>,
    }

    impl Drop for FakeLease {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct FakeDriver {
        begin_gate: Option<Arc<Gate>>,
        finalize_gate: Option<Arc<Gate>>,
        recovery_gate: Option<Arc<Gate>>,
        begin_started: Arc<AtomicBool>,
        finalize_started: Arc<AtomicBool>,
        recovery_started: Arc<AtomicBool>,
        begin_calls: AtomicUsize,
        finalize_calls: AtomicUsize,
        recovery_calls: AtomicUsize,
        drops: Arc<AtomicUsize>,
        fail_finalize: bool,
        fail_recovery: AtomicBool,
    }

    impl FakeDriver {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                begin_gate: None,
                finalize_gate: None,
                recovery_gate: None,
                begin_started: Arc::new(AtomicBool::new(false)),
                finalize_started: Arc::new(AtomicBool::new(false)),
                recovery_started: Arc::new(AtomicBool::new(false)),
                begin_calls: AtomicUsize::new(0),
                finalize_calls: AtomicUsize::new(0),
                recovery_calls: AtomicUsize::new(0),
                drops: Arc::new(AtomicUsize::new(0)),
                fail_finalize: false,
                fail_recovery: AtomicBool::new(false),
            })
        }
    }

    impl CaptureDriver for FakeDriver {
        fn begin(
            &self,
            _request: GuardedUndoCaptureRequest,
        ) -> Result<DynCaptureLease, DriverFailure> {
            self.begin_calls.fetch_add(1, Ordering::SeqCst);
            self.begin_started.store(true, Ordering::SeqCst);
            if let Some(gate) = self.begin_gate.as_ref() {
                gate.wait();
            }
            Ok(Box::new(FakeLease {
                drops: Arc::clone(&self.drops),
            }))
        }

        fn finalize(
            &self,
            lease: DynCaptureLease,
            _mode: CaptureTerminalMode,
        ) -> Result<GuardedUndoCaptureSummary, DriverFailure> {
            self.finalize_calls.fetch_add(1, Ordering::SeqCst);
            self.finalize_started.store(true, Ordering::SeqCst);
            if let Some(gate) = self.finalize_gate.as_ref() {
                gate.wait();
            }
            drop(lease);
            if self.fail_finalize {
                return Err(DriverFailure);
            }
            Ok(GuardedUndoCaptureSummary {
                state: "eligible".to_owned(),
                reason_code: None,
                file_count: 1,
                artifact_bytes: 7,
            })
        }

        fn recover_all(&self, _roots: Vec<PathBuf>) -> Result<(), DriverFailure> {
            self.recovery_calls.fetch_add(1, Ordering::SeqCst);
            self.recovery_started.store(true, Ordering::SeqCst);
            if let Some(gate) = self.recovery_gate.as_ref() {
                gate.wait();
            }
            if self.fail_recovery.load(Ordering::SeqCst) {
                Err(DriverFailure)
            } else {
                Ok(())
            }
        }
    }

    fn key() -> TerminalKey {
        key_for(0)
    }

    fn request() -> GuardedUndoCaptureRequest {
        request_for(0)
    }

    fn key_for(index: usize) -> TerminalKey {
        TerminalKey::new(
            SessionId(format!("session-{index}")),
            TurnId(format!("turn-{index}")),
        )
    }

    fn request_for(index: usize) -> GuardedUndoCaptureRequest {
        GuardedUndoCaptureRequest {
            snapshot_id: format!("snapshot-{index}"),
            session_id: SessionId(format!("session-{index}")),
            turn_id: TurnId(format!("turn-{index}")),
            workspace_id: WorkspaceId(format!("workspace-{index}")),
            workspace_absolute: PathBuf::from(format!("/redacted-test-workspace-{index}")),
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            expected_physical_root_id: PhysicalRootId(vec![1, 1, index as u8]),
        }
    }

    fn request_in_default_turn(index: usize) -> GuardedUndoCaptureRequest {
        GuardedUndoCaptureRequest {
            session_id: key().session_id,
            turn_id: key().turn_id,
            ..request_for(index)
        }
    }

    async fn wait_until(mut predicate: impl FnMut() -> bool) {
        for _ in 0..1_000 {
            if predicate() {
                return;
            }
            // `spawn_blocking` startup is not guaranteed to be scheduled by a
            // bounded number of async yields on a current-thread runtime.
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        panic!("condition did not become true");
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    fn linked_git_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        fn git(root: &Path, args: &[&str]) {
            let output = Command::new("/usr/bin/git")
                .arg("-C")
                .arg(root)
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_TERMINAL_PROMPT", "0")
                .output()
                .expect("run fixture git");
            assert!(
                output.status.success(),
                "fixture git failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let temporary = tempfile::tempdir_in("/private/tmp").expect("git fixture");
        let main = temporary.path().join("main");
        let linked = temporary.path().join("linked");
        std::fs::create_dir(&main).expect("main worktree");
        git(&main, &["init", "--initial-branch=main"]);
        git(&main, &["config", "user.name", "DCC Test"]);
        git(&main, &["config", "user.email", "dcc-test@example.invalid"]);
        std::fs::write(main.join("tracked.txt"), b"baseline\n").expect("tracked fixture");
        git(&main, &["add", "tracked.txt"]);
        git(&main, &["commit", "-m", "baseline"]);
        git(
            &main,
            &[
                "worktree",
                "add",
                "-b",
                "linked-worktree",
                linked.to_str().expect("utf-8 linked path"),
            ],
        );
        let main = std::fs::canonicalize(main).expect("physical main");
        let linked = std::fs::canonicalize(linked).expect("physical linked");
        let main_authority = MacGitMutationAuthority::open(&main).expect("main Git authority");
        let linked_authority =
            MacGitMutationAuthority::open(&linked).expect("linked Git authority");
        assert_ne!(
            main_authority.worktree_root_id(),
            linked_authority.worktree_root_id()
        );
        assert_eq!(
            main_authority.common_dir_id(),
            linked_authority.common_dir_id()
        );
        (temporary, main, linked)
    }

    #[test]
    fn runtime_and_platform_contracts_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        fn assert_send<T: Send>() {}
        assert_send_sync::<GuardedUndoRuntime>();

        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        {
            assert_send_sync::<dcc_infra::guarded_undo::capture_v2_service::CaptureV2Service>();
            assert_send::<dcc_infra::guarded_undo::capture_v2_service::CaptureHandle>();
        }
        let _ = assert_send::<GuardedUndoRuntime>;
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[tokio::test(flavor = "current_thread")]
    async fn feature_off_workspace_mutation_does_no_io() {
        let runtime = GuardedUndoRuntime::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let nonexistent = PathBuf::from("relative/path/that/must/not/be-opened");
        let result = runtime
            .run_workspace_mutation(nonexistent.clone(), move |path| {
                observed.fetch_add(1, Ordering::SeqCst);
                assert_eq!(path, nonexistent);
                Ok::<_, ()>(41_u8)
            })
            .await;
        assert_eq!(result.unwrap(), 41);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[tokio::test(flavor = "current_thread")]
    async fn feature_off_blocking_mutation_uses_blocking_worker() {
        let runtime = GuardedUndoRuntime::new();
        let caller = std::thread::current().id();
        let result = runtime
            .run_workspace_mutation_blocking(PathBuf::from("relative/workspace"), move |_| {
                assert_ne!(std::thread::current().id(), caller);
                Ok::<_, ()>(())
            })
            .await;
        assert!(result.is_ok());
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[tokio::test(flavor = "current_thread")]
    async fn feature_off_git_mutations_preserve_no_io_and_executor_contracts() {
        let runtime = GuardedUndoRuntime::new();
        let nonexistent = PathBuf::from("relative/non-git/path/that-must-not-be-opened");
        let observed = nonexistent.clone();
        let direct = runtime
            .run_git_workspace_mutation(nonexistent.clone(), move |path| {
                assert_eq!(path, observed);
                Ok::<_, ()>(17_u8)
            })
            .await;
        assert_eq!(direct.unwrap(), 17);

        let caller = std::thread::current().id();
        let observed = nonexistent.clone();
        let blocking = runtime
            .run_git_workspace_mutation_blocking(nonexistent.clone(), move |path| {
                assert_eq!(path, observed);
                assert_ne!(std::thread::current().id(), caller);
                Ok::<_, ()>(23_u8)
            })
            .await;
        assert_eq!(blocking.unwrap(), 23);

        let secondary = PathBuf::from("relative/secondary/path/that-must-not-be-opened");
        let observed_primary = nonexistent.clone();
        let observed_secondary = secondary.clone();
        let pair = runtime
            .run_git_workspace_pair_mutation(
                nonexistent.clone(),
                secondary.clone(),
                move |primary, secondary| {
                    assert_eq!(primary, observed_primary);
                    assert_eq!(secondary, observed_secondary);
                    Ok::<_, ()>(29_u8)
                },
            )
            .await;
        assert_eq!(pair.unwrap(), 29);

        let caller = std::thread::current().id();
        let blocking_pair = runtime
            .run_git_workspace_pair_mutation_blocking(nonexistent, secondary, move |_, _| {
                assert_ne!(std::thread::current().id(), caller);
                Ok::<_, ()>(31_u8)
            })
            .await;
        assert_eq!(blocking_pair.unwrap(), 31);
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn git_mutation_fails_closed_for_a_non_git_workspace() {
        let temporary = tempfile::tempdir_in("/private/tmp").unwrap();
        let workspace = std::fs::canonicalize(temporary.path()).unwrap();
        let runtime = GuardedUndoRuntime::new();
        let called = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&called);
        let result = runtime
            .run_git_workspace_mutation(workspace, move |_| {
                observed.store(true, Ordering::SeqCst);
                Ok::<_, ()>(())
            })
            .await;
        assert!(matches!(
            result,
            Err(WorkspaceMutationRunError::PhysicalRootUnavailable)
        ));
        assert!(!called.load(Ordering::SeqCst));
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn linked_worktree_git_mutations_contend_on_the_common_directory() {
        let (_temporary, main, linked) = linked_git_fixture();
        let coordinator = Arc::new(WorkspaceMutationCoordinator::new());
        let runtime = GuardedUndoRuntime::new_with_coordinator(coordinator);
        let gate = Arc::new(Gate::closed());
        let started = Arc::new(AtomicBool::new(false));

        let first_runtime = runtime.clone();
        let first_gate = Arc::clone(&gate);
        let first_started = Arc::clone(&started);
        let first = tokio::spawn(async move {
            first_runtime
                .run_git_workspace_mutation(main, move |_| {
                    first_started.store(true, Ordering::SeqCst);
                    first_gate.wait();
                    Ok::<_, ()>(())
                })
                .await
        });
        wait_until(|| started.load(Ordering::SeqCst)).await;
        assert!(matches!(
            runtime
                .run_git_workspace_mutation(linked.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));
        gate.release();
        first.await.unwrap().unwrap();
        runtime
            .run_git_workspace_mutation(linked, |_| Ok::<_, ()>(()))
            .await
            .unwrap();
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_git_waiter_retains_worktree_and_common_directory_leases() {
        let (_temporary, main, linked) = linked_git_fixture();
        let coordinator = Arc::new(WorkspaceMutationCoordinator::new());
        let runtime = GuardedUndoRuntime::new_with_coordinator(coordinator);
        let gate = Arc::new(Gate::closed());
        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));

        let worker_runtime = runtime.clone();
        let worker_main = main.clone();
        let worker_gate = Arc::clone(&gate);
        let worker_started = Arc::clone(&started);
        let worker_finished = Arc::clone(&finished);
        let waiter = tokio::spawn(async move {
            worker_runtime
                .run_git_workspace_mutation(worker_main, move |_| {
                    worker_started.store(true, Ordering::SeqCst);
                    worker_gate.wait();
                    worker_finished.store(true, Ordering::SeqCst);
                    Ok::<_, ()>(())
                })
                .await
        });
        wait_until(|| started.load(Ordering::SeqCst)).await;
        waiter.abort();

        // The ordinary runner observes the retained worktree root, while the
        // linked Git runner proves that the shared common-dir is also retained.
        assert!(matches!(
            runtime
                .run_workspace_mutation(main.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));
        assert!(matches!(
            runtime
                .run_git_workspace_mutation(linked.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));

        gate.release();
        wait_until(|| finished.load(Ordering::SeqCst)).await;
        runtime
            .run_workspace_mutation(main, |_| Ok::<_, ()>(()))
            .await
            .unwrap();
        runtime
            .run_git_workspace_mutation(linked, |_| Ok::<_, ()>(()))
            .await
            .unwrap();
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn pair_git_mutations_reject_a_foreign_repository() {
        let (_temporary, main, linked) = linked_git_fixture();
        let foreign_temporary = tempfile::tempdir_in("/private/tmp").unwrap();
        let foreign = foreign_temporary.path().join("foreign");
        std::fs::create_dir(&foreign).unwrap();
        let output = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(&foreign)
            .args(["init", "--initial-branch=main"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success());
        let foreign = std::fs::canonicalize(foreign).unwrap();
        let runtime = GuardedUndoRuntime::new();
        let called = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&called);
        let result = runtime
            .run_git_workspace_pair_mutation(main, foreign, move |_, _| {
                observed.store(true, Ordering::SeqCst);
                Ok::<_, ()>(())
            })
            .await;
        assert!(matches!(
            result,
            Err(WorkspaceMutationRunError::PhysicalRootUnavailable)
        ));
        assert!(!called.load(Ordering::SeqCst));

        // Keep the fixture's linked worktree used by the next pair test
        // compiler-visible on all supported macOS configurations.
        let _ = linked;
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn pair_git_mutations_contend_on_both_worktrees_and_common_directory() {
        let (_temporary, main, linked) = linked_git_fixture();
        let coordinator = Arc::new(WorkspaceMutationCoordinator::new());
        let runtime = GuardedUndoRuntime::new_with_coordinator(coordinator);
        let gate = Arc::new(Gate::closed());
        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));

        let worker_runtime = runtime.clone();
        let worker_main = main.clone();
        let worker_linked = linked.clone();
        let worker_gate = Arc::clone(&gate);
        let worker_started = Arc::clone(&started);
        let worker_finished = Arc::clone(&finished);
        let waiter = tokio::spawn(async move {
            worker_runtime
                .run_git_workspace_pair_mutation(worker_main, worker_linked, move |_, _| {
                    worker_started.store(true, Ordering::SeqCst);
                    worker_gate.wait();
                    worker_finished.store(true, Ordering::SeqCst);
                    Ok::<_, ()>(())
                })
                .await
        });
        wait_until(|| started.load(Ordering::SeqCst)).await;

        // The pair owns both physical worktree roots and the shared common
        // directory. Any single-root or reversed pair operation must wait.
        assert!(matches!(
            runtime
                .run_git_workspace_mutation(linked.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));
        assert!(matches!(
            runtime
                .run_git_workspace_pair_mutation(linked.clone(), main.clone(), |_, _| {
                    Ok::<_, ()>(())
                })
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));

        waiter.abort();
        // Cancellation only drops the waiter. The spawn_blocking closure and
        // all three physical leases remain alive until its gate opens.
        assert!(!finished.load(Ordering::SeqCst));
        assert!(matches!(
            runtime
                .run_git_workspace_mutation(main.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));
        assert!(matches!(
            runtime
                .run_git_workspace_mutation(linked.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));

        gate.release();
        wait_until(|| finished.load(Ordering::SeqCst)).await;
        runtime
            .run_git_workspace_pair_mutation(main, linked, |_, _| Ok::<_, ()>(()))
            .await
            .unwrap();
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn shared_capture_edge_blocks_mutation_and_physical_aliases_contend() {
        let temporary = tempfile::tempdir_in("/private/tmp").unwrap();
        let workspace = std::fs::canonicalize(temporary.path()).unwrap();
        let alias = PathBuf::from(format!("{}/", workspace.display()));
        let coordinator = Arc::new(WorkspaceMutationCoordinator::new());
        let runtime = GuardedUndoRuntime::new_with_coordinator(Arc::clone(&coordinator));
        let physical = MacWorkspaceRoot::open_absolute(&workspace).unwrap();
        let root_id = physical.physical_root_id();

        let edge = coordinator.try_acquire_capture_edge(&root_id).unwrap();
        assert!(matches!(
            runtime
                .run_workspace_mutation(workspace.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));
        drop(edge);

        let gate = Arc::new(Gate::closed());
        let started = Arc::new(AtomicBool::new(false));
        let first_runtime = runtime.clone();
        let first_gate = Arc::clone(&gate);
        let first_started = Arc::clone(&started);
        let first = tokio::spawn(async move {
            first_runtime
                .run_workspace_mutation(workspace, move |_| {
                    first_started.store(true, Ordering::SeqCst);
                    first_gate.wait();
                    Ok::<_, ()>(())
                })
                .await
        });
        wait_until(|| started.load(Ordering::SeqCst)).await;
        assert_eq!(coordinator.generation(&root_id).unwrap(), 1);
        assert!(matches!(
            runtime
                .run_workspace_mutation(alias.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));
        gate.release();
        first.await.unwrap().unwrap();
        runtime
            .run_workspace_mutation(alias, |_| Ok::<_, ()>(()))
            .await
            .unwrap();
        assert_eq!(coordinator.generation(&root_id).unwrap(), 2);
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_waiter_does_not_release_running_mutation() {
        let temporary = tempfile::tempdir_in("/private/tmp").unwrap();
        let workspace = std::fs::canonicalize(temporary.path()).unwrap();
        let coordinator = Arc::new(WorkspaceMutationCoordinator::new());
        let runtime = GuardedUndoRuntime::new_with_coordinator(Arc::clone(&coordinator));
        let gate = Arc::new(Gate::closed());
        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));

        let worker_runtime = runtime.clone();
        let worker_workspace = workspace.clone();
        let worker_gate = Arc::clone(&gate);
        let worker_started = Arc::clone(&started);
        let worker_finished = Arc::clone(&finished);
        let waiter = tokio::spawn(async move {
            worker_runtime
                .run_workspace_mutation(worker_workspace, move |_| {
                    worker_started.store(true, Ordering::SeqCst);
                    worker_gate.wait();
                    worker_finished.store(true, Ordering::SeqCst);
                    Ok::<_, ()>(())
                })
                .await
        });
        wait_until(|| started.load(Ordering::SeqCst)).await;
        waiter.abort();

        assert!(matches!(
            runtime
                .run_workspace_mutation(workspace.clone(), |_| Ok::<_, ()>(()))
                .await,
            Err(WorkspaceMutationRunError::Busy)
        ));
        gate.release();
        wait_until(|| finished.load(Ordering::SeqCst)).await;
        runtime
            .run_workspace_mutation(workspace, |_| Ok::<_, ()>(()))
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unconfigured_runtime_is_feature_off_noop() {
        let runtime = GuardedUndoRuntime::new();
        let begin = runtime.begin_turn(key(), vec![request()]).await.unwrap();
        assert_eq!(begin.disposition, BeginDisposition::Disabled);
        assert_eq!(runtime.phase(&key()), None);
        assert_eq!(
            runtime
                .finalize_turn(&key(), CaptureTerminalMode::Completed)
                .await
                .unwrap(),
            FinalizeTurnOutcome::NotTracked
        );
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[tokio::test(flavor = "current_thread")]
    async fn unsupported_configuration_and_recovery_are_noops() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = SqliteSessionRepo::open(temporary.path().join("sessions.sqlite3")).unwrap();
        let runtime = GuardedUndoRuntime::new();
        assert_eq!(
            runtime
                .configure_capture_v2_scoped(temporary.path().join("app-data"), repo, [7; 32],)
                .await
                .unwrap(),
            ConfigureOutcome::Disabled
        );
        assert_eq!(
            runtime.recovery_all(Vec::new()).await.unwrap(),
            RecoveryOutcome::Disabled
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_configuration_builds_exactly_one_driver_and_rejects_mismatch() {
        let runtime = GuardedUndoRuntime::new();
        let fingerprint = configuration_fingerprint(&[1; 32]);
        let gate = Arc::new(Gate::closed());
        let started = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut waiters = Vec::new();
        for _ in 0..32 {
            let runtime = runtime.clone();
            let gate = Arc::clone(&gate);
            let started = Arc::clone(&started);
            let calls = Arc::clone(&calls);
            waiters.push(tokio::spawn(async move {
                runtime
                    .configure_with_factory(
                        fingerprint,
                        Box::new(move || {
                            calls.fetch_add(1, Ordering::SeqCst);
                            started.store(true, Ordering::SeqCst);
                            gate.wait();
                            Ok(FakeDriver::new() as Arc<dyn CaptureDriver>)
                        }),
                    )
                    .await
            }));
        }
        wait_until(|| started.load(Ordering::SeqCst)).await;
        gate.release();
        let mut configured = 0;
        for waiter in waiters {
            match waiter.await.unwrap().unwrap() {
                ConfigureOutcome::Configured => configured += 1,
                ConfigureOutcome::AlreadyConfigured => {}
                unexpected => panic!("unexpected configure outcome: {unexpected:?}"),
            }
        }
        assert_eq!(configured, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let divergent = configuration_fingerprint(&[2; 32]);
        assert_eq!(
            runtime
                .configure_with_factory(
                    divergent,
                    Box::new(|| Ok(FakeDriver::new() as Arc<dyn CaptureDriver>)),
                )
                .await,
            Err(GuardedUndoRuntimeError::ConfigurationMismatch)
        );
        assert!(
            !format!("{:?}", GuardedUndoRuntimeError::ConfigurationMismatch)
                .contains("other-redacted")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_configuration_waiter_does_not_duplicate_worker_and_failure_retries() {
        let runtime = GuardedUndoRuntime::new();
        let fingerprint = configuration_fingerprint(&[1; 32]);
        let gate = Arc::new(Gate::closed());
        let started = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let waiter = tokio::spawn({
            let runtime = runtime.clone();
            let gate = Arc::clone(&gate);
            let started = Arc::clone(&started);
            let calls = Arc::clone(&calls);
            async move {
                runtime
                    .configure_with_factory(
                        fingerprint,
                        Box::new(move || {
                            calls.fetch_add(1, Ordering::SeqCst);
                            started.store(true, Ordering::SeqCst);
                            gate.wait();
                            Err(DriverFailure)
                        }),
                    )
                    .await
            }
        });
        wait_until(|| started.load(Ordering::SeqCst)).await;
        waiter.abort();
        gate.release();
        let retried = runtime
            .configure_with_factory(
                fingerprint,
                Box::new({
                    let calls = Arc::clone(&calls);
                    move || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(FakeDriver::new() as Arc<dyn CaptureDriver>)
                    }
                }),
            )
            .await
            .unwrap();
        assert!(matches!(
            retried,
            ConfigureOutcome::Configured | ConfigureOutcome::Unavailable
        ));
        if retried == ConfigureOutcome::Unavailable {
            assert_eq!(
                runtime
                    .configure_with_factory(
                        fingerprint,
                        Box::new({
                            let calls = Arc::clone(&calls);
                            move || {
                                calls.fetch_add(1, Ordering::SeqCst);
                                Ok(FakeDriver::new() as Arc<dyn CaptureDriver>)
                            }
                        }),
                    )
                    .await
                    .unwrap(),
                ConfigureOutcome::Configured
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recovery_is_single_flight_order_independent_and_cancel_safe() {
        let gate = Arc::new(Gate::closed());
        let driver = Arc::new(FakeDriver {
            begin_gate: None,
            finalize_gate: None,
            recovery_gate: Some(Arc::clone(&gate)),
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            recovery_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            recovery_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
            fail_recovery: AtomicBool::new(false),
        });
        let runtime = GuardedUndoRuntime::with_unrecovered_test_driver(driver.clone());
        let roots = vec![PathBuf::from("/root-a"), PathBuf::from("/root-b")];
        let leader = tokio::spawn({
            let runtime = runtime.clone();
            let roots = roots.clone();
            async move { runtime.recovery_all_with_driver(roots).await }
        });
        wait_until(|| driver.recovery_started.load(Ordering::SeqCst)).await;
        assert_eq!(
            runtime
                .recovery_all_with_driver(vec![PathBuf::from("/different-root")])
                .await,
            Err(GuardedUndoRuntimeError::RecoveryRootsMismatch)
        );
        assert_eq!(driver.recovery_calls.load(Ordering::SeqCst), 1);
        leader.abort();
        let gated = runtime.begin_turn(key(), vec![request()]).await.unwrap();
        assert_eq!(gated.disposition, BeginDisposition::Disabled);
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 0);
        let follower = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .recovery_all_with_driver(vec![
                        PathBuf::from("/root-b"),
                        PathBuf::from("/root-a"),
                    ])
                    .await
            }
        });
        gate.release();
        assert_eq!(
            follower.await.unwrap().unwrap(),
            RecoveryOutcome::AlreadyRecovered
        );
        assert_eq!(driver.recovery_calls.load(Ordering::SeqCst), 1);
        let admitted = runtime.begin_turn(key(), vec![request()]).await.unwrap();
        assert_eq!(admitted.disposition, BeginDisposition::Started);
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            runtime
                .recovery_all_with_driver(vec![PathBuf::from("/different-root")])
                .await,
            Ok(RecoveryOutcome::AlreadyRecovered)
        );
        assert_eq!(driver.recovery_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_recovery_is_retryable_and_invalid_sets_never_call_driver() {
        let driver = FakeDriver::new();
        driver.fail_recovery.store(true, Ordering::SeqCst);
        let runtime = GuardedUndoRuntime::with_unrecovered_test_driver(driver.clone());
        assert_eq!(
            runtime
                .recovery_all_with_driver(vec![PathBuf::from("/root-a")])
                .await
                .unwrap(),
            RecoveryOutcome::Unavailable
        );
        let gated = runtime.begin_turn(key(), vec![request()]).await.unwrap();
        assert_eq!(gated.disposition, BeginDisposition::Disabled);
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 0);
        driver.fail_recovery.store(false, Ordering::SeqCst);
        assert_eq!(
            runtime
                .recovery_all_with_driver(vec![PathBuf::from("/corrected-root")])
                .await
                .unwrap(),
            RecoveryOutcome::Recovered
        );
        assert_eq!(driver.recovery_calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            runtime
                .begin_turn(key(), vec![request()])
                .await
                .unwrap()
                .disposition,
            BeginDisposition::Started
        );
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 1);

        let other = GuardedUndoRuntime::with_unrecovered_test_driver(FakeDriver::new());
        assert_eq!(
            other.recovery_all_with_driver(Vec::new()).await,
            Err(GuardedUndoRuntimeError::EmptyRecoverySet)
        );
        assert_eq!(
            other
                .recovery_all_with_driver(vec![PathBuf::from("/same"), PathBuf::from("/same")])
                .await,
            Err(GuardedUndoRuntimeError::DuplicateRecoveryRoot)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recovery_allows_a_bounded_global_root_set_larger_than_turn_capture_limit() {
        let driver = FakeDriver::new();
        let runtime = GuardedUndoRuntime::with_unrecovered_test_driver(driver.clone());
        let roots = (0..=MAX_CAPTURE_ROOTS_PER_TURN)
            .map(|index| PathBuf::from(format!("/recovery-root-{index}")))
            .collect::<Vec<_>>();

        assert_eq!(
            runtime.recovery_all_with_driver(roots).await.unwrap(),
            RecoveryOutcome::Recovered
        );
        assert_eq!(driver.recovery_calls.load(Ordering::SeqCst), 1);

        let oversized = (0..=MAX_RECOVERY_ROOTS)
            .map(|index| PathBuf::from(format!("/oversized-recovery-root-{index}")))
            .collect::<Vec<_>>();
        assert_eq!(
            runtime.recovery_all_with_driver(oversized).await,
            Err(GuardedUndoRuntimeError::RecoveryRootLimitExceeded)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_begin_callers_share_one_blocking_begin() {
        let gate = Arc::new(Gate::closed());
        let driver = Arc::new(FakeDriver {
            begin_gate: Some(Arc::clone(&gate)),
            finalize_gate: None,
            recovery_gate: None,
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            recovery_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            recovery_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
            fail_recovery: AtomicBool::new(false),
        });
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());
        let mut waiters = Vec::new();
        for _ in 0..32 {
            let runtime = runtime.clone();
            waiters.push(tokio::spawn(async move {
                runtime.begin_turn(key(), vec![request()]).await.unwrap()
            }));
        }
        wait_until(|| driver.begin_started.load(Ordering::SeqCst)).await;
        gate.release();
        for waiter in waiters {
            let report = waiter.await.unwrap();
            assert_eq!(report.active_captures, 1);
        }
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replay_identity_rejects_snapshot_or_path_changes() {
        let gate = Arc::new(Gate::closed());
        let driver = Arc::new(FakeDriver {
            begin_gate: Some(Arc::clone(&gate)),
            finalize_gate: None,
            recovery_gate: None,
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            recovery_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            recovery_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
            fail_recovery: AtomicBool::new(false),
        });
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());
        let first = request_in_default_turn(1);
        let leader = tokio::spawn({
            let runtime = runtime.clone();
            let first = first.clone();
            async move { runtime.begin_turn(key(), vec![first]).await }
        });
        wait_until(|| driver.begin_started.load(Ordering::SeqCst)).await;
        let replay = tokio::spawn({
            let runtime = runtime.clone();
            let first = first.clone();
            async move { runtime.begin_turn(key(), vec![first]).await }
        });
        gate.release();
        assert_eq!(
            leader.await.unwrap().unwrap().disposition,
            BeginDisposition::Started
        );
        assert_eq!(
            replay.await.unwrap().unwrap().disposition,
            BeginDisposition::Replayed
        );
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 1);

        let mut path_changed = first.clone();
        path_changed.workspace_absolute = PathBuf::from("/different-redacted-workspace");
        assert_eq!(
            runtime.begin_turn(key(), vec![path_changed]).await,
            Err(GuardedUndoRuntimeError::ReplayRequestMismatch)
        );
        let mut set_changed = first;
        set_changed.snapshot_id = "different-snapshot".to_owned();
        assert_eq!(
            runtime.begin_turn(key(), vec![set_changed]).await,
            Err(GuardedUndoRuntimeError::ReplayRequestMismatch)
        );
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn mismatched_attribution_never_reaches_driver() {
        let driver = FakeDriver::new();
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());
        let mut mismatched = request();
        mismatched.turn_id = TurnId("different-turn".to_owned());
        assert_eq!(
            runtime.begin_turn(key(), vec![mismatched]).await,
            Err(GuardedUndoRuntimeError::InvalidAttribution)
        );
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 0);
        assert_eq!(runtime.phase(&key()), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_capture_sets_never_reach_driver() {
        let driver = FakeDriver::new();
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());

        assert_eq!(
            runtime.begin_turn(key(), Vec::new()).await,
            Err(GuardedUndoRuntimeError::EmptyCaptureSet)
        );

        let overflow = (0..=MAX_CAPTURE_ROOTS_PER_TURN)
            .map(|index| GuardedUndoCaptureRequest {
                session_id: key().session_id,
                turn_id: key().turn_id,
                ..request_for(index)
            })
            .collect();
        assert_eq!(
            runtime.begin_turn(key(), overflow).await,
            Err(GuardedUndoRuntimeError::CaptureRootLimitExceeded)
        );

        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 0);
        assert_eq!(runtime.phase(&key()), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_begin_waiter_does_not_cancel_blocking_begin_or_drop_lease() {
        let gate = Arc::new(Gate::closed());
        let driver = Arc::new(FakeDriver {
            begin_gate: Some(Arc::clone(&gate)),
            finalize_gate: None,
            recovery_gate: None,
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            recovery_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            recovery_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
            fail_recovery: AtomicBool::new(false),
        });
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());
        let waiter = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.begin_turn(key(), vec![request()]).await }
        });
        wait_until(|| driver.begin_started.load(Ordering::SeqCst)).await;
        waiter.abort();
        gate.release();
        wait_until(|| runtime.phase(&key()) == Some("active")).await;
        assert_eq!(driver.drops.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_finalization_waiter_leaves_finalizing_until_worker_finishes() {
        let gate = Arc::new(Gate::closed());
        let driver = Arc::new(FakeDriver {
            begin_gate: None,
            finalize_gate: Some(Arc::clone(&gate)),
            recovery_gate: None,
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            recovery_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            recovery_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
            fail_recovery: AtomicBool::new(false),
        });
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());
        runtime.begin_turn(key(), vec![request()]).await.unwrap();
        let waiter = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .finalize_turn(&key(), CaptureTerminalMode::Completed)
                    .await
            }
        });
        wait_until(|| driver.finalize_started.load(Ordering::SeqCst)).await;
        waiter.abort();
        assert_eq!(runtime.phase(&key()), Some("finalizing"));
        gate.release();
        wait_until(|| runtime.phase(&key()) == Some("finalized")).await;
        assert_eq!(driver.finalize_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_finalizers_share_one_blocking_finalization() {
        let driver = FakeDriver::new();
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());
        runtime.begin_turn(key(), vec![request()]).await.unwrap();
        let mut waiters = Vec::new();
        for _ in 0..32 {
            let runtime = runtime.clone();
            waiters.push(tokio::spawn(async move {
                runtime
                    .finalize_turn(&key(), CaptureTerminalMode::Completed)
                    .await
                    .unwrap()
            }));
        }
        for waiter in waiters {
            assert!(matches!(
                waiter.await.unwrap(),
                FinalizeTurnOutcome::Finalized(_)
            ));
        }
        assert_eq!(driver.finalize_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn capture_error_is_reported_without_failing_lifecycle() {
        let driver = Arc::new(FakeDriver {
            begin_gate: None,
            finalize_gate: None,
            recovery_gate: None,
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            recovery_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            recovery_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: true,
            fail_recovery: AtomicBool::new(false),
        });
        let runtime = GuardedUndoRuntime::with_test_driver(driver);
        runtime.begin_turn(key(), vec![request()]).await.unwrap();
        let outcome = runtime
            .finalize_turn(&key(), CaptureTerminalMode::Completed)
            .await
            .expect("coordination remains successful");
        let FinalizeTurnOutcome::Finalized(report) = outcome else {
            panic!("expected finalized report");
        };
        assert_eq!(report.attempted_captures, 1);
        assert_eq!(report.succeeded_captures, 0);
        assert_eq!(report.failed_captures, 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dropping_runtime_drops_active_capture_lease() {
        let driver = FakeDriver::new();
        let drops = Arc::clone(&driver.drops);
        let runtime = GuardedUndoRuntime::with_test_driver(driver);
        runtime.begin_turn(key(), vec![request()]).await.unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(runtime);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_generation_cannot_forget_finalized_successor() {
        let driver = FakeDriver::new();
        let runtime = GuardedUndoRuntime::with_test_driver(driver);
        let first = runtime.begin_turn(key(), vec![request()]).await.unwrap();
        let first_generation = first.generation.unwrap();
        let first_final = runtime
            .finalize_turn(&key(), CaptureTerminalMode::Completed)
            .await
            .unwrap();
        let FinalizeTurnOutcome::Finalized(first_report) = first_final else {
            panic!("first finalization");
        };
        assert!(runtime
            .forget_finalized_after_terminal_append(&key(), &first_report)
            .unwrap());

        let second = runtime.begin_turn(key(), vec![request()]).await.unwrap();
        assert_ne!(second.generation, Some(first_generation));
        runtime
            .finalize_turn(&key(), CaptureTerminalMode::Cancelled)
            .await
            .unwrap();
        assert!(!runtime
            .forget_finalized_after_terminal_append(&key(), &first_report)
            .unwrap());
        assert_eq!(runtime.phase(&key()), Some("finalized"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn capacity_evicts_only_finalized_and_stale_report_cannot_erase_successor() {
        let driver = FakeDriver::new();
        let runtime = GuardedUndoRuntime::with_test_driver(driver);
        let mut oldest_report = None;

        for index in 0..MAX_TRACKED_TURNS {
            let turn_key = key_for(index);
            runtime
                .begin_turn(turn_key.clone(), vec![request_for(index)])
                .await
                .unwrap();
            let outcome = runtime
                .finalize_turn(&turn_key, CaptureTerminalMode::Completed)
                .await
                .unwrap();
            let FinalizeTurnOutcome::Finalized(report) = outcome else {
                panic!("turn must finalize");
            };
            if index == 0 {
                oldest_report = Some(report);
            }
        }
        assert_eq!(runtime.tracked_count(), MAX_TRACKED_TURNS);

        let extra_index = MAX_TRACKED_TURNS;
        runtime
            .begin_turn(key_for(extra_index), vec![request_for(extra_index)])
            .await
            .unwrap();
        assert_eq!(runtime.phase(&key_for(0)), None);
        assert_eq!(runtime.tracked_count(), MAX_TRACKED_TURNS);

        runtime
            .begin_turn(key_for(0), vec![request_for(0)])
            .await
            .unwrap();
        let stale = oldest_report.expect("oldest finalized report");
        assert!(!runtime
            .forget_finalized_after_terminal_append(&key_for(0), &stale)
            .unwrap());
        assert_eq!(runtime.phase(&key_for(0)), Some("active"));
        assert_eq!(runtime.phase(&key_for(extra_index)), Some("active"));
        assert_eq!(runtime.tracked_count(), MAX_TRACKED_TURNS);
    }
}
