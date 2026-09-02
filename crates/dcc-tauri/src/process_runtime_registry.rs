//! Process-wide ownership of runtime coordination primitives.
//!
//! Scope keys contain only physical SQLite-file and app-data-directory
//! identities. Paths are used transiently after the consumer creates/migrates
//! those resources and are never retained or formatted by this module.
//!
//! Identity is not authorization. Owner/type/link/mode checks below are only
//! minimal stability preconditions for process-runtime coalescing. This module
//! does not inspect ACLs, xattrs, data privacy, or Guarded Undo storage policy;
//! platform/store adapters remain responsible for those stronger guarantees.
//! Concurrent or hostile same-user path replacement is explicitly outside
//! this cooperative in-process contract; this registry is not a filesystem
//! authorization or security boundary.

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

use crate::guarded_undo_runtime::ConfigureOutcome;
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
use crate::guarded_undo_runtime::GuardedUndoRuntimeError;
use crate::guarded_undo_runtime::{GuardedUndoRuntime, WorkspaceMutationRunError};
use crate::state::AuthorizedWorkspaceMutation;
use crate::terminal_arbiter::TerminalArbiter;
use dcc_core::Result as CoreResult;
use dcc_core::{
    domain::session::{SessionEventRecord, SessionId},
    ports::{
        events::CoreEvent, EventBus, SessionLiveDurableIdentity, SessionLiveEventEnvelope,
        MAX_SESSION_LIVE_RUNTIME_SEQUENCE,
    },
};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PhysicalIdentity {
    device: u64,
    inode: u64,
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::{convert::Infallible, fs};

    #[test]
    fn windows_path_fallback_coalesces_canonical_scope_without_exposing_paths() {
        let root = tempfile::tempdir().expect("runtime test root");
        let app_data = root.path().join("app-data");
        let sqlite = root.path().join("sessions.sqlite");
        let registry = ProcessRuntimeRegistry::isolated();
        let (_, first) = registry
            .acquire_after_open(&sqlite, &app_data, || {
                fs::create_dir_all(&app_data).expect("app data");
                fs::write(&sqlite, b"sqlite").expect("database");
                Ok::<_, Infallible>(())
            })
            .expect("windows fallback scope");
        let (_, second) = registry
            .acquire_after_open(&sqlite, &app_data, || Ok::<_, Infallible>(()))
            .expect("windows fallback replay");
        assert!(Arc::ptr_eq(&first, &second));
        assert!(format!("{first:?}").contains("[redacted]"));
        assert!(!format!("{first:?}").contains("sessions.sqlite"));
    }
}

impl fmt::Debug for PhysicalIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PhysicalIdentity([redacted])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RuntimeScopeKey {
    sqlite_file: PhysicalIdentity,
    app_data_directory: PhysicalIdentity,
}

impl fmt::Debug for RuntimeScopeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeScopeKey([redacted])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessRuntimeRegistryError {
    InvalidPath,
    UnsafeTarget,
    Io,
    Poisoned,
    UnsupportedPlatform,
}

impl fmt::Display for ProcessRuntimeRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPath => "invalid runtime scope path",
            Self::UnsafeTarget => "runtime scope target failed safety validation",
            Self::Io => "runtime scope inspection failed",
            Self::Poisoned => "process runtime registry unavailable",
            Self::UnsupportedPlatform => "process runtime registry unsupported on this platform",
        })
    }
}

impl std::error::Error for ProcessRuntimeRegistryError {}

#[derive(PartialEq, Eq)]
pub enum AcquireAfterOpenError<E> {
    Scope(ProcessRuntimeRegistryError),
    Consumer(E),
}

impl<E> fmt::Debug for AcquireAfterOpenError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scope(error) => formatter.debug_tuple("Scope").field(error).finish(),
            Self::Consumer(_) => formatter.write_str("Consumer([redacted])"),
        }
    }
}

impl<E: fmt::Display> fmt::Display for AcquireAfterOpenError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scope(error) => error.fmt(formatter),
            Self::Consumer(_) => formatter.write_str("runtime consumer open failed"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for AcquireAfterOpenError<E> {}

/// Shared process-level coordination for one physical SQLite/app-data scope.
///
/// Additional cooperative process-owned coordination may belong here so
/// unrelated State instances never create parallel arbiters. Guarded Undo
/// must retain its own stronger store/lease validation and must never inherit
/// filesystem authorization from this registry.
pub struct ProcessRuntime {
    #[allow(dead_code)] // Retained even when capture-v2 is compiled out.
    scope: RuntimeScopeKey,
    terminal_arbiter: Arc<TerminalArbiter>,
    #[allow(dead_code)] // Wired by the guarded-undo lifecycle integration.
    guarded_undo_runtime: Arc<GuardedUndoRuntime>,
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[allow(dead_code)] // Explicit ownership proof; runtime receives this same Arc.
    workspace_mutations: Arc<dcc_infra::guarded_undo::coordinator::WorkspaceMutationCoordinator>,
    session_store: Arc<Mutex<crate::state::SessionStore>>,
    /// Per-session provider-selection transitions. Entries are weak so this
    /// registry retains no historical session ids once a transition ends.
    /// The synchronous map lock is held only while finding/creating a lock;
    /// callers await the per-session lock after it has been released.
    provider_transition_locks: Mutex<HashMap<SessionId, Weak<AsyncMutex<()>>>>,
    event_buses: Mutex<Vec<Weak<dyn EventBus>>>,
    /// Public process-local identifier, never persisted or used as authority.
    runtime_generation: String,
    /// Allocated once per session-live event before fanout. It never wraps.
    next_live_sequence: AtomicU64,
}

impl fmt::Debug for ProcessRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProcessRuntime([redacted])")
    }
}

