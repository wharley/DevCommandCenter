//! Process-wide, cancellation-safe ownership of guarded-undo capture handles.
//!
//! This module deliberately does not start or terminalize provider turns. It
//! only provides the lifecycle primitive that those flows will call after a
//! separately reviewed integration.

#![allow(dead_code)] // Foundation APIs are wired by the next lifecycle slice.

use std::{
    any::Any,
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use dcc_core::domain::{
    session::{SessionId, TurnId},
    workspace::WorkspaceId,
};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use crate::terminal_arbiter::TerminalKey;

const MAX_TRACKED_TURNS: usize = 1_024;
const MAX_CAPTURE_ROOTS_PER_TURN: usize = 16;

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
    DuplicateSnapshot,
    DuplicateWorkspace,
    EmptyCaptureSet,
    GenerationExhausted,
    InvalidAttribution,
    Poisoned,
    ReplayRequestMismatch,
    AlreadyConfigured,
}

impl fmt::Display for GuardedUndoRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CapacityExhausted => "guarded undo runtime capacity exhausted",
            Self::CaptureRootLimitExceeded => "guarded undo capture root limit exceeded",
            Self::DuplicateSnapshot => "guarded undo capture has a duplicate snapshot",
            Self::DuplicateWorkspace => "guarded undo capture has a duplicate workspace",
            Self::EmptyCaptureSet => "guarded undo capture set is empty",
            Self::GenerationExhausted => "guarded undo runtime generation exhausted",
            Self::InvalidAttribution => "guarded undo capture attribution is invalid",
            Self::Poisoned => "guarded undo runtime unavailable",
            Self::ReplayRequestMismatch => "guarded undo capture replay does not match",
            Self::AlreadyConfigured => "guarded undo runtime already configured",
        })
    }
}

impl std::error::Error for GuardedUndoRuntimeError {}

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
    registry: Mutex<TurnRegistry>,
    next_generation: AtomicU64,
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
        Self {
            inner: Arc::new(Inner {
                driver: Mutex::new(None),
                registry: Mutex::new(TurnRegistry::default()),
                next_generation: AtomicU64::new(0),
            }),
        }
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) fn install_capture_v2_service(
        &self,
        service: Arc<dcc_infra::guarded_undo::capture_v2_service::CaptureV2Service>,
    ) -> Result<(), GuardedUndoRuntimeError> {
        self.install_driver(Arc::new(CaptureV2Driver { service }))
    }

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

    fn allocate_generation(&self) -> Result<u64, GuardedUndoRuntimeError> {
        self.inner
            .next_generation
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
}

#[cfg(test)]
mod tests {
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
        begin_started: Arc<AtomicBool>,
        finalize_started: Arc<AtomicBool>,
        begin_calls: AtomicUsize,
        finalize_calls: AtomicUsize,
        drops: Arc<AtomicUsize>,
        fail_finalize: bool,
    }

    impl FakeDriver {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                begin_gate: None,
                finalize_gate: None,
                begin_started: Arc::new(AtomicBool::new(false)),
                finalize_started: Arc::new(AtomicBool::new(false)),
                begin_calls: AtomicUsize::new(0),
                finalize_calls: AtomicUsize::new(0),
                drops: Arc::new(AtomicUsize::new(0)),
                fail_finalize: false,
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
            tokio::task::yield_now().await;
        }
        panic!("condition did not become true");
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

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_begin_callers_share_one_blocking_begin() {
        let gate = Arc::new(Gate::closed());
        let driver = Arc::new(FakeDriver {
            begin_gate: Some(Arc::clone(&gate)),
            finalize_gate: None,
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
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
    async fn replay_identity_is_order_independent_and_rejects_set_or_path_changes() {
        let gate = Arc::new(Gate::closed());
        let driver = Arc::new(FakeDriver {
            begin_gate: Some(Arc::clone(&gate)),
            finalize_gate: None,
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
        });
        let runtime = GuardedUndoRuntime::with_test_driver(driver.clone());
        let first = request_in_default_turn(1);
        let second = request_in_default_turn(2);
        let leader = tokio::spawn({
            let runtime = runtime.clone();
            let first = first.clone();
            let second = second.clone();
            async move { runtime.begin_turn(key(), vec![first, second]).await }
        });
        wait_until(|| driver.begin_started.load(Ordering::SeqCst)).await;
        let replay = tokio::spawn({
            let runtime = runtime.clone();
            let first = first.clone();
            let second = second.clone();
            async move { runtime.begin_turn(key(), vec![second, first]).await }
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
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 2);

        let mut path_changed = first.clone();
        path_changed.workspace_absolute = PathBuf::from("/different-redacted-workspace");
        assert_eq!(
            runtime
                .begin_turn(key(), vec![path_changed, second.clone()])
                .await,
            Err(GuardedUndoRuntimeError::ReplayRequestMismatch)
        );
        let mut set_changed = first;
        set_changed.snapshot_id = "different-snapshot".to_owned();
        assert_eq!(
            runtime.begin_turn(key(), vec![set_changed, second]).await,
            Err(GuardedUndoRuntimeError::ReplayRequestMismatch)
        );
        assert_eq!(driver.begin_calls.load(Ordering::SeqCst), 2);
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

        let first = request();
        let mut duplicate_snapshot = request_for(1);
        duplicate_snapshot.session_id = key().session_id;
        duplicate_snapshot.turn_id = key().turn_id;
        duplicate_snapshot.snapshot_id = first.snapshot_id.clone();
        assert_eq!(
            runtime
                .begin_turn(key(), vec![first.clone(), duplicate_snapshot])
                .await,
            Err(GuardedUndoRuntimeError::DuplicateSnapshot)
        );

        let mut duplicate_workspace = request_for(2);
        duplicate_workspace.session_id = key().session_id;
        duplicate_workspace.turn_id = key().turn_id;
        duplicate_workspace.workspace_id = first.workspace_id.clone();
        assert_eq!(
            runtime
                .begin_turn(key(), vec![first, duplicate_workspace])
                .await,
            Err(GuardedUndoRuntimeError::DuplicateWorkspace)
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
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
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
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: false,
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
            begin_started: Arc::new(AtomicBool::new(false)),
            finalize_started: Arc::new(AtomicBool::new(false)),
            begin_calls: AtomicUsize::new(0),
            finalize_calls: AtomicUsize::new(0),
            drops: Arc::new(AtomicUsize::new(0)),
            fail_finalize: true,
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