impl ProcessRuntime {
    fn new(scope: RuntimeScopeKey) -> Self {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let workspace_mutations =
            Arc::new(dcc_infra::guarded_undo::coordinator::WorkspaceMutationCoordinator::new());
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let guarded_undo_runtime = Arc::new(GuardedUndoRuntime::new_with_coordinator(Arc::clone(
            &workspace_mutations,
        )));
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let guarded_undo_runtime = Arc::new(GuardedUndoRuntime::new());

        Self {
            scope,
            terminal_arbiter: Arc::new(TerminalArbiter::default()),
            guarded_undo_runtime,
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            workspace_mutations,
            session_store: Arc::new(Mutex::new(crate::state::SessionStore::default())),
            provider_transition_locks: Mutex::new(HashMap::new()),
            event_buses: Mutex::new(Vec::new()),
            runtime_generation: Uuid::new_v4().to_string(),
            next_live_sequence: AtomicU64::new(0),
        }
    }

    pub fn terminal_arbiter(&self) -> Arc<TerminalArbiter> {
        Arc::clone(&self.terminal_arbiter)
    }

    #[allow(dead_code)] // Wired by the guarded-undo lifecycle integration.
    pub(crate) fn guarded_undo_runtime(&self) -> Arc<GuardedUndoRuntime> {
        Arc::clone(&self.guarded_undo_runtime)
    }

    /// Delegates a durably authorized root to the one coordinator shared with
    /// capture-v2. The binding type cannot be constructed from a raw command
    /// path outside `WorkspaceCommandState`.
    pub(crate) async fn run_workspace_mutation<T, E, F>(
        &self,
        binding: AuthorizedWorkspaceMutation,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        self.guarded_undo_runtime
            .run_workspace_mutation(binding.into_workspace_absolute(), operation)
            .await
    }

    /// Variant for synchronous operations which can run child processes.  On
    /// feature-on macOS it is the same physical mutation worker; feature-off
    /// it retains the command layer's blocking executor boundary.
    pub(crate) async fn run_workspace_mutation_blocking<T, E, F>(
        &self,
        binding: AuthorizedWorkspaceMutation,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        self.guarded_undo_runtime
            .run_workspace_mutation_blocking(binding.into_workspace_absolute(), operation)
            .await
    }

    /// Coordinates mutations which may touch both one worktree and the Git
    /// common directory shared by every linked worktree. The durable binding
    /// remains the only path authority accepted by this layer.
    pub(crate) async fn run_git_workspace_mutation<T, E, F>(
        &self,
        binding: AuthorizedWorkspaceMutation,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        self.guarded_undo_runtime
            .run_git_workspace_mutation(binding.into_workspace_absolute(), operation)
            .await
    }

    /// Blocking-executor variant for Git operations which launch child
    /// processes while retaining both physical mutation authorities.
    pub(crate) async fn run_git_workspace_mutation_blocking<T, E, F>(
        &self,
        binding: AuthorizedWorkspaceMutation,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    {
        self.guarded_undo_runtime
            .run_git_workspace_mutation_blocking(binding.into_workspace_absolute(), operation)
            .await
    }

    /// Coordinates an operation that reads or mutates two linked worktrees.
    /// The primary path has already been authorized against this runtime's
    /// SQLite registry; the command layer scopes the secondary path and the
    /// guarded-undo runtime physically proves it before the closure runs. A
    /// durable delegation binding remains a lifecycle-journal responsibility.
    pub(crate) async fn run_git_workspace_pair_mutation<T, E, F>(
        &self,
        binding: AuthorizedWorkspaceMutation,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path, &Path) -> Result<T, E> + Send + 'static,
    {
        self.guarded_undo_runtime
            .run_git_workspace_pair_mutation(
                binding.into_workspace_absolute(),
                secondary_absolute,
                operation,
            )
            .await
    }

    /// Blocking-executor variant for pair operations which launch child
    /// processes. Cancellation of the async waiter cannot release either
    /// worktree or the shared common-directory admission.
    pub(crate) async fn run_git_workspace_pair_mutation_blocking<T, E, F>(
        &self,
        binding: AuthorizedWorkspaceMutation,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> Result<T, WorkspaceMutationRunError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path, &Path) -> Result<T, E> + Send + 'static,
    {
        self.guarded_undo_runtime
            .run_git_workspace_pair_mutation_blocking(
                binding.into_workspace_absolute(),
                secondary_absolute,
                operation,
            )
            .await
    }

    /// Configures guarded undo only for the physical scope from which this
    /// process runtime was acquired. The SQLite repository is opened here so
    /// callers cannot inject a repository from another runtime scope.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub(crate) async fn configure_guarded_undo_capture(
        &self,
        sqlite_path: &Path,
        app_data_directory: &Path,
    ) -> Result<ConfigureOutcome, ConfigureGuardedUndoCaptureError> {
        // Scope walking and SQLite opening are synchronous filesystem work.
        // Keep it out of the async caller and make cancellation of that caller
        // harmless: this worker only validates/opens transient resources; it
        // cannot install a capture driver or acquire the artifact-store lease.
        let expected_scope = self.scope;
        let sqlite_path = sqlite_path.to_path_buf();
        let app_data_directory = app_data_directory.to_path_buf();
        let (repo, app_data_directory) = tokio::task::spawn_blocking(move || {
            open_guarded_undo_configuration(expected_scope, sqlite_path, app_data_directory)
        })
        .await
        .map_err(|_| ConfigureGuardedUndoCaptureError::Worker)??;
        self.guarded_undo_runtime
            .configure_capture_v2_scoped(
                app_data_directory,
                repo,
                self.scope.configuration_identity(),
            )
            .await
            .map_err(ConfigureGuardedUndoCaptureError::Runtime)
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[allow(dead_code)] // Feature-off state facade returns before configuration.
    pub(crate) async fn configure_guarded_undo_capture(
        &self,
        _sqlite_path: &Path,
        _app_data_directory: &Path,
    ) -> Result<ConfigureOutcome, ConfigureGuardedUndoCaptureError> {
        Ok(ConfigureOutcome::Disabled)
    }

    #[cfg(test)]
    fn validate_guarded_undo_scope(
        &self,
        sqlite_path: &Path,
        app_data_directory: &Path,
    ) -> Result<(), ConfigureGuardedUndoCaptureError> {
        let current = resolve_scope(sqlite_path, app_data_directory)
            .map_err(|_| ConfigureGuardedUndoCaptureError::Scope)?;
        if current != self.scope {
            return Err(ConfigureGuardedUndoCaptureError::ScopeMismatch);
        }
        Ok(())
    }

    pub(crate) fn register_event_bus(&self, bus: &Arc<dyn EventBus>) -> CoreResult<()> {
        let mut buses = self
            .event_buses
            .lock()
            .map_err(|_| dcc_core::CoreError::Repository("event hub unavailable".to_string()))?;
        buses.retain(|entry| entry.strong_count() != 0);
        if !buses
            .iter()
            .any(|entry| Weak::ptr_eq(entry, &Arc::downgrade(bus)))
        {
            buses.push(Arc::downgrade(bus));
        }
        Ok(())
    }

    /// Publishes the legacy event and, for session-scoped events, an additive
    /// live envelope. Workspace events deliberately have no session envelope.
    pub(crate) async fn publish_event(&self, event: CoreEvent) -> CoreResult<()> {
        let live_event = self.new_live_event(event.clone(), None);
        self.publish_to_buses(event, live_event).await
    }

    /// Publishes an event whose durable identity came from the canonical
    /// SQLite append outcome. The record is never reconstructed from caller
    /// input, so a racing append cannot mislabel a live envelope.
    pub(crate) async fn publish_durable_session_event(
        &self,
        record: &SessionEventRecord,
        event: CoreEvent,
    ) -> CoreResult<()> {
        if !event.matches_session_record(record) {
            return Err(dcc_core::CoreError::Repository(
                "durable session event does not match its canonical record".to_string(),
            ));
        }
        let durable = SessionLiveDurableIdentity {
            session_id: record.session_id.0.clone(),
            event_id: record.event_id.clone(),
            sequence: record.sequence,
        };
        let live_event = self.new_live_event(event.clone(), Some(durable));
        self.publish_to_buses(event, live_event).await
    }

    fn new_live_event(
        &self,
        event: CoreEvent,
        durable: Option<SessionLiveDurableIdentity>,
    ) -> Option<SessionLiveEventEnvelope> {
        if event.session_id().is_none() {
            return None;
        }
        let runtime_sequence = self.allocate_live_sequence()?;
        Some(SessionLiveEventEnvelope {
            runtime_generation: self.runtime_generation.clone(),
            runtime_sequence,
            durable,
            event,
        })
    }

    fn allocate_live_sequence(&self) -> Option<u64> {
        let mut current = self.next_live_sequence.load(Ordering::Relaxed);
        loop {
            if current >= MAX_SESSION_LIVE_RUNTIME_SEQUENCE {
                // Fail closed instead of wrapping and making an old envelope
                // appear current. Legacy CoreEvent delivery remains intact.
                return None;
            }
            let next = current + 1;
            match self.next_live_sequence.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(next),
                Err(observed) => current = observed,
            }
        }
    }

    async fn publish_to_buses(
        &self,
        event: CoreEvent,
        live_event: Option<SessionLiveEventEnvelope>,
    ) -> CoreResult<()> {
        let buses = {
            let mut entries = self.event_buses.lock().map_err(|_| {
                dcc_core::CoreError::Repository("event hub unavailable".to_string())
            })?;
            let mut live = Vec::with_capacity(entries.len());
            entries.retain(|entry| {
                if let Some(bus) = entry.upgrade() {
                    live.push(bus);
                    true
                } else {
                    false
                }
            });
            live
        };
        let mut first_error = None;
        let mut delivered = false;
        for bus in buses {
            match bus.publish(event.clone()).await {
                Ok(()) => delivered = true,
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
            if let Some(live_event) = live_event.as_ref() {
                // This is an additive transport: a failure to emit it must not
                // change legacy CoreEvent delivery semantics.
                if bus.publish_session_live(live_event.clone()).await.is_err() {
                    eprintln!("[DCC] session live event publication failed");
                }
            }
        }
        if delivered {
            Ok(())
        } else {
            Err(first_error.unwrap_or_else(|| {
                dcc_core::CoreError::Repository("event hub has no subscribers".to_string())
            }))
        }
    }

    /// Public process-local generation used only to reconcile the additive
    /// session-live transport with a durable snapshot. It is not authority or
    /// persisted state.
    pub(crate) fn runtime_generation(&self) -> &str {
        &self.runtime_generation
    }

    #[cfg(test)]
    fn set_next_live_sequence_for_test(&self, sequence: u64) {
        self.next_live_sequence.store(sequence, Ordering::Relaxed);
    }

    pub(crate) fn session_store(&self) -> Arc<Mutex<crate::state::SessionStore>> {
        Arc::clone(&self.session_store)
    }

    /// Returns the one short-lived lock for provider selection/binding work
    /// on this session. It coordinates all `SessionCommandState` instances
    /// sharing this physical runtime without serializing unrelated sessions.
    pub(crate) fn provider_transition_lock(
        &self,
        session_id: &SessionId,
    ) -> CoreResult<Arc<AsyncMutex<()>>> {
        let mut locks = self.provider_transition_locks.lock().map_err(|_| {
            dcc_core::CoreError::Repository("provider transition state unavailable".to_string())
        })?;
        locks.retain(|_, lock| lock.strong_count() != 0);
        if let Some(lock) = locks.get(session_id).and_then(Weak::upgrade) {
            return Ok(lock);
        }
        let lock = Arc::new(AsyncMutex::new(()));
        locks.insert(session_id.clone(), Arc::downgrade(&lock));
        Ok(lock)
    }

    pub(crate) fn remove_provider_transition_lock_if_idle(
        &self,
        session_id: &SessionId,
        lock: &Arc<AsyncMutex<()>>,
    ) {
        let Ok(mut locks) = self.provider_transition_locks.lock() else {
            return;
        };
        let is_same_idle_lock = locks
            .get(session_id)
            .and_then(Weak::upgrade)
            // `current` is the temporary Arc created by `upgrade`; the only
            // other strong reference allowed for an idle entry is `lock` from
            // the dropping guard. A queued waiter has already upgraded the
            // Weak and therefore makes this count greater than two.
            .is_some_and(|current| Arc::ptr_eq(&current, lock) && Arc::strong_count(lock) == 2);
        if is_same_idle_lock {
            locks.remove(session_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn provider_transition_lock_entry_count(&self) -> usize {
        self.provider_transition_locks
            .lock()
            .expect("provider transition lock registry")
            .len()
    }
}

#[cfg(any(test, all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
impl RuntimeScopeKey {
    fn configuration_identity(self) -> [u8; 32] {
        let mut identity = [0_u8; 32];
        identity[..8].copy_from_slice(&self.sqlite_file.device.to_be_bytes());
        identity[8..16].copy_from_slice(&self.sqlite_file.inode.to_be_bytes());
        identity[16..24].copy_from_slice(&self.app_data_directory.device.to_be_bytes());
        identity[24..].copy_from_slice(&self.app_data_directory.inode.to_be_bytes());
        identity
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Feature-off configuration is an infallible no-op.
pub(crate) enum ConfigureGuardedUndoCaptureError {
    Scope,
    ScopeMismatch,
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    Repository,
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    Worker,
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    Runtime(GuardedUndoRuntimeError),
}

impl fmt::Debug for ConfigureGuardedUndoCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConfigureGuardedUndoCaptureError([redacted])")
    }
}

impl fmt::Display for ConfigureGuardedUndoCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Scope => "guarded undo runtime scope unavailable",
            Self::ScopeMismatch => "guarded undo runtime scope does not match",
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            Self::Repository => "guarded undo repository unavailable",
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            Self::Worker => "guarded undo configuration unavailable",
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            Self::Runtime(_) => "guarded undo runtime unavailable",
        })
    }
}

impl std::error::Error for ConfigureGuardedUndoCaptureError {}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
fn open_guarded_undo_configuration(
    expected_scope: RuntimeScopeKey,
    sqlite_path: PathBuf,
    app_data_directory: PathBuf,
) -> Result<(dcc_infra::db::SqliteSessionRepo, PathBuf), ConfigureGuardedUndoCaptureError> {
    let current = resolve_scope(&sqlite_path, &app_data_directory)
        .map_err(|_| ConfigureGuardedUndoCaptureError::Scope)?;
    if current != expected_scope {
        return Err(ConfigureGuardedUndoCaptureError::ScopeMismatch);
    }
    let repo = dcc_infra::db::SqliteSessionRepo::open(&sqlite_path)
        .map_err(|_| ConfigureGuardedUndoCaptureError::Repository)?;
    Ok((repo, app_data_directory))
}

pub struct ProcessRuntimeRegistry {
    runtimes: Mutex<HashMap<RuntimeScopeKey, Weak<ProcessRuntime>>>,
}

impl fmt::Debug for ProcessRuntimeRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProcessRuntimeRegistry([redacted])")
    }
}

static PROCESS_RUNTIME_REGISTRY: OnceLock<ProcessRuntimeRegistry> = OnceLock::new();

impl ProcessRuntimeRegistry {
    /// Returns the sole production registry for this process.
    pub fn global() -> &'static Self {
        PROCESS_RUNTIME_REGISTRY.get_or_init(Self::isolated)
    }

    /// Opens/creates the consumer, then coalesces its resulting physical scope.
    ///
    /// `open_consumer` runs first so first-run callers may create app-data and
    /// create/migrate SQLite. The registry mutex is never held during that work
    /// or subsequent scope inspection. This cooperative ordering intentionally
    /// does not protect against hostile swap-and-restore.
    pub fn acquire_after_open<C, E, F>(
        &self,
        sqlite_path: &Path,
        app_data_directory: &Path,
        open_consumer: F,
    ) -> Result<(C, Arc<ProcessRuntime>), AcquireAfterOpenError<E>>
    where
        F: FnOnce() -> Result<C, E>,
    {
        let consumer = open_consumer().map_err(AcquireAfterOpenError::Consumer)?;
        let key =
            resolve_scope(sqlite_path, app_data_directory).map_err(AcquireAfterOpenError::Scope)?;
        let mut runtimes = self
            .runtimes
            .lock()
            .map_err(|_| AcquireAfterOpenError::Scope(ProcessRuntimeRegistryError::Poisoned))?;
        runtimes.retain(|_, runtime| runtime.strong_count() != 0);
        if let Some(runtime) = runtimes.get(&key).and_then(Weak::upgrade) {
            return Ok((consumer, runtime));
        }
        let runtime = Arc::new(ProcessRuntime::new(key));
        runtimes.insert(key, Arc::downgrade(&runtime));
        Ok((consumer, runtime))
    }

    // Kept private so production code cannot accidentally split process-wide
    // ownership. Unit tests in this module use isolated registries.
    fn isolated() -> Self {
        Self {
            runtimes: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn entry_count(&self) -> usize {
        self.runtimes.lock().unwrap().len()
    }

    #[cfg(test)]
    fn poison(&self) {
        let _guard = self.runtimes.lock().unwrap();
        panic!("intentional process runtime registry poison");
    }
}

#[cfg(unix)]
fn resolve_scope(
    sqlite_path: &Path,
    app_data_directory: &Path,
) -> Result<RuntimeScopeKey, ProcessRuntimeRegistryError> {
    let sqlite = unix::open_absolute(sqlite_path, unix::TargetKind::RegularFile)?;
    let app_data = unix::open_absolute(app_data_directory, unix::TargetKind::Directory)?;
    Ok(RuntimeScopeKey {
        sqlite_file: unix::validated_identity(&sqlite, unix::TargetKind::RegularFile)?,
        app_data_directory: unix::validated_identity(&app_data, unix::TargetKind::Directory)?,
    })
}

#[cfg(windows)]
fn resolve_scope(
    sqlite_path: &Path,
    app_data_directory: &Path,
) -> Result<RuntimeScopeKey, ProcessRuntimeRegistryError> {
    // Windows lacks the descriptor-relative, physical identity adapter used
    // on Unix in this phase. This is deliberately only a cooperative
    // path-based fallback: canonicalization follows aliases after the
    // consumer has opened the resources, and is neither authorization nor a
    // Guarded Undo filesystem boundary. Debug output still contains no paths.
    let sqlite = std::fs::canonicalize(sqlite_path).map_err(|_| ProcessRuntimeRegistryError::Io)?;
    let app_data =
        std::fs::canonicalize(app_data_directory).map_err(|_| ProcessRuntimeRegistryError::Io)?;
    if !sqlite.is_file() || !app_data.is_dir() {
        return Err(ProcessRuntimeRegistryError::UnsafeTarget);
    }
    Ok(RuntimeScopeKey {
        sqlite_file: windows_identity(&sqlite, b"sqlite-file"),
        app_data_directory: windows_identity(&app_data, b"app-data-directory"),
    })
}

#[cfg(all(not(unix), not(windows)))]
fn resolve_scope(
    _sqlite_path: &Path,
    _app_data_directory: &Path,
) -> Result<RuntimeScopeKey, ProcessRuntimeRegistryError> {
    Err(ProcessRuntimeRegistryError::UnsupportedPlatform)
}

#[cfg(windows)]
fn windows_identity(path: &Path, domain: &[u8]) -> PhysicalIdentity {
    use sha2::{Digest, Sha256};
    use std::os::windows::ffi::OsStrExt;

    let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut hasher = Sha256::new();
    hasher.update(b"dcc-process-runtime:v1:");
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    hasher.update((wide.len() as u64).to_be_bytes());
    for unit in wide {
        hasher.update(unit.to_le_bytes());
    }
    let digest = hasher.finalize();
    PhysicalIdentity {
        device: u64::from_le_bytes(digest[..8].try_into().expect("digest width")),
        inode: u64::from_le_bytes(digest[8..16].try_into().expect("digest width")),
    }
}

#[cfg(unix)]
mod unix {
    use super::{PhysicalIdentity, ProcessRuntimeRegistryError};
    use std::{
        ffi::{CString, OsStr},
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
        path::{Component, Path},
    };

    #[derive(Clone, Copy)]
    pub(super) enum TargetKind {
        Directory,
        RegularFile,
    }

    pub(super) fn open_absolute(
        path: &Path,
        target: TargetKind,
    ) -> Result<File, ProcessRuntimeRegistryError> {
        if !path.is_absolute() {
            return Err(ProcessRuntimeRegistryError::InvalidPath);
        }
        let root = open_component(-1, OsStr::new("/"), TargetKind::Directory)?;
        let components = path
            .components()
            .filter_map(|component| match component {
                Component::RootDir | Component::CurDir => None,
                Component::Normal(component) => Some(Ok(component)),
                Component::ParentDir | Component::Prefix(_) => {
                    Some(Err(ProcessRuntimeRegistryError::InvalidPath))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if components.is_empty() {
            return match target {
                TargetKind::Directory => Ok(root),
                TargetKind::RegularFile => Err(ProcessRuntimeRegistryError::InvalidPath),
            };
        }
        let mut current = root;
        for (index, component) in components.iter().enumerate() {
            let kind = if index + 1 == components.len() {
                target
            } else {
                TargetKind::Directory
            };
            current = open_component(current.as_raw_fd(), component, kind)?;
        }
        Ok(current)
    }

    fn open_component(
        parent: libc::c_int,
        name: &OsStr,
        target: TargetKind,
    ) -> Result<File, ProcessRuntimeRegistryError> {
        use std::os::fd::AsRawFd;

        let name =
            CString::new(name.as_bytes()).map_err(|_| ProcessRuntimeRegistryError::InvalidPath)?;
        let directory_flag = match target {
            TargetKind::Directory => libc::O_DIRECTORY,
            TargetKind::RegularFile => 0,
        };
        // O_NONBLOCK prevents an attacker-controlled non-regular final entry
        // (for example a FIFO) from blocking before fstat rejects its kind.
        let flags =
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK | directory_flag;
        let fd = if parent < 0 {
            unsafe { libc::open(name.as_ptr(), flags) }
        } else {
            unsafe { libc::openat(parent, name.as_ptr(), flags) }
        };
        if fd < 0 {
            let error = std::io::Error::last_os_error();
            return Err(match error.raw_os_error() {
                Some(libc::ELOOP) | Some(libc::ENOTDIR) => {
                    ProcessRuntimeRegistryError::UnsafeTarget
                }
                _ => ProcessRuntimeRegistryError::Io,
            });
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let descriptor_flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
        if descriptor_flags < 0 || descriptor_flags & libc::FD_CLOEXEC == 0 {
            return Err(ProcessRuntimeRegistryError::UnsafeTarget);
        }
        Ok(file)
    }

    pub(super) fn validated_identity(
        file: &File,
        target: TargetKind,
    ) -> Result<PhysicalIdentity, ProcessRuntimeRegistryError> {
        use std::os::fd::AsRawFd;

        let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
        if unsafe { libc::fstat(file.as_raw_fd(), &mut stat) } != 0 {
            return Err(ProcessRuntimeRegistryError::Io);
        }
        let file_kind = stat.st_mode & libc::S_IFMT;
        let expected_kind = match target {
            TargetKind::Directory => libc::S_IFDIR,
            TargetKind::RegularFile => libc::S_IFREG,
        };
        if file_kind != expected_kind
            || stat.st_uid != unsafe { libc::geteuid() }
            || stat.st_mode & (libc::S_IWGRP | libc::S_IWOTH) != 0
            || matches!(target, TargetKind::RegularFile) && stat.st_nlink != 1
        {
            return Err(ProcessRuntimeRegistryError::UnsafeTarget);
        }
        Ok(PhysicalIdentity {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        convert::Infallible,
        ffi::CString,
        fs,
        os::unix::{ffi::OsStrExt, fs::symlink, fs::PermissionsExt},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };

    #[derive(Default)]
    struct RecordingEventBus {
        legacy: Mutex<Vec<CoreEvent>>,
        live: Mutex<Vec<SessionLiveEventEnvelope>>,
    }

    #[async_trait::async_trait]
    impl EventBus for RecordingEventBus {
        async fn publish(&self, event: CoreEvent) -> CoreResult<()> {
            self.legacy.lock().unwrap().push(event);
            Ok(())
        }

        async fn publish_session_live(&self, event: SessionLiveEventEnvelope) -> CoreResult<()> {
            self.live.lock().unwrap().push(event);
            Ok(())
        }
    }

    struct Scope {
        _root: tempfile::TempDir,
        physical_root: std::path::PathBuf,
        db: std::path::PathBuf,
        app_data: std::path::PathBuf,
    }

    impl Scope {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            // On macOS tempfile paths use the `/var` symlink alias. Resolve
            // the fixture once; the registry itself still rejects every
            // intermediate symlink.
            let physical_root = fs::canonicalize(root.path()).unwrap();
            let app_data = physical_root.join("app-data");
            fs::create_dir(&app_data).unwrap();
            let db = physical_root.join("sessions.sqlite");
            fs::write(&db, b"sqlite-identity-fixture").unwrap();
            Self {
                _root: root,
                physical_root,
                db,
                app_data,
            }
        }
    }

    fn acquire(
        registry: &ProcessRuntimeRegistry,
        sqlite: &Path,
        app_data: &Path,
    ) -> Result<Arc<ProcessRuntime>, ProcessRuntimeRegistryError> {
        match registry.acquire_after_open(sqlite, app_data, || Ok::<_, Infallible>(())) {
            Ok(((), runtime)) => Ok(runtime),
            Err(AcquireAfterOpenError::Scope(error)) => Err(error),
            Err(AcquireAfterOpenError::Consumer(never)) => match never {},
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn live_session_envelopes_are_unique_and_preserve_legacy_fanout() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let runtime = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        let bus = Arc::new(RecordingEventBus::default());
        let event_bus: Arc<dyn EventBus> = bus.clone();
        runtime.register_event_bus(&event_bus).unwrap();

        runtime
            .publish_event(CoreEvent::WorkspaceReady {
                workspace_id: "workspace-1".to_string(),
                project_id: "project-1".to_string(),
                worktree_path: "/redacted".to_string(),
            })
            .await
            .unwrap();
        runtime
            .publish_event(CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: "session-1".to_string(),
                statuses: Vec::new(),
            })
            .await
            .unwrap();
        let record = SessionEventRecord {
            event_id: "canonical-event-id".to_string(),
            session_id: dcc_core::domain::session::SessionId("session-1".to_string()),
            sequence: 41,
            occurred_at: "2026-09-01T00:00:00Z".to_string(),
            kind: dcc_core::domain::session::SessionEventKind::TurnCompleted {
                turn_id: dcc_core::domain::session::TurnId("turn-1".to_string()),
            },
        };
        runtime
            .publish_durable_session_event(
                &record,
                CoreEvent::SessionTurnCompleted {
                    session_id: "session-1".to_string(),
                    turn_id: "turn-1".to_string(),
                },
            )
            .await
            .unwrap();

        let legacy = bus.legacy.lock().unwrap();
        assert_eq!(legacy.len(), 3, "legacy fanout stays unchanged");
        let live = bus.live.lock().unwrap();
        assert_eq!(live.len(), 2, "workspace events have no session envelope");
        assert_eq!(live[0].runtime_sequence, 1);
        assert_eq!(live[1].runtime_sequence, 2);
        assert_eq!(live[0].runtime_generation, live[1].runtime_generation);
        assert_eq!(live[0].runtime_generation, runtime.runtime_generation());
        assert!(live[0].durable.is_none());
        assert_eq!(
            live[1].durable.as_ref().unwrap(),
            &SessionLiveDurableIdentity {
                session_id: "session-1".to_string(),
                event_id: "canonical-event-id".to_string(),
                sequence: 41,
            }
        );
    }

    #[test]
    fn provider_transition_lock_registry_discards_stale_weak_entries() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let runtime = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        let stale_session = SessionId("stale-transition".to_string());
        let live_session = SessionId("live-transition".to_string());

        {
            let _lock = runtime.provider_transition_lock(&stale_session).unwrap();
            assert_eq!(runtime.provider_transition_lock_entry_count(), 1);
        }
        // The next acquisition performs opportunistic cleanup; stale session
        // keys do not accumulate through sequential provider changes.
        let _lock = runtime.provider_transition_lock(&live_session).unwrap();
        assert_eq!(runtime.provider_transition_lock_entry_count(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn live_sequence_never_wraps_into_a_reused_identity() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let runtime = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        let bus = Arc::new(RecordingEventBus::default());
        let event_bus: Arc<dyn EventBus> = bus.clone();
        runtime.register_event_bus(&event_bus).unwrap();
        runtime.set_next_live_sequence_for_test(MAX_SESSION_LIVE_RUNTIME_SEQUENCE);

        runtime
            .publish_event(CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: "session-1".to_string(),
                statuses: Vec::new(),
            })
            .await
            .unwrap();

        assert_eq!(bus.legacy.lock().unwrap().len(), 1);
        assert!(bus.live.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn js_safe_live_sequence_is_emitted_once_then_fails_closed() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let runtime = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        let bus = Arc::new(RecordingEventBus::default());
        let event_bus: Arc<dyn EventBus> = bus.clone();
        runtime.register_event_bus(&event_bus).unwrap();
        runtime.set_next_live_sequence_for_test(MAX_SESSION_LIVE_RUNTIME_SEQUENCE - 1);

        runtime
            .publish_event(CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: "session-1".to_string(),
                statuses: Vec::new(),
            })
            .await
            .unwrap();
        runtime
            .publish_event(CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: "session-1".to_string(),
                statuses: Vec::new(),
            })
            .await
            .unwrap();

        assert_eq!(bus.legacy.lock().unwrap().len(), 2);
        assert_eq!(bus.live.lock().unwrap().len(), 1);
        assert_eq!(
            bus.live.lock().unwrap()[0].runtime_sequence,
            MAX_SESSION_LIVE_RUNTIME_SEQUENCE
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn mismatched_canonical_record_is_not_published_to_any_transport() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let runtime = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        let bus = Arc::new(RecordingEventBus::default());
        let event_bus: Arc<dyn EventBus> = bus.clone();
        runtime.register_event_bus(&event_bus).unwrap();
        let record = SessionEventRecord {
            event_id: "canonical-event-id".to_string(),
            session_id: dcc_core::domain::session::SessionId("session-1".to_string()),
            sequence: 1,
            occurred_at: "2026-09-01T00:00:00Z".to_string(),
            kind: dcc_core::domain::session::SessionEventKind::TurnCompleted {
                turn_id: dcc_core::domain::session::TurnId("turn-1".to_string()),
            },
        };

        let result = runtime
            .publish_durable_session_event(
                &record,
                CoreEvent::SessionTurnAborted {
                    session_id: "session-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    reason: None,
                },
            )
            .await;

        assert!(result.is_err());
        assert!(bus.legacy.lock().unwrap().is_empty());
        assert!(bus.live.lock().unwrap().is_empty());
    }

    #[test]
    fn safe_aliases_share_exact_runtime_and_arbiter() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let db_alias = scope.db.parent().unwrap().join(".").join("sessions.sqlite");
        let app_alias = scope.app_data.join(".");
        let first = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        let second = acquire(&registry, &db_alias, &app_alias).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(
            &first.terminal_arbiter(),
            &second.terminal_arbiter()
        ));
        assert!(Arc::ptr_eq(
            &first.guarded_undo_runtime(),
            &second.guarded_undo_runtime()
        ));
        first
            .validate_guarded_undo_scope(&db_alias, &app_alias)
            .unwrap();
        assert_eq!(
            first.scope.configuration_identity(),
            second.scope.configuration_identity()
        );
        assert_eq!(registry.entry_count(), 1);
    }

    #[test]
    fn first_run_closure_creates_app_data_and_sqlite_before_scope_resolution() {
        let registry = ProcessRuntimeRegistry::isolated();
        let root = tempfile::tempdir().unwrap();
        let physical_root = fs::canonicalize(root.path()).unwrap();
        let app_data = physical_root.join("new-app-data");
        let sqlite = app_data.join("sessions.sqlite");
        assert!(!app_data.exists());
        assert!(!sqlite.exists());

        let (consumer, runtime) = registry
            .acquire_after_open(&sqlite, &app_data, || {
                fs::create_dir_all(&app_data).unwrap();
                fs::write(&sqlite, b"new sqlite database").unwrap();
                Ok::<_, Infallible>("first-run-consumer")
            })
            .unwrap();
        assert_eq!(consumer, "first-run-consumer");
        let (_, replay_runtime) = registry
            .acquire_after_open(&sqlite, &app_data, || Ok::<_, Infallible>("reopened"))
            .unwrap();
        assert!(Arc::ptr_eq(&runtime, &replay_runtime));
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[tokio::test(flavor = "current_thread")]
    async fn feature_off_configuration_is_a_noop_without_scope_io() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let runtime = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        assert_eq!(
            runtime
                .configure_guarded_undo_capture(
                    Path::new("/missing-redacted-db"),
                    Path::new("/missing-redacted-app-data"),
                )
                .await
                .unwrap(),
            ConfigureOutcome::Disabled
        );
    }

    #[test]
    fn distinct_database_or_app_data_identity_is_isolated() {
        let registry = ProcessRuntimeRegistry::isolated();
        let first = Scope::new();
        let second = Scope::new();
        let first_runtime = acquire(&registry, &first.db, &first.app_data).unwrap();
        let other_db = acquire(&registry, &second.db, &first.app_data).unwrap();
        let other_app = acquire(&registry, &first.db, &second.app_data).unwrap();
        assert!(!Arc::ptr_eq(&first_runtime, &other_db));
        assert!(!Arc::ptr_eq(&first_runtime, &other_app));
        assert!(!Arc::ptr_eq(&other_db, &other_app));
        assert_eq!(
            first_runtime.validate_guarded_undo_scope(&second.db, &first.app_data),
            Err(ConfigureGuardedUndoCaptureError::ScopeMismatch)
        );
        assert_eq!(
            first_runtime.validate_guarded_undo_scope(&first.db, &second.app_data),
            Err(ConfigureGuardedUndoCaptureError::ScopeMismatch)
        );
        assert_ne!(
            first_runtime.scope.configuration_identity(),
            other_db.scope.configuration_identity()
        );
    }

    #[test]
    fn dead_weak_entries_are_cleaned_without_retaining_runtime() {
        let registry = ProcessRuntimeRegistry::isolated();
        let first = Scope::new();
        let weak = {
            let runtime = acquire(&registry, &first.db, &first.app_data).unwrap();
            Arc::downgrade(&runtime)
        };
        assert!(weak.upgrade().is_none());
        assert_eq!(registry.entry_count(), 1);
        let second = Scope::new();
        let _ = acquire(&registry, &second.db, &second.app_data).unwrap();
        assert_eq!(registry.entry_count(), 1);
    }

    #[test]
    fn symlink_targets_are_rejected() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let db_link = scope.db.parent().unwrap().join("db-link");
        symlink(&scope.db, &db_link).unwrap();
        assert_eq!(
            acquire(&registry, &db_link, &scope.app_data).unwrap_err(),
            ProcessRuntimeRegistryError::UnsafeTarget
        );
        let app_link = scope.db.parent().unwrap().join("app-link");
        symlink(&scope.app_data, &app_link).unwrap();
        assert_eq!(
            acquire(&registry, &scope.db, &app_link).unwrap_err(),
            ProcessRuntimeRegistryError::UnsafeTarget
        );
    }

    #[test]
    fn intermediate_symlink_is_rejected() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let real = scope.physical_root.join("real");
        fs::create_dir(&real).unwrap();
        let db = real.join("nested.sqlite");
        fs::write(&db, b"nested").unwrap();
        let link = scope.physical_root.join("linked-directory");
        symlink(&real, &link).unwrap();
        assert_eq!(
            acquire(&registry, &link.join("nested.sqlite"), &scope.app_data).unwrap_err(),
            ProcessRuntimeRegistryError::UnsafeTarget
        );
    }

    #[test]
    fn hardlink_fifo_and_writable_targets_are_rejected_without_blocking() {
        let registry = ProcessRuntimeRegistry::isolated();

        let hardlink_scope = Scope::new();
        let hardlink = hardlink_scope.physical_root.join("hardlink.sqlite");
        fs::hard_link(&hardlink_scope.db, &hardlink).unwrap();
        assert_eq!(
            acquire(&registry, &hardlink, &hardlink_scope.app_data).unwrap_err(),
            ProcessRuntimeRegistryError::UnsafeTarget
        );

        let fifo_scope = Scope::new();
        let fifo = fifo_scope.physical_root.join("fifo.sqlite");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        assert_eq!(
            acquire(&registry, &fifo, &fifo_scope.app_data).unwrap_err(),
            ProcessRuntimeRegistryError::UnsafeTarget
        );

        let writable_db = Scope::new();
        fs::set_permissions(&writable_db.db, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(
            acquire(&registry, &writable_db.db, &writable_db.app_data).unwrap_err(),
            ProcessRuntimeRegistryError::UnsafeTarget
        );

        let writable_app = Scope::new();
        fs::set_permissions(&writable_app.app_data, fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            acquire(&registry, &writable_app.db, &writable_app.app_data).unwrap_err(),
            ProcessRuntimeRegistryError::UnsafeTarget
        );
    }

    #[test]
    fn poisoned_registry_fails_closed_after_cooperative_consumer_open() {
        let registry = Arc::new(ProcessRuntimeRegistry::isolated());
        let poisoner = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || registry.poison())
        };
        assert!(poisoner.join().is_err());
        let scope = Scope::new();
        let opened = AtomicUsize::new(0);
        let result = registry.acquire_after_open(&scope.db, &scope.app_data, || {
            opened.fetch_add(1, Ordering::SeqCst);
            Ok::<_, Infallible>(())
        });
        assert!(matches!(
            result,
            Err(AcquireAfterOpenError::Scope(
                ProcessRuntimeRegistryError::Poisoned
            ))
        ));
        assert_eq!(opened.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_acquire_coalesces_after_independent_consumer_open() {
        let registry = Arc::new(ProcessRuntimeRegistry::isolated());
        let scope = Scope::new();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut tasks = Vec::new();
        for consumer in 0..2 {
            let registry = Arc::clone(&registry);
            let db = scope.db.clone();
            let app_data = scope.app_data.clone();
            let barrier = Arc::clone(&barrier);
            tasks.push(std::thread::spawn(move || {
                registry
                    .acquire_after_open(&db, &app_data, || {
                        barrier.wait();
                        Ok::<_, Infallible>(consumer)
                    })
                    .unwrap()
            }));
        }
        let (first_consumer, first_runtime) = tasks.remove(0).join().unwrap();
        let (second_consumer, second_runtime) = tasks.remove(0).join().unwrap();
        assert_ne!(first_consumer, second_consumer);
        assert!(Arc::ptr_eq(&first_runtime, &second_runtime));
        assert_eq!(registry.entry_count(), 1);
    }

    #[test]
    fn debug_output_is_fully_redacted() {
        let registry = ProcessRuntimeRegistry::isolated();
        let scope = Scope::new();
        let runtime = acquire(&registry, &scope.db, &scope.app_data).unwrap();
        let scope_key = resolve_scope(&scope.db, &scope.app_data).unwrap();
        for debug in [
            format!("{registry:?}"),
            format!("{runtime:?}"),
            format!("{scope_key:?}"),
            format!("{:?}", scope_key.sqlite_file),
        ] {
            assert!(debug.contains("[redacted]"));
            assert!(!debug.contains("sessions.sqlite"));
            assert!(!debug.contains(scope.db.to_string_lossy().as_ref()));
        }
        let consumer_error = AcquireAfterOpenError::Consumer("secret consumer error");
        let debug = format!("{consumer_error:?}");
        assert_eq!(debug, "Consumer([redacted])");
        assert!(!debug.contains("secret"));
    }
}
