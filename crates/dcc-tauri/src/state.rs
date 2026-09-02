use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use dcc_core::{
    application::{
        list_turn_queue, mark_queued_turn_dispatched, merge_send_turn_session_selection,
        prepare_session_for_turn, resolve_session_mcp_servers, send_turn as run_send_turn,
        send_turn_selection_differs_from_session, ResolveSessionMcpInput, SendTurnInput,
        StartThreadInput,
    },
    domain::{
        delegation::{
            Delegation, DelegationBudget, DelegationId, DelegationMode, DelegationStatus,
        },
        delegation_apply::{
            DelegationApplyTransaction, DelegationApplyTransactionId,
            DelegationApplyTransactionState,
        },
        delegation_worktree::{
            DelegationWorktreeOperation, DelegationWorktreeOperationId,
            DelegationWorktreeOperationState,
        },
        mcp::{
            mcp_oauth_resource_fingerprint, McpDefinitionId, McpErrorCategory, McpOauthGrant,
            McpRuntimeError, McpRuntimeState, McpRuntimeStatus, McpSecretReferenceId, McpTransport,
        },
        project::{Project, ProjectId},
        provider::{
            McpOauthSupport, ProviderApprovalPolicy, ProviderEvent, ProviderId, SessionHandle,
        },
        repository::{Repository, RepositoryId},
        session::{
            AssistantMessagePhase, Session, SessionEventKind, SessionEventRecord, SessionId,
            SessionSearchResult, TurnChangeSet, TurnId, WorkspaceSessionSummary,
        },
        thread::{Thread, ThreadId},
        usage::{ModelTokenUsage, UsageDashboard, UsageDashboardInput},
        workspace::{Workspace, WorkspaceId},
        workspace_bundle::WorkspaceBundleState,
    },
    ports::{
        AppendEventOutcome, CredentialStore, DelegationApplyTransactionRepo, DelegationRepo,
        DelegationWorktreeOperationRepo, EventBus, Input, McpRepo, ProjectRepo, Provider,
        ProviderMcpOauthStart, ProviderMcpServerConfig, ProviderRuntimeConfig, RepositoryRepo,
        SessionConfig, SessionEventRepo, SessionRepo, ThreadRepo, UsageRepo, WorkspaceBundleRepo,
        WorkspaceRepo,
    },
    Result,
};
use dcc_infra::{
    credential_store::SystemCredentialStore,
    db::{ProviderAvailabilityRecord, SqliteSessionRepo, SqliteWorkspaceRepo},
    mcp_db::SqliteMcpRepo,
};

use crate::turn_review::{
    capture_baseline, capture_result, cleanup_all_snapshot_quarantines, cleanup_snapshot,
    current_snapshot_matches, observed_validations_for_turn, GitTurnBaseline,
    TURN_REVIEW_CAPTURE_VERSION,
};

#[cfg(test)]
use crate::delivery_failure::WorkspaceDeliveryFailureClassification;
use crate::delivery_failure::{
    sanitize_delivery_failure_output, WorkspaceDeliveryFailureOperation,
    WorkspaceDeliveryFailureSnapshot,
};
use crate::events::TauriEventBus;
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
use crate::guarded_undo_runtime::{
    BeginDisposition, ConfigureOutcome, GuardedUndoCaptureRequest, RecoveryOutcome,
};
use crate::guarded_undo_runtime::{
    CaptureTerminalMode, FinalizeTurnOutcome, GuardedUndoExecuteResult, GuardedUndoPrepareResult,
    WorkspaceMutationRunError,
};
use crate::process_runtime_registry::{
    ProcessRuntime, ProcessRuntimeRegistry, ProviderAvailabilityPhase,
    ProviderAvailabilityRuntimeState,
};
use crate::terminal_arbiter::{
    PersistThenCommitError, TerminalArbiterError, TerminalClaimResult, TerminalIntent, TerminalKey,
};
use dcc_core::domain::objective::{
    merge_objective_instructions, ObjectiveTransition, ObjectiveTurnOutcome, SessionObjective,
    SessionObjectiveDraft,
};
use dcc_providers::{
    provider_registration, provider_runtime, supports_provider_approval_policy,
    supports_provider_capability, validate_provider_model, validate_provider_runtime_config,
    ProviderCapability, ProviderRegistration, PROVIDER_IDS,
};

const DELIVERY_FAILURE_WORKSPACE_LIMIT: usize = 64;
/// A dynamic-model snapshot older than this is re-asked before a miss is
/// treated as an unknown model.
const DYNAMIC_MODEL_SNAPSHOT_TTL: std::time::Duration = std::time::Duration::from_secs(600);
const EPHEMERAL_MCP_LEASE_ID_MAX_CHARS: usize = 128;

fn registered_provider(provider_id: &str) -> Result<ProviderRegistration> {
    provider_registration(provider_id).ok_or_else(|| {
        dcc_core::CoreError::Provider(format!("unknown provider runtime: {provider_id}"))
    })
}

fn require_provider_capability(
    provider_id: &str,
    capability: ProviderCapability,
    operation: &str,
) -> Result<ProviderRegistration> {
    let registration = registered_provider(provider_id)?;
    if !supports_provider_capability(&registration.capabilities, capability) {
        return Err(dcc_core::CoreError::Provider(format!(
            "provider {provider_id} does not support {operation}"
        )));
    }
    Ok(registration)
}

fn require_provider_approval_policy(
    provider_id: &str,
    policy: ProviderApprovalPolicy,
) -> Result<ProviderRegistration> {
    let registration = registered_provider(provider_id)?;
    if !supports_provider_approval_policy(&registration.capabilities, policy) {
        return Err(dcc_core::CoreError::Provider(format!(
            "provider {provider_id} does not support the requested approval policy"
        )));
    }
    Ok(registration)
}

/// Fields that scope a provider handle. Timestamps are intentionally omitted:
/// resume/preparation legitimately updates them without changing the runtime
/// identity that an attach must verify.
fn provider_attach_snapshot_matches(current: &Session, expected: &Session) -> bool {
    current.id == expected.id
        && current.project_id == expected.project_id
        && current.workspace_id == expected.workspace_id
        && current.additional_workspace_ids == expected.additional_workspace_ids
        && current.provider_id == expected.provider_id
        && current.model == expected.model
        && current.provider_runtime == expected.provider_runtime
        && current.working_directory_override == expected.working_directory_override
        && current.state == expected.state
}

#[derive(Default)]
struct DeliveryFailureStore {
    failures: HashMap<
        String,
        HashMap<WorkspaceDeliveryFailureOperation, WorkspaceDeliveryFailureSnapshot>,
    >,
    active_recovery: HashMap<(String, WorkspaceDeliveryFailureOperation), u64>,
    next_recovery_owner: u64,
}

/// An in-flight recovery claim. The claim is deliberately independent from
/// the snapshot so a failed retry can replace the snapshot before this guard
/// is dropped. Its owner identity is never exposed in formatting.
pub(crate) struct DeliveryRecoveryClaim {
    store: Arc<Mutex<DeliveryFailureStore>>,
    root: String,
    operation: WorkspaceDeliveryFailureOperation,
    attempt_token: String,
    owner: u64,
}

impl std::fmt::Debug for DeliveryRecoveryClaim {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DeliveryRecoveryClaim([redacted])")
    }
}

impl DeliveryRecoveryClaim {
    pub(crate) fn operation(&self) -> WorkspaceDeliveryFailureOperation {
        self.operation
    }

    /// Clears only the snapshot that was claimed. A newer snapshot produced
    /// while this claim was active is preserved by the token comparison.
    pub(crate) fn clear_current_snapshot(&self) -> std::result::Result<(), String> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| "delivery recovery state is unavailable".to_string())?;
        let key = (self.root.clone(), self.operation);
        if store.active_recovery.get(&key) != Some(&self.owner) {
            return Err("delivery recovery claim is no longer active".to_string());
        }
        let remove_root = store
            .failures
            .get_mut(&self.root)
            .map(|operations| {
                let matches = operations
                    .get(&self.operation)
                    .is_some_and(|snapshot| snapshot.attempt_token == self.attempt_token);
                if matches {
                    operations.remove(&self.operation);
                }
                operations.is_empty()
            })
            .unwrap_or(false);
        if remove_root {
            store.failures.remove(&self.root);
        }
        Ok(())
    }
}

impl Drop for DeliveryRecoveryClaim {
    fn drop(&mut self) {
        let key = (self.root.clone(), self.operation);
        let Ok(mut store) = self.store.lock() else {
            // A poisoned recovery mutex is fail-closed: do not attempt to
            // reconstruct state or remove an unrelated owner.
            return;
        };
        if store.active_recovery.get(&key) == Some(&self.owner) {
            store.active_recovery.remove(&key);
        }
    }
}

/// Durable, content-free identity of an M3 turn-review snapshot.
///
/// A reference is returned only after its `TurnChangeSet` row has been
/// persisted. It intentionally carries no filesystem location, review
/// content, fingerprints, or artifact information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct M3SnapshotRef {
    pub snapshot_id: String,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub workspace_id: WorkspaceId,
}

/// The durable M3 snapshots created while a provider turn is starting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct M3BaselineCapture {
    pub snapshots: Vec<M3SnapshotRef>,
    root_bindings: Vec<M3RootBinding>,
}

#[derive(Clone, PartialEq, Eq)]
struct M3RootBinding {
    workspace_id: WorkspaceId,
    workspace_absolute: PathBuf,
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    physical_root_id: dcc_core::domain::guarded_undo::PhysicalRootId,
}

impl std::fmt::Debug for M3RootBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("M3RootBinding([redacted])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureV2StartDisposition {
    Disabled,
    Started,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureV2StartReport {
    pub disposition: CaptureV2StartDisposition,
    pub active_captures: u32,
    pub failed_captures: u32,
}

impl CaptureV2StartReport {
    fn disabled() -> Self {
        Self {
            disposition: CaptureV2StartDisposition::Disabled,
            active_captures: 0,
            failed_captures: 0,
        }
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    fn skipped() -> Self {
        Self {
            disposition: CaptureV2StartDisposition::Skipped,
            active_captures: 0,
            failed_captures: 0,
        }
    }
}

impl M3SnapshotRef {
    fn after_persist(change_set: &TurnChangeSet) -> Self {
        Self {
            snapshot_id: change_set.snapshot_id.clone(),
            session_id: change_set.session_id.clone(),
            turn_id: change_set.turn_id.clone(),
            workspace_id: change_set.workspace_id.clone(),
        }
    }
}

#[derive(Clone)]
pub struct WorkspaceCommandState {
    pub db_path: PathBuf,
    pub(crate) app_data_dir: PathBuf,
    runtime: Arc<ProcessRuntime>,
    delivery_failures: Arc<Mutex<DeliveryFailureStore>>,
}

impl std::fmt::Debug for WorkspaceCommandState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("WorkspaceCommandState([redacted])")
    }
}

/// One-shot binding resolved from the SQLite workspace registry owned by this
/// state. Its fields are private so a command cannot turn a caller-supplied
/// path into mutation authority.
pub(crate) struct AuthorizedWorkspaceMutation {
    workspace_absolute: PathBuf,
}

impl std::fmt::Debug for AuthorizedWorkspaceMutation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AuthorizedWorkspaceMutation([redacted])")
    }
}

impl AuthorizedWorkspaceMutation {
    pub(crate) fn into_workspace_absolute(self) -> PathBuf {
        self.workspace_absolute
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceMutationAuthorizationError {
    InvalidRequest,
    RepositoryUnavailable,
    UnknownMapping,
    AmbiguousMapping,
}

impl std::fmt::Display for WorkspaceMutationAuthorizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "workspace mutation request is invalid",
            Self::RepositoryUnavailable => "workspace registry is unavailable",
            Self::UnknownMapping => "workspace mutation mapping is unknown",
            Self::AmbiguousMapping => "workspace mutation mapping is ambiguous",
        })
    }
}

impl std::error::Error for WorkspaceMutationAuthorizationError {}

pub(crate) enum WorkspaceMutationRequestError<E> {
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    Authorization(WorkspaceMutationAuthorizationError),
    Runtime(WorkspaceMutationRunError<E>),
}

impl<E> std::fmt::Debug for WorkspaceMutationRequestError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            Self::Authorization(error) => {
                formatter.debug_tuple("Authorization").field(error).finish()
            }
            Self::Runtime(error) => formatter.debug_tuple("Runtime").field(error).finish(),
        }
    }
}

impl<E> std::fmt::Display for WorkspaceMutationRequestError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            Self::Authorization(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl WorkspaceCommandState {
    /// Builds workspace command state from the already coalesced session
    /// runtime. This is the production constructor; it cannot accidentally
    /// create a second mutation coordinator for the same physical scope.
    pub fn from_session(session: &SessionCommandState) -> Self {
        Self {
            db_path: session.db_path.clone(),
            app_data_dir: session.app_data_dir.clone(),
            runtime: Arc::clone(&session.runtime),
            delivery_failures: Arc::new(Mutex::new(DeliveryFailureStore::default())),
        }
    }

    #[allow(dead_code)] // Foundation API; handlers are integrated separately.
    pub(crate) async fn authorize_workspace_mutation(
        &self,
        requested_root: &str,
    ) -> std::result::Result<AuthorizedWorkspaceMutation, WorkspaceMutationAuthorizationError> {
        let db_path = self.db_path.clone();
        let requested_root = requested_root.to_owned();
        tokio::task::spawn_blocking(move || {
            resolve_authorized_workspace_mutation(&db_path, &requested_root)
        })
        .await
        .map_err(|_| WorkspaceMutationAuthorizationError::RepositoryUnavailable)?
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    async fn authorize_workspace_mutation_for_id(
        &self,
        workspace_id: &WorkspaceId,
        requested_root: &str,
    ) -> std::result::Result<AuthorizedWorkspaceMutation, WorkspaceMutationAuthorizationError> {
        let db_path = self.db_path.clone();
        let workspace_id = workspace_id.clone();
        let requested_root = requested_root.to_owned();
        tokio::task::spawn_blocking(move || {
            resolve_authorized_workspace_mutation_for_id(&db_path, &workspace_id, &requested_root)
        })
        .await
        .map_err(|_| WorkspaceMutationAuthorizationError::RepositoryUnavailable)?
    }

    /// Resolves durable authority first, then delegates physical admission and
    /// the complete synchronous operation to the process runtime.
    #[allow(dead_code)] // Foundation API; handlers are integrated separately.
    pub(crate) async fn run_workspace_mutation<T, E, F>(
        &self,
        requested_root: &str,
        operation: F,
    ) -> std::result::Result<T, WorkspaceMutationRequestError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation(requested_root)
            .await
            .map_err(WorkspaceMutationRequestError::Authorization)?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = AuthorizedWorkspaceMutation {
            workspace_absolute: PathBuf::from(requested_root),
        };
        self.runtime
            .run_workspace_mutation(binding, operation)
            .await
            .map_err(WorkspaceMutationRequestError::Runtime)
    }

    /// Public command-shell bridge for legacy desktop handlers that live in
    /// the binary crate. It preserves operation errors without exposing the
    /// private authorization binding or coordinator error types.
    pub async fn run_registered_workspace_mutation<T, E, F>(
        &self,
        requested_root: &str,
        operation: F,
    ) -> std::result::Result<std::result::Result<T, E>, ()>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> std::result::Result<T, E> + Send + 'static,
    {
        match self.run_workspace_mutation(requested_root, operation).await {
            Ok(value) => Ok(Ok(value)),
            Err(WorkspaceMutationRequestError::Runtime(WorkspaceMutationRunError::Operation(
                error,
            ))) => Ok(Err(error)),
            Err(_) => Err(()),
        }
    }

    /// Public command-shell bridge for mutations whose caller has an exact
    /// workspace row. Guarded macOS builds check the row id against the active
    /// root, so shared repository roots cannot produce an ambiguous mapping;
    /// feature-off builds preserve the existing direct-runner behavior.
    pub async fn run_registered_workspace_mutation_for_workspace_id<T, E, F>(
        &self,
        workspace_id: &WorkspaceId,
        requested_root: &str,
        operation: F,
    ) -> std::result::Result<std::result::Result<T, E>, String>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation_for_id(workspace_id, requested_root)
            .await
            .map_err(|error| error.to_string())?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = {
            let _ = workspace_id;
            AuthorizedWorkspaceMutation {
                workspace_absolute: PathBuf::from(requested_root),
            }
        };
        match self
            .runtime
            .run_workspace_mutation(binding, operation)
            .await
        {
            Ok(value) => Ok(Ok(value)),
            Err(WorkspaceMutationRunError::Operation(error)) => Ok(Err(error)),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Runs a synchronous child-process-capable operation under the same
    /// durable authorization and physical mutation lease as other workspace
    /// mutations.  Feature-off preserves the pre-M4 blocking executor.
    pub(crate) async fn run_workspace_mutation_blocking<T, E, F>(
        &self,
        requested_root: &str,
        operation: F,
    ) -> std::result::Result<T, WorkspaceMutationRequestError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation(requested_root)
            .await
            .map_err(WorkspaceMutationRequestError::Authorization)?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = AuthorizedWorkspaceMutation {
            workspace_absolute: PathBuf::from(requested_root),
        };
        self.runtime
            .run_workspace_mutation_blocking(binding, operation)
            .await
            .map_err(WorkspaceMutationRequestError::Runtime)
    }

    /// Resolves the requested root from the durable workspace registry before
    /// admitting a mutation of both its worktree and shared Git common-dir.
    /// With capture v2 disabled this intentionally follows the existing direct
    /// runner without consulting SQLite or Git.
    pub(crate) async fn run_git_workspace_mutation<T, E, F>(
        &self,
        requested_root: &str,
        operation: F,
    ) -> std::result::Result<T, WorkspaceMutationRequestError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation(requested_root)
            .await
            .map_err(WorkspaceMutationRequestError::Authorization)?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = AuthorizedWorkspaceMutation {
            workspace_absolute: PathBuf::from(requested_root),
        };
        self.runtime
            .run_git_workspace_mutation(binding, operation)
            .await
            .map_err(WorkspaceMutationRequestError::Runtime)
    }

    /// Blocking-executor variant for child-process-capable Git mutations. The
    /// feature-off path preserves the command layer's existing executor and
    /// performs no durable authorization or common-dir inspection.
    pub(crate) async fn run_git_workspace_mutation_blocking<T, E, F>(
        &self,
        requested_root: &str,
        operation: F,
    ) -> std::result::Result<T, WorkspaceMutationRequestError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation(requested_root)
            .await
            .map_err(WorkspaceMutationRequestError::Authorization)?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = AuthorizedWorkspaceMutation {
            workspace_absolute: PathBuf::from(requested_root),
        };
        self.runtime
            .run_git_workspace_mutation_blocking(binding, operation)
            .await
            .map_err(WorkspaceMutationRequestError::Runtime)
    }

    /// Recovery variant for a workspace whose worktree may already be gone.
    /// The workspace id removes ambiguity when several task rows share the
    /// repository root, while the requested root must still match that exact
    /// durable row before the physical Git lease is acquired.
    pub(crate) async fn run_git_workspace_mutation_for_workspace_blocking<T, E, F>(
        &self,
        workspace_id: &WorkspaceId,
        requested_root: &str,
        operation: F,
    ) -> std::result::Result<T, WorkspaceMutationRequestError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation_for_id(workspace_id, requested_root)
            .await
            .map_err(WorkspaceMutationRequestError::Authorization)?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let _ = workspace_id;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = AuthorizedWorkspaceMutation {
            workspace_absolute: PathBuf::from(requested_root),
        };
        self.runtime
            .run_git_workspace_mutation_blocking(binding, operation)
            .await
            .map_err(WorkspaceMutationRequestError::Runtime)
    }

    /// Runs a closure with an authorized primary worktree and a secondary
    /// absolute worktree. On macOS with capture-v2 enabled, the runtime opens
    /// both Git layouts, proves that they share the same physical common-dir,
    /// acquires all three roots atomically, and retains both authorities until
    /// the closure finishes. The command layer remains responsible for
    /// scoping the secondary path to DCC's delegation directory before calling
    /// this foundation API. Durable delegation ownership is intentionally left
    /// to the lifecycle-journal slice.
    #[allow(dead_code)] // Wired by the delegation apply/remove lifecycle.
    pub(crate) async fn run_git_workspace_pair_mutation<T, E, F>(
        &self,
        requested_root: &str,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> std::result::Result<T, WorkspaceMutationRequestError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path, &Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation(requested_root)
            .await
            .map_err(WorkspaceMutationRequestError::Authorization)?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = AuthorizedWorkspaceMutation {
            workspace_absolute: PathBuf::from(requested_root),
        };
        self.runtime
            .run_git_workspace_pair_mutation(binding, secondary_absolute, operation)
            .await
            .map_err(WorkspaceMutationRequestError::Runtime)
    }

    /// Blocking-executor variant for delegation operations that may invoke
    /// Git child processes. Feature-off retains the existing direct path and
    /// blocking worker semantics without SQLite or filesystem inspection.
    #[allow(dead_code)] // Wired by the delegation apply/remove lifecycle.
    pub(crate) async fn run_git_workspace_pair_mutation_blocking<T, E, F>(
        &self,
        requested_root: &str,
        secondary_absolute: PathBuf,
        operation: F,
    ) -> std::result::Result<T, WorkspaceMutationRequestError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&Path, &Path) -> std::result::Result<T, E> + Send + 'static,
    {
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        let binding = self
            .authorize_workspace_mutation(requested_root)
            .await
            .map_err(WorkspaceMutationRequestError::Authorization)?;
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        let binding = AuthorizedWorkspaceMutation {
            workspace_absolute: PathBuf::from(requested_root),
        };
        self.runtime
            .run_git_workspace_pair_mutation_blocking(binding, secondary_absolute, operation)
            .await
            .map_err(WorkspaceMutationRequestError::Runtime)
    }

    /// Claims one in-memory delivery recovery attempt after its workspace
    /// mutation lease has been acquired. The snapshot remains present until
    /// the caller explicitly clears it through the returned RAII claim.
    pub(crate) fn claim_delivery_recovery(
        &self,
        workspace_root: &str,
        operation: WorkspaceDeliveryFailureOperation,
        attempt_token: &str,
    ) -> std::result::Result<DeliveryRecoveryClaim, String> {
        let root = workspace_root.trim();
        let token = attempt_token.trim();
        if root.is_empty() || token.is_empty() {
            return Err("delivery recovery context is stale".to_string());
        }

        let mut store = self
            .delivery_failures
            .lock()
            .map_err(|_| "delivery recovery state is unavailable".to_string())?;
        let key = (root.to_string(), operation);
        let current_token = store
            .failures
            .get(root)
            .and_then(|operations| operations.get(&operation))
            .map(|snapshot| snapshot.attempt_token.as_str())
            .ok_or_else(|| "delivery recovery context is stale".to_string())?;
        if current_token != token {
            return Err("delivery recovery context is stale".to_string());
        }
        if store.active_recovery.contains_key(&key) {
            return Err("delivery recovery is already in progress".to_string());
        }
        let owner = store
            .next_recovery_owner
            .checked_add(1)
            .ok_or_else(|| "delivery recovery state is unavailable".to_string())?;
        store.next_recovery_owner = owner;
        store.active_recovery.insert(key, owner);
        Ok(DeliveryRecoveryClaim {
            store: Arc::clone(&self.delivery_failures),
            root: root.to_string(),
            operation,
            attempt_token: token.to_string(),
            owner,
        })
    }

    pub(crate) fn record_delivery_failure(
        &self,
        snapshot: WorkspaceDeliveryFailureSnapshot,
    ) -> WorkspaceDeliveryFailureSnapshot {
        let Ok(mut store) = self.delivery_failures.lock() else {
            return snapshot;
        };
        let root = snapshot.workspace_root.clone();
        if let Some(existing) = store
            .failures
            .get(&root)
            .and_then(|operations| operations.get(&snapshot.operation))
        {
            let same_failure = existing.branch == snapshot.branch
                && existing.head_sha == snapshot.head_sha
                && existing.classification == snapshot.classification
                && existing.remote == snapshot.remote
                && existing.operation_target == snapshot.operation_target
                && existing.push_target == snapshot.push_target
                && existing.output == snapshot.output
                && existing.changed_files == snapshot.changed_files
                && existing.external_url == snapshot.external_url
                && existing.available_actions == snapshot.available_actions;
            if same_failure {
                return existing.clone();
            }
        }

        if !store.failures.contains_key(&root)
            && store.failures.len() >= DELIVERY_FAILURE_WORKSPACE_LIMIT
        {
            let oldest_root = store
                .failures
                .iter()
                .filter_map(|(candidate_root, operations)| {
                    operations
                        .values()
                        .map(|failure| failure.created_at.as_str())
                        .max()
                        .map(|latest| (candidate_root.clone(), latest.to_string()))
                })
                .min_by(|left, right| left.1.cmp(&right.1))
                .map(|(candidate_root, _)| candidate_root);
            if let Some(oldest_root) = oldest_root {
                store.failures.remove(&oldest_root);
            }
        }

        store
            .failures
            .entry(root)
            .or_default()
            .insert(snapshot.operation, snapshot.clone());
        snapshot
    }

    pub(crate) fn clear_delivery_failure(
        &self,
        workspace_root: &str,
        operation: WorkspaceDeliveryFailureOperation,
    ) {
        let Ok(mut store) = self.delivery_failures.lock() else {
            return;
        };
        let root = workspace_root.trim();
        let remove_root = store
            .failures
            .get_mut(root)
            .map(|operations| {
                operations.remove(&operation);
                operations.is_empty()
            })
            .unwrap_or(false);
        if remove_root {
            store.failures.remove(root);
        }
    }

    pub(crate) fn clear_delivery_failures(&self, workspace_root: &str) {
        let Ok(mut store) = self.delivery_failures.lock() else {
            return;
        };
        store.failures.remove(workspace_root.trim());
    }

    pub(crate) fn has_delivery_failure(&self, workspace_root: &str) -> bool {
        self.delivery_failures
            .lock()
            .ok()
            .and_then(|store| {
                store
                    .failures
                    .get(workspace_root.trim())
                    .map(|operations| !operations.is_empty())
            })
            .unwrap_or(false)
    }

    pub(crate) fn latest_delivery_failure(
        &self,
        workspace_root: &str,
        branch: Option<&str>,
        head_sha: Option<&str>,
    ) -> Option<WorkspaceDeliveryFailureSnapshot> {
        let store = self.delivery_failures.lock().ok()?;
        store
            .failures
            .get(workspace_root.trim())?
            .values()
            .filter(|failure| {
                failure.branch.as_deref() == branch && failure.head_sha.as_deref() == head_sha
            })
            .max_by(|left, right| left.created_at.cmp(&right.created_at))
            .cloned()
    }
}

fn resolve_authorized_workspace_mutation(
    db_path: &Path,
    requested_root: &str,
) -> std::result::Result<AuthorizedWorkspaceMutation, WorkspaceMutationAuthorizationError> {
    let requested_root = requested_root.trim();
    if requested_root.is_empty() || !Path::new(requested_root).is_absolute() {
        return Err(WorkspaceMutationAuthorizationError::InvalidRequest);
    }
    let repo = SqliteWorkspaceRepo::open(db_path)
        .map_err(|_| WorkspaceMutationAuthorizationError::RepositoryUnavailable)?;
    let workspaces = futures::executor::block_on(repo.list_workspaces())
        .map_err(|_| WorkspaceMutationAuthorizationError::RepositoryUnavailable)?;

    let mut selected: Option<PathBuf> = None;
    for workspace in workspaces {
        let stored_root = if workspace.root_path == requested_root {
            Some(workspace.root_path.as_str())
        } else if workspace.worktree_path.as_deref() == Some(requested_root) {
            workspace.worktree_path.as_deref()
        } else {
            None
        };
        let Some(stored_root) = stored_root else {
            continue;
        };
        if selected.is_some() {
            return Err(WorkspaceMutationAuthorizationError::AmbiguousMapping);
        }
        selected = Some(PathBuf::from(stored_root));
    }

    let Some(workspace_absolute) = selected else {
        return Err(WorkspaceMutationAuthorizationError::UnknownMapping);
    };
    Ok(AuthorizedWorkspaceMutation { workspace_absolute })
}

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
fn resolve_authorized_workspace_mutation_for_id(
    db_path: &Path,
    workspace_id: &WorkspaceId,
    requested_root: &str,
) -> std::result::Result<AuthorizedWorkspaceMutation, WorkspaceMutationAuthorizationError> {
    let requested_root = requested_root.trim();
    if requested_root.is_empty() || !Path::new(requested_root).is_absolute() {
        return Err(WorkspaceMutationAuthorizationError::InvalidRequest);
    }
    let repo = SqliteWorkspaceRepo::open(db_path)
        .map_err(|_| WorkspaceMutationAuthorizationError::RepositoryUnavailable)?;
    let workspace = futures::executor::block_on(repo.get_workspace(workspace_id))
        .map_err(|_| WorkspaceMutationAuthorizationError::RepositoryUnavailable)?
        .ok_or(WorkspaceMutationAuthorizationError::UnknownMapping)?;
    if workspace.root_path != requested_root
        && workspace.worktree_path.as_deref() != Some(requested_root)
    {
        return Err(WorkspaceMutationAuthorizationError::UnknownMapping);
    }
    Ok(AuthorizedWorkspaceMutation {
        workspace_absolute: PathBuf::from(requested_root),
    })
}

pub trait EphemeralMcpProjection: Send + Sync {
    fn project_for_session(&self, session: &Session)
        -> Result<Option<EphemeralMcpProjectionLease>>;
    fn revoke_session(&self, session_id: &SessionId, lease_id: &str);
}

#[derive(Clone, Debug)]
pub struct EphemeralMcpProjectionLease {
    pub server: ProviderMcpServerConfig,
    pub lease_id: String,
}

#[derive(Clone)]
pub struct SessionCommandState {
    app_data_dir: PathBuf,
    db_path: PathBuf,
    session_repo: SqliteSessionRepo,
    _event_bus: Arc<dyn EventBus>,
    store: Arc<Mutex<SessionStore>>,
    ephemeral_mcp_projection: Arc<Mutex<Option<Arc<dyn EphemeralMcpProjection>>>>,
    runtime: Arc<ProcessRuntime>,
}

/// Owns the process-shared transition lock for one session's provider
/// selection and binding. Command pipelines retain this through durable turn
/// creation and provider input acceptance so a newer selection cannot receive
/// a prompt prepared for the prior binding.
pub struct ProviderTransitionGuard {
    session_id: SessionId,
    runtime: Arc<ProcessRuntime>,
    lock: Arc<AsyncMutex<()>>,
    guard: Option<tokio::sync::OwnedMutexGuard<()>>,
}

/// Public, server-backed availability for a registered DCC provider. It is
/// separate from adapter health and credentials: disabled means new work is
/// refused even if the installed adapter is otherwise healthy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProviderAvailabilityState {
    Enabled,
    Disabling,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAvailability {
    pub provider_id: String,
    pub enabled: bool,
    pub state: ProviderAvailabilityState,
    pub generation: u64,
}

/// Serializes durable availability changes for one registered provider. It is
/// intentionally independent of per-session transitions: readers only take a
/// synchronous snapshot, preventing a provider-disable/session-start ABBA
/// deadlock.
pub struct ProviderAvailabilityTransitionGuard {
    provider_id: String,
    runtime: Arc<ProcessRuntime>,
    lock: Arc<AsyncMutex<()>>,
    guard: Option<tokio::sync::OwnedMutexGuard<()>>,
}

impl Drop for ProviderAvailabilityTransitionGuard {
    fn drop(&mut self) {
        drop(self.guard.take());
        self.runtime
            .remove_provider_availability_lock_if_idle(&self.provider_id, &self.lock);
    }
}

impl Drop for ProviderTransitionGuard {
    fn drop(&mut self) {
        // Release the session mutex before inspecting the weak registry. A
        // waiter already holding an Arc keeps the entry alive, while a later
        // acquirer after removal can only create a new mutex once no previous
        // holder or waiter remains.
        drop(self.guard.take());
        self.runtime
            .remove_provider_transition_lock_if_idle(&self.session_id, &self.lock);
    }
}

#[derive(Clone, Debug)]
struct ProviderSessionBinding {
    provider_id: String,
    handle: SessionHandle,
    current_turn_id: Arc<AsyncMutex<Option<String>>>,
    // Only coordinates the short binding transition/cleanup section. No
    // provider, database, evidence, or MCP I/O runs while it is held.
    terminal_lock: Arc<AsyncMutex<()>>,
    terminal_token: Arc<TerminalTokenState>,
    usage_turn_id: Arc<AsyncMutex<Option<String>>>,
    assistant_messages: Arc<AsyncMutex<AssistantMessageTracker>>,
    /// Persistent registry definition ids only; ephemeral app-owned MCP
    /// projections are tracked by their opaque lease instead.
    projected_mcp_definition_ids: Arc<HashSet<McpDefinitionId>>,
    ephemeral_mcp_lease_id: Option<String>,
}

#[derive(Clone, Debug)]
enum TerminalRequest {
    Completed,
    Aborted {
        reason: Option<String>,
        source: TerminalSource,
    },
}

#[derive(Clone, Copy, Debug)]
enum TerminalSource {
    ProviderFailed,
    Quiesce,
    Cancel,
    /// A TurnStarted whose provider binding was never installed. This path
    /// intentionally has no M4 capture to finalize.
    Unbound,
}

#[derive(Default)]
struct TerminalTokenState {
    active: Mutex<Option<(String, u64)>>,
    generation: AtomicU64,
}

impl std::fmt::Debug for TerminalTokenState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TerminalTokenState([redacted])")
    }
}

struct TerminalTokenGuard {
    state: Arc<TerminalTokenState>,
    turn_id: String,
    generation: u64,
}

impl Drop for TerminalTokenGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.state.active.lock() {
            if active.as_ref().is_some_and(|(turn, generation)| {
                turn == &self.turn_id && *generation == self.generation
            }) {
                *active = None;
            }
        }
    }
}

#[derive(Clone, Debug)]
struct CanonicalTerminalResult {
    outcome: TerminalIntent,
    inserted: bool,
}

#[derive(Clone, Debug)]
struct TerminalPersistence {
    record: SessionEventRecord,
    inserted: bool,
}

#[derive(Default, Debug)]
struct AssistantMessageTracker {
    active: HashMap<String, AssistantMessagePhase>,
    synthetic_current: Option<String>,
    synthetic_index: u32,
}

impl AssistantMessageTracker {
    fn synthetic_append_target(&mut self, turn_id: &str) -> (String, bool) {
        if let Some(message_id) = self.synthetic_current.clone() {
            return (message_id, false);
        }
        let message_id = format!("assistant:{turn_id}:synthetic-{}", self.synthetic_index);
        self.synthetic_index += 1;
        self.synthetic_current = Some(message_id.clone());
        self.active
            .insert(message_id.clone(), AssistantMessagePhase::Unknown);
        (message_id, true)
    }

    fn take_synthetic_completion(&mut self) -> Option<(String, AssistantMessagePhase)> {
        let message_id = self.synthetic_current.take()?;
        let phase = self
            .active
            .remove(&message_id)
            .unwrap_or(AssistantMessagePhase::Unknown);
        Some((message_id, phase))
    }
}

#[derive(Default, Debug)]
pub(crate) struct SessionStore {
    provider_sessions: HashMap<SessionId, ProviderSessionBinding>,
    mcp_runtime_statuses: HashMap<SessionId, Vec<McpRuntimeStatus>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct NoopEventBus;

#[async_trait]
impl EventBus for NoopEventBus {
    async fn publish(&self, _event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        Ok(())
    }
}

impl SessionCommandState {
    pub(crate) fn runtime_generation(&self) -> String {
        self.runtime.runtime_generation().to_string()
    }

    pub fn new(app: AppHandle, db_path: PathBuf) -> Self {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        Self::from_parts(
            db_path,
            app_data_dir,
            Arc::new(TauriEventBus::new(app)),
            true,
        )
    }

    pub fn new_headless(db_path: PathBuf, app_data_dir: PathBuf) -> Self {
        Self::from_parts(db_path, app_data_dir, Arc::new(NoopEventBus), true)
    }

    pub fn new_with_event_bus(
        db_path: PathBuf,
        app_data_dir: PathBuf,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self::from_parts(db_path, app_data_dir, event_bus, false)
    }

    fn from_parts(
        db_path: PathBuf,
        app_data_dir: PathBuf,
        event_bus: Arc<dyn EventBus>,
        recover_interrupted: bool,
    ) -> Self {
        let registry_db_path = lexical_absolute_path(&db_path);
        let registry_app_data_dir = lexical_absolute_path(&app_data_dir);
        let (session_repo, runtime) = ProcessRuntimeRegistry::global()
            .acquire_after_open(&registry_db_path, &registry_app_data_dir, || {
                std::fs::create_dir_all(&app_data_dir).map_err(|_| {
                    dcc_core::CoreError::Repository("failed to initialize app data".to_string())
                })?;
                SqliteSessionRepo::open(&db_path)
            })
            .unwrap_or_else(|_| panic!("failed to initialize session runtime"));
        runtime
            .register_event_bus(&event_bus)
            .unwrap_or_else(|_| panic!("failed to initialize session runtime"));
        // Absence is explicitly enabled for backwards compatibility. Seed
        // only once per physical runtime so a clone cannot overwrite an
        // in-flight Disabling state with an older on-disk snapshot.
        for provider_id in PROVIDER_IDS {
            let state = match session_repo.load_provider_availability(provider_id) {
                Ok(Some(record)) => ProviderAvailabilityRuntimeState {
                    phase: if record.enabled {
                        ProviderAvailabilityPhase::Enabled
                    } else {
                        ProviderAvailabilityPhase::Disabled
                    },
                    generation: record.generation,
                },
                Ok(None) => ProviderAvailabilityRuntimeState {
                    phase: ProviderAvailabilityPhase::Enabled,
                    generation: 0,
                },
                // The availability table is server authority. A corrupted or
                // temporarily unreadable row must refuse new provider work,
                // never crash initialization or silently reopen adapters.
                Err(_) => ProviderAvailabilityRuntimeState {
                    phase: ProviderAvailabilityPhase::Disabled,
                    generation: 0,
                },
            };
            // ProcessRuntime was already acquired successfully. If its small
            // in-memory cache is poisoned, keep the existing constructor's
            // fail-closed runtime behavior rather than introducing a second
            // panic path for availability initialization.
            let _ = runtime.initialize_provider_availability_state(provider_id, state);
        }
        if recover_interrupted {
            cleanup_all_snapshot_quarantines(&app_data_dir.join("turn-review").join("snapshots"));
            let now = Utc::now().to_rfc3339();
            let _ = session_repo
                .recover_interrupted_turn_change_sets(&now)
                .unwrap_or_default();
            // A turn left running by a dead process must end in durable
            // history, otherwise the session stays blocked forever.
            match session_repo.recover_orphaned_running_turns(&now) {
                Ok(recovered) if !recovered.is_empty() => {
                    eprintln!(
                        "[DCC] recovered {} turn(s) interrupted by the previous process",
                        recovered.len()
                    );
                }
                Ok(_) => {}
                Err(error) => eprintln!("[DCC] orphaned turn recovery failed: {error}"),
            }
        }
        Self {
            app_data_dir,
            session_repo,
            db_path,
            _event_bus: event_bus,
            store: runtime.session_store(),
            ephemeral_mcp_projection: Arc::new(Mutex::new(None)),
            runtime,
        }
    }

    pub fn process_runtime(&self) -> Arc<ProcessRuntime> {
        Arc::clone(&self.runtime)
    }

    /// Installs the one app-owned ephemeral MCP projection factory. The
    /// factory is deliberately shared by all clones of this state and cannot
    /// be replaced after setup, so existing provider sessions never receive a
    /// surprise hot injection.
    pub fn install_ephemeral_mcp_projection(
        &self,
        projection: Arc<dyn EphemeralMcpProjection>,
    ) -> Result<()> {
        let mut installed = self
            .ephemeral_mcp_projection
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        if installed.is_some() {
            return Err(dcc_core::CoreError::InvalidInput(
                "ephemeral MCP projection is already installed".to_string(),
            ));
        }
        *installed = Some(projection);
        Ok(())
    }

    fn ephemeral_mcp_projection(&self) -> Result<Option<Arc<dyn EphemeralMcpProjection>>> {
        self.ephemeral_mcp_projection
            .lock()
            .map(|projection| projection.clone())
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    fn project_ephemeral_mcp_server(
        &self,
        provider_projection_version: Option<&str>,
        session: &Session,
    ) -> Result<Option<EphemeralMcpProjectionLease>> {
        if provider_projection_version.is_none() {
            return Ok(None);
        }
        let projection = match self.ephemeral_mcp_projection() {
            Ok(projection) => projection,
            Err(_) => {
                eprintln!("[DCC] ephemeral MCP projection unavailable; continuing without it");
                return Ok(None);
            }
        };
        let Some(projection) = projection else {
            return Ok(None);
        };
        let lease = match projection.project_for_session(session) {
            Ok(lease) => lease,
            Err(_) => {
                eprintln!("[DCC] ephemeral MCP projection unavailable; continuing without it");
                return Ok(None);
            }
        };
        if let Some(lease) = lease.as_ref() {
            if lease.lease_id.trim().is_empty()
                || lease.lease_id.chars().count() > EPHEMERAL_MCP_LEASE_ID_MAX_CHARS
            {
                projection.revoke_session(&session.id, &lease.lease_id);
                eprintln!(
                    "[DCC] ephemeral MCP projection returned an invalid lease; continuing without it"
                );
                return Ok(None);
            }
        }
        Ok(lease)
    }

    fn revoke_ephemeral_mcp_projection(&self, session_id: &SessionId, lease_id: Option<&str>) {
        let Some(lease_id) = lease_id else {
            return;
        };
        let projection = self.ephemeral_mcp_projection().ok().flatten();
        if let Some(projection) = projection {
            projection.revoke_session(session_id, lease_id);
        }
    }

    /// Persists only a sanitized, expiring Browser URL for the supplied
    /// workspace/session scope. Callers own URL policy and should treat an
    /// error as best-effort persistence failure.
    pub fn save_browser_location(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        safe_url: &str,
        saved_at_ms: i64,
        expires_at_ms: i64,
    ) -> Result<()> {
        self.session_repo.save_browser_location(
            workspace_id,
            session_id,
            safe_url,
            saved_at_ms,
            expires_at_ms,
        )
    }

    /// Loads a current Browser URL, removing an expired row opportunistically.
    pub fn load_browser_location(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
        now_ms: i64,
    ) -> Result<Option<String>> {
        self.session_repo
            .load_browser_location(workspace_id, session_id, now_ms)
    }

    pub fn delete_browser_location(
        &self,
        workspace_id: &str,
        session_id: Option<&str>,
    ) -> Result<bool> {
        self.session_repo
            .delete_browser_location(workspace_id, session_id)
    }

    pub fn delete_browser_locations_for_workspace(&self, workspace_id: &str) -> Result<usize> {
        self.session_repo
            .delete_browser_locations_for_workspace(workspace_id)
    }

    pub(crate) fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }

    pub(crate) async fn prepare_guarded_undo(
        &self,
        snapshot_id: String,
    ) -> GuardedUndoPrepareResult {
        let unavailable = |reason_code: &str| GuardedUndoPrepareResult::Unavailable {
            snapshot_id: snapshot_id.clone(),
            reason_code: reason_code.to_owned(),
        };
        let Some((restore, _)) = self
            .session_repo
            .get_turn_restore_set_by_snapshot(&snapshot_id)
            .ok()
            .flatten()
        else {
            return unavailable("capture_v2_missing");
        };
        let workspace_repo = match SqliteWorkspaceRepo::open(&self.db_path) {
            Ok(repo) => repo,
            Err(_) => return unavailable("workspace_missing"),
        };
        let Some(workspace) = WorkspaceRepo::get_workspace(&workspace_repo, &restore.workspace_id)
            .await
            .ok()
            .flatten()
        else {
            return unavailable("workspace_missing");
        };
        let durable_path = workspace
            .worktree_path
            .as_deref()
            .unwrap_or(workspace.root_path.as_str());
        let Some(workspace_absolute) = Self::lexical_absolute_root(Path::new(durable_path)) else {
            return unavailable("workspace_missing");
        };

        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        {
            let configured = self
                .runtime
                .configure_guarded_undo_capture(&self.db_path, &self.app_data_dir)
                .await;
            if !matches!(
                configured,
                Ok(ConfigureOutcome::Configured | ConfigureOutcome::AlreadyConfigured)
            ) {
                return unavailable("adapter_unsupported");
            }
            let Some(roots) = self.guarded_undo_recovery_roots().await else {
                return unavailable("operation_interrupted");
            };
            if !matches!(
                self.runtime
                    .guarded_undo_runtime()
                    .recovery_all(roots)
                    .await,
                Ok(RecoveryOutcome::Recovered | RecoveryOutcome::AlreadyRecovered)
            ) {
                return unavailable("operation_interrupted");
            }
        }

        self.runtime
            .guarded_undo_runtime()
            .prepare_guarded_undo(snapshot_id, workspace_absolute)
            .await
    }

    pub(crate) async fn execute_guarded_undo(
        &self,
        preview_token: String,
        confirmed: bool,
    ) -> GuardedUndoExecuteResult {
        self.runtime
            .guarded_undo_runtime()
            .execute_guarded_undo(preview_token, confirmed)
            .await
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    async fn guarded_undo_recovery_roots(&self) -> Option<Vec<PathBuf>> {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let workspace_repo = SqliteWorkspaceRepo::open(&db_path).ok()?;
            let workspaces = futures::executor::block_on(workspace_repo.list_workspaces()).ok()?;
            let mut roots = Vec::new();
            for workspace in workspaces {
                let path = workspace
                    .worktree_path
                    .as_deref()
                    .unwrap_or(workspace.root_path.as_str());
                let root = Self::lexical_absolute_root(Path::new(path))?;
                match std::fs::metadata(&root) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    _ => {}
                }
                if !roots.iter().any(|existing| existing == &root) {
                    roots.push(root);
                }
            }
            (!roots.is_empty()).then_some(roots)
        })
        .await
        .ok()
        .flatten()
    }

    fn lock_store(&self) -> Result<std::sync::MutexGuard<'_, SessionStore>> {
        self.store
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
    }

    fn provider_binding(&self, session_id: &SessionId) -> Result<Option<ProviderSessionBinding>> {
        let store = self.lock_store()?;
        Ok(store.provider_sessions.get(session_id).cloned())
    }

    /// Normalizes and validates a runtime config against the registered
    /// adapter contract. Fields the adapter would silently ignore are rejected
    /// so attach, turn selection, delegation, review runs, and usage lookups
    /// all fail closed instead of pretending a preference took effect.
    pub(crate) fn provider_runtime_config(
        &self,
        provider_id: &str,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<ProviderRuntimeConfig> {
        let runtime = runtime.cloned().unwrap_or_default();
        if self.is_legacy_managed_provider_home(provider_id, &runtime) {
            return Ok(ProviderRuntimeConfig::default());
        }
        let registration = registered_provider(provider_id)?;
        validate_provider_runtime_config(provider_id, &registration.capabilities, &runtime)
            .map_err(dcc_core::CoreError::Provider)?;
        Ok(runtime)
    }

    pub async fn run_ephemeral_read_only_turn(
        &self,
        working_directory: String,
        provider_id: String,
        model: Option<String>,
        runtime_config: Option<ProviderRuntimeConfig>,
        prompt: String,
    ) -> Result<String> {
        let provider = require_provider_capability(
            &provider_id,
            ProviderCapability::ReadOnlyDelegation,
            "read-only review runs",
        )?
        .runtime;
        // This work has no durable ProviderSessionBinding for disable to
        // drain. Hold the provider availability transition through the first
        // accepted input so disable orders entirely before or after runtime
        // initialization. Once input is accepted the review may finish: the
        // availability contract cancels tracked bindings and blocks new
        // starts, rather than retroactively terminating this isolated run.
        let availability_transition = self
            .acquire_provider_availability_transition(&provider_id)
            .await?;
        let availability = self.provider_availability_snapshot(&provider_id)?;
        if !availability.enabled {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {provider_id} is disabled"
            )));
        }
        let runtime = self.provider_runtime_config(&provider_id, runtime_config.as_ref())?;
        let ephemeral_id = Uuid::new_v4().to_string();
        let handle = provider
            .prepare_session(SessionConfig {
                workspace_id: WorkspaceId(format!("pr-review:{ephemeral_id}")),
                session_id: SessionId(ephemeral_id),
                model,
                working_directory: Some(working_directory),
                additional_working_directories: Vec::new(),
                provider_runtime: Some(runtime),
                mcp_servers: Vec::new(),
            })
            .await?;
        let mut events = provider.stream_events(&handle);
        if let Err(error) = provider
            .send_input(
                &handle,
                Input::Turn(dcc_core::ports::ProviderTurnInput {
                    prompt,
                    tool_instructions: Some(
                        "Read-only pull request review. Do not edit files, execute mutating commands, create branches, commits, tasks, worktrees, or publish anything. Return only the requested draft response."
                            .to_string(),
                    ),
                    plan_mode: Some(true),
                    effort: None,
                    fast_mode: None,
                    approval_policy: None,
                }),
            )
            .await
        {
            return match provider.cancel(&handle).await {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(dcc_core::CoreError::Provider(format!(
                    "read-only review input failed: {error}; prepared handle cleanup failed: {cleanup_error}"
                ))),
            };
        }
        // Do not hold availability through the five-minute stream. A disable
        // that wins after input was accepted deliberately does not abort this
        // untracked, read-only review.
        drop(availability_transition);

        let result = tokio::time::timeout(std::time::Duration::from_secs(300), async {
            let mut response = String::new();
            while let Some(event) = events.next().await {
                match event? {
                    ProviderEvent::TextDelta { content } => response.push_str(&content),
                    ProviderEvent::AssistantMessageDelta { content, .. } => {
                        response.push_str(&content)
                    }
                    ProviderEvent::AssistantMessageCompleted {
                        phase: AssistantMessagePhase::FinalAnswer,
                        content: Some(content),
                        ..
                    } => response = content,
                    ProviderEvent::Completed { .. } => return Ok(response),
                    ProviderEvent::Failed { message, .. } => {
                        return Err(dcc_core::CoreError::Provider(message));
                    }
                    ProviderEvent::PermissionRequested { .. }
                    | ProviderEvent::UserInputRequested { .. } => {
                        return Err(dcc_core::CoreError::Provider(
                            "The review agent requested an interactive or mutating action. The run was cancelled."
                                .to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            Err(dcc_core::CoreError::Provider(
                "The review agent ended without a completed response.".to_string(),
            ))
        })
        .await
        .map_err(|_| {
            dcc_core::CoreError::Provider("The review agent timed out after 5 minutes.".to_string())
        });
        // Preserve the timeout error while still disposing the prepared
        // handle; a timeout is also a post-prepare failure.
        let result = match result {
            Ok(result) => result,
            Err(error) => Err(error),
        };
        let response = match (result, provider.cancel(&handle).await) {
            (Ok(response), Ok(())) => response,
            (Err(error), Ok(())) => return Err(error),
            (Ok(_), Err(cleanup_error)) => {
                return Err(dcc_core::CoreError::Provider(format!(
                    "read-only review completed but handle cleanup failed: {cleanup_error}"
                )));
            }
            (Err(error), Err(cleanup_error)) => {
                return Err(dcc_core::CoreError::Provider(format!(
                    "read-only review failed: {error}; prepared handle cleanup failed: {cleanup_error}"
                )));
            }
        };
        if response.trim().is_empty() {
            return Err(dcc_core::CoreError::Provider(
                "The review agent returned an empty response.".to_string(),
            ));
        }
        Ok(response)
    }

    fn provider_home_root(&self) -> PathBuf {
        self.app_data_dir.join("provider-homes")
    }

    fn is_legacy_managed_provider_home(
        &self,
        provider_id: &str,
        runtime: &ProviderRuntimeConfig,
    ) -> bool {
        if !matches!(provider_id, "claude_code" | "gemini" | "grok") {
            return false;
        }

        if runtime.shadow_home_path.is_some() {
            return false;
        }

        let Some(home_path) = runtime.home_path.as_deref() else {
            return false;
        };

        PathBuf::from(home_path) == self.provider_home_root().join(provider_id)
    }

    pub async fn peek_session(&self, session_id: &SessionId) -> Result<Option<Session>> {
        SessionRepo::get_session(&self.session_repo, session_id).await
    }

    pub fn list_workspace_sessions(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<WorkspaceSessionSummary>> {
        self.session_repo.list_workspace_sessions(workspace_id)
    }

    pub fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSearchResult>> {
        self.session_repo.search_sessions(query, limit)
    }

    pub async fn usage_dashboard(&self, input: &UsageDashboardInput) -> Result<UsageDashboard> {
        UsageRepo::usage_dashboard(&self.session_repo, input).await
    }

    async fn record_turn_usage(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        recorded_at: &str,
        models: &[ModelTokenUsage],
    ) -> Result<()> {
        UsageRepo::replace_turn_usage(&self.session_repo, session_id, turn_id, recorded_at, models)
            .await
    }

    pub(crate) async fn append_session_event(
        &self,
        session_id: &SessionId,
        kind: SessionEventKind,
    ) -> Result<AppendEventOutcome> {
        let events =
            SessionEventRepo::list_events_by_session(&self.session_repo, session_id).await?;
        let sequence = events.last().map(|event| event.sequence + 1).unwrap_or(1);
        let record = SessionEventRecord {
            event_id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            sequence,
            occurred_at: Utc::now().to_rfc3339(),
            kind,
        };
        SessionEventRepo::append_event(&self.session_repo, &record).await
    }

    fn turn_review_snapshot_root(&self, snapshot_id: &str) -> PathBuf {
        self.app_data_dir
            .join("turn-review")
            .join("snapshots")
            .join(snapshot_id)
    }

    fn turn_review_compatibility_root(&self) -> PathBuf {
        self.turn_review_snapshot_root(&Uuid::new_v4().to_string())
    }

    pub fn list_turn_change_sets(&self, session_id: &SessionId) -> Result<Vec<TurnChangeSet>> {
        self.session_repo
            .list_turn_change_sets_by_session(session_id)
    }

    pub fn get_turn_change_set(&self, snapshot_id: &str) -> Result<Option<TurnChangeSet>> {
        self.session_repo.get_turn_change_set(snapshot_id)
    }

    pub async fn normalize_interrupted_turn_change_set(
        &self,
        change_set: TurnChangeSet,
    ) -> Result<TurnChangeSet> {
        // A missing provider binding is only a transient runtime observation:
        // startup and attach can both briefly have no binding. Reads therefore
        // never mutate durable capture state. Terminal paths own finalization.
        Ok(change_set)
    }

    pub async fn turn_change_set_compatibility(&self, change_set: &TurnChangeSet) -> String {
        if !matches!(
            change_set.state.as_str(),
            "available" | "partial" | "no_changes"
        ) {
            return "unavailable".to_string();
        }
        let Some(expected_tree) = change_set.result_tree.as_deref() else {
            return "unavailable".to_string();
        };
        let captured_new_untracked = change_set
            .files
            .iter()
            .filter(|file| file.untracked)
            .count();
        if !change_set.baseline_untracked.is_empty()
            || captured_new_untracked != change_set.result_untracked.len()
        {
            return "unavailable".to_string();
        }
        let Ok(Some(session)) =
            SessionRepo::get_session(&self.session_repo, &change_set.session_id).await
        else {
            return "unavailable".to_string();
        };
        let Ok(roots) = self.turn_review_roots(&session).await else {
            return "unavailable".to_string();
        };
        let Some((_, root)) = roots
            .into_iter()
            .find(|(workspace_id, _)| workspace_id == &change_set.workspace_id)
        else {
            return "unavailable".to_string();
        };
        match current_snapshot_matches(
            &root,
            &self.turn_review_compatibility_root(),
            expected_tree,
            &change_set.baseline_untracked,
            &change_set.result_untracked,
        ) {
            Ok(true) => "matches_result".to_string(),
            Ok(false) => "diverged".to_string(),
            Err(_) => "unavailable".to_string(),
        }
    }

    async fn turn_review_roots(&self, session: &Session) -> Result<Vec<(WorkspaceId, String)>> {
        let (primary, additional) = self
            .resolve_session_working_directories(session, true)
            .await?;
        let mut roots = vec![(session.workspace_id.clone(), primary)];
        roots.extend(
            session
                .additional_workspace_ids
                .iter()
                .cloned()
                .zip(additional),
        );
        Ok(roots)
    }

    fn lexical_absolute_root(path: &Path) -> Option<PathBuf> {
        if !path.is_absolute() {
            return None;
        }
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir => return None,
                Component::CurDir => {}
                Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                    normalized.push(component.as_os_str())
                }
            }
        }
        Some(normalized)
    }

    pub async fn capture_turn_review_baseline(
        &self,
        session: &Session,
        turn_id: &TurnId,
    ) -> Result<M3BaselineCapture> {
        let now = Utc::now().to_rfc3339();
        let roots = match self.turn_review_roots(session).await {
            Ok(roots) => roots,
            Err(error) => {
                let unavailable = TurnChangeSet {
                    snapshot_id: Uuid::new_v4().to_string(),
                    session_id: session.id.clone(),
                    turn_id: turn_id.clone(),
                    workspace_id: session.workspace_id.clone(),
                    capture_version: TURN_REVIEW_CAPTURE_VERSION,
                    state: "unavailable".to_string(),
                    base_tree: None,
                    result_tree: None,
                    baseline_untracked: Vec::new(),
                    result_untracked: Vec::new(),
                    files: Vec::new(),
                    file_diffs: Default::default(),
                    observed_validations: Vec::new(),
                    diff_truncated: false,
                    turn_outcome: None,
                    outcome_reason: None,
                    error: Some(error.to_string()),
                    created_at: now.clone(),
                    completed_at: Some(now),
                };
                self.session_repo.save_turn_change_set(&unavailable)?;
                return Ok(M3BaselineCapture {
                    snapshots: vec![M3SnapshotRef::after_persist(&unavailable)],
                    root_bindings: Vec::new(),
                });
            }
        };
        let mut snapshots = Vec::with_capacity(roots.len());
        let mut root_bindings = Vec::with_capacity(roots.len());
        for (workspace_id, root) in roots {
            let workspace_absolute = Self::lexical_absolute_root(Path::new(&root));
            #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
            let physical_before = workspace_absolute.as_ref().and_then(|workspace_absolute| {
                dcc_infra::guarded_undo::macos_root::MacWorkspaceRoot::open_absolute(
                    workspace_absolute,
                )
                .ok()
                .map(|opened| opened.physical_root_id())
            });
            let snapshot_id = Uuid::new_v4().to_string();
            let snapshot_root = self.turn_review_snapshot_root(&snapshot_id);
            let (state, base_tree, baseline_untracked, error, completed_at) =
                if !dcc_infra::git::is_git_repo(PathBuf::from(&root).as_path()) {
                    (
                        "unavailable".to_string(),
                        None,
                        Vec::new(),
                        Some("workspace is not an available Git worktree".to_string()),
                        Some(now.clone()),
                    )
                } else {
                    match capture_baseline(&root, &snapshot_root) {
                        Ok(baseline) => (
                            "collecting".to_string(),
                            Some(baseline.tree),
                            baseline.untracked,
                            None,
                            None,
                        ),
                        Err(error) => {
                            cleanup_snapshot(&snapshot_root);
                            (
                                "failed".to_string(),
                                None,
                                Vec::new(),
                                Some(error),
                                Some(now.clone()),
                            )
                        }
                    }
                };
            let keep_quarantine = state == "collecting";
            let change_set = TurnChangeSet {
                snapshot_id,
                session_id: session.id.clone(),
                turn_id: turn_id.clone(),
                workspace_id,
                capture_version: TURN_REVIEW_CAPTURE_VERSION,
                state,
                base_tree,
                result_tree: None,
                baseline_untracked,
                result_untracked: Vec::new(),
                files: Vec::new(),
                file_diffs: Default::default(),
                observed_validations: Vec::new(),
                diff_truncated: false,
                turn_outcome: None,
                outcome_reason: None,
                error,
                created_at: now.clone(),
                completed_at,
            };
            if let Err(error) = self.session_repo.save_turn_change_set(&change_set) {
                cleanup_snapshot(&snapshot_root);
                return Err(error);
            }
            snapshots.push(M3SnapshotRef::after_persist(&change_set));
            if keep_quarantine {
                #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
                if let (Some(workspace_absolute), Some(expected)) =
                    (workspace_absolute, physical_before)
                {
                    if let Ok(reopened) =
                        dcc_infra::guarded_undo::macos_root::MacWorkspaceRoot::open_absolute(
                            &workspace_absolute,
                        )
                    {
                        if reopened.physical_root_id() == expected {
                            root_bindings.push(M3RootBinding {
                                workspace_id: change_set.workspace_id.clone(),
                                workspace_absolute,
                                physical_root_id: expected,
                            });
                        }
                    }
                }
                #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
                if let Some(workspace_absolute) = workspace_absolute {
                    root_bindings.push(M3RootBinding {
                        workspace_id: change_set.workspace_id.clone(),
                        workspace_absolute,
                    });
                }
            }
            if !keep_quarantine {
                cleanup_snapshot(&snapshot_root);
            }
        }
        Ok(M3BaselineCapture {
            snapshots,
            root_bindings,
        })
    }

    /// Starts capture-v2 only from the exact M3 rows just persisted for this
    /// turn. Any validation or adapter failure is deliberately fail-soft: the
    /// provider turn may continue with the M3 path and terminalization remains
    /// responsible for durable session state.
    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    pub async fn begin_capture_v2_after_m3(
        &self,
        session: &Session,
        turn_id: &TurnId,
        baseline: M3BaselineCapture,
    ) -> CaptureV2StartReport {
        const MAX_CAPTURE_ROOTS: usize = 1;
        if baseline.snapshots.len() != MAX_CAPTURE_ROOTS {
            return CaptureV2StartReport::skipped();
        }

        let roots = match self.turn_review_roots(session).await {
            Ok(roots) => roots,
            Err(_) => return CaptureV2StartReport::skipped(),
        };
        if roots.len() != baseline.snapshots.len() {
            return CaptureV2StartReport::skipped();
        }

        let mut roots_by_workspace = HashMap::with_capacity(roots.len());
        for (workspace_id, root) in roots {
            let Some(root) = Self::lexical_absolute_root(Path::new(&root)) else {
                return CaptureV2StartReport::skipped();
            };
            if roots_by_workspace.insert(workspace_id, root).is_some() {
                return CaptureV2StartReport::skipped();
            }
        }

        if baseline.root_bindings.len() != baseline.snapshots.len() {
            return CaptureV2StartReport::skipped();
        }
        let mut bindings_by_workspace = HashMap::with_capacity(baseline.root_bindings.len());
        for binding in &baseline.root_bindings {
            if bindings_by_workspace
                .insert(binding.workspace_id.clone(), binding)
                .is_some()
            {
                return CaptureV2StartReport::skipped();
            }
            if roots_by_workspace.get(&binding.workspace_id) != Some(&binding.workspace_absolute) {
                return CaptureV2StartReport::skipped();
            }
        }

        let mut requested_workspaces = HashSet::with_capacity(baseline.snapshots.len());
        for snapshot in &baseline.snapshots {
            if snapshot.session_id != session.id
                || snapshot.turn_id != *turn_id
                || !requested_workspaces.insert(snapshot.workspace_id.clone())
            {
                return CaptureV2StartReport::skipped();
            }
            let Some(root) = roots_by_workspace.get(&snapshot.workspace_id) else {
                return CaptureV2StartReport::skipped();
            };
            if !bindings_by_workspace.contains_key(&snapshot.workspace_id) {
                return CaptureV2StartReport::skipped();
            }
            if roots_by_workspace
                .values()
                .any(|other| other != root && (other.starts_with(root) || root.starts_with(other)))
            {
                return CaptureV2StartReport::skipped();
            }
        }
        if requested_workspaces.len() != roots_by_workspace.len() {
            return CaptureV2StartReport::skipped();
        }

        let references = baseline.snapshots.clone();
        let session_id = session.id.clone();
        let expected_turn = turn_id.clone();
        let repo = self.session_repo.clone();
        let rows_valid = tokio::task::spawn_blocking(move || {
            references.iter().all(|reference| {
                let Ok(Some(row)) = repo.get_turn_change_set(&reference.snapshot_id) else {
                    return false;
                };
                row.snapshot_id == reference.snapshot_id
                    && row.session_id == session_id
                    && row.session_id == reference.session_id
                    && row.turn_id == expected_turn
                    && row.turn_id == reference.turn_id
                    && row.workspace_id == reference.workspace_id
                    && row.capture_version == TURN_REVIEW_CAPTURE_VERSION
                    && row.state == "collecting"
                    && row.turn_outcome.is_none()
                    && row.completed_at.is_none()
            })
        })
        .await
        .unwrap_or(false);
        if !rows_valid {
            return CaptureV2StartReport::skipped();
        }

        let runtime = self.runtime.guarded_undo_runtime();
        let configured = match self
            .runtime
            .configure_guarded_undo_capture(&self.db_path, &self.app_data_dir)
            .await
        {
            Ok(outcome) => outcome,
            Err(_) => return CaptureV2StartReport::skipped(),
        };
        if matches!(configured, ConfigureOutcome::Disabled) {
            return CaptureV2StartReport::disabled();
        }
        if matches!(configured, ConfigureOutcome::Unavailable) {
            return CaptureV2StartReport::skipped();
        }

        let Some(recovery_roots) = self.capture_v2_recovery_roots(session).await else {
            return CaptureV2StartReport::skipped();
        };
        match runtime.recovery_all(recovery_roots).await {
            Ok(RecoveryOutcome::Recovered | RecoveryOutcome::AlreadyRecovered) => {}
            Ok(RecoveryOutcome::Disabled | RecoveryOutcome::Unavailable) | Err(_) => {
                return CaptureV2StartReport::skipped()
            }
        }

        // Recovery/configuration can yield while workspace metadata changes.
        // Re-read the cooperative mapping immediately before handing the
        // immutable M3 binding to the blocking capture worker.
        let late_roots = match self.turn_review_roots(session).await {
            Ok(roots) if roots.len() == bindings_by_workspace.len() => roots,
            _ => return CaptureV2StartReport::skipped(),
        };
        for (workspace_id, root) in late_roots {
            let Some(root) = Self::lexical_absolute_root(Path::new(&root)) else {
                return CaptureV2StartReport::skipped();
            };
            let Some(binding) = bindings_by_workspace.get(&workspace_id) else {
                return CaptureV2StartReport::skipped();
            };
            if binding.workspace_absolute != root {
                return CaptureV2StartReport::skipped();
            }
        }

        let requests = baseline
            .snapshots
            .into_iter()
            .filter_map(|snapshot| {
                roots_by_workspace
                    .get(&snapshot.workspace_id)
                    .and_then(|workspace_absolute| {
                        bindings_by_workspace
                            .get(&snapshot.workspace_id)
                            .map(|binding| (workspace_absolute.clone(), *binding))
                    })
                    .map(|(workspace_absolute, binding)| GuardedUndoCaptureRequest {
                        snapshot_id: snapshot.snapshot_id,
                        session_id: snapshot.session_id,
                        turn_id: snapshot.turn_id,
                        workspace_id: snapshot.workspace_id,
                        workspace_absolute,
                        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
                        expected_physical_root_id: binding.physical_root_id.clone(),
                    })
            })
            .collect::<Vec<_>>();
        if requests.len() != requested_workspaces.len() {
            return CaptureV2StartReport::skipped();
        }

        let key = TerminalKey::new(session.id.clone(), turn_id.clone());
        let report = match runtime.begin_turn(key, requests).await {
            Ok(report) => report,
            Err(_) => return CaptureV2StartReport::skipped(),
        };
        match report.disposition {
            BeginDisposition::Disabled => CaptureV2StartReport::disabled(),
            BeginDisposition::Started | BeginDisposition::Replayed => CaptureV2StartReport {
                disposition: CaptureV2StartDisposition::Started,
                active_captures: report.active_captures,
                failed_captures: report.failed_captures,
            },
        }
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    pub async fn begin_capture_v2_after_m3(
        &self,
        _session: &Session,
        _turn_id: &TurnId,
        _baseline: M3BaselineCapture,
    ) -> CaptureV2StartReport {
        CaptureV2StartReport::disabled()
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    async fn capture_v2_recovery_roots(&self, session: &Session) -> Option<Vec<PathBuf>> {
        let db_path = self.db_path.clone();
        let mut roots = tokio::task::spawn_blocking(move || {
            let workspace_repo = SqliteWorkspaceRepo::open(&db_path).ok()?;
            let workspaces = futures::executor::block_on(workspace_repo.list_workspaces()).ok()?;
            let mut roots = Vec::new();
            for workspace in workspaces {
                let path = workspace
                    .worktree_path
                    .as_deref()
                    .unwrap_or(workspace.root_path.as_str());
                let root = Self::lexical_absolute_root(Path::new(path))?;
                // A removed worktree is a stale registry entry, not an
                // authorized live root. Skip NotFound only; permission,
                // alias, type, and policy errors remain in the set so the
                // platform recovery adapter can fail closed.
                match std::fs::metadata(&root) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    _ => {}
                }
                if !roots.iter().any(|existing| existing == &root) {
                    roots.push(root);
                }
            }
            Some(roots)
        })
        .await
        .ok()
        .flatten()?;
        let session_roots = self.turn_review_roots(session).await.ok()?;
        for (_, path) in session_roots {
            let root = Self::lexical_absolute_root(Path::new(&path))?;
            if !roots.iter().any(|existing| existing == &root) {
                roots.push(root);
            }
        }
        Some(roots)
    }

    /// Finalizes immutable review evidence before TurnCompleted is made visible.
    /// Failure is represented as a durable review state instead of failing the
    /// provider turn itself.
    pub async fn capture_turn_review_result(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        turn_outcome: &str,
        outcome_reason: Option<&str>,
        force_partial: bool,
    ) -> Result<Vec<TurnChangeSet>> {
        let history =
            SessionEventRepo::list_events_by_session(&self.session_repo, session_id).await?;
        let session = SessionRepo::get_session(&self.session_repo, session_id).await?;
        let observed_validations =
            observed_validations_for_turn(history.iter().filter_map(|event| match &event.kind {
                SessionEventKind::TurnToolCallStarted {
                    turn_id: candidate_turn_id,
                    command: Some(command),
                    ..
                } if candidate_turn_id == turn_id => Some(command.clone()),
                _ => None,
            }));
        let now = Utc::now().to_rfc3339();
        let outcome_reason = outcome_reason.and_then(|reason| {
            let (sanitized, _) = sanitize_delivery_failure_output(reason);
            let bounded = sanitized.chars().take(512).collect::<String>();
            (!bounded.trim().is_empty()).then_some(bounded)
        });
        let mut change_sets = self
            .session_repo
            .list_turn_change_sets_by_session(session_id)?
            .into_iter()
            .filter(|item| &item.turn_id == turn_id)
            .collect::<Vec<_>>();
        if change_sets.is_empty() {
            if let Some(session) = session.as_ref() {
                let missing = TurnChangeSet {
                    snapshot_id: Uuid::new_v4().to_string(),
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    workspace_id: session.workspace_id.clone(),
                    capture_version: TURN_REVIEW_CAPTURE_VERSION,
                    state: "unavailable".to_string(),
                    base_tree: None,
                    result_tree: None,
                    baseline_untracked: Vec::new(),
                    result_untracked: Vec::new(),
                    files: Vec::new(),
                    file_diffs: Default::default(),
                    observed_validations: observed_validations.clone(),
                    diff_truncated: false,
                    turn_outcome: Some(turn_outcome.to_string()),
                    outcome_reason: outcome_reason.clone(),
                    error: Some("turn baseline is unavailable".to_string()),
                    created_at: now.clone(),
                    completed_at: Some(now.clone()),
                };
                self.session_repo.save_turn_change_set(&missing)?;
                return Ok(vec![missing]);
            }
            return Ok(Vec::new());
        }
        let roots = match session.as_ref() {
            Some(session) => self.turn_review_roots(session).await.unwrap_or_default(),
            None => Vec::new(),
        }
        .into_iter()
        .collect::<HashMap<_, _>>();
        for change_set in &mut change_sets {
            if change_set.turn_outcome.is_some() {
                continue;
            }
            change_set.observed_validations = observed_validations.clone();
            change_set.turn_outcome = Some(turn_outcome.to_string());
            change_set.outcome_reason = outcome_reason.clone();
            if change_set.state != "collecting" {
                if force_partial && matches!(change_set.state.as_str(), "available" | "no_changes")
                {
                    change_set.state = "partial".to_string();
                }
                change_set.completed_at = Some(now.clone());
                cleanup_snapshot(&self.turn_review_snapshot_root(&change_set.snapshot_id));
                self.session_repo.save_turn_change_set(change_set)?;
                continue;
            }
            let Some(root) = roots.get(&change_set.workspace_id) else {
                change_set.state = "unavailable".to_string();
                change_set.error =
                    Some("workspace root is unavailable at turn completion".to_string());
                change_set.completed_at = Some(now.clone());
                cleanup_snapshot(&self.turn_review_snapshot_root(&change_set.snapshot_id));
                self.session_repo.save_turn_change_set(change_set)?;
                continue;
            };
            let Some(base_tree) = change_set.base_tree.clone() else {
                change_set.state = "failed".to_string();
                change_set.error = Some("turn baseline fingerprint is missing".to_string());
                change_set.completed_at = Some(now.clone());
                cleanup_snapshot(&self.turn_review_snapshot_root(&change_set.snapshot_id));
                self.session_repo.save_turn_change_set(change_set)?;
                continue;
            };
            let baseline = GitTurnBaseline {
                tree: base_tree,
                untracked: change_set.baseline_untracked.clone(),
            };
            let snapshot_root = self.turn_review_snapshot_root(&change_set.snapshot_id);
            let captured = capture_result(root, &snapshot_root, &baseline);
            cleanup_snapshot(&snapshot_root);
            match captured {
                Ok(result) => {
                    change_set.state = result.status;
                    if force_partial
                        && matches!(change_set.state.as_str(), "available" | "no_changes")
                    {
                        change_set.state = "partial".to_string();
                    }
                    change_set.result_tree = Some(result.tree);
                    change_set.baseline_untracked = result.excluded_preexisting_untracked;
                    change_set.result_untracked = result.result_untracked;
                    change_set.files = result.files;
                    change_set.file_diffs = result.file_diffs;
                    change_set.diff_truncated = result.diff_truncated;
                    change_set.error = None;
                }
                Err(error) => {
                    change_set.state = "failed".to_string();
                    change_set.error = Some(error);
                }
            }
            change_set.completed_at = Some(now.clone());
            self.session_repo.save_turn_change_set(change_set)?;
        }
        Ok(change_sets)
    }

    async fn append_and_publish_session_event(
        &self,
        session_id: &SessionId,
        kind: SessionEventKind,
        core_event: dcc_core::ports::events::CoreEvent,
    ) -> Result<()> {
        let outcome = self.append_session_event(session_id, kind).await?;
        self.publish_appended_session_event(outcome, core_event)
            .await
    }

    async fn publish_appended_session_event(
        &self,
        outcome: AppendEventOutcome,
        core_event: dcc_core::ports::events::CoreEvent,
    ) -> Result<()> {
        if let AppendEventOutcome::Inserted(record) = outcome {
            EventBus::publish_durable_session(self, &record, core_event).await?;
        }
        Ok(())
    }

    async fn find_terminal_event(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
    ) -> Result<Option<SessionEventRecord>> {
        SessionEventRepo::find_terminal_event(&self.session_repo, session_id, turn_id).await
    }

    fn terminal_outcome(record: &SessionEventRecord) -> Result<TerminalIntent> {
        match record.kind {
            SessionEventKind::TurnCompleted { .. } => Ok(TerminalIntent::Completed),
            SessionEventKind::TurnAborted { .. } => Ok(TerminalIntent::Aborted),
            _ => Err(dcc_core::CoreError::Repository(
                "durable terminal event has an invalid kind".to_string(),
            )),
        }
    }

    async fn acquire_terminal_token(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        expected: &ProviderSessionBinding,
    ) -> Result<TerminalTokenGuard> {
        let _terminal = expected.terminal_lock.lock().await;
        let current = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Repository("provider turn binding changed".to_string())
        })?;
        if !Arc::ptr_eq(&current.current_turn_id, &expected.current_turn_id)
            || current.handle.handle_id != expected.handle.handle_id
            || current.current_turn_id.lock().await.as_deref() != Some(turn_id.0.as_str())
        {
            return Err(dcc_core::CoreError::Repository(
                "provider turn binding changed".to_string(),
            ));
        }
        let mut active = expected.terminal_token.active.lock().map_err(|_| {
            dcc_core::CoreError::Repository("terminal token unavailable".to_string())
        })?;
        if active.is_some() {
            return Err(dcc_core::CoreError::Repository(
                "terminal turn transition already in progress".to_string(),
            ));
        }
        let previous_generation = expected
            .terminal_token
            .generation
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .map_err(|_| {
                dcc_core::CoreError::Repository("terminal token generation exhausted".to_string())
            })?;
        let generation = previous_generation.checked_add(1).ok_or_else(|| {
            dcc_core::CoreError::Repository("terminal token generation exhausted".to_string())
        })?;
        *active = Some((turn_id.0.clone(), generation));
        Ok(TerminalTokenGuard {
            state: Arc::clone(&expected.terminal_token),
            turn_id: turn_id.0.clone(),
            generation,
        })
    }

    async fn acquire_idle_terminal_token(
        &self,
        session_id: &SessionId,
        expected: &ProviderSessionBinding,
    ) -> Result<Option<TerminalTokenGuard>> {
        let _terminal = expected.terminal_lock.lock().await;
        let Some(current) = self.provider_binding(session_id)? else {
            return Ok(None);
        };
        if !Arc::ptr_eq(&current.current_turn_id, &expected.current_turn_id)
            || current.handle.handle_id != expected.handle.handle_id
            || current.current_turn_id.lock().await.is_some()
        {
            return Ok(None);
        }
        let mut active = expected.terminal_token.active.lock().map_err(|_| {
            dcc_core::CoreError::Repository("terminal token unavailable".to_string())
        })?;
        if active.is_some() {
            return Ok(None);
        }
        let previous_generation = expected
            .terminal_token
            .generation
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .map_err(|_| {
                dcc_core::CoreError::Repository("terminal token generation exhausted".to_string())
            })?;
        let generation = previous_generation.checked_add(1).ok_or_else(|| {
            dcc_core::CoreError::Repository("terminal token generation exhausted".to_string())
        })?;
        *active = Some((String::new(), generation));
        Ok(Some(TerminalTokenGuard {
            state: Arc::clone(&expected.terminal_token),
            turn_id: String::new(),
            generation,
        }))
    }

    async fn cleanup_terminal_binding(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        expected: Option<&ProviderSessionBinding>,
        remove_binding: bool,
    ) -> Result<()> {
        let Some(expected) = expected else {
            return Ok(());
        };
        let Some(current) = self.provider_binding(session_id)? else {
            return Ok(());
        };
        if !Arc::ptr_eq(&current.current_turn_id, &expected.current_turn_id)
            || current.handle.handle_id != expected.handle.handle_id
        {
            return Ok(());
        }
        let (removed, cleared) = {
            let _terminal = current.terminal_lock.lock().await;
            let mut current_turn = current.current_turn_id.lock().await;
            if current_turn.as_deref() != Some(turn_id.0.as_str()) {
                return Ok(());
            }
            let Ok(mut store) = self.store.lock() else {
                return Ok(());
            };
            let same = store
                .provider_sessions
                .get(session_id)
                .is_some_and(|binding| {
                    Arc::ptr_eq(&binding.current_turn_id, &expected.current_turn_id)
                });
            if !same {
                return Ok(());
            }
            if remove_binding {
                (store.provider_sessions.remove(session_id).is_some(), false)
            } else {
                *current_turn = None;
                (false, true)
            }
        };
        if removed {
            self.revoke_ephemeral_mcp_projection(
                session_id,
                expected.ephemeral_mcp_lease_id.as_deref(),
            );
            let _ = self
                .clear_mcp_runtime_statuses_if_binding_absent(session_id, expected)
                .await;
        }
        if cleared {
            *current.usage_turn_id.lock().await = None;
        }
        Ok(())
    }

    async fn remove_binding_if_same(
        &self,
        session_id: &SessionId,
        expected: &ProviderSessionBinding,
    ) -> Result<()> {
        let removed = {
            let _terminal = expected.terminal_lock.lock().await;
            if expected.current_turn_id.lock().await.is_some() {
                return Ok(());
            }
            if let Ok(mut store) = self.store.lock() {
                let same = store
                    .provider_sessions
                    .get(session_id)
                    .is_some_and(|binding| {
                        Arc::ptr_eq(&binding.current_turn_id, &expected.current_turn_id)
                    });
                same && store.provider_sessions.remove(session_id).is_some()
            } else {
                false
            }
        };
        if removed {
            self.revoke_ephemeral_mcp_projection(
                session_id,
                expected.ephemeral_mcp_lease_id.as_deref(),
            );
            let _ = self
                .clear_mcp_runtime_statuses_if_binding_absent(session_id, expected)
                .await;
        }
        Ok(())
    }

    async fn terminalize_turn(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        request: TerminalRequest,
    ) -> Result<CanonicalTerminalResult> {
        let expected_binding = self.provider_binding(session_id)?;
        self.terminalize_turn_with_binding(session_id, turn_id, request, expected_binding)
            .await
    }

    /// Finalize the process-scoped M4 capture only after the durable terminal
    /// event has been inserted or found. The database event is authoritative:
    /// a pre-existing/competing outcome is finalized conservatively as a
    /// cancellation, while only the leader's inserted outcome selects the
    /// provider-failure mode. M4 is deliberately fail-soft; a failure here
    /// must never undo or delay the durable terminal transition.
    async fn finalize_capture_v2_after_terminal(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        outcome: TerminalIntent,
        inserted: bool,
        source: Option<TerminalSource>,
    ) {
        let Some(mode) = Self::capture_mode_for_terminal(outcome, inserted, source) else {
            return;
        };
        let key = TerminalKey::new(session_id.clone(), turn_id.clone());
        let runtime = self.runtime.guarded_undo_runtime();
        let report = match runtime.finalize_turn(&key, mode).await {
            Ok(FinalizeTurnOutcome::NotTracked) => return,
            Ok(FinalizeTurnOutcome::Finalized(report)) => report,
            Err(_) => {
                eprintln!("[DCC] guarded undo terminal finalization failed");
                return;
            }
        };
        if runtime
            .forget_finalized_after_terminal_append(&key, &report)
            .is_err()
        {
            eprintln!("[DCC] guarded undo finalized capture cleanup failed");
        }
    }

    fn capture_mode_for_terminal(
        outcome: TerminalIntent,
        inserted: bool,
        source: Option<TerminalSource>,
    ) -> Option<CaptureTerminalMode> {
        if matches!(source, Some(TerminalSource::Unbound)) {
            return None;
        }
        Some(match (outcome, inserted, source) {
            // Completion is fully represented by the canonical durable event,
            // so a retry can safely finish M4 even when it did not insert it.
            (TerminalIntent::Completed, _, _) => CaptureTerminalMode::Completed,
            (TerminalIntent::Aborted, true, Some(TerminalSource::ProviderFailed)) => {
                CaptureTerminalMode::ProviderFailed
            }
            // An aborted event does not durably encode whether the original
            // source was cancellation or provider failure. Existing/opposite
            // outcomes therefore use the conservative interrupted mode.
            (TerminalIntent::Aborted, _, _) => CaptureTerminalMode::Cancelled,
            // This branch is defensive: terminalize_turn currently claims
            // Aborted for every non-completed request.
            (TerminalIntent::ProviderFailed, _, _) => CaptureTerminalMode::ProviderFailed,
        })
    }

    async fn terminalize_turn_with_binding(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        request: TerminalRequest,
        expected_binding: Option<ProviderSessionBinding>,
    ) -> Result<CanonicalTerminalResult> {
        let intent = match &request {
            TerminalRequest::Completed => TerminalIntent::Completed,
            TerminalRequest::Aborted { .. } => TerminalIntent::Aborted,
        };
        let claim = self
            .runtime
            .terminal_arbiter()
            .claim(
                TerminalKey::new(session_id.clone(), turn_id.clone()),
                intent,
            )
            .await
            .map_err(|error| match error {
                TerminalArbiterError::Poisoned => {
                    dcc_core::CoreError::Repository("terminal coordination unavailable".to_string())
                }
                _ => dcc_core::CoreError::Repository(error.to_string()),
            })?;
        let TerminalClaimResult::Leader(claim) = claim else {
            let TerminalClaimResult::AlreadyCommitted(outcome) = claim else {
                unreachable!()
            };
            self.finalize_capture_v2_after_terminal(
                session_id,
                turn_id,
                outcome,
                false,
                match &request {
                    TerminalRequest::Aborted { source, .. } => Some(*source),
                    TerminalRequest::Completed => None,
                },
            )
            .await;
            let remove_binding = matches!(
                (&request, outcome),
                (
                    TerminalRequest::Aborted {
                        source: TerminalSource::Quiesce | TerminalSource::Cancel,
                        ..
                    },
                    TerminalIntent::Aborted
                )
            );
            self.cleanup_terminal_binding(
                session_id,
                turn_id,
                expected_binding.as_ref(),
                remove_binding,
            )
            .await?;
            return Ok(CanonicalTerminalResult {
                outcome,
                inserted: false,
            });
        };
        // The binding token spans the entire leader transaction and cleanup,
        // but the terminal lock itself is held only while acquiring it.
        let _terminal_token = if let Some(expected) = expected_binding.as_ref() {
            Some(
                self.acquire_terminal_token(session_id, turn_id, expected)
                    .await?,
            )
        } else {
            None
        };
        let persistence = claim
            .persist_then_commit_with(|_| async {
                if let Some(existing) = self.find_terminal_event(session_id, turn_id).await? {
                    let outcome = Self::terminal_outcome(&existing)?;
                    self.finalize_capture_v2_after_terminal(
                        session_id,
                        turn_id,
                        outcome,
                        false,
                        match &request {
                            TerminalRequest::Aborted { source, .. } => Some(*source),
                            TerminalRequest::Completed => None,
                        },
                    )
                    .await;
                    return Ok((
                        outcome,
                        TerminalPersistence {
                            record: existing,
                            inserted: false,
                        },
                    ));
                }
                // A provider stream can finish after its session has been
                // rebound to a newer turn. Revalidate the short-lived
                // binding identity immediately before cancellation/evidence
                // so an old stream cannot finalize the replacement turn.
                if matches!(&request, TerminalRequest::Completed) {
                    if let Some(binding) = expected_binding.as_ref() {
                        self.flush_assistant_messages(session_id, binding, turn_id)
                            .await?;
                    }
                }
                if let TerminalRequest::Aborted { source, .. } = &request {
                    if matches!(source, TerminalSource::Quiesce | TerminalSource::Cancel) {
                        if let Some(binding) = expected_binding.as_ref() {
                            let provider =
                                provider_runtime(&binding.provider_id).ok_or_else(|| {
                                    dcc_core::CoreError::Provider(format!(
                                        "unknown provider runtime: {}",
                                        binding.provider_id
                                    ))
                                })?;
                            // Do not commit an aborted terminal state or tear
                            // down this binding if its provider rejects the
                            // cancellation request. A retry remains possible.
                            provider.cancel(&binding.handle).await?;
                        }
                    }
                }
                let (kind, outcome_name, reason, partial) = match &request {
                    TerminalRequest::Completed => (
                        SessionEventKind::TurnCompleted {
                            turn_id: turn_id.clone(),
                        },
                        "completed",
                        None,
                        false,
                    ),
                    TerminalRequest::Aborted { reason, .. } => (
                        SessionEventKind::TurnAborted {
                            turn_id: turn_id.clone(),
                            reason: reason.clone(),
                        },
                        "aborted",
                        reason.as_deref(),
                        true,
                    ),
                };
                if let Err(error) = self
                    .capture_turn_review_result(session_id, turn_id, outcome_name, reason, partial)
                    .await
                {
                    let _ = error;
                    eprintln!("[DCC] terminal turn review capture failed");
                }
                let append = self.append_session_event(session_id, kind).await?;
                let (record, inserted) = match append {
                    AppendEventOutcome::Inserted(record) => (record, true),
                    AppendEventOutcome::Existing(record) => (record, false),
                };
                let outcome = Self::terminal_outcome(&record)?;
                // Poll finalization in the same persistence future that
                // observed the durable append. finalize_turn starts its
                // cancellation-safe worker before yielding, closing the
                // post-append window without making M4 authoritative.
                self.finalize_capture_v2_after_terminal(
                    session_id,
                    turn_id,
                    outcome,
                    inserted,
                    match &request {
                        TerminalRequest::Aborted { source, .. } => Some(*source),
                        TerminalRequest::Completed => None,
                    },
                )
                .await;
                Ok((outcome, TerminalPersistence { record, inserted }))
            })
            .await
            .map_err(|error| match error {
                PersistThenCommitError::Persistence(error) => error,
                PersistThenCommitError::Arbiter(error) => {
                    dcc_core::CoreError::Repository(error.to_string())
                }
            })?;
        let (outcome, payload) = persistence;
        let publish_result = if payload.inserted {
            match &payload.record.kind {
                SessionEventKind::TurnCompleted { turn_id } => {
                    EventBus::publish_durable_session(
                        self,
                        &payload.record,
                        dcc_core::ports::events::CoreEvent::SessionTurnCompleted {
                            session_id: payload.record.session_id.0.clone(),
                            turn_id: turn_id.0.clone(),
                        },
                    )
                    .await
                }
                SessionEventKind::TurnAborted { turn_id, reason } => {
                    EventBus::publish_durable_session(
                        self,
                        &payload.record,
                        dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                            session_id: payload.record.session_id.0.clone(),
                            turn_id: turn_id.0.clone(),
                            reason: reason.clone(),
                        },
                    )
                    .await
                }
                _ => Ok(()),
            }
        } else {
            Ok(())
        };
        if payload.inserted {
            let objective_outcome = match (&payload.record.kind, &request) {
                (SessionEventKind::TurnCompleted { .. }, _) => {
                    Some(ObjectiveTurnOutcome::Completed)
                }
                (
                    SessionEventKind::TurnAborted { .. },
                    TerminalRequest::Aborted {
                        source: TerminalSource::ProviderFailed,
                        ..
                    },
                ) => Some(ObjectiveTurnOutcome::Failed),
                // A deliberate cancel or an unbound start is not a provider
                // failure and must not consume the failure budget.
                _ => None,
            };
            if let Some(outcome) = objective_outcome {
                match self.record_objective_turn_outcome(session_id, turn_id, outcome) {
                    Ok(Some(objective))
                        if objective.status
                            == dcc_core::domain::objective::ObjectiveStatus::Paused
                            && objective.pause_reason.is_some_and(|reason| {
                                reason != dcc_core::domain::objective::ObjectivePauseReason::Manual
                            }) =>
                    {
                        // The pause was decided by this very outcome; record it
                        // durably so the timeline can explain the stop.
                        let reason = objective.pause_reason.expect("checked above");
                        if let Err(error) = self
                            .append_and_publish_session_event(
                                session_id,
                                SessionEventKind::ObjectivePaused {
                                    reason,
                                    consecutive_failures: objective.consecutive_failures,
                                    turns_used: objective.turns_used,
                                },
                                dcc_core::ports::events::CoreEvent::SessionObjectivePaused {
                                    session_id: session_id.0.clone(),
                                    reason,
                                    consecutive_failures: objective.consecutive_failures,
                                    turns_used: objective.turns_used,
                                },
                            )
                            .await
                        {
                            eprintln!("[DCC] objective pause event failed: {error}");
                        }
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("[DCC] objective outcome accounting failed: {error}"),
                }
            }
        }
        let remove_binding = matches!(
            (&request, outcome),
            (
                TerminalRequest::Aborted {
                    source: TerminalSource::Quiesce | TerminalSource::Cancel,
                    ..
                },
                TerminalIntent::Aborted
            )
        );
        let cleanup_result = self
            .cleanup_terminal_binding(
                session_id,
                turn_id,
                expected_binding.as_ref(),
                remove_binding,
            )
            .await;
        drop(_terminal_token);
        if publish_result.is_err() {
            eprintln!("[DCC] terminal event publication failed after durable commit");
        }
        cleanup_result?;
        Ok(CanonicalTerminalResult {
            outcome,
            inserted: payload.inserted,
        })
    }

    async fn flush_assistant_messages(
        &self,
        session_id: &SessionId,
        binding: &ProviderSessionBinding,
        turn_id: &TurnId,
    ) -> Result<()> {
        let mut remaining = {
            let mut tracker = binding.assistant_messages.lock().await;
            tracker.active.drain().collect::<Vec<_>>()
        };
        remaining.sort_by(|left, right| left.0.cmp(&right.0));
        for (message_id, phase) in remaining {
            let outcome = self
                .append_session_event(
                    session_id,
                    SessionEventKind::TurnAssistantMessageCompleted {
                        turn_id: turn_id.clone(),
                        message_id: message_id.clone(),
                        phase: phase.clone(),
                        content: None,
                    },
                )
                .await?;
            if let AppendEventOutcome::Inserted(record) = outcome {
                if self
                    .publish_durable_session(
                        &record,
                        dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageCompleted {
                            session_id: session_id.0.clone(),
                            turn_id: turn_id.0.clone(),
                            message_id,
                            phase,
                            content: None,
                        },
                    )
                    .await
                    .is_err()
                {
                    eprintln!("[DCC] assistant completion publication failed");
                }
            }
        }
        Ok(())
    }

    pub async fn emit_turn_completed(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
    ) -> Result<bool> {
        if let Some(binding) = self.provider_binding(session_id)? {
            if binding.current_turn_id.lock().await.as_deref() != Some(turn_id.0.as_str()) {
                return Ok(false);
            }
            let result = self
                .terminalize_turn(session_id, turn_id, TerminalRequest::Completed)
                .await?;
            let _canonical_outcome = result.outcome;
            return Ok(result.inserted);
        }
        let result = self
            .terminalize_turn(session_id, turn_id, TerminalRequest::Completed)
            .await?;
        let _canonical_outcome = result.outcome;
        Ok(result.inserted)
    }

    pub async fn emit_turn_aborted(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        reason: Option<String>,
    ) -> Result<()> {
        if let Some(binding) = self.provider_binding(session_id)? {
            let current_turn_id = binding.current_turn_id.lock().await.clone();
            if current_turn_id
                .as_deref()
                .is_some_and(|current| current != turn_id.0.as_str())
            {
                return Ok(());
            }
            let result = self
                .terminalize_turn(
                    session_id,
                    turn_id,
                    TerminalRequest::Aborted {
                        reason,
                        source: TerminalSource::ProviderFailed,
                    },
                )
                .await?;
            let _canonical_outcome = result.outcome;
            return Ok(());
        }
        let result = self
            .terminalize_turn(
                session_id,
                turn_id,
                TerminalRequest::Aborted {
                    reason,
                    source: TerminalSource::ProviderFailed,
                },
            )
            .await?;
        let _canonical_outcome = result.outcome;
        Ok(())
    }

    /// Finalizes a just-recorded TurnStarted when binding the new turn failed.
    /// This deliberately never inspects, cancels, or clears the binding for a
    /// still-running older turn in the same session.
    pub async fn emit_unbound_started_turn_aborted(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        reason: Option<String>,
    ) -> Result<()> {
        let result = self
            .terminalize_turn_with_binding(
                session_id,
                turn_id,
                TerminalRequest::Aborted {
                    reason,
                    source: TerminalSource::Unbound,
                },
                None,
            )
            .await?;
        let _canonical_outcome = result.outcome;
        Ok(())
    }

    /// Claims the turn in the process-wide arbiter, cancels the provider
    /// outside binding locks, and captures conservative evidence. Provider
    /// cancellation is not proof that no final write raced with it, so
    /// aborted reviews are always finalized as partial.
    pub async fn quiesce_turn_for_abort(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        reason: Option<&str>,
    ) -> Result<()> {
        if let Some(binding) = self.provider_binding(session_id)? {
            if binding.current_turn_id.lock().await.as_deref() != Some(turn_id.0.as_str()) {
                return Ok(());
            }
        }
        let result = self
            .terminalize_turn(
                session_id,
                turn_id,
                TerminalRequest::Aborted {
                    reason: reason.map(str::to_string),
                    source: TerminalSource::Quiesce,
                },
            )
            .await?;
        let _canonical_outcome = result.outcome;
        Ok(())
    }

    /// Attaches through the same per-session transition used by turn start,
    /// resume and HTTP lifecycle paths.
    pub async fn attach_provider_session(&self, session: &Session) -> Result<()> {
        let transition = self.acquire_provider_transition(&session.id).await?;
        self.attach_current_provider_session_under_transition(&transition, session)
            .await
    }

    /// Re-reads the durable row after the transition is acquired. A caller
    /// may hold an old output from start/resume while close/delete or another
    /// selection won first; that snapshot must never create a new binding.
    pub async fn attach_current_provider_session_under_transition(
        &self,
        transition: &ProviderTransitionGuard,
        expected: &Session,
    ) -> Result<()> {
        self.validate_provider_transition(transition, &expected.id)?;
        let current = SessionRepo::get_session(&self.session_repo, &expected.id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository(
                    "session no longer exists while attaching provider".to_string(),
                )
            })?;
        if current.state != dcc_core::domain::session::SessionState::Active {
            return Err(dcc_core::CoreError::InvalidInput(
                "session is not active while attaching provider".to_string(),
            ));
        }
        if !provider_attach_snapshot_matches(&current, expected) {
            return Err(dcc_core::CoreError::Repository(
                "session changed while attaching provider".to_string(),
            ));
        }
        self.require_active_session_thread(&expected.id).await?;
        self.attach_provider_session_under_transition(transition, &current)
            .await
    }

    /// Attaches while the caller owns the matching provider transition. This
    /// prevents a concurrent selection/attach from installing a loser binding
    /// between session preparation and turn input.
    pub async fn attach_provider_session_under_transition(
        &self,
        transition: &ProviderTransitionGuard,
        session: &Session,
    ) -> Result<()> {
        self.validate_provider_transition(transition, &session.id)?;
        if self.provider_binding(&session.id)?.is_some() {
            return Ok(());
        }

        let registration = self.require_provider_available(&session.provider_id)?;
        let expected_availability = self.provider_availability_snapshot(&session.provider_id)?;
        if !expected_availability.enabled {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} is disabled",
                session.provider_id
            )));
        }
        let supports_multi_root = registration.capabilities.supports_multi_root;
        let provider = registration.runtime;
        let (working_directory, additional_working_directories) = self
            .resolve_session_working_directories(session, supports_multi_root)
            .await?;
        let provider_runtime =
            self.provider_runtime_config(&session.provider_id, session.provider_runtime.as_ref())?;
        let mcp_projection_version = provider.dcc_mcp_projection_version().map(str::to_string);
        let mut mcp_servers = self
            .resolve_provider_mcp_servers(session, provider.as_ref())
            .await?;
        let projected_definition_ids = mcp_servers
            .iter()
            .map(|server| server.definition_id.clone())
            .collect::<Vec<_>>();
        let mut ephemeral_lease_id = None;
        let ephemeral_projection =
            match self.project_ephemeral_mcp_server(mcp_projection_version.as_deref(), session) {
                Ok(projection) => projection,
                Err(error) => {
                    return Err(error);
                }
            };
        if let Some(projection) = ephemeral_projection {
            let lease_id = projection.lease_id.clone();
            match append_ephemeral_mcp_server(&mut mcp_servers, projection.server) {
                Ok(()) => ephemeral_lease_id = Some(lease_id),
                Err(_) => {
                    self.revoke_ephemeral_mcp_projection(&session.id, Some(&lease_id));
                    eprintln!(
                        "[DCC] ephemeral MCP projection identity conflict; continuing without it"
                    );
                }
            }
        }

        let handle = match provider
            .prepare_session(SessionConfig {
                workspace_id: session.workspace_id.clone(),
                session_id: session.id.clone(),
                model: session.model.clone(),
                working_directory: Some(working_directory),
                additional_working_directories,
                provider_runtime: Some(provider_runtime),
                mcp_servers,
            })
            .await
        {
            Ok(handle) => handle,
            Err(error) => {
                self.revoke_ephemeral_mcp_projection(&session.id, ephemeral_lease_id.as_deref());
                if let Some(provider_version) = mcp_projection_version.as_deref() {
                    if !projected_definition_ids.is_empty() {
                        let checked_at = Utc::now().to_rfc3339();
                        let statuses = projected_definition_ids
                            .iter()
                            .cloned()
                            .map(|definition_id| McpRuntimeStatus {
                                definition_id,
                                provider_id: ProviderId(session.provider_id.clone()),
                                provider_version: provider_version.to_string(),
                                session_id: session.id.clone(),
                                state: McpRuntimeState::Failed,
                                tools: Vec::new(),
                                checked_at: checked_at.clone(),
                                bounded_error: Some(McpRuntimeError::bounded(
                                    McpErrorCategory::Protocol,
                                    format!(
                                        "MCP bridge contract negotiation failed for {provider_version}"
                                    ),
                                )),
                            })
                            .collect();
                        let _ = self
                            .replace_mcp_runtime_statuses(
                                &session.id,
                                &session.provider_id,
                                provider_version,
                                statuses,
                            )
                            .await;
                    }
                }
                return Err(error);
            }
        };

        let binding = ProviderSessionBinding {
            provider_id: session.provider_id.clone(),
            handle: handle.clone(),
            current_turn_id: Arc::new(AsyncMutex::new(None)),
            terminal_lock: Arc::new(AsyncMutex::new(())),
            terminal_token: Arc::new(TerminalTokenState::default()),
            usage_turn_id: Arc::new(AsyncMutex::new(None)),
            assistant_messages: Arc::new(AsyncMutex::new(AssistantMessageTracker::default())),
            projected_mcp_definition_ids: Arc::new(
                projected_definition_ids.iter().cloned().collect(),
            ),
            ephemeral_mcp_lease_id: ephemeral_lease_id.clone(),
        };

        // A disable can begin while `prepare_session` awaits. Recheck the
        // exact enabled generation immediately before publishing the binding;
        // otherwise a handle omitted from disable's initial snapshot could be
        // installed after the provider became Disabling.
        if !self
            .provider_availability_matches(&session.provider_id, expected_availability.generation)?
        {
            self.revoke_ephemeral_mcp_projection(&session.id, ephemeral_lease_id.as_deref());
            return match provider.cancel(&handle).await {
                Ok(()) => Err(dcc_core::CoreError::Provider(
                    "provider availability changed while attaching".to_string(),
                )),
                Err(cleanup_error) => Err(dcc_core::CoreError::Provider(format!(
                    "provider availability changed while attaching; prepared handle cleanup failed: {cleanup_error}"
                ))),
            };
        }

        let binding_result = self.lock_store().map(|mut store| {
            match store.provider_sessions.entry(session.id.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(binding.clone());
                    true
                }
                Entry::Occupied(_) => false,
            }
        });
        let won_binding = match binding_result {
            Ok(won_binding) => won_binding,
            Err(error) => {
                self.revoke_ephemeral_mcp_projection(&session.id, ephemeral_lease_id.as_deref());
                return match provider.cancel(&handle).await {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(dcc_core::CoreError::Provider(format!(
                        "provider binding state failed: {error}; prepared handle cleanup failed: {cleanup_error}"
                    ))),
                };
            }
        };
        if !won_binding {
            // Another state attached the same session while this adapter was
            // preparing. Do not overwrite its binding or publish loser
            // statuses; dispose only the handle we prepared.
            self.revoke_ephemeral_mcp_projection(&session.id, ephemeral_lease_id.as_deref());
            return provider.cancel(&handle).await.map_err(|cleanup_error| {
                dcc_core::CoreError::Provider(format!(
                    "provider binding lost its attach race and prepared handle cleanup failed: {cleanup_error}"
                ))
            });
        }

        // The availability transition may have started after the pre-insert
        // check but before the store mutex was acquired. This binding is
        // still owned by the current session transition, so canceling it here
        // cannot touch a successor selected after re-enable.
        if !self
            .provider_availability_matches(&session.provider_id, expected_availability.generation)?
        {
            return match self.cancel_provider_session(&session.id).await {
                Ok(()) => Err(dcc_core::CoreError::Provider(
                    "provider availability changed while attaching".to_string(),
                )),
                Err(cleanup_error) => Err(dcc_core::CoreError::Provider(format!(
                    "provider availability changed while attaching; binding cleanup failed: {cleanup_error}"
                ))),
            };
        }

        if let Some(provider_version) = mcp_projection_version {
            let checked_at = Utc::now().to_rfc3339();
            let statuses = projected_definition_ids
                .into_iter()
                .map(|definition_id| McpRuntimeStatus {
                    definition_id,
                    provider_id: ProviderId(session.provider_id.clone()),
                    provider_version: provider_version.clone(),
                    session_id: session.id.clone(),
                    state: McpRuntimeState::AttachingProvider,
                    tools: Vec::new(),
                    checked_at: checked_at.clone(),
                    bounded_error: None,
                })
                .collect();
            let _ = self
                .replace_mcp_runtime_statuses(
                    &session.id,
                    &session.provider_id,
                    &provider_version,
                    statuses,
                )
                .await;
        }

        self.spawn_provider_bridge(session.id.clone(), binding, provider)
            .await;
        Ok(())
    }

    /// Applies provider/model selection and attaches its runtime before a turn
    /// is recorded. This keeps OAuth and MCP startup failures outside durable
    /// user-turn history.
    pub async fn prepare_provider_session_for_turn(
        &self,
        input: &SendTurnInput,
    ) -> Result<Session> {
        let transition = self.acquire_provider_transition(&input.session_id).await?;
        self.prepare_provider_session_for_turn_under_transition(&transition, input)
            .await
    }

    /// Acquires the process-shared provider transition for this session. Do
    /// not hold unrelated synchronous locks while awaiting this guard.
    pub async fn acquire_provider_transition(
        &self,
        session_id: &SessionId,
    ) -> Result<ProviderTransitionGuard> {
        let lock = self.runtime.provider_transition_lock(session_id)?;
        Ok(ProviderTransitionGuard {
            session_id: session_id.clone(),
            runtime: Arc::clone(&self.runtime),
            guard: Some(Arc::clone(&lock).lock_owned().await),
            lock,
        })
    }

    async fn acquire_provider_availability_transition(
        &self,
        provider_id: &str,
    ) -> Result<ProviderAvailabilityTransitionGuard> {
        // Registration is the authority boundary: only its fixed provider set
        // may allocate a process-runtime availability lock.
        registered_provider(provider_id)?;
        let lock = self.runtime.provider_availability_lock(provider_id)?;
        Ok(ProviderAvailabilityTransitionGuard {
            provider_id: provider_id.to_string(),
            runtime: Arc::clone(&self.runtime),
            guard: Some(Arc::clone(&lock).lock_owned().await),
            lock,
        })
    }

    /// MCP OAuth is the only flow that needs both provider-wide availability
    /// and a stable session binding. Keep acquisition centralized in the same
    /// order as disable draining: provider availability first, then session.
    async fn acquire_mcp_oauth_transitions(
        &self,
        provider_id: &str,
        session_id: &SessionId,
    ) -> Result<(ProviderAvailabilityTransitionGuard, ProviderTransitionGuard)> {
        let availability = self
            .acquire_provider_availability_transition(provider_id)
            .await?;
        let session = self.acquire_provider_transition(session_id).await?;
        Ok((availability, session))
    }

    fn provider_availability_snapshot(&self, provider_id: &str) -> Result<ProviderAvailability> {
        registered_provider(provider_id)?;
        let runtime_state = self
            .runtime
            .provider_availability_state(provider_id)?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository(
                    "provider availability was not initialized".to_string(),
                )
            })?;
        let state = match runtime_state.phase {
            ProviderAvailabilityPhase::Enabled => ProviderAvailabilityState::Enabled,
            ProviderAvailabilityPhase::Disabling => ProviderAvailabilityState::Disabling,
            ProviderAvailabilityPhase::Disabled => ProviderAvailabilityState::Disabled,
        };
        Ok(ProviderAvailability {
            provider_id: provider_id.to_string(),
            enabled: matches!(state, ProviderAvailabilityState::Enabled),
            state,
            generation: runtime_state.generation,
        })
    }

    /// Returns availability for a registered provider. This never runs a
    /// healthcheck and never starts an adapter.
    pub fn provider_availability(&self, provider_id: &str) -> Result<ProviderAvailability> {
        self.provider_availability_snapshot(provider_id)
    }

    fn provider_availability_matches(&self, provider_id: &str, generation: u64) -> Result<bool> {
        let current = self.provider_availability_snapshot(provider_id)?;
        Ok(current.enabled && current.generation == generation)
    }

    /// The single server-side gate for work that can create or attach a
    /// provider runtime. Existing bindings retain their cleanup/steering
    /// paths even while this gate is closed.
    pub(crate) fn require_provider_available(
        &self,
        provider_id: &str,
    ) -> Result<ProviderRegistration> {
        let registration = registered_provider(provider_id)?;
        let availability = self.provider_availability_snapshot(provider_id)?;
        if !availability.enabled {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {provider_id} is disabled"
            )));
        }
        Ok(registration)
    }

    /// Changes server-backed availability. Disable first closes the runtime
    /// gate, persists the disabled record, then drains matching bindings. A
    /// persistence failure restores the exact prior in-memory state before
    /// any provider cancellation can begin.
    pub async fn set_provider_enabled(
        &self,
        provider_id: &str,
        enabled: bool,
    ) -> Result<ProviderAvailability> {
        registered_provider(provider_id)?;
        let transition = self
            .acquire_provider_availability_transition(provider_id)
            .await?;
        let previous = self.provider_availability_snapshot(provider_id)?;
        let previous_runtime = self
            .runtime
            .provider_availability_state(provider_id)?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository(
                    "provider availability was not initialized".to_string(),
                )
            })?;

        if enabled && previous.state == ProviderAvailabilityState::Enabled {
            return Ok(previous);
        }
        let generation = if !enabled && previous.state == ProviderAvailabilityState::Disabled {
            previous.generation
        } else {
            previous.generation.checked_add(1).ok_or_else(|| {
                dcc_core::CoreError::Repository(
                    "provider availability generation exhausted".to_string(),
                )
            })?
        };
        let updated_at_ms = u64::try_from(Utc::now().timestamp_millis()).map_err(|_| {
            dcc_core::CoreError::Repository("provider availability clock is invalid".to_string())
        })?;

        if !enabled {
            self.runtime.set_provider_availability_state(
                provider_id,
                ProviderAvailabilityRuntimeState {
                    phase: ProviderAvailabilityPhase::Disabling,
                    generation,
                },
            )?;
        }
        let record = ProviderAvailabilityRecord {
            provider_id: provider_id.to_string(),
            enabled,
            generation,
            updated_at_ms,
        };
        if let Err(error) = self.session_repo.save_provider_availability(&record) {
            // A different process may have committed a newer generation
            // between our local snapshot and conditional upsert. Project that
            // durable authority instead of restoring a stale Enabled cache.
            // For an unreadable/ordinary database failure retain the exact
            // previous runtime state, so no cleanup starts on a failed write.
            let replacement = match self.session_repo.load_provider_availability(provider_id) {
                Ok(Some(durable))
                    if durable.generation > record.generation
                        || (durable.generation == record.generation
                            && durable.enabled != record.enabled) =>
                {
                    ProviderAvailabilityRuntimeState {
                        phase: if durable.enabled {
                            ProviderAvailabilityPhase::Enabled
                        } else {
                            ProviderAvailabilityPhase::Disabled
                        },
                        generation: durable.generation,
                    }
                }
                _ => previous_runtime,
            };
            let _ = self
                .runtime
                .set_provider_availability_state(provider_id, replacement);
            return Err(error);
        }

        self.runtime.set_provider_availability_state(
            provider_id,
            ProviderAvailabilityRuntimeState {
                phase: if enabled {
                    ProviderAvailabilityPhase::Enabled
                } else {
                    ProviderAvailabilityPhase::Disabled
                },
                generation,
            },
        )?;

        if !enabled {
            if let Err(error) = self
                .drain_disabled_provider_bindings(provider_id, generation)
                .await
            {
                // The durable/in-memory gate remains Disabled. Returning this
                // bounded error lets a caller retry cleanup without reopening
                // the provider or risking a newly attached successor.
                return Err(dcc_core::CoreError::Provider(format!(
                    "provider disabled but runtime cleanup failed: {error}"
                )));
            }
        }
        drop(transition);
        self.provider_availability_snapshot(provider_id)
    }

    async fn drain_disabled_provider_bindings(
        &self,
        provider_id: &str,
        generation: u64,
    ) -> Result<()> {
        let candidates = self
            .lock_store()?
            .provider_sessions
            .iter()
            .filter_map(|(session_id, binding)| {
                (binding.provider_id == provider_id).then(|| {
                    (
                        session_id.clone(),
                        binding.handle.handle_id.clone(),
                        Arc::clone(&binding.current_turn_id),
                    )
                })
            })
            .collect::<Vec<_>>();
        for (session_id, expected_handle_id, expected_turn) in candidates {
            let transition = self.acquire_provider_transition(&session_id).await?;
            let still_disabled = self
                .runtime
                .provider_availability_state(provider_id)?
                .is_some_and(|state| {
                    state.phase == ProviderAvailabilityPhase::Disabled
                        && state.generation == generation
                });
            if !still_disabled {
                return Ok(());
            }
            let matches = self.provider_binding(&session_id)?.is_some_and(|binding| {
                binding.provider_id == provider_id
                    && binding.handle.handle_id == expected_handle_id
                    && Arc::ptr_eq(&binding.current_turn_id, &expected_turn)
            });
            if matches {
                self.cancel_provider_session(&session_id).await?;
            }
            drop(transition);
        }
        Ok(())
    }

    fn validate_provider_transition(
        &self,
        transition: &ProviderTransitionGuard,
        session_id: &SessionId,
    ) -> Result<()> {
        if transition.session_id != *session_id || !Arc::ptr_eq(&transition.runtime, &self.runtime)
        {
            return Err(dcc_core::CoreError::InvalidInput(
                "provider transition does not match session runtime".to_string(),
            ));
        }
        Ok(())
    }

    /// Checks that the provider selection stored on a resumable session can
    /// still be attached, without changing session state or binding. This is
    /// deliberately performed before `resume_session` appends its event.
    pub async fn validate_provider_resume_preflight_under_transition(
        &self,
        transition: &ProviderTransitionGuard,
        session_id: &SessionId,
    ) -> Result<()> {
        self.validate_provider_transition(transition, session_id)?;
        let session = SessionRepo::get_session(&self.session_repo, session_id)
            .await?
            .ok_or_else(|| dcc_core::CoreError::Repository("session not found".to_string()))?;
        self.validate_provider_session_attachment(&session).await
    }

    async fn validate_provider_session_attachment(&self, session: &Session) -> Result<()> {
        self.require_active_session_thread(&session.id).await?;
        let registration = self.require_provider_available(&session.provider_id)?;
        self.resolve_session_working_directories(
            session,
            registration.capabilities.supports_multi_root,
        )
        .await?;
        self.provider_runtime_config(&session.provider_id, session.provider_runtime.as_ref())?;
        Ok(())
    }

    /// A provider runtime can only be attached to a live, unarchived thread
    /// that still points at the same session. Keep this proof centralized so
    /// resume, selection changes, and late attach callbacks cannot diverge.
    async fn require_active_session_thread(&self, session_id: &SessionId) -> Result<()> {
        let thread = ThreadRepo::find_thread_by_session_id(&self.session_repo, session_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository(
                    "session thread no longer exists while preparing provider".to_string(),
                )
            })?;
        if thread.session_id.as_ref() != Some(session_id) || thread.archived_at.is_some() {
            return Err(dcc_core::CoreError::InvalidInput(
                "session thread is not active while preparing provider".to_string(),
            ));
        }
        Ok(())
    }

    /// Cancels an attached provider, if any, while the matching transition is
    /// held. A missing binding is normal for restored/legacy sessions; a
    /// cancellation failure is intentionally returned before close/delete can
    /// invalidate the only retryable provider handle.
    pub async fn cancel_provider_session_if_attached_under_transition(
        &self,
        transition: &ProviderTransitionGuard,
        session_id: &SessionId,
    ) -> Result<()> {
        self.validate_provider_transition(transition, session_id)?;
        if self.provider_binding(session_id)?.is_some() {
            self.cancel_provider_session(session_id).await?;
        }
        Ok(())
    }

    /// Validates, persists and attaches a selection while `transition` owns
    /// the matching session lock. Turn-start pipelines must keep that guard
    /// until their provider input has been accepted.
    pub async fn prepare_provider_session_for_turn_under_transition(
        &self,
        transition: &ProviderTransitionGuard,
        input: &SendTurnInput,
    ) -> Result<Session> {
        self.validate_provider_transition(transition, &input.session_id)?;
        // Selection, binding cancellation, persistence, and attachment must
        // move together for one session. The lock lives in ProcessRuntime, so
        // cloned/independently constructed command states for this physical
        // scope cannot leave a provider binding behind for a losing selection.
        let current = self
            .peek_session(&input.session_id)
            .await?
            .ok_or_else(|| dcc_core::CoreError::Repository("session not found".to_string()))?;
        // Validate the selected runtime capability contract before changing a
        // durable selection or cancelling a still-valid provider binding.
        self.validate_provider_turn_selection(&current, input)
            .await?;
        if send_turn_selection_differs_from_session(&current, input) {
            if self.provider_binding(&input.session_id)?.is_some() {
                // A failed cancellation leaves the prior binding authoritative;
                // do not persist or attach a different selection over it.
                self.cancel_provider_session(&input.session_id).await?;
            }
        }
        let session = prepare_session_for_turn(self, input).await?;
        self.attach_current_provider_session_under_transition(transition, &session)
            .await?;
        Ok(session)
    }

    async fn validate_provider_turn_selection(
        &self,
        current: &Session,
        input: &SendTurnInput,
    ) -> Result<()> {
        self.require_active_session_thread(&current.id).await?;
        let (provider_id, model, provider_runtime) =
            merge_send_turn_session_selection(current, input);
        let registration = if let Some(policy) = input.approval_policy {
            require_provider_approval_policy(&provider_id, policy)?;
            self.require_provider_available(&provider_id)?
        } else {
            self.require_provider_available(&provider_id)?
        };
        let candidate = Session {
            provider_id,
            model,
            provider_runtime,
            ..current.clone()
        };
        self.provider_runtime_config(&candidate.provider_id, candidate.provider_runtime.as_ref())?;
        // Model authority follows the static catalog unless the adapter
        // discovers models at runtime (Cursor); aliases resolve.
        self.validate_model_authority(
            &candidate.provider_id,
            &registration,
            candidate.model.as_deref(),
        )
        .await?;
        if !candidate.additional_workspace_ids.is_empty()
            && !supports_provider_capability(
                &registration.capabilities,
                ProviderCapability::MultiRoot,
            )
        {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} does not support isolated multi-workspace sessions yet",
                candidate.provider_id
            )));
        }
        self.resolve_session_working_directories(
            &candidate,
            registration.capabilities.supports_multi_root,
        )
        .await
        .map(|_| ())
    }

    /// Validates a queued policy against the provider currently persisted on
    /// the session. It is intentionally repeated at dispatch time because
    /// older queue records may predate capability enforcement.
    pub async fn validate_queued_turn_approval_policy(
        &self,
        session_id: &SessionId,
        approval_policy: Option<ProviderApprovalPolicy>,
    ) -> Result<()> {
        let session = self
            .peek_session(session_id)
            .await?
            .ok_or_else(|| dcc_core::CoreError::Repository("session not found".to_string()))?;
        match approval_policy {
            Some(approval_policy) => {
                require_provider_approval_policy(&session.provider_id, approval_policy)?;
                self.require_provider_available(&session.provider_id)
                    .map(|_| ())
            }
            None => self
                .require_provider_available(&session.provider_id)
                .map(|_| ()),
        }
    }

    /// Preflights manual delegation target capabilities. This deliberately
    /// does not require `can_request_delegation`: this flow has no trusted
    /// provider-origin signal and is initiated by DCC itself.
    pub fn validate_delegation_target(
        &self,
        target_provider_id: &ProviderId,
        mode: &DelegationMode,
        budget: &DelegationBudget,
    ) -> Result<()> {
        let registration = require_provider_capability(
            &target_provider_id.0,
            ProviderCapability::DelegationTarget,
            "being a delegation target",
        )?;
        let required = if matches!(mode, DelegationMode::Implement) || budget.allow_file_edits {
            ProviderCapability::EditDelegation
        } else {
            ProviderCapability::ReadOnlyDelegation
        };
        if !supports_provider_capability(&registration.capabilities, required) {
            let operation = if required == ProviderCapability::EditDelegation {
                "edit-capable delegation"
            } else {
                "read-only delegation"
            };
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} does not support {operation}",
                target_provider_id.0
            )));
        }
        self.require_provider_available(&target_provider_id.0)?;
        Ok(())
    }

    pub fn session_mcp_oauth_support(&self, session_id: &SessionId) -> Result<McpOauthSupport> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        Ok(registered_provider(&binding.provider_id)?
            .capabilities
            .mcp_oauth_support)
    }

    async fn resolve_provider_mcp_servers(
        &self,
        session: &Session,
        provider: &dyn Provider,
    ) -> Result<Vec<ProviderMcpServerConfig>> {
        // Only adapters with an explicit DCC projection path may receive
        // registry definitions. Native provider configuration remains
        // independent for every other provider.
        if provider.dcc_mcp_projection_version().is_none() {
            return Ok(Vec::new());
        }

        let repo = SqliteMcpRepo::open(&self.db_path)?;
        resolve_session_mcp_servers(
            &repo,
            &SystemCredentialStore::default(),
            &ResolveSessionMcpInput {
                provider_id: ProviderId(session.provider_id.clone()),
                project_id: session.project_id.clone(),
                session_id: session.id.clone(),
            },
        )
        .await
    }

    pub fn list_mcp_runtime_statuses(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<McpRuntimeStatus>> {
        Ok(self
            .lock_store()?
            .mcp_runtime_statuses
            .get(session_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn replace_mcp_runtime_statuses(
        &self,
        session_id: &SessionId,
        provider_id: &str,
        provider_version: &str,
        mut statuses: Vec<McpRuntimeStatus>,
    ) -> Result<()> {
        let mut definition_ids = HashSet::with_capacity(statuses.len());
        for status in &statuses {
            status.validate().map_err(|_| {
                dcc_core::CoreError::Provider(
                    "provider returned an invalid MCP runtime status".to_string(),
                )
            })?;
            if &status.session_id != session_id
                || status.provider_id.0 != provider_id
                || status.provider_version != provider_version
                || !definition_ids.insert(status.definition_id.clone())
            {
                return Err(dcc_core::CoreError::Provider(
                    "provider returned an invalid MCP runtime status".to_string(),
                ));
            }
        }
        statuses.sort_unstable_by(|left, right| left.definition_id.0.cmp(&right.definition_id.0));

        {
            let mut store = self.lock_store()?;
            if statuses.is_empty() {
                store.mcp_runtime_statuses.remove(session_id);
            } else {
                store
                    .mcp_runtime_statuses
                    .insert(session_id.clone(), statuses.clone());
            }
        }

        self.publish(
            dcc_core::ports::events::CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: session_id.0.clone(),
                statuses,
            },
        )
        .await
    }

    async fn clear_mcp_runtime_statuses(&self, session_id: &SessionId) -> Result<()> {
        let removed = self
            .lock_store()?
            .mcp_runtime_statuses
            .remove(session_id)
            .is_some();
        if !removed {
            return Ok(());
        }
        self.publish(
            dcc_core::ports::events::CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: session_id.0.clone(),
                statuses: Vec::new(),
            },
        )
        .await
    }

    async fn clear_mcp_runtime_statuses_if_binding_absent(
        &self,
        session_id: &SessionId,
        _expected: &ProviderSessionBinding,
    ) -> Result<()> {
        let removed = {
            let mut store = self.lock_store()?;
            if store.provider_sessions.contains_key(session_id) {
                return Ok(());
            }
            store.mcp_runtime_statuses.remove(session_id).is_some()
        };
        if !removed {
            return Ok(());
        }
        self.publish(
            dcc_core::ports::events::CoreEvent::SessionMcpRuntimeStatusChanged {
                session_id: session_id.0.clone(),
                statuses: Vec::new(),
            },
        )
        .await
    }

    fn mcp_oauth_credential_reference(
        provider_id: &str,
        definition_id: &McpDefinitionId,
    ) -> McpSecretReferenceId {
        let mut digest = Sha256::new();
        digest.update(b"dcc-mcp-oauth-grant-v1\0");
        digest.update(provider_id.as_bytes());
        digest.update(b"\0");
        digest.update(definition_id.0.as_bytes());
        McpSecretReferenceId(format!("oauth-grant:{:x}", digest.finalize()))
    }

    async fn persist_provider_mcp_oauth_updates(
        &self,
        binding: &ProviderSessionBinding,
        provider: &dyn Provider,
        signaled_definition_id: &McpDefinitionId,
    ) -> Result<()> {
        if !binding
            .projected_mcp_definition_ids
            .contains(signaled_definition_id)
        {
            return Err(dcc_core::CoreError::Provider(
                "provider returned OAuth state for an unknown MCP definition".to_string(),
            ));
        }

        let updates = provider.take_mcp_oauth_updates(&binding.handle).await?;
        if updates.is_empty() {
            return Ok(());
        }

        let repo = SqliteMcpRepo::open(&self.db_path)?;
        let credential_store = SystemCredentialStore::default();
        let provider_id = ProviderId(binding.provider_id.clone());
        for update in updates {
            if !binding
                .projected_mcp_definition_ids
                .contains(&update.definition_id)
            {
                return Err(dcc_core::CoreError::Provider(
                    "provider returned OAuth state for an unknown MCP definition".to_string(),
                ));
            }
            let definition = repo
                .get_mcp_definition(&update.definition_id)
                .await?
                .ok_or_else(|| {
                    dcc_core::CoreError::Repository(
                        "OAuth state references a missing MCP definition".to_string(),
                    )
                })?;
            let McpTransport::Http { .. } = &definition.transport else {
                return Err(dcc_core::CoreError::Provider(
                    "provider returned OAuth state for a non-HTTP MCP definition".to_string(),
                ));
            };
            let resource_fingerprint =
                mcp_oauth_resource_fingerprint(&definition).map_err(|_| {
                    dcc_core::CoreError::Provider(
                        "provider returned OAuth state for an invalid MCP resource".to_string(),
                    )
                })?;
            let existing = repo
                .get_mcp_oauth_grant(&update.definition_id, &provider_id)
                .await?;
            let Some(state) = update.state else {
                if let Some(existing) = existing {
                    credential_store
                        .delete_secret(&existing.secret_ref)
                        .await
                        .map_err(|_| {
                            dcc_core::CoreError::Provider(
                                "MCP OAuth credential persistence failed".to_string(),
                            )
                        })?;
                    repo.delete_mcp_oauth_grant(&update.definition_id, &provider_id)
                        .await?;
                }
                continue;
            };
            let now = Utc::now().to_rfc3339();
            let secret_ref =
                Self::mcp_oauth_credential_reference(&binding.provider_id, &update.definition_id);
            let created_at = existing
                .as_ref()
                .filter(|grant| grant.resource_fingerprint == resource_fingerprint)
                .map(|grant| grant.created_at.clone())
                .unwrap_or_else(|| now.clone());

            credential_store
                .store_secret(&secret_ref, state.into_secret())
                .await
                .map_err(|_| {
                    dcc_core::CoreError::Provider(
                        "MCP OAuth credential persistence failed".to_string(),
                    )
                })?;
            repo.save_mcp_oauth_grant(&McpOauthGrant {
                definition_id: update.definition_id,
                provider_id: provider_id.clone(),
                resource_fingerprint,
                secret_ref,
                created_at,
                updated_at: now,
            })
            .await?;
        }
        Ok(())
    }

    /// Model authority for every provider. Static catalogs are checked in the
    /// registry; dynamic runtimes are checked against the last complete list
    /// the runtime reported, refreshed on a miss or when stale, and a runtime
    /// that cannot be consulted never validates a model by assumption.
    pub(crate) async fn validate_model_authority(
        &self,
        provider_id: &str,
        registration: &ProviderRegistration,
        model: Option<&str>,
    ) -> Result<()> {
        validate_provider_model(provider_id, &registration.capabilities, model)
            .map_err(dcc_core::CoreError::InvalidInput)?;
        let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
            return Ok(());
        };
        if !supports_provider_capability(
            &registration.capabilities,
            ProviderCapability::DynamicModels,
        ) {
            return Ok(());
        }
        let fresh = self
            .runtime
            .dynamic_models(provider_id)
            .filter(|snapshot| snapshot.refreshed_at.elapsed() <= DYNAMIC_MODEL_SNAPSHOT_TTL);
        if fresh
            .as_ref()
            .is_some_and(|snapshot| snapshot.ids.contains(model))
        {
            return Ok(());
        }
        // Miss or stale: ask the runtime once more before deciding.
        let discovered = registration
            .runtime
            .discover_models()
            .await
            .map_err(|error| {
                dcc_core::CoreError::InvalidInput(format!(
                    "model {model} could not be verified against the {provider_id} runtime: {error}"
                ))
            })?;
        let Some(discovered) = discovered else {
            return Ok(());
        };
        let ids: HashSet<String> = discovered.into_iter().map(|entry| entry.id).collect();
        let known = ids.contains(model);
        self.runtime.store_dynamic_models(provider_id, ids);
        if known {
            Ok(())
        } else {
            Err(dcc_core::CoreError::InvalidInput(format!(
                "model {model} is not offered by the {provider_id} runtime"
            )))
        }
    }

    /// Seeds the dynamic-model snapshot from a catalog listing, so validation
    /// rarely needs to spawn the runtime again.
    pub(crate) fn seed_dynamic_models(&self, provider_id: &str, ids: HashSet<String>) {
        self.runtime.store_dynamic_models(provider_id, ids);
    }

    pub async fn validate_start_thread_scope(&self, input: &StartThreadInput) -> Result<()> {
        let registration = self.require_provider_available(&input.provider_id)?;
        self.validate_model_authority(&input.provider_id, &registration, input.model.as_deref())
            .await?;
        if input.additional_workspace_ids.is_empty() {
            return Ok(());
        }
        let candidate = Session {
            id: SessionId("scope-validation".to_string()),
            project_id: input.project_id.clone(),
            workspace_id: input.workspace_id.clone(),
            additional_workspace_ids: input.additional_workspace_ids.clone(),
            provider_id: input.provider_id.clone(),
            model: input.model.clone(),
            provider_runtime: input.provider_runtime.clone(),
            working_directory_override: input.working_directory_override.clone(),
            state: dcc_core::domain::session::SessionState::Draft,
            created_at: String::new(),
            updated_at: String::new(),
        };
        self.resolve_session_working_directories(
            &candidate,
            registration.capabilities.supports_multi_root,
        )
        .await
        .map(|_| ())
    }

    async fn resolve_session_working_directories(
        &self,
        session: &Session,
        provider_supports_multi_root: bool,
    ) -> Result<(String, Vec<String>)> {
        let workspace_repo = SqliteWorkspaceRepo::open(&self.db_path)?;
        let primary = workspace_repo
            .get_workspace(&session.workspace_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository(format!(
                    "workspace not found for session {}",
                    session.id.0
                ))
            })?;

        if session.additional_workspace_ids.is_empty() {
            let working_directory = session
                .working_directory_override
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .or_else(|| {
                    primary
                        .worktree_path
                        .as_ref()
                        .filter(|value| !value.trim().is_empty())
                        .cloned()
                })
                .unwrap_or_else(|| primary.root_path.clone());
            return Ok((working_directory, Vec::new()));
        }

        if !provider_supports_multi_root {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} does not support isolated multi-workspace sessions yet",
                session.provider_id
            )));
        }
        if session
            .working_directory_override
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(dcc_core::CoreError::InvalidInput(
                "working_directory_override is not allowed for multi-workspace sessions"
                    .to_string(),
            ));
        }

        let bundle = workspace_repo
            .get_workspace_bundle_for_workspace(&session.workspace_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::InvalidInput(
                    "multi-workspace session must use a DCC workspace bundle".to_string(),
                )
            })?;
        if bundle.bundle.state != WorkspaceBundleState::Ready {
            return Err(dcc_core::CoreError::InvalidInput(
                "multi-workspace bundle must be ready".to_string(),
            ));
        }
        if bundle.bundle.primary_workspace_id != session.workspace_id {
            return Err(dcc_core::CoreError::InvalidInput(
                "session primary workspace must match the bundle primary workspace".to_string(),
            ));
        }

        let expected_workspace_ids = bundle
            .members
            .iter()
            .map(|member| member.workspace_id.clone())
            .collect::<HashSet<_>>();
        let mut requested_workspace_ids = HashSet::from([session.workspace_id.clone()]);
        requested_workspace_ids.extend(session.additional_workspace_ids.iter().cloned());
        if requested_workspace_ids != expected_workspace_ids {
            return Err(dcc_core::CoreError::InvalidInput(
                "session workspace scope must contain every member of its bundle exactly once"
                    .to_string(),
            ));
        }

        let resolve_managed_root = |workspace: &Workspace| -> Result<String> {
            if !matches!(
                workspace.state,
                dcc_core::domain::workspace::WorkspaceState::Ready
                    | dcc_core::domain::workspace::WorkspaceState::SetupPending
            ) {
                return Err(dcc_core::CoreError::InvalidInput(format!(
                    "workspace {} must be ready or have setup pending",
                    workspace.id.0
                )));
            }
            let root = workspace
                .worktree_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    dcc_core::CoreError::InvalidInput(format!(
                        "workspace {} has no DCC-managed worktree",
                        workspace.id.0
                    ))
                })?;
            if !PathBuf::from(root).is_absolute() {
                return Err(dcc_core::CoreError::InvalidInput(format!(
                    "workspace {} worktree path must be absolute",
                    workspace.id.0
                )));
            }
            Ok(root.to_string())
        };

        let primary_root = resolve_managed_root(&primary)?;
        let mut seen_roots = HashSet::from([primary_root.clone()]);
        let mut additional_roots = Vec::with_capacity(session.additional_workspace_ids.len());
        for workspace_id in &session.additional_workspace_ids {
            let workspace = workspace_repo
                .get_workspace(workspace_id)
                .await?
                .ok_or_else(|| {
                    dcc_core::CoreError::Repository(format!(
                        "workspace not found for multi-workspace session: {}",
                        workspace_id.0
                    ))
                })?;
            let root = resolve_managed_root(&workspace)?;
            if !seen_roots.insert(root.clone()) {
                return Err(dcc_core::CoreError::InvalidInput(
                    "multi-workspace roots must be distinct".to_string(),
                ));
            }
            additional_roots.push(root);
        }

        Ok((primary_root, additional_roots))
    }

    async fn spawn_provider_bridge(
        &self,
        session_id: SessionId,
        binding: ProviderSessionBinding,
        provider: Arc<dyn Provider>,
    ) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut events = provider.stream_events(&binding.handle);

            while let Some(event) = events.next().await {
                match event {
                    Ok(ProviderEvent::Started { .. }) => {}
                    Ok(ProviderEvent::McpRuntimeStatusSnapshot { statuses }) => {
                        if let Some(provider_version) = provider.dcc_mcp_projection_version() {
                            let _ = state
                                .replace_mcp_runtime_statuses(
                                    &session_id,
                                    &binding.provider_id,
                                    provider_version,
                                    statuses,
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::McpOauthStateChanged { definition_id }) => {
                        if state
                            .persist_provider_mcp_oauth_updates(
                                &binding,
                                provider.as_ref(),
                                &definition_id,
                            )
                            .await
                            .is_err()
                        {
                            if let Some(provider_version) = provider.dcc_mcp_projection_version() {
                                let mut statuses = state
                                    .lock_store()
                                    .ok()
                                    .and_then(|store| {
                                        store.mcp_runtime_statuses.get(&session_id).cloned()
                                    })
                                    .unwrap_or_default();
                                if let Some(status) = statuses
                                    .iter_mut()
                                    .find(|status| status.definition_id == definition_id)
                                {
                                    status.state = McpRuntimeState::Failed;
                                    status.tools.clear();
                                    status.checked_at = Utc::now().to_rfc3339();
                                    status.bounded_error = Some(McpRuntimeError::bounded(
                                        McpErrorCategory::Authentication,
                                        "MCP OAuth credential persistence failed",
                                    ));
                                    let _ = state
                                        .replace_mcp_runtime_statuses(
                                            &session_id,
                                            &binding.provider_id,
                                            provider_version,
                                            statuses,
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                    Ok(ProviderEvent::NativeSubagentActivity {
                        id,
                        agent_id,
                        agent_thread_id,
                        path,
                        name,
                        role,
                        model,
                        status,
                        ..
                    }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnNativeSubagentActivity {
                                        turn_id: TurnId(turn_id.clone()),
                                        id: id.clone(),
                                        agent_id: agent_id.clone(),
                                        agent_thread_id: agent_thread_id.clone(),
                                        path: path.clone(),
                                        name: name.clone(),
                                        role: role.clone(),
                                        model: model.clone(),
                                        status: status.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnNativeSubagentActivity {
                                        session_id: session_id.0.clone(),
                                        turn_id: turn_id.clone(),
                                        id,
                                        agent_id,
                                        agent_thread_id,
                                        path,
                                        name,
                                        role,
                                        model,
                                        status,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::NativeSubagentModelRequested {
                        correlation_id,
                        model,
                        ..
                    }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state.append_and_publish_session_event(
                                &session_id,
                                SessionEventKind::TurnNativeSubagentModelRequested {
                                    turn_id: TurnId(turn_id.clone()),
                                    correlation_id: correlation_id.clone(),
                                    model: model.clone(),
                                },
                                dcc_core::ports::events::CoreEvent::SessionTurnNativeSubagentModelRequested {
                                    session_id: session_id.0.clone(),
                                    turn_id,
                                    correlation_id,
                                    model,
                                },
                            ).await;
                        }
                    }
                    Ok(ProviderEvent::NativeSubagentModelConfirmed {
                        correlation_id,
                        model,
                        ..
                    }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state.append_and_publish_session_event(
                                &session_id,
                                SessionEventKind::TurnNativeSubagentModelConfirmed {
                                    turn_id: TurnId(turn_id.clone()),
                                    correlation_id: correlation_id.clone(),
                                    model: model.clone(),
                                },
                                dcc_core::ports::events::CoreEvent::SessionTurnNativeSubagentModelConfirmed {
                                    session_id: session_id.0.clone(),
                                    turn_id,
                                    correlation_id,
                                    model,
                                },
                            ).await;
                        }
                    }
                    Ok(ProviderEvent::ModelEffective { model, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnModelEffective {
                                        turn_id: TurnId(turn_id.clone()),
                                        model: model.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnModelEffective {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        model,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::TextDelta { content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            // Simple providers expose text without item
                            // lifecycle. Keep a stable synthetic item until a
                            // semantic boundary (tool/reasoning/input), then
                            // start a new segment for subsequent text.
                            let (message_id, should_start) = {
                                let mut tracker = binding.assistant_messages.lock().await;
                                tracker.synthetic_append_target(&turn_id)
                            };
                            if should_start {
                                let _ = state
                                    .append_and_publish_session_event(
                                        &session_id,
                                        SessionEventKind::TurnAssistantMessageStarted {
                                            turn_id: TurnId(turn_id.clone()),
                                            message_id: message_id.clone(),
                                            phase: AssistantMessagePhase::Unknown,
                                        },
                                        dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageStarted {
                                            session_id: session_id.0.clone(),
                                            turn_id: turn_id.clone(),
                                            message_id: message_id.clone(),
                                            phase: AssistantMessagePhase::Unknown,
                                        },
                                    )
                                    .await;
                            }
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAssistantMessageDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        message_id: message_id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        message_id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::AssistantMessageStarted { id, phase, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            binding
                                .assistant_messages
                                .lock()
                                .await
                                .active
                                .insert(id.clone(), phase.clone());
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAssistantMessageStarted {
                                        turn_id: TurnId(turn_id.clone()),
                                        message_id: id.clone(),
                                        phase: phase.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageStarted {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        message_id: id,
                                        phase,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::AssistantMessageDelta { id, content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            // Be defensive when providers omit or reorder the
                            // start notification: synthesize only that missing
                            // lifecycle edge while preserving the native ID.
                            let should_start = {
                                let mut tracker = binding.assistant_messages.lock().await;
                                if tracker.active.contains_key(&id) {
                                    false
                                } else {
                                    tracker
                                        .active
                                        .insert(id.clone(), AssistantMessagePhase::Unknown);
                                    true
                                }
                            };
                            if should_start {
                                let _ = state
                                    .append_and_publish_session_event(
                                        &session_id,
                                        SessionEventKind::TurnAssistantMessageStarted {
                                            turn_id: TurnId(turn_id.clone()),
                                            message_id: id.clone(),
                                            phase: AssistantMessagePhase::Unknown,
                                        },
                                        dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageStarted {
                                            session_id: session_id.0.clone(),
                                            turn_id: turn_id.clone(),
                                            message_id: id.clone(),
                                            phase: AssistantMessagePhase::Unknown,
                                        },
                                    )
                                    .await;
                            }
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAssistantMessageDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        message_id: id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        message_id: id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::AssistantMessageCompleted {
                        id,
                        phase,
                        content,
                        model,
                        ..
                    }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            binding.assistant_messages.lock().await.active.remove(&id);
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnAssistantMessageCompleted {
                                        turn_id: TurnId(turn_id.clone()),
                                        message_id: id.clone(),
                                        phase: phase.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageCompleted {
                                        session_id: session_id.0.clone(),
                                        turn_id: turn_id.clone(),
                                        message_id: id,
                                        phase,
                                        content,
                                    },
                                )
                                .await;
                            if let Some(model) = model {
                                let _ = state
                                    .append_and_publish_session_event(
                                        &session_id,
                                        SessionEventKind::TurnModelEffective {
                                            turn_id: TurnId(turn_id.clone()),
                                            model: model.clone(),
                                        },
                                        dcc_core::ports::events::CoreEvent::SessionTurnModelEffective {
                                            session_id: session_id.0.clone(),
                                            turn_id: turn_id.clone(),
                                            model,
                                        },
                                    )
                                    .await;
                            }
                        }
                    }
                    Ok(ProviderEvent::ReasoningStarted { id, label, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            state
                                .complete_synthetic_assistant_message(
                                    &session_id,
                                    &binding,
                                    &turn_id,
                                )
                                .await;
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnReasoningStarted {
										turn_id: TurnId(turn_id.clone()),
										reasoning_id: id.clone(),
										label: label.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnReasoningStarted {
										session_id: session_id.0.clone(),
										turn_id,
										reasoning_id: id,
										label,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningDelta { id, content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnReasoningDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        reasoning_id: id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnReasoningDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        reasoning_id: id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ReasoningCompleted { id, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnReasoningCompleted {
										turn_id: TurnId(turn_id.clone()),
										reasoning_id: id.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnReasoningCompleted {
										session_id: session_id.0.clone(),
										turn_id,
										reasoning_id: id,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallStarted {
                        id,
                        action,
                        command,
                        file,
                        ..
                    }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            state
                                .complete_synthetic_assistant_message(
                                    &session_id,
                                    &binding,
                                    &turn_id,
                                )
                                .await;
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnToolCallStarted {
										turn_id: TurnId(turn_id.clone()),
										tool_call_id: id.clone(),
										action: action.clone(),
										command: command.clone(),
										file: file.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnToolCallStarted {
										session_id: session_id.0.clone(),
										turn_id,
										tool_call_id: id,
										action,
										command,
										file,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallDelta { id, content }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnToolCallDelta {
                                        turn_id: TurnId(turn_id.clone()),
                                        tool_call_id: id.clone(),
                                        content: content.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnToolCallDelta {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        tool_call_id: id,
                                        content,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallCompleted { id, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
								.append_and_publish_session_event(
									&session_id,
									SessionEventKind::TurnToolCallCompleted {
										turn_id: TurnId(turn_id.clone()),
										tool_call_id: id.clone(),
									},
									dcc_core::ports::events::CoreEvent::SessionTurnToolCallCompleted {
										session_id: session_id.0.clone(),
										turn_id,
										tool_call_id: id,
									},
								)
								.await;
                        }
                    }
                    Ok(ProviderEvent::ToolCallFailed { id, reason, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnToolCallFailed {
                                        turn_id: TurnId(turn_id.clone()),
                                        tool_call_id: id.clone(),
                                        reason: reason.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnToolCallFailed {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        tool_call_id: id,
                                        reason,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::UserInputRequested { id, questions, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            state
                                .complete_synthetic_assistant_message(
                                    &session_id,
                                    &binding,
                                    &turn_id,
                                )
                                .await;
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnUserInputRequested {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        questions: questions.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnUserInputRequested {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        questions,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::UserInputResolved { id, answers, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnUserInputResolved {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        answers: answers.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnUserInputResolved {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        answers,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::PermissionRequested { request, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            state
                                .complete_synthetic_assistant_message(
                                    &session_id,
                                    &binding,
                                    &turn_id,
                                )
                                .await;
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnPermissionRequested {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: request.request_id.clone(),
                                        tool_name: request.tool_name.clone(),
                                        title: request.title.clone(),
                                        description: request.description.clone(),
                                        command: request.command.clone(),
                                        file: request.file.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnPermissionRequested {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: request.request_id,
                                        tool_name: request.tool_name,
                                        title: request.title,
                                        description: request.description,
                                        command: request.command,
                                        file: request.file,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::PermissionResolved { id, behavior, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            let _ = state
                                .append_and_publish_session_event(
                                    &session_id,
                                    SessionEventKind::TurnPermissionResolved {
                                        turn_id: TurnId(turn_id.clone()),
                                        request_id: id.clone(),
                                        behavior: behavior.clone(),
                                    },
                                    dcc_core::ports::events::CoreEvent::SessionTurnPermissionResolved {
                                        session_id: session_id.0.clone(),
                                        turn_id,
                                        request_id: id,
                                        behavior,
                                    },
                                )
                                .await;
                        }
                    }
                    Ok(ProviderEvent::TurnUsage { models, at }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone().or(binding
                            .usage_turn_id
                            .lock()
                            .await
                            .clone());
                        if let Some(turn_id) = turn_id {
                            if let Err(error) = state
                                .record_turn_usage(&session_id, &TurnId(turn_id), &at, &models)
                                .await
                            {
                                eprintln!("[DCC] turn usage persistence failed: {error}");
                            }
                        }
                    }
                    Ok(ProviderEvent::Completed { .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        let mut completed = false;
                        if let Some(turn_id) = turn_id {
                            match state
                                .emit_turn_completed(&session_id, &TurnId(turn_id))
                                .await
                            {
                                Ok(emitted) => completed = emitted,
                                Err(error) => {
                                    eprintln!("[DCC] completed turn finalization failed: {error}")
                                }
                            }
                        }
                        if completed {
                            if let Err(error) = state
                                .dispatch_next_queued_turn_if_objective_allows(&session_id)
                                .await
                            {
                                eprintln!("[DCC] queued turn dispatch failed: {error}");
                            }
                        }
                    }
                    Ok(ProviderEvent::Failed { message, .. }) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            binding.assistant_messages.lock().await.active.clear();
                            let _ = state
                                .emit_turn_aborted(&session_id, &TurnId(turn_id), Some(message))
                                .await;
                        }
                    }
                    Err(error) => {
                        let turn_id = binding.current_turn_id.lock().await.clone();
                        if let Some(turn_id) = turn_id {
                            binding.assistant_messages.lock().await.active.clear();
                            let reason = error.to_string();
                            let _ = state
                                .emit_turn_aborted(&session_id, &TurnId(turn_id), Some(reason))
                                .await;
                        }
                    }
                }
            }

            // The stream may outlive a replacement binding.  Only its own
            // binding may be removed, and the store lock is never held over
            // the async MCP cleanup.
            let removed = {
                let _terminal = binding.terminal_lock.lock().await;
                if binding.current_turn_id.lock().await.is_some() {
                    false
                } else if let Ok(mut store) = state.store.lock() {
                    let same = store
                        .provider_sessions
                        .get(&session_id)
                        .is_some_and(|current| {
                            Arc::ptr_eq(&current.current_turn_id, &binding.current_turn_id)
                        });
                    same && store.provider_sessions.remove(&session_id).is_some()
                } else {
                    false
                }
            };
            if removed {
                state.revoke_ephemeral_mcp_projection(
                    &session_id,
                    binding.ephemeral_mcp_lease_id.as_deref(),
                );
                let _ = state
                    .clear_mcp_runtime_statuses_if_binding_absent(&session_id, &binding)
                    .await;
            }
        });
    }

    async fn complete_synthetic_assistant_message(
        &self,
        session_id: &SessionId,
        binding: &ProviderSessionBinding,
        turn_id: &str,
    ) {
        let completion = {
            let mut tracker = binding.assistant_messages.lock().await;
            let Some(completion) = tracker.take_synthetic_completion() else {
                return;
            };
            completion
        };
        let (message_id, phase) = completion;
        let _ = self
            .append_and_publish_session_event(
                session_id,
                SessionEventKind::TurnAssistantMessageCompleted {
                    turn_id: TurnId(turn_id.to_string()),
                    message_id: message_id.clone(),
                    phase: phase.clone(),
                    content: None,
                },
                dcc_core::ports::events::CoreEvent::SessionTurnAssistantMessageCompleted {
                    session_id: session_id.0.clone(),
                    turn_id: turn_id.to_string(),
                    message_id,
                    phase,
                    content: None,
                },
            )
            .await;
    }

    pub async fn set_active_turn(
        &self,
        session_id: &SessionId,
        turn_id: Option<String>,
    ) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let _terminal = binding.terminal_lock.lock().await;
        let transition_active = binding
            .terminal_token
            .active
            .lock()
            .map_err(|_| dcc_core::CoreError::Repository("terminal token unavailable".to_string()))?
            .is_some();
        if transition_active {
            return Err(dcc_core::CoreError::Repository(
                "terminal turn transition already in progress".to_string(),
            ));
        }
        *binding.assistant_messages.lock().await = AssistantMessageTracker::default();
        *binding.current_turn_id.lock().await = turn_id.clone();
        *binding.usage_turn_id.lock().await = turn_id;
        Ok(())
    }

    pub async fn send_provider_input(&self, session_id: &SessionId, input: Input) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        let input = match input {
            Input::Turn(mut turn) => {
                let session = SessionRepo::get_session(&self.session_repo, session_id)
                    .await?
                    .ok_or_else(|| {
                        dcc_core::CoreError::Repository(format!(
                            "session not found while preparing provider input: {}",
                            session_id.0
                        ))
                    })?;
                if let Some(scope_instructions) =
                    self.multi_workspace_scope_instructions(&session).await?
                {
                    turn.tool_instructions = Some(match turn.tool_instructions {
                        Some(existing) if !existing.trim().is_empty() => {
                            format!("{scope_instructions}\n\n{existing}")
                        }
                        _ => scope_instructions,
                    });
                }
                Input::Turn(turn)
            }
            other => other,
        };
        provider.send_input(&binding.handle, input).await
    }

    // ---- Durable task objective -------------------------------------------

    pub fn session_objective(&self, session_id: &SessionId) -> Result<Option<SessionObjective>> {
        self.session_repo.load_session_objective(session_id)
    }

    fn persist_objective(&self, objective: &mut SessionObjective) -> Result<()> {
        objective.generation = objective.generation.checked_add(1).ok_or_else(|| {
            dcc_core::CoreError::Repository("session objective generation exhausted".to_string())
        })?;
        self.session_repo.save_session_objective(objective)
    }

    fn require_objective_generation(
        objective: &SessionObjective,
        expected_generation: Option<u64>,
    ) -> Result<()> {
        if expected_generation.is_some_and(|expected| expected != objective.generation) {
            return Err(dcc_core::CoreError::InvalidInput(
                "session objective changed since it was loaded".to_string(),
            ));
        }
        Ok(())
    }

    /// Creates or rewrites the person-authored objective of an existing
    /// session. `expected_generation` makes concurrent edits fail closed.
    pub async fn set_session_objective(
        &self,
        session_id: &SessionId,
        draft: SessionObjectiveDraft,
        expected_generation: Option<u64>,
    ) -> Result<SessionObjective> {
        let validated = draft
            .validate()
            .map_err(dcc_core::CoreError::InvalidInput)?;
        if self.peek_session(session_id).await?.is_none() {
            return Err(dcc_core::CoreError::Repository(
                "session not found".to_string(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        let mut objective = match self.session_repo.load_session_objective(session_id)? {
            Some(mut existing) => {
                Self::require_objective_generation(&existing, expected_generation)?;
                existing.apply_draft(validated, &now);
                existing
            }
            None => {
                if expected_generation.is_some() {
                    return Err(dcc_core::CoreError::InvalidInput(
                        "session objective no longer exists".to_string(),
                    ));
                }
                SessionObjective::new(session_id.clone(), validated, &now)
            }
        };
        self.persist_objective(&mut objective)?;
        Ok(objective)
    }

    pub fn transition_session_objective(
        &self,
        session_id: &SessionId,
        transition: ObjectiveTransition,
        expected_generation: Option<u64>,
    ) -> Result<SessionObjective> {
        let mut objective = self
            .session_repo
            .load_session_objective(session_id)?
            .ok_or_else(|| {
                dcc_core::CoreError::Repository("session objective not found".to_string())
            })?;
        Self::require_objective_generation(&objective, expected_generation)?;
        if objective.transition(transition, &Utc::now().to_rfc3339()) {
            self.persist_objective(&mut objective)?;
        }
        Ok(objective)
    }

    /// Gives a delegated child session its own copy of the parent's objective.
    /// Idempotent: an existing child objective is kept, and a parent objective
    /// already marked done is not propagated. Returns the child's objective.
    pub fn inherit_session_objective(
        &self,
        parent_session_id: &SessionId,
        child_session_id: &SessionId,
    ) -> Result<Option<SessionObjective>> {
        if parent_session_id == child_session_id {
            return self.session_repo.load_session_objective(child_session_id);
        }
        if let Some(existing) = self.session_repo.load_session_objective(child_session_id)? {
            return Ok(Some(existing));
        }
        let Some(parent) = self
            .session_repo
            .load_session_objective(parent_session_id)?
        else {
            return Ok(None);
        };
        if parent.status == dcc_core::domain::objective::ObjectiveStatus::Done {
            return Ok(None);
        }
        let mut child = parent.inherit_for(child_session_id.clone(), &Utc::now().to_rfc3339());
        match self.persist_objective(&mut child) {
            Ok(()) => Ok(Some(child)),
            // A concurrent writer already created the child's objective.
            Err(dcc_core::CoreError::Repository(message))
                if message.contains("generation is stale") =>
            {
                self.session_repo.load_session_objective(child_session_id)
            }
            Err(error) => Err(error),
        }
    }

    pub fn clear_session_objective(&self, session_id: &SessionId) -> Result<bool> {
        self.session_repo.delete_session_objective(session_id)
    }

    /// Bounded background context appended to every turn's instructions so
    /// the objective survives provider switches, compaction and restarts.
    pub fn objective_tool_instructions(
        &self,
        session_id: &SessionId,
        base: Option<String>,
    ) -> Result<Option<String>> {
        let objective = self.session_repo.load_session_objective(session_id)?;
        Ok(merge_objective_instructions(base, objective.as_ref()))
    }

    /// Idempotent per turn. Retries once on a generation race with a
    /// concurrent person edit; never blocks the turn pipeline.
    pub(crate) fn record_objective_turn_outcome(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
        outcome: ObjectiveTurnOutcome,
    ) -> Result<Option<SessionObjective>> {
        for _attempt in 0..3 {
            let Some(mut objective) = self.session_repo.load_session_objective(session_id)? else {
                return Ok(None);
            };
            if !objective.record_turn_outcome(&turn_id.0, outcome, &Utc::now().to_rfc3339()) {
                return Ok(None);
            }
            match self.persist_objective(&mut objective) {
                Ok(()) => return Ok(Some(objective)),
                Err(dcc_core::CoreError::Repository(message))
                    if message.contains("generation is stale") =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Err(dcc_core::CoreError::Repository(
            "session objective outcome could not be recorded".to_string(),
        ))
    }

    /// Automatic follow-ups respect the objective: a paused or done objective
    /// stops the queue until the person resumes it. Direct turns and manual
    /// dispatch stay available because a new instruction has priority.
    pub async fn dispatch_next_queued_turn_if_objective_allows(
        &self,
        session_id: &SessionId,
    ) -> Result<bool> {
        if let Some(objective) = self.session_repo.load_session_objective(session_id)? {
            if !objective.allows_automatic_dispatch() {
                return Ok(false);
            }
        }
        self.dispatch_next_queued_turn(session_id).await
    }

    /// Dispatch the oldest durable follow-up after a provider completes. The
    /// queue is projected from session events, so it survives UI remounts and
    /// app restarts instead of living in composer state.
    pub async fn dispatch_next_queued_turn(&self, session_id: &SessionId) -> Result<bool> {
        // Keep the binding selected for this queued turn stable through its
        // durable TurnStarted record and provider input acceptance.
        let _transition = self.acquire_provider_transition(session_id).await?;
        let Some(queued) = list_turn_queue(self, session_id).await?.into_iter().next() else {
            return Ok(false);
        };
        let input = SendTurnInput {
            session_id: session_id.clone(),
            prompt: queued.prompt.clone(),
            tool_instructions: queued.tool_instructions.clone(),
            provider_id: None,
            model: None,
            provider_runtime: None,
            plan_mode: queued.plan_mode,
            effort: queued.effort.clone(),
            fast_mode: queued.fast_mode,
            approval_policy: queued.approval_policy,
            evidence: queued.evidence.clone(),
            retry_of_turn_id: None,
        };
        // Recheck before `run_send_turn`: persisted queues from before this
        // guard must never create TurnStarted or remove themselves on failure.
        self.validate_queued_turn_approval_policy(session_id, input.approval_policy)
            .await?;
        let provider_input = dcc_core::ports::ProviderTurnInput {
            prompt: input.prompt.clone(),
            tool_instructions: self
                .objective_tool_instructions(session_id, input.tool_instructions.clone())?,
            plan_mode: input.plan_mode,
            effort: input.effort.clone(),
            fast_mode: input.fast_mode,
            approval_policy: input.approval_policy,
        };
        let output = run_send_turn(self, self, self, input).await?;
        let turn_id = output.turn.id.clone();
        if let Err(error) = self
            .set_active_turn(session_id, Some(turn_id.0.clone()))
            .await
        {
            let _ = self
                .emit_unbound_started_turn_aborted(session_id, &turn_id, Some(error.to_string()))
                .await;
            return Err(error);
        }
        match self
            .capture_turn_review_baseline(&output.session, &turn_id)
            .await
        {
            Ok(baseline) => {
                let _ = self
                    .begin_capture_v2_after_m3(&output.session, &turn_id, baseline)
                    .await;
            }
            Err(error) => {
                eprintln!("[DCC] queued turn review baseline persistence failed: {error}")
            }
        }
        if let Err(error) =
            mark_queued_turn_dispatched(self, self, session_id, queued.id, turn_id.clone()).await
        {
            let _ = self
                .emit_turn_aborted(session_id, &turn_id, Some(error.to_string()))
                .await;
            return Err(error);
        }
        if let Err(error) = self
            .send_provider_input(session_id, Input::Turn(provider_input))
            .await
        {
            let _ = self
                .emit_turn_aborted(session_id, &turn_id, Some(error.to_string()))
                .await;
            return Err(error);
        }
        Ok(true)
    }

    pub async fn steer_provider_turn(&self, session_id: &SessionId, prompt: &str) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = require_provider_capability(
            &binding.provider_id,
            ProviderCapability::Steering,
            "steering an active turn",
        )?
        .runtime;
        provider.steer(&binding.handle, prompt).await
    }

    pub async fn steer_native_subagent(
        &self,
        session_id: &SessionId,
        agent_thread_id: &str,
        prompt: &str,
    ) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = require_provider_capability(
            &binding.provider_id,
            ProviderCapability::NativeSubagentSteering,
            "steering native subagents",
        )?
        .runtime;
        provider
            .steer_native_subagent(&binding.handle, agent_thread_id, prompt)
            .await
    }

    pub async fn interrupt_native_subagent(
        &self,
        session_id: &SessionId,
        agent_thread_id: &str,
    ) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = require_provider_capability(
            &binding.provider_id,
            ProviderCapability::NativeSubagentInterrupt,
            "interrupting native subagents",
        )?
        .runtime;
        provider
            .interrupt_native_subagent(&binding.handle, agent_thread_id)
            .await
    }

    pub async fn start_mcp_oauth(
        &self,
        session_id: &SessionId,
        definition_id: &dcc_core::domain::mcp::McpDefinitionId,
    ) -> Result<ProviderMcpOauthStart> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        // OAuth starts new adapter work. The disable drain takes these locks
        // in exactly this order (availability, then session), so preserve it
        // here to avoid ABBA with close/switch/send. Re-read the binding only
        // after both guards are held; it stays stable through adapter
        // acceptance below.
        let (availability_transition, session_transition) = self
            .acquire_mcp_oauth_transitions(&binding.provider_id, session_id)
            .await?;
        let current_binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        if current_binding.provider_id != binding.provider_id
            || current_binding.handle.handle_id != binding.handle.handle_id
        {
            return Err(dcc_core::CoreError::Provider(
                "provider binding changed before MCP OAuth could start".to_string(),
            ));
        }
        let provider = self
            .require_provider_available(&current_binding.provider_id)?
            .runtime;
        let result = provider
            .start_mcp_oauth(&current_binding.handle, definition_id)
            .await;
        // Keep both transitions through adapter acceptance, then release the
        // session before availability, mirroring the acquisition order.
        drop(session_transition);
        drop(availability_transition);
        result
    }

    async fn multi_workspace_scope_instructions(
        &self,
        session: &Session,
    ) -> Result<Option<String>> {
        if session.additional_workspace_ids.is_empty() {
            return Ok(None);
        }
        let workspace_repo = SqliteWorkspaceRepo::open(&self.db_path)?;
        let bundle = workspace_repo
            .get_workspace_bundle_for_workspace(&session.workspace_id)
            .await?
            .ok_or_else(|| {
                dcc_core::CoreError::InvalidInput(
                    "multi-workspace session bundle is no longer available".to_string(),
                )
            })?;
        let mut lines = vec![
            "DCC authorized multi-workspace scope:".to_string(),
            "Use only the isolated worktree paths listed below for file reads and writes. Never edit the repositories' original checkouts or any unlisted local project. Decide which listed projects need changes, keep producer/consumer contracts consistent, and test the affected projects in the same task context.".to_string(),
        ];
        for member in &bundle.members {
            let workspace = workspace_repo
                .get_workspace(&member.workspace_id)
                .await?
                .ok_or_else(|| {
                    dcc_core::CoreError::Repository(format!(
                        "workspace not found while building session scope: {}",
                        member.workspace_id.0
                    ))
                })?;
            let root = workspace
                .worktree_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    dcc_core::CoreError::InvalidInput(format!(
                        "workspace {} has no isolated worktree",
                        workspace.id.0
                    ))
                })?;
            let role = if workspace.id == session.workspace_id {
                "primary"
            } else {
                "additional"
            };
            lines.push(format!(
                "- {role}: {} | project={} | base={} | worktree={root}",
                workspace.name.as_deref().unwrap_or(&workspace.id.0),
                workspace.project_id.0,
                workspace.base_branch,
            ));
        }
        Ok(Some(lines.join("\n")))
    }

    pub async fn cancel_provider_session(&self, session_id: &SessionId) -> Result<()> {
        let binding = self.provider_binding(session_id)?.ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "no provider binding for session {}",
                session_id.0
            ))
        })?;
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        let cancelling_turn = binding.current_turn_id.lock().await.clone();
        if let Some(turn_id) = cancelling_turn {
            let result = self
                .terminalize_turn(
                    session_id,
                    &TurnId(turn_id),
                    TerminalRequest::Aborted {
                        reason: Some("Provider session cancelled".to_string()),
                        source: TerminalSource::Cancel,
                    },
                )
                .await?;
            let _canonical_outcome = result.outcome;
            Ok(())
        } else {
            if let Some(_idle_token) = self
                .acquire_idle_terminal_token(session_id, &binding)
                .await?
            {
                // Keep the binding installed if cancellation fails so callers
                // can retry against the same authoritative handle.
                provider.cancel(&binding.handle).await?;
                self.remove_binding_if_same(session_id, &binding).await?;
            }
            Ok(())
        }
    }
}

fn lexical_absolute_path(path: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|directory| directory.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn append_ephemeral_mcp_server(
    servers: &mut Vec<ProviderMcpServerConfig>,
    projection: ProviderMcpServerConfig,
) -> Result<()> {
    let definition_id = projection.definition_id.clone();
    if servers.iter().any(|server| {
        server.server_name == projection.server_name || server.definition_id == definition_id
    }) {
        return Err(dcc_core::CoreError::Provider(
            "ephemeral MCP projection collides with a persistent definition".to_string(),
        ));
    }
    servers.push(projection);
    Ok(())
}

#[async_trait]
impl WorkspaceRepo for SessionCommandState {
    async fn save_workspace(&self, _workspace: &Workspace) -> Result<()> {
        Ok(())
    }

    async fn get_workspace(&self, _id: &WorkspaceId) -> Result<Option<Workspace>> {
        Ok(None)
    }

    async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        Ok(Vec::new())
    }

    async fn delete_workspace(&self, _id: &WorkspaceId) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl RepositoryRepo for SessionCommandState {
    async fn save_repository(&self, _repository: &Repository) -> Result<()> {
        Ok(())
    }

    async fn get_repository(&self, _id: &RepositoryId) -> Result<Option<Repository>> {
        Ok(None)
    }

    async fn list_repositories(&self) -> Result<Vec<Repository>> {
        Ok(Vec::new())
    }

    async fn delete_repository(&self, _id: &RepositoryId) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl ProjectRepo for SessionCommandState {
    async fn save_project(&self, _project: &Project) -> Result<()> {
        Ok(())
    }

    async fn get_project(&self, _id: &ProjectId) -> Result<Option<Project>> {
        Ok(None)
    }
}

#[async_trait]
impl SessionRepo for SessionCommandState {
    async fn save_session(&self, session: &Session) -> Result<()> {
        SessionRepo::save_session(&self.session_repo, session).await
    }

    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
        SessionRepo::get_session(&self.session_repo, id).await
    }

    async fn delete_session(&self, id: &SessionId) -> Result<()> {
        let result = SessionRepo::delete_session(&self.session_repo, id).await;
        if result.is_ok() {
            let binding = {
                let mut store = self.lock_store()?;
                store.provider_sessions.remove(id)
            };
            self.revoke_ephemeral_mcp_projection(
                id,
                binding
                    .as_ref()
                    .and_then(|binding| binding.ephemeral_mcp_lease_id.as_deref()),
            );
            let _ = self.clear_mcp_runtime_statuses(id).await;
        }
        result
    }
}

#[async_trait]
impl ThreadRepo for SessionCommandState {
    async fn save_thread(&self, thread: &Thread) -> Result<()> {
        ThreadRepo::save_thread(&self.session_repo, thread).await
    }

    async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        ThreadRepo::get_thread(&self.session_repo, id).await
    }

    async fn find_thread_by_session_id(&self, session_id: &SessionId) -> Result<Option<Thread>> {
        ThreadRepo::find_thread_by_session_id(&self.session_repo, session_id).await
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        ThreadRepo::delete_thread(&self.session_repo, id).await
    }
}

#[async_trait]
impl SessionEventRepo for SessionCommandState {
    async fn append_event(
        &self,
        event: &SessionEventRecord,
    ) -> Result<dcc_core::ports::AppendEventOutcome> {
        SessionEventRepo::append_event(&self.session_repo, event).await
    }

    async fn list_events_by_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SessionEventRecord>> {
        SessionEventRepo::list_events_by_session(&self.session_repo, session_id).await
    }

    async fn list_events_by_session_limited(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<SessionEventRecord>> {
        SessionEventRepo::list_events_by_session_limited(&self.session_repo, session_id, limit)
            .await
    }

    async fn find_terminal_event(
        &self,
        session_id: &SessionId,
        turn_id: &TurnId,
    ) -> Result<Option<SessionEventRecord>> {
        SessionEventRepo::find_terminal_event(&self.session_repo, session_id, turn_id).await
    }

    async fn delete_events_by_session(&self, session_id: &SessionId) -> Result<()> {
        SessionEventRepo::delete_events_by_session(&self.session_repo, session_id).await
    }
}

#[async_trait]
impl DelegationRepo for SessionCommandState {
    async fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        DelegationRepo::save_delegation(&self.session_repo, delegation).await
    }

    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        DelegationRepo::get_delegation(&self.session_repo, id).await
    }

    async fn list_delegations(
        &self,
        workspace_id: Option<&WorkspaceId>,
        parent_session_id: Option<&SessionId>,
    ) -> Result<Vec<Delegation>> {
        DelegationRepo::list_delegations(&self.session_repo, workspace_id, parent_session_id).await
    }

    async fn update_delegation_status(
        &self,
        id: &DelegationId,
        status: DelegationStatus,
        updated_at: String,
    ) -> Result<Option<Delegation>> {
        DelegationRepo::update_delegation_status(&self.session_repo, id, status, updated_at).await
    }
}

#[async_trait]
impl DelegationWorktreeOperationRepo for SessionCommandState {
    async fn create_delegation_worktree_operation(
        &self,
        operation: &DelegationWorktreeOperation,
    ) -> Result<()> {
        DelegationWorktreeOperationRepo::create_delegation_worktree_operation(
            &self.session_repo,
            operation,
        )
        .await
    }

    async fn get_delegation_worktree_operation(
        &self,
        id: &DelegationWorktreeOperationId,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        DelegationWorktreeOperationRepo::get_delegation_worktree_operation(&self.session_repo, id)
            .await
    }

    async fn get_delegation_worktree_operation_by_delegation_id(
        &self,
        delegation_id: &DelegationId,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        DelegationWorktreeOperationRepo::get_delegation_worktree_operation_by_delegation_id(
            &self.session_repo,
            delegation_id,
        )
        .await
    }

    async fn list_delegation_worktree_operations_by_workspace(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<DelegationWorktreeOperation>> {
        DelegationWorktreeOperationRepo::list_delegation_worktree_operations_by_workspace(
            &self.session_repo,
            workspace_id,
        )
        .await
    }

    async fn compare_and_swap_delegation_worktree_operation(
        &self,
        expected_state: DelegationWorktreeOperationState,
        operation: &DelegationWorktreeOperation,
    ) -> Result<bool> {
        DelegationWorktreeOperationRepo::compare_and_swap_delegation_worktree_operation(
            &self.session_repo,
            expected_state,
            operation,
        )
        .await
    }

    async fn list_delegation_worktree_operations_requiring_recovery(
        &self,
    ) -> Result<Vec<DelegationWorktreeOperation>> {
        DelegationWorktreeOperationRepo::list_delegation_worktree_operations_requiring_recovery(
            &self.session_repo,
        )
        .await
    }

    async fn claim_delegation_worktree_removal(
        &self,
        id: &DelegationWorktreeOperationId,
        recovery_owner: &str,
        now: &str,
        lease_until: &str,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        DelegationWorktreeOperationRepo::claim_delegation_worktree_removal(
            &self.session_repo,
            id,
            recovery_owner,
            now,
            lease_until,
        )
        .await
    }

    async fn finalize_delegation_worktree_removal(
        &self,
        id: &DelegationWorktreeOperationId,
        recovery_owner: &str,
        final_state: DelegationWorktreeOperationState,
        last_error: Option<String>,
        updated_at: &str,
    ) -> Result<Option<DelegationWorktreeOperation>> {
        DelegationWorktreeOperationRepo::finalize_delegation_worktree_removal(
            &self.session_repo,
            id,
            recovery_owner,
            final_state,
            last_error,
            updated_at,
        )
        .await
    }

    async fn delete_removed_delegation_worktree_operation(
        &self,
        id: &DelegationWorktreeOperationId,
    ) -> Result<bool> {
        DelegationWorktreeOperationRepo::delete_removed_delegation_worktree_operation(
            &self.session_repo,
            id,
        )
        .await
    }
}

#[async_trait]
impl DelegationApplyTransactionRepo for SessionCommandState {
    async fn create_delegation_apply_transaction(
        &self,
        transaction: &DelegationApplyTransaction,
    ) -> Result<()> {
        DelegationApplyTransactionRepo::create_delegation_apply_transaction(
            &self.session_repo,
            transaction,
        )
        .await
    }

    async fn get_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
    ) -> Result<Option<DelegationApplyTransaction>> {
        DelegationApplyTransactionRepo::get_delegation_apply_transaction(&self.session_repo, id)
            .await
    }

    async fn get_delegation_apply_transaction_by_operation_id(
        &self,
        operation_id: &DelegationWorktreeOperationId,
    ) -> Result<Option<DelegationApplyTransaction>> {
        DelegationApplyTransactionRepo::get_delegation_apply_transaction_by_operation_id(
            &self.session_repo,
            operation_id,
        )
        .await
    }

    async fn compare_and_swap_delegation_apply_transaction(
        &self,
        expected_state: DelegationApplyTransactionState,
        transaction: &DelegationApplyTransaction,
    ) -> Result<bool> {
        DelegationApplyTransactionRepo::compare_and_swap_delegation_apply_transaction(
            &self.session_repo,
            expected_state,
            transaction,
        )
        .await
    }

    async fn claim_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
        recovery_owner: &str,
        now: &str,
        lease_until: &str,
        operation_lock_held: bool,
    ) -> Result<Option<DelegationApplyTransaction>> {
        DelegationApplyTransactionRepo::claim_delegation_apply_transaction(
            &self.session_repo,
            id,
            recovery_owner,
            now,
            lease_until,
            operation_lock_held,
        )
        .await
    }

    async fn finalize_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
        recovery_owner: &str,
        final_state: DelegationApplyTransactionState,
        last_error: Option<String>,
        updated_at: &str,
    ) -> Result<Option<DelegationApplyTransaction>> {
        DelegationApplyTransactionRepo::finalize_delegation_apply_transaction(
            &self.session_repo,
            id,
            recovery_owner,
            final_state,
            last_error,
            updated_at,
        )
        .await
    }

    async fn list_delegation_apply_transactions_requiring_recovery(
        &self,
    ) -> Result<Vec<DelegationApplyTransaction>> {
        DelegationApplyTransactionRepo::list_delegation_apply_transactions_requiring_recovery(
            &self.session_repo,
        )
        .await
    }

    async fn delete_terminal_delegation_apply_transaction(
        &self,
        id: &DelegationApplyTransactionId,
    ) -> Result<bool> {
        DelegationApplyTransactionRepo::delete_terminal_delegation_apply_transaction(
            &self.session_repo,
            id,
        )
        .await
    }
}

#[async_trait]
impl EventBus for SessionCommandState {
    async fn publish(&self, event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        self.runtime.publish_event(event).await
    }

    async fn publish_durable_session(
        &self,
        record: &SessionEventRecord,
        event: dcc_core::ports::events::CoreEvent,
    ) -> Result<()> {
        self.runtime
            .publish_durable_session_event(record, event)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::{
        mcp::McpDefinitionId,
        session::SessionState,
        workspace::WorkspaceState,
        workspace_bundle::{
            WorkspaceBundle, WorkspaceBundleId, WorkspaceBundleMember, WorkspaceBundleState,
        },
    };
    use dcc_core::ports::ProviderMcpTransport;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[test]
    fn capture_v2_terminal_mode_uses_durable_insert_and_source() {
        assert_eq!(
            SessionCommandState::capture_mode_for_terminal(TerminalIntent::Completed, true, None,),
            Some(CaptureTerminalMode::Completed)
        );
        assert_eq!(
            SessionCommandState::capture_mode_for_terminal(
                TerminalIntent::Aborted,
                true,
                Some(TerminalSource::ProviderFailed),
            ),
            Some(CaptureTerminalMode::ProviderFailed)
        );
        assert_eq!(
            SessionCommandState::capture_mode_for_terminal(
                TerminalIntent::Aborted,
                true,
                Some(TerminalSource::Quiesce),
            ),
            Some(CaptureTerminalMode::Cancelled)
        );
    }

    #[test]
    fn capture_v2_preexisting_terminal_uses_canonical_outcome_and_unbound_is_skipped() {
        assert_eq!(
            SessionCommandState::capture_mode_for_terminal(TerminalIntent::Completed, false, None,),
            Some(CaptureTerminalMode::Completed)
        );
        assert_eq!(
            SessionCommandState::capture_mode_for_terminal(TerminalIntent::Aborted, false, None,),
            Some(CaptureTerminalMode::Cancelled)
        );
        assert_eq!(
            SessionCommandState::capture_mode_for_terminal(
                TerminalIntent::Aborted,
                true,
                Some(TerminalSource::Unbound),
            ),
            None
        );
    }

    #[derive(Clone, Default)]
    struct CountingEventBus(Arc<AtomicUsize>);

    #[async_trait]
    impl EventBus for CountingEventBus {
        async fn publish(&self, _event: dcc_core::ports::events::CoreEvent) -> Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingLiveEventBus {
        legacy: Arc<Mutex<Vec<dcc_core::ports::events::CoreEvent>>>,
        live: Arc<Mutex<Vec<dcc_core::ports::SessionLiveEventEnvelope>>>,
    }

    #[async_trait]
    impl EventBus for RecordingLiveEventBus {
        async fn publish(&self, event: dcc_core::ports::events::CoreEvent) -> Result<()> {
            self.legacy.lock().expect("legacy event lock").push(event);
            Ok(())
        }

        async fn publish_session_live(
            &self,
            event: dcc_core::ports::SessionLiveEventEnvelope,
        ) -> Result<()> {
            self.live.lock().expect("live event lock").push(event);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FailingEventBus;

    #[async_trait]
    impl EventBus for FailingEventBus {
        async fn publish(&self, _event: dcc_core::ports::events::CoreEvent) -> Result<()> {
            Err(dcc_core::CoreError::Repository(
                "event bus failure".to_string(),
            ))
        }
    }

    fn sample_session(id: &str) -> Session {
        Session {
            id: SessionId(id.to_string()),
            project_id: ProjectId(format!("project-{id}")),
            workspace_id: WorkspaceId(format!("workspace-{id}")),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    async fn save_active_session_thread(state: &SessionCommandState, session: &Session) {
        ThreadRepo::save_thread(
            state,
            &Thread {
                id: ThreadId(format!("active-thread-{}", session.id.0)),
                project_id: session.project_id.clone(),
                session_id: Some(session.id.clone()),
                title: "active".to_string(),
                archived_at: None,
            },
        )
        .await
        .expect("save active session thread");
    }

    fn selection_input(
        session_id: SessionId,
        provider_id: Option<&str>,
        approval_policy: Option<ProviderApprovalPolicy>,
    ) -> SendTurnInput {
        SendTurnInput {
            session_id,
            prompt: "capability validation".to_string(),
            tool_instructions: None,
            provider_id: provider_id.map(str::to_string),
            model: None,
            provider_runtime: None,
            plan_mode: None,
            effort: None,
            fast_mode: None,
            approval_policy,
            evidence: None,
            retry_of_turn_id: None,
        }
    }

    fn inert_provider_binding(session_id: &SessionId) -> ProviderSessionBinding {
        inert_provider_binding_for(session_id, "codex")
    }

    fn inert_provider_binding_for(
        session_id: &SessionId,
        provider_id: &str,
    ) -> ProviderSessionBinding {
        ProviderSessionBinding {
            provider_id: provider_id.to_string(),
            handle: SessionHandle {
                provider_id: ProviderId(provider_id.to_string()),
                session_id: session_id.clone(),
                handle_id: "capability-validation-binding".to_string(),
            },
            current_turn_id: Arc::new(AsyncMutex::new(None)),
            terminal_lock: Arc::new(AsyncMutex::new(())),
            terminal_token: Arc::new(TerminalTokenState::default()),
            usage_turn_id: Arc::new(AsyncMutex::new(None)),
            assistant_messages: Arc::new(AsyncMutex::new(AssistantMessageTracker::default())),
            projected_mcp_definition_ids: Arc::new(HashSet::new()),
            ephemeral_mcp_lease_id: None,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provider_selection_transitions_share_one_lock_per_session() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let sibling = state.clone();
        let session_id = SessionId("provider-transition".to_string());

        let held_pipeline = state
            .acquire_provider_transition(&session_id)
            .await
            .expect("pipeline transition lock");
        let shared = sibling
            .runtime
            .provider_transition_lock(&session_id)
            .expect("shared transition lock");
        assert!(
            shared.try_lock().is_err(),
            "a competing selection cannot enter while a turn pipeline owns the guard"
        );
        drop(held_pipeline);
        let _next_pipeline = sibling
            .acquire_provider_transition(&session_id)
            .await
            .expect("transition releases after provider input pipeline ends");

        let other = state
            .acquire_provider_transition(&SessionId("other-transition".to_string()))
            .await
            .expect("other session transition lock");
        assert!(other.session_id != session_id);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provider_transition_guard_removes_idle_entry_without_stranding_waiters() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session_id = SessionId("transition-cleanup".to_string());

        let guard = state
            .acquire_provider_transition(&session_id)
            .await
            .expect("transition guard");
        assert_eq!(state.runtime.provider_transition_lock_entry_count(), 1);
        drop(guard);
        assert_eq!(
            state.runtime.provider_transition_lock_entry_count(),
            0,
            "the final guard removes its own weak entry"
        );

        let first = state
            .acquire_provider_transition(&session_id)
            .await
            .expect("first transition");
        // This Arc models a caller that has found the current mutex and is
        // waiting to acquire it while the first pipeline still owns it.
        let waiter = state
            .runtime
            .provider_transition_lock(&session_id)
            .expect("waiter lock");
        drop(first);
        assert_eq!(
            state.runtime.provider_transition_lock_entry_count(),
            1,
            "a waiter keeps the exact mutex registered"
        );
        let successor = state
            .acquire_provider_transition(&session_id)
            .await
            .expect("successor transition");
        drop(waiter);
        drop(successor);
        assert_eq!(state.runtime.provider_transition_lock_entry_count(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn close_transition_accepts_a_session_without_provider_binding() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session = sample_session("close-without-binding");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        let transition = state
            .acquire_provider_transition(&session.id)
            .await
            .expect("close transition");

        state
            .cancel_provider_session_if_attached_under_transition(&transition, &session.id)
            .await
            .expect("missing binding is not a close failure");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn attach_current_rejects_deleted_stale_or_closed_snapshot_before_binding() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));

        // Models start -> close/delete winning before the attach wrapper gets
        // its guard: no provider prepare/binding may occur for this snapshot.
        let deleted = sample_session("attach-deleted");
        let transition = state
            .acquire_provider_transition(&deleted.id)
            .await
            .expect("deleted transition");
        assert!(state
            .attach_current_provider_session_under_transition(&transition, &deleted)
            .await
            .is_err());
        assert!(state
            .provider_binding(&deleted.id)
            .expect("binding")
            .is_none());
        drop(transition);

        let current = sample_session("attach-stale");
        SessionRepo::save_session(&state, &current)
            .await
            .expect("save current session");
        let mut stale = current.clone();
        stale.provider_id = "droid".to_string();
        let transition = state
            .acquire_provider_transition(&current.id)
            .await
            .expect("stale transition");
        assert!(state
            .attach_current_provider_session_under_transition(&transition, &stale)
            .await
            .is_err());
        assert!(state
            .provider_binding(&current.id)
            .expect("binding")
            .is_none());
        drop(transition);

        let mut closed = sample_session("attach-closed");
        closed.state = SessionState::Completed;
        SessionRepo::save_session(&state, &closed)
            .await
            .expect("save closed session");
        let transition = state
            .acquire_provider_transition(&closed.id)
            .await
            .expect("closed transition");
        assert!(state
            .attach_current_provider_session_under_transition(&transition, &closed)
            .await
            .is_err());
        assert!(state
            .provider_binding(&closed.id)
            .expect("binding")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn attach_current_rejects_archived_session_thread_before_provider_prepare() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session = sample_session("attach-archived-thread");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        ThreadRepo::save_thread(
            &state,
            &Thread {
                id: ThreadId("attach-archived-thread-record".to_string()),
                project_id: session.project_id.clone(),
                session_id: Some(session.id.clone()),
                title: "archived".to_string(),
                archived_at: Some("2026-09-01T00:00:00Z".to_string()),
            },
        )
        .await
        .expect("save archived thread");
        let transition = state
            .acquire_provider_transition(&session.id)
            .await
            .expect("attach transition");

        assert!(state
            .attach_current_provider_session_under_transition(&transition, &session)
            .await
            .is_err());
        assert!(state
            .provider_binding(&session.id)
            .expect("binding")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resume_preflight_rejects_legacy_unknown_provider_without_mutating_session() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let mut session = sample_session("resume-unknown-provider");
        session.provider_id = "legacy-unknown-provider".to_string();
        session.state = SessionState::Completed;
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save legacy session");
        save_active_session_thread(&state, &session).await;
        let transition = state
            .acquire_provider_transition(&session.id)
            .await
            .expect("resume transition");

        assert!(state
            .validate_provider_resume_preflight_under_transition(&transition, &session.id)
            .await
            .is_err());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("reload session")
                .expect("session")
                .state,
            SessionState::Completed
        );
        assert!(
            SessionEventRepo::list_events_by_session(&state, &session.id)
                .await
                .expect("session events")
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resume_preflight_rejects_archived_thread_without_mutating_session() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let mut session = sample_session("resume-archived-thread");
        session.state = SessionState::Completed;
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save completed session");
        ThreadRepo::save_thread(
            &state,
            &Thread {
                id: ThreadId("resume-archived-thread-record".to_string()),
                project_id: session.project_id.clone(),
                session_id: Some(session.id.clone()),
                title: "archived".to_string(),
                archived_at: Some("2026-09-01T00:00:00Z".to_string()),
            },
        )
        .await
        .expect("save archived thread");
        let transition = state
            .acquire_provider_transition(&session.id)
            .await
            .expect("resume transition");

        assert!(state
            .validate_provider_resume_preflight_under_transition(&transition, &session.id)
            .await
            .is_err());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("reload session")
                .expect("session")
                .state,
            SessionState::Completed
        );
        assert!(
            SessionEventRepo::list_events_by_session(&state, &session.id)
                .await
                .expect("session events")
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_provider_capabilities_do_not_cancel_bindings_or_persist_selection() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let mut session = sample_session("capability-validation");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        save_active_session_thread(&state, &session).await;
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session.id.clone(), inert_provider_binding(&session.id));

        let unknown = selection_input(session.id.clone(), Some("unknown-provider"), None);
        assert!(state
            .prepare_provider_session_for_turn(&unknown)
            .await
            .is_err());
        assert!(state
            .provider_binding(&session.id)
            .expect("binding")
            .is_some());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("load session")
                .expect("session")
                .provider_id,
            "codex"
        );

        let initial_unknown = StartThreadInput {
            workspace_id: session.workspace_id.clone(),
            additional_workspace_ids: Vec::new(),
            project_id: session.project_id.clone(),
            provider_id: "unknown-provider".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            title: None,
        };
        assert!(state
            .validate_start_thread_scope(&initial_unknown)
            .await
            .is_err());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("load session")
                .expect("session")
                .provider_id,
            "codex"
        );

        session.additional_workspace_ids = vec![WorkspaceId("secondary".to_string())];
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save multi-root session");
        let multi_root = selection_input(session.id.clone(), Some("droid"), None);
        assert!(matches!(
            state.prepare_provider_session_for_turn(&multi_root).await,
            Err(dcc_core::CoreError::Provider(message)) if message.contains("multi-workspace")
        ));
        assert!(state
            .provider_binding(&session.id)
            .expect("binding")
            .is_some());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("load session")
                .expect("session")
                .provider_id,
            "codex"
        );

        session.additional_workspace_ids.clear();
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save single-root session");
        let approval = selection_input(
            session.id.clone(),
            Some("droid"),
            Some(ProviderApprovalPolicy::Ask),
        );
        assert!(matches!(
            state.prepare_provider_session_for_turn(&approval).await,
            Err(dcc_core::CoreError::Provider(message)) if message.contains("approval policy")
        ));
        assert!(state
            .provider_binding(&session.id)
            .expect("binding")
            .is_some());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("load session")
                .expect("session")
                .provider_id,
            "codex"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn valid_provider_selection_passes_preflight_without_static_model_rejection() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let db_path = root.join("state.sqlite");
        let state = SessionCommandState::new_headless(db_path.clone(), root.join("app-data"));
        let session = sample_session("capability-valid");
        let workspace = sample_workspace(&session.workspace_id.0, "/tmp/capability-valid");
        SqliteWorkspaceRepo::open(&db_path)
            .expect("workspace repo")
            .save_workspace(&workspace)
            .await
            .expect("save workspace");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        save_active_session_thread(&state, &session).await;
        let mut input = selection_input(session.id.clone(), Some("cursor"), None);
        // Cursor models are discovered dynamically; the runtime's last complete
        // list is the authority, not the static registry.
        state.seed_dynamic_models(
            "cursor",
            HashSet::from([
                "auto".to_string(),
                "runtime-discovered-cursor-model".to_string(),
            ]),
        );
        input.model = Some("runtime-discovered-cursor-model".to_string());
        state
            .validate_provider_turn_selection(&session, &input)
            .await
            .expect("registered dynamic provider selection");
        // A model the runtime never reported is re-checked against the runtime
        // and rejected either way: no binary, a failing binary, or a real list
        // that does not contain it.
        input.model = Some("model-the-cursor-runtime-never-offered".to_string());
        let error = state
            .validate_provider_turn_selection(&session, &input)
            .await
            .expect_err("unknown dynamic model");
        let message = error.to_string();
        assert!(
            message.contains("could not be verified") || message.contains("is not offered"),
            "{message}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_objective_is_durable_idempotent_and_gates_automatic_dispatch() {
        use dcc_core::domain::objective::{ObjectivePauseReason, ObjectiveStatus};
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let db_path = root.join("state.sqlite");
        let state = SessionCommandState::new_headless(db_path.clone(), root.join("app-data"));
        let session = sample_session("objective");
        let missing = SessionId("missing".to_string());
        let draft = SessionObjectiveDraft {
            intent: "make checkout resilient".to_string(),
            done_when: "retry test passes".to_string(),
            max_consecutive_failures: Some(2),
            max_turns: None,
        };
        assert!(state
            .set_session_objective(&missing, draft.clone(), None)
            .await
            .is_err());
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        assert_eq!(
            state
                .objective_tool_instructions(&session.id, Some("base".to_string()))
                .unwrap(),
            Some("base".to_string())
        );
        let objective = state
            .set_session_objective(&session.id, draft.clone(), None)
            .await
            .expect("create objective");
        assert_eq!(objective.generation, 1);
        let instructions = state
            .objective_tool_instructions(&session.id, Some("base".to_string()))
            .unwrap()
            .expect("merged instructions");
        assert!(instructions.starts_with("base\n\n<dcc_objective status=\"active\""));
        assert!(instructions.contains("intent: make checkout resilient"));

        // Stale expected generation fails closed.
        assert!(state
            .set_session_objective(&session.id, draft.clone(), Some(0))
            .await
            .is_err());

        let turn = TurnId("turn-1".to_string());
        assert!(state
            .record_objective_turn_outcome(&session.id, &turn, ObjectiveTurnOutcome::Failed)
            .unwrap()
            .is_some());
        assert!(state
            .record_objective_turn_outcome(&session.id, &turn, ObjectiveTurnOutcome::Failed)
            .unwrap()
            .is_none());
        let turn2 = TurnId("turn-2".to_string());
        let paused_now = state
            .record_objective_turn_outcome(&session.id, &turn2, ObjectiveTurnOutcome::Failed)
            .unwrap()
            .expect("changed");
        assert_eq!(paused_now.status, ObjectiveStatus::Paused);
        let paused = state
            .session_objective(&session.id)
            .unwrap()
            .expect("objective");
        assert_eq!(paused.status, ObjectiveStatus::Paused);
        assert_eq!(
            paused.pause_reason,
            Some(ObjectivePauseReason::ConsecutiveFailures)
        );
        assert_eq!(paused.consecutive_failures, 2);
        assert!(!paused.allows_automatic_dispatch());
        // A paused objective stops automatic dispatch without touching the queue.
        assert!(!state
            .dispatch_next_queued_turn_if_objective_allows(&session.id)
            .await
            .unwrap());

        let resumed = state
            .transition_session_objective(
                &session.id,
                ObjectiveTransition::Resume,
                Some(paused.generation),
            )
            .unwrap();
        assert_eq!(resumed.status, ObjectiveStatus::Active);
        assert_eq!(resumed.consecutive_failures, 0);
        assert!(state
            .transition_session_objective(&session.id, ObjectiveTransition::Resume, Some(0))
            .is_err());

        // Survives a fresh state over the same database.
        let reopened = SessionCommandState::new_headless(db_path, root.join("app-data"));
        let loaded = reopened
            .session_objective(&session.id)
            .unwrap()
            .expect("durable");
        assert_eq!(loaded.generation, resumed.generation);
        assert_eq!(loaded.intent, "make checkout resilient");
        assert!(reopened.clear_session_objective(&session.id).unwrap());
        assert_eq!(reopened.session_objective(&session.id).unwrap(), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn delegated_child_inherits_parent_objective_once_with_fresh_counters() {
        use dcc_core::domain::objective::ObjectiveStatus;
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let parent = sample_session("objective-parent");
        let child = sample_session("objective-child");
        SessionRepo::save_session(&state, &parent)
            .await
            .expect("parent");
        SessionRepo::save_session(&state, &child)
            .await
            .expect("child");
        assert_eq!(
            state
                .inherit_session_objective(&parent.id, &child.id)
                .unwrap(),
            None,
            "no parent objective, nothing to inherit"
        );
        let draft = SessionObjectiveDraft {
            intent: "keep the API contract".to_string(),
            done_when: "contract tests pass".to_string(),
            max_consecutive_failures: Some(2),
            max_turns: Some(5),
        };
        state
            .set_session_objective(&parent.id, draft, None)
            .await
            .expect("parent objective");
        state
            .record_objective_turn_outcome(
                &parent.id,
                &TurnId("p-1".to_string()),
                ObjectiveTurnOutcome::Failed,
            )
            .unwrap();
        let inherited = state
            .inherit_session_objective(&parent.id, &child.id)
            .unwrap()
            .expect("child objective");
        assert_eq!(inherited.session_id, child.id);
        assert_eq!(inherited.intent, "keep the API contract");
        assert_eq!(inherited.max_turns, Some(5));
        assert_eq!(inherited.consecutive_failures, 0);
        assert_eq!(inherited.turns_used, 0);
        assert_eq!(inherited.status, ObjectiveStatus::Active);
        assert_eq!(inherited.generation, 1);
        // Idempotent: the child's own record is kept afterwards.
        let again = state
            .inherit_session_objective(&parent.id, &child.id)
            .unwrap()
            .expect("kept");
        assert_eq!(again.generation, inherited.generation);
        // A done parent objective is not propagated to a new child.
        let done_child = sample_session("objective-child-2");
        SessionRepo::save_session(&state, &done_child)
            .await
            .expect("child 2");
        state
            .transition_session_objective(&parent.id, ObjectiveTransition::Complete, None)
            .unwrap();
        assert_eq!(
            state
                .inherit_session_objective(&parent.id, &done_child.id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn runtime_config_authority_rejects_fields_the_adapter_ignores() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));

        let concurrency = ProviderRuntimeConfig {
            max_concurrent_subagents: Some(2),
            ..ProviderRuntimeConfig::default()
        };
        assert_eq!(
            state
                .provider_runtime_config("codex", Some(&concurrency))
                .expect("codex enforces subagent limits"),
            concurrency
        );
        let error = state
            .provider_runtime_config("droid", Some(&concurrency))
            .expect_err("droid ignores subagent limits");
        assert!(error
            .to_string()
            .contains("does not support subagent concurrency limits"));

        let home = ProviderRuntimeConfig {
            home_path: Some(root.join("custom-home").display().to_string()),
            ..ProviderRuntimeConfig::default()
        };
        assert!(state.provider_runtime_config("gemini", Some(&home)).is_ok());
        assert!(state
            .provider_runtime_config("cursor", Some(&home))
            .is_err());

        let shadow = ProviderRuntimeConfig {
            shadow_home_path: Some(root.join("shadow").display().to_string()),
            ..ProviderRuntimeConfig::default()
        };
        assert!(state
            .provider_runtime_config("codex", Some(&shadow))
            .is_ok());
        assert!(state
            .provider_runtime_config("claude_code", Some(&shadow))
            .is_err());

        // A legacy DCC-managed home is still normalized away before checks.
        let legacy = ProviderRuntimeConfig {
            home_path: Some(
                state
                    .provider_home_root()
                    .join("claude_code")
                    .display()
                    .to_string(),
            ),
            ..ProviderRuntimeConfig::default()
        };
        assert_eq!(
            state
                .provider_runtime_config("claude_code", Some(&legacy))
                .expect("legacy home"),
            ProviderRuntimeConfig::default()
        );
        assert!(state
            .provider_runtime_config("unknown-provider", None)
            .is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn turn_selection_rejects_unknown_models_for_static_catalog_providers() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let db_path = root.join("state.sqlite");
        let state = SessionCommandState::new_headless(db_path.clone(), root.join("app-data"));
        let session = sample_session("model-authority");
        let workspace = sample_workspace(&session.workspace_id.0, "/tmp/model-authority");
        SqliteWorkspaceRepo::open(&db_path)
            .expect("workspace repo")
            .save_workspace(&workspace)
            .await
            .expect("save workspace");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        save_active_session_thread(&state, &session).await;

        let mut input = selection_input(session.id.clone(), Some("codex"), None);
        input.model = Some("not-a-real-codex-model".to_string());
        let error = state
            .validate_provider_turn_selection(&session, &input)
            .await
            .expect_err("static catalog rejects unknown model");
        assert!(error.to_string().contains("not in the codex catalog"));

        let mut input = selection_input(session.id.clone(), Some("codex"), None);
        input.model = Some(dcc_core::domain::model_registry::CODEX[0].id.to_string());
        state
            .validate_provider_turn_selection(&session, &input)
            .await
            .expect("catalog model accepted");

        let thread = StartThreadInput {
            workspace_id: session.workspace_id.clone(),
            additional_workspace_ids: Vec::new(),
            project_id: session.project_id.clone(),
            provider_id: "droid".to_string(),
            model: Some("droid-does-not-have-this".to_string()),
            provider_runtime: None,
            working_directory_override: None,
            title: None,
        };
        assert!(state.validate_start_thread_scope(&thread).await.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn turn_selection_rejects_unsupported_runtime_config_before_attach() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let db_path = root.join("state.sqlite");
        let state = SessionCommandState::new_headless(db_path.clone(), root.join("app-data"));
        let session = sample_session("runtime-capability");
        let workspace = sample_workspace(&session.workspace_id.0, "/tmp/runtime-capability");
        SqliteWorkspaceRepo::open(&db_path)
            .expect("workspace repo")
            .save_workspace(&workspace)
            .await
            .expect("save workspace");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        save_active_session_thread(&state, &session).await;

        let mut input = selection_input(session.id.clone(), Some("droid"), None);
        input.provider_runtime = Some(ProviderRuntimeConfig {
            max_concurrent_subagents: Some(4),
            ..ProviderRuntimeConfig::default()
        });
        let error = state
            .validate_provider_turn_selection(&session, &input)
            .await
            .expect_err("droid cannot accept subagent limits");
        assert!(error
            .to_string()
            .contains("does not support subagent concurrency limits"));

        let mut input = selection_input(session.id.clone(), Some("codex"), None);
        input.provider_runtime = Some(ProviderRuntimeConfig {
            max_concurrent_subagents: Some(4),
            ..ProviderRuntimeConfig::default()
        });
        state
            .validate_provider_turn_selection(&session, &input)
            .await
            .expect("codex enforces subagent limits");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn archived_thread_rejects_provider_selection_before_cancelling_or_persisting() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session = sample_session("selection-archived-thread");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        ThreadRepo::save_thread(
            &state,
            &Thread {
                id: ThreadId("selection-archived-thread-record".to_string()),
                project_id: session.project_id.clone(),
                session_id: Some(session.id.clone()),
                title: "archived".to_string(),
                archived_at: Some("2026-09-01T00:00:00Z".to_string()),
            },
        )
        .await
        .expect("save archived thread");
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session.id.clone(), inert_provider_binding(&session.id));

        let input = selection_input(session.id.clone(), Some("droid"), None);
        assert!(state
            .prepare_provider_session_for_turn(&input)
            .await
            .is_err());
        assert!(state
            .provider_binding(&session.id)
            .expect("binding")
            .is_some());
        let persisted = state
            .peek_session(&session.id)
            .await
            .expect("reload session")
            .expect("session");
        assert_eq!(persisted.provider_id, "codex");
        assert_eq!(persisted.model, None);
        assert_eq!(persisted.provider_runtime, None);
        assert!(
            SessionEventRepo::list_events_by_session(&state, &session.id)
                .await
                .expect("session events")
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provider_availability_defaults_enabled_persists_and_is_shared_by_runtime_scope() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let db_path = root.join("state.sqlite");
        let app_data = root.join("app-data");
        let first = SessionCommandState::new_headless(db_path.clone(), app_data.clone());
        assert_eq!(
            first
                .provider_availability("droid")
                .expect("default availability"),
            ProviderAvailability {
                provider_id: "droid".to_string(),
                enabled: true,
                state: ProviderAvailabilityState::Enabled,
                generation: 0,
            }
        );
        let disabled = first
            .set_provider_enabled("droid", false)
            .await
            .expect("disable provider without bindings");
        assert!(!disabled.enabled);
        assert_eq!(disabled.state, ProviderAvailabilityState::Disabled);
        let clone = SessionCommandState::new_headless(db_path.clone(), app_data.clone());
        assert_eq!(
            clone
                .provider_availability("droid")
                .expect("shared availability"),
            disabled
        );
        drop(clone);
        drop(first);

        let reopened = SessionCommandState::new_headless(db_path, app_data);
        assert_eq!(
            reopened
                .provider_availability("droid")
                .expect("durable availability"),
            disabled
        );
        let enabled = reopened
            .set_provider_enabled("droid", true)
            .await
            .expect("re-enable provider");
        assert!(enabled.enabled);
        assert_eq!(enabled.state, ProviderAvailabilityState::Enabled);
        assert_eq!(enabled.generation, disabled.generation + 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disabled_provider_fails_before_start_or_selection_mutates_existing_binding() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session = sample_session("availability-selection");
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");
        save_active_session_thread(&state, &session).await;
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session.id.clone(), inert_provider_binding(&session.id));
        state
            .set_provider_enabled("droid", false)
            .await
            .expect("disable provider without droid binding");

        let input = selection_input(session.id.clone(), Some("droid"), None);
        assert!(state
            .prepare_provider_session_for_turn(&input)
            .await
            .is_err());
        assert!(state
            .provider_binding(&session.id)
            .expect("binding")
            .is_some());
        assert_eq!(
            state
                .peek_session(&session.id)
                .await
                .expect("reload")
                .expect("session")
                .provider_id,
            "codex"
        );
        assert!(
            SessionEventRepo::list_events_by_session(&state, &session.id)
                .await
                .expect("events")
                .is_empty()
        );
        assert!(state
            .validate_start_thread_scope(&StartThreadInput {
                workspace_id: session.workspace_id.clone(),
                additional_workspace_ids: Vec::new(),
                project_id: session.project_id.clone(),
                provider_id: "droid".to_string(),
                model: None,
                provider_runtime: None,
                working_directory_override: None,
                title: None,
            })
            .await
            .is_err());
        assert!(state
            .validate_delegation_target(
                &ProviderId("droid".to_string()),
                &DelegationMode::Review,
                &DelegationBudget::default(),
            )
            .is_err());
        assert!(state.set_provider_enabled("unknown", false).await.is_err());
        assert!(state
            .session_repo
            .load_provider_availability("unknown")
            .expect("unknown row")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_availability_persistence_rolls_back_disabling_gate_before_cleanup() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let db_path = root.join("state.sqlite");
        let state = SessionCommandState::new_headless(db_path.clone(), root.join("app-data"));
        // Model another process committing a newer generation after this
        // runtime seeded its cache. The stale conditional upsert fails, so no
        // local cleanup can begin and the durable authority wins the cache.
        let external = SqliteSessionRepo::open(&db_path).expect("external repo");
        external
            .save_provider_availability(&ProviderAvailabilityRecord {
                provider_id: "droid".to_string(),
                enabled: false,
                generation: 9,
                updated_at_ms: 9,
            })
            .expect("external newer generation");

        assert!(state.set_provider_enabled("droid", false).await.is_err());
        assert_eq!(
            state
                .provider_availability("droid")
                .expect("reconciled availability")
                .state,
            ProviderAvailabilityState::Disabled
        );
        assert_eq!(
            state
                .provider_availability("droid")
                .expect("reconciled generation")
                .generation,
            9
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn availability_guard_orders_new_work_before_a_disable_transition() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));

        // Ephemeral review and MCP OAuth use this same per-provider guard
        // through their first provider call. A disable cannot publish
        // Disabling/Disabled until that short initialization section exits.
        let guard = state
            .acquire_provider_availability_transition("droid")
            .await
            .expect("availability guard");
        let (completed, mut result) = tokio::sync::oneshot::channel();
        let contender = {
            let state = state.clone();
            tokio::spawn(async move {
                let _ = completed.send(state.set_provider_enabled("droid", false).await);
            })
        };
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut result)
                .await
                .is_err()
        );
        drop(guard);

        // The queued request proves no waiter was stranded when the guard
        // dropped and reaches Disabled only after the holder's critical work.
        let disabled = tokio::time::timeout(std::time::Duration::from_secs(1), &mut result)
            .await
            .expect("disable unblocked")
            .expect("disable task")
            .expect("disable after guard release");
        assert_eq!(disabled.state, ProviderAvailabilityState::Disabled);
        contender.await.expect("disable join");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn mcp_oauth_lock_order_stabilizes_binding_without_abba() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session_id = SessionId("oauth-availability-order".to_string());
        state.store.lock().expect("store").provider_sessions.insert(
            session_id.clone(),
            inert_provider_binding_for(&session_id, "droid"),
        );
        let initial = state
            .provider_binding(&session_id)
            .expect("initial binding")
            .expect("binding exists");

        // This is the exact lock pair used by start_mcp_oauth. It proves the
        // binding identity while both guards are held and makes a concurrent
        // session transition wait. The disable contender waits on the same
        // provider guard, so both paths share availability -> session order.
        let (availability, session) = state
            .acquire_mcp_oauth_transitions(&initial.provider_id, &session_id)
            .await
            .expect("oauth transitions");
        let current = state
            .provider_binding(&session_id)
            .expect("current binding")
            .expect("binding remains");
        assert_eq!(current.provider_id, initial.provider_id);
        assert_eq!(current.handle.handle_id, initial.handle.handle_id);

        let (session_done, mut session_result) = tokio::sync::oneshot::channel();
        let session_waiter = {
            let state = state.clone();
            let session_id = session_id.clone();
            tokio::spawn(async move {
                let result = state.acquire_provider_transition(&session_id).await;
                let _ = session_done.send(result.is_ok());
            })
        };
        let (disable_done, mut disable_result) = tokio::sync::oneshot::channel();
        let disable_waiter = {
            let state = state.clone();
            tokio::spawn(async move {
                let _ = disable_done.send(state.set_provider_enabled("droid", false).await);
            })
        };
        tokio::task::yield_now().await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut session_result)
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut disable_result)
                .await
                .is_err()
        );

        // Release in reverse acquisition order. The session waiter may now
        // finish, while disable remains blocked on availability. Once OAuth's
        // availability guard drops, disable can take the session lock and
        // complete, demonstrating there is no ABBA cycle.
        drop(session);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), &mut session_result)
                .await
                .expect("session transition unblocked")
                .expect("session result")
        );
        drop(availability);
        let disable = tokio::time::timeout(std::time::Duration::from_secs(1), &mut disable_result)
            .await
            .expect("disable unblocked")
            .expect("disable result");
        // The inert adapter intentionally makes cancellation fail, but the
        // durable disable must still finish fail-closed rather than deadlock.
        assert!(disable.is_err());
        session_waiter.await.expect("session waiter join");
        disable_waiter.await.expect("disable waiter join");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_provider_cancel_keeps_the_durable_gate_disabled_for_retry() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let mut session = sample_session("availability-cancel-failure");
        session.provider_id = "droid".to_string();
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save droid session");
        state.store.lock().expect("store").provider_sessions.insert(
            session.id.clone(),
            inert_provider_binding_for(&session.id, "droid"),
        );

        assert!(state.set_provider_enabled("droid", false).await.is_err());
        assert_eq!(
            state
                .provider_availability("droid")
                .expect("fail-closed availability")
                .state,
            ProviderAvailabilityState::Disabled
        );
        assert!(state
            .provider_binding(&session.id)
            .expect("binding")
            .is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn queued_approval_and_delegation_preflights_reject_before_any_history_mutation() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let mut session = sample_session("capability-queue");
        session.provider_id = "droid".to_string();
        SessionRepo::save_session(&state, &session)
            .await
            .expect("save session");

        assert!(state
            .validate_queued_turn_approval_policy(&session.id, Some(ProviderApprovalPolicy::Ask))
            .await
            .is_err());
        assert!(
            SessionEventRepo::list_events_by_session(&state, &session.id)
                .await
                .expect("history")
                .is_empty()
        );
        state
            .validate_queued_turn_approval_policy(&session.id, None)
            .await
            .expect("empty policy remains compatible");
        let mut legacy_session = session.clone();
        legacy_session.id = SessionId("capability-queue-legacy".to_string());
        legacy_session.provider_id = "unknown-provider".to_string();
        SessionRepo::save_session(&state, &legacy_session)
            .await
            .expect("save legacy session");
        assert!(state
            .validate_queued_turn_approval_policy(&legacy_session.id, None)
            .await
            .is_err());

        let review_budget = DelegationBudget::default();
        state
            .validate_delegation_target(
                &ProviderId("droid".to_string()),
                &DelegationMode::Review,
                &review_budget,
            )
            .expect("registered read-only target");
        state
            .validate_delegation_target(
                &ProviderId("droid".to_string()),
                &DelegationMode::Implement,
                &review_budget,
            )
            .expect("registered edit target");
        assert!(state
            .validate_delegation_target(
                &ProviderId("unknown-provider".to_string()),
                &DelegationMode::Review,
                &review_budget,
            )
            .is_err());
        assert!(
            SessionEventRepo::list_events_by_session(&state, &session.id)
                .await
                .expect("history")
                .is_empty()
        );
    }

    #[test]
    fn appended_live_event_uses_canonical_record_and_existing_is_silent() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-live-event-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let bus = RecordingLiveEventBus::default();
        let state = SessionCommandState::new_with_event_bus(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
            Arc::new(bus.clone()),
        );
        let record = SessionEventRecord {
            event_id: "canonical-event".to_string(),
            session_id: SessionId("session-live".to_string()),
            sequence: 73,
            occurred_at: "2026-09-01T00:00:00Z".to_string(),
            kind: SessionEventKind::TurnCompleted {
                turn_id: TurnId("turn-live".to_string()),
            },
        };
        let event = dcc_core::ports::events::CoreEvent::SessionTurnCompleted {
            session_id: "session-live".to_string(),
            turn_id: "turn-live".to_string(),
        };

        futures::executor::block_on(state.publish_appended_session_event(
            AppendEventOutcome::Inserted(record.clone()),
            event.clone(),
        ))
        .expect("inserted event publishes");
        futures::executor::block_on(
            state.publish_appended_session_event(AppendEventOutcome::Existing(record), event),
        )
        .expect("existing event remains silent");

        assert_eq!(bus.legacy.lock().expect("legacy event lock").len(), 1);
        let live = bus.live.lock().expect("live event lock");
        assert_eq!(live.len(), 1);
        assert_eq!(
            live[0].durable.as_ref().expect("durable identity").event_id,
            "canonical-event"
        );
        assert_eq!(
            live[0].durable.as_ref().expect("durable identity").sequence,
            73
        );
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn terminal_racers_share_one_durable_terminal_and_publication() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-terminal-race-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let app_data = std::fs::canonicalize(app_data.path()).expect("physical app data");
        let first_bus = CountingEventBus::default();
        let second_bus = CountingEventBus::default();
        let first = SessionCommandState::new_with_event_bus(
            db_path.clone(),
            app_data.clone(),
            Arc::new(first_bus.clone()),
        );
        let second = SessionCommandState::new_with_event_bus(
            db_path.clone(),
            app_data,
            Arc::new(second_bus.clone()),
        );
        let session = sample_session("terminal-race");
        let workspace_root = tempfile::tempdir().expect("workspace root");
        let workspace = sample_workspace(
            &session.workspace_id.0,
            workspace_root.path().to_str().expect("workspace path"),
        );
        let workspace_repo = SqliteWorkspaceRepo::open(&db_path).expect("workspace repo");
        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");
        futures::executor::block_on(SessionRepo::save_session(&first, &session))
            .expect("save session");
        let turn_id = TurnId("turn-1".to_string());
        let session_id = session.id.clone();
        let first_call = first.clone();
        let second_call = second.clone();
        let (completed, aborted) = futures::executor::block_on(async move {
            futures::join!(
                first_call.emit_turn_completed(&session_id, &turn_id),
                second_call.emit_turn_aborted(
                    &session_id,
                    &turn_id,
                    Some("provider failed".to_string())
                )
            )
        });
        assert!(completed.is_ok(), "completed racer failed: {completed:?}");
        assert!(aborted.is_ok(), "aborted racer failed: {aborted:?}");
        let events = futures::executor::block_on(SessionEventRepo::list_events_by_session(
            &first,
            &session.id,
        ))
        .expect("list terminal events");
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    &event.kind,
                    SessionEventKind::TurnCompleted { .. } | SessionEventKind::TurnAborted { .. }
                ))
                .count(),
            1
        );
        assert_eq!(first_bus.0.load(Ordering::SeqCst), 1);
        assert_eq!(second_bus.0.load(Ordering::SeqCst), 1);
        drop(first);
        drop(second);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn preexisting_terminal_skips_capture_and_publication() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-terminal-existing-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let bus = CountingEventBus::default();
        let state = SessionCommandState::new_with_event_bus(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
            Arc::new(bus.clone()),
        );
        let session = sample_session("terminal-existing");
        futures::executor::block_on(SessionRepo::save_session(&state, &session))
            .expect("save session");
        let turn_id = TurnId("turn-existing".to_string());
        futures::executor::block_on(state.append_session_event(
            &session.id,
            SessionEventKind::TurnCompleted {
                turn_id: turn_id.clone(),
            },
        ))
        .expect("insert terminal");
        futures::executor::block_on(state.emit_turn_aborted(
            &session.id,
            &turn_id,
            Some("loser".to_string()),
        ))
        .expect("canonical replay");
        assert!(state
            .list_turn_change_sets(&session.id)
            .expect("list review rows")
            .is_empty());
        assert_eq!(bus.0.load(Ordering::SeqCst), 0);
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn synthetic_assistant_items_are_stable_until_a_semantic_boundary() {
        let mut tracker = AssistantMessageTracker::default();

        let (first_id, first_started) = tracker.synthetic_append_target("turn-1");
        let (same_id, same_started) = tracker.synthetic_append_target("turn-1");
        assert!(first_started);
        assert!(!same_started);
        assert_eq!(same_id, first_id);

        let completed = tracker
            .take_synthetic_completion()
            .expect("active synthetic item");
        assert_eq!(completed.0, first_id);
        assert_eq!(completed.1, AssistantMessagePhase::Unknown);

        let (second_id, second_started) = tracker.synthetic_append_target("turn-1");
        assert!(second_started);
        assert_ne!(second_id, first_id);
        assert!(second_id.ends_with("synthetic-1"));
    }

    fn sample_workspace(id: &str, root: &str) -> Workspace {
        Workspace {
            id: WorkspaceId(id.to_string()),
            project_id: ProjectId(format!("project-{id}")),
            name: Some(id.to_string()),
            root_path: format!("/original/{id}"),
            base_branch: "main".to_string(),
            worktree_path: Some(root.to_string()),
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn workspace_mutation_authority_requires_exact_durable_mapping_and_is_redacted() {
        let temporary = tempfile::tempdir().expect("mutation authority root");
        let physical_temporary =
            std::fs::canonicalize(temporary.path()).expect("physical mutation authority root");
        let db_path = physical_temporary.join("sessions.sqlite");
        let app_data = physical_temporary.join("app-data");
        let session_state = SessionCommandState::new_headless(db_path.clone(), app_data);
        let workspace_state = WorkspaceCommandState::from_session(&session_state);
        assert!(Arc::ptr_eq(
            &workspace_state.runtime,
            &session_state.process_runtime()
        ));

        let durable_worktree = physical_temporary.join("worktree");
        std::fs::create_dir(&durable_worktree).expect("durable worktree");
        let durable_worktree = durable_worktree.to_string_lossy().into_owned();
        let workspace = sample_workspace("mutation-authority", &durable_worktree);
        let repo = SqliteWorkspaceRepo::open(&db_path).expect("workspace repo");
        WorkspaceRepo::save_workspace(&repo, &workspace)
            .await
            .expect("save workspace");

        let root_binding = workspace_state
            .authorize_workspace_mutation(&workspace.root_path)
            .await
            .expect("root_path mapping");
        let root_debug = format!("{root_binding:?}");
        assert_eq!(
            root_binding.into_workspace_absolute(),
            PathBuf::from(&workspace.root_path)
        );
        assert!(root_debug.contains("[redacted]"));
        assert!(!root_debug.contains(&workspace.root_path));

        let worktree_binding = workspace_state
            .authorize_workspace_mutation(&durable_worktree)
            .await
            .expect("worktree_path mapping");
        assert_eq!(
            worktree_binding.into_workspace_absolute(),
            PathBuf::from(&durable_worktree)
        );

        assert!(matches!(
            workspace_state
                .authorize_workspace_mutation("/unknown/dcc-workspace")
                .await,
            Err(WorkspaceMutationAuthorizationError::UnknownMapping)
        ));
        assert!(matches!(
            workspace_state.authorize_workspace_mutation("").await,
            Err(WorkspaceMutationAuthorizationError::InvalidRequest)
        ));
        assert!(matches!(
            workspace_state
                .authorize_workspace_mutation("relative/workspace")
                .await,
            Err(WorkspaceMutationAuthorizationError::InvalidRequest)
        ));

        let duplicate = sample_workspace("mutation-authority-duplicate", &durable_worktree);
        WorkspaceRepo::save_workspace(&repo, &duplicate)
            .await
            .expect("save duplicate mapping");
        let ambiguous = workspace_state
            .authorize_workspace_mutation(&durable_worktree)
            .await;
        assert!(matches!(
            ambiguous,
            Err(WorkspaceMutationAuthorizationError::AmbiguousMapping)
        ));

        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        {
            let exact = workspace_state
                .authorize_workspace_mutation_for_id(&workspace.id, &durable_worktree)
                .await
                .expect("workspace id disambiguates a shared repository root");
            assert_eq!(
                exact.into_workspace_absolute(),
                PathBuf::from(&durable_worktree)
            );
            assert!(matches!(
                workspace_state
                    .authorize_workspace_mutation_for_id(&workspace.id, &duplicate.root_path)
                    .await,
                Err(WorkspaceMutationAuthorizationError::UnknownMapping)
            ));
        }

        let debug = format!("{ambiguous:?}");
        assert!(!debug.contains(&durable_worktree));
        assert!(!format!("{workspace_state:?}").contains(db_path.to_string_lossy().as_ref()));
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[tokio::test(flavor = "current_thread")]
    async fn feature_off_git_runner_needs_neither_sqlite_nor_an_existing_root() {
        let temporary = tempfile::tempdir().expect("runtime scope");
        let physical = std::fs::canonicalize(temporary.path()).expect("physical runtime scope");
        let session_state = SessionCommandState::new_headless(
            physical.join("sessions.sqlite"),
            physical.join("app-data"),
        );
        let mut workspace_state = WorkspaceCommandState::from_session(&session_state);
        workspace_state.db_path = physical.join("missing-registry.sqlite");

        let nonexistent = "relative/non-git/workspace";
        let direct = workspace_state
            .run_git_workspace_mutation(nonexistent, |path| {
                assert_eq!(path, Path::new(nonexistent));
                Ok::<_, ()>(31_u8)
            })
            .await;
        assert_eq!(direct.unwrap(), 31);

        let caller = std::thread::current().id();
        let blocking = workspace_state
            .run_git_workspace_mutation_blocking(nonexistent, move |path| {
                assert_eq!(path, Path::new(nonexistent));
                assert_ne!(std::thread::current().id(), caller);
                Ok::<_, ()>(37_u8)
            })
            .await;
        assert_eq!(blocking.unwrap(), 37);
        assert!(!workspace_state.db_path.exists());
    }

    fn physical_db_path(path: PathBuf) -> PathBuf {
        let parent = std::fs::canonicalize(path.parent().expect("database parent"))
            .expect("canonical database parent");
        parent.join(path.file_name().expect("database filename"))
    }

    fn mcp_status(
        definition_id: &str,
        session_id: &SessionId,
        state: McpRuntimeState,
    ) -> McpRuntimeStatus {
        McpRuntimeStatus {
            definition_id: McpDefinitionId(definition_id.to_string()),
            provider_id: ProviderId("claude_code".to_string()),
            provider_version: "claude-agent-sdk@test+claude-code@test".to_string(),
            session_id: session_id.clone(),
            state,
            tools: Vec::new(),
            checked_at: "2026-07-28T00:00:00Z".to_string(),
            bounded_error: None,
        }
    }

    #[test]
    fn mcp_runtime_snapshots_are_ephemeral_sorted_and_identity_bound() {
        let app_data = tempfile::tempdir().expect("app data directory");
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-mcp-{}.sqlite", Uuid::new_v4())),
        );
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let session_id = SessionId("session-1".to_string());
        let provider_version = "claude-agent-sdk@test+claude-code@test";

        futures::executor::block_on(state.replace_mcp_runtime_statuses(
            &session_id,
            "claude_code",
            provider_version,
            vec![
                mcp_status("zeta", &session_id, McpRuntimeState::Connected),
                mcp_status("alpha", &session_id, McpRuntimeState::AttachingProvider),
            ],
        ))
        .expect("replace MCP status snapshot");

        assert_eq!(
            state
                .list_mcp_runtime_statuses(&session_id)
                .expect("list statuses")
                .iter()
                .map(|status| status.definition_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );

        let mismatch = futures::executor::block_on(state.replace_mcp_runtime_statuses(
            &session_id,
            "claude_code",
            "different-version",
            vec![mcp_status(
                "replacement",
                &session_id,
                McpRuntimeState::Connected,
            )],
        ));
        assert!(matches!(mismatch, Err(dcc_core::CoreError::Provider(_))));
        assert_eq!(
            state
                .list_mcp_runtime_statuses(&session_id)
                .expect("snapshot remains")
                .len(),
            2
        );

        futures::executor::block_on(state.clear_mcp_runtime_statuses(&session_id))
            .expect("clear snapshot");
        assert!(state
            .list_mcp_runtime_statuses(&session_id)
            .expect("list cleared statuses")
            .is_empty());

        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn multi_workspace_scope_resolves_only_bundle_worktrees_and_gates_provider() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-scope-{}.sqlite", Uuid::new_v4())),
        );
        let repo = SqliteWorkspaceRepo::open(&db_path).expect("open workspace repo");
        let primary = sample_workspace("primary", "/tmp/dcc-primary-worktree");
        let mut secondary = sample_workspace("secondary", "/tmp/dcc-secondary-worktree");
        secondary.state = WorkspaceState::SetupPending;
        futures::executor::block_on(repo.save_workspace(&primary)).expect("save primary");
        futures::executor::block_on(repo.save_workspace(&secondary)).expect("save secondary");
        let bundle_id = WorkspaceBundleId("bundle-1".to_string());
        futures::executor::block_on(repo.save_workspace_bundle(
            &WorkspaceBundle {
                id: bundle_id.clone(),
                name: "feature".to_string(),
                primary_workspace_id: primary.id.clone(),
                state: WorkspaceBundleState::Ready,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
            &[
                WorkspaceBundleMember {
                    bundle_id: bundle_id.clone(),
                    workspace_id: primary.id.clone(),
                    created_for_bundle: true,
                    position: 0,
                },
                WorkspaceBundleMember {
                    bundle_id,
                    workspace_id: secondary.id.clone(),
                    created_for_bundle: true,
                    position: 1,
                },
            ],
        ))
        .expect("save bundle");

        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let session = Session {
            id: SessionId("session-1".to_string()),
            project_id: primary.project_id.clone(),
            workspace_id: primary.id.clone(),
            additional_workspace_ids: vec![secondary.id.clone()],
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let unsupported =
            futures::executor::block_on(state.resolve_session_working_directories(&session, false));
        assert!(matches!(unsupported, Err(dcc_core::CoreError::Provider(_))));

        let (primary_root, additional_roots) =
            futures::executor::block_on(state.resolve_session_working_directories(&session, true))
                .expect("resolve multi-root scope");
        assert_eq!(primary_root, "/tmp/dcc-primary-worktree");
        assert_eq!(additional_roots, vec!["/tmp/dcc-secondary-worktree"]);
        let instructions =
            futures::executor::block_on(state.multi_workspace_scope_instructions(&session))
                .expect("build scope instructions")
                .expect("multi scope instructions");
        assert!(instructions.contains("/tmp/dcc-primary-worktree"));
        assert!(instructions.contains("/tmp/dcc-secondary-worktree"));
        assert!(!instructions.contains("/original/primary"));

        secondary.worktree_path = Some("relative/dcc-secondary-worktree".to_string());
        futures::executor::block_on(repo.save_workspace(&secondary))
            .expect("save relative worktree path");
        let relative_path =
            futures::executor::block_on(state.resolve_session_working_directories(&session, true))
                .expect_err("relative worktree path must be rejected");
        assert!(
            matches!(relative_path, dcc_core::CoreError::InvalidInput(message) if message.contains("worktree path must be absolute"))
        );

        secondary.worktree_path = Some("   ".to_string());
        futures::executor::block_on(repo.save_workspace(&secondary))
            .expect("save empty worktree path");
        let empty_path =
            futures::executor::block_on(state.resolve_session_working_directories(&session, true))
                .expect_err("empty worktree path must be rejected");
        assert!(
            matches!(empty_path, dcc_core::CoreError::InvalidInput(message) if message.contains("has no DCC-managed worktree"))
        );

        secondary.worktree_path = Some("/tmp/dcc-secondary-worktree".to_string());
        for unavailable_state in [
            WorkspaceState::Initializing,
            WorkspaceState::Archived,
            WorkspaceState::Completed,
        ] {
            secondary.state = unavailable_state;
            futures::executor::block_on(repo.save_workspace(&secondary))
                .expect("save unavailable workspace state");
            let result = futures::executor::block_on(
                state.resolve_session_working_directories(&session, true),
            );
            let error = result.expect_err("unavailable workspace state must be rejected");
            assert!(
                matches!(error, dcc_core::CoreError::InvalidInput(message) if message.contains("must be ready or have setup pending"))
            );
        }

        drop(state);
        drop(repo);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn baseline_capture_refs_match_the_durable_rows_without_cross_root_attribution() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-baseline-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let primary_root = tempfile::tempdir().expect("primary root");
        let secondary_root = tempfile::tempdir().expect("secondary root");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let repo = SqliteWorkspaceRepo::open(&db_path).expect("open workspace repo");
        let primary = sample_workspace(
            "primary",
            primary_root.path().to_str().expect("UTF-8 primary path"),
        );
        let secondary = sample_workspace(
            "secondary",
            secondary_root
                .path()
                .to_str()
                .expect("UTF-8 secondary path"),
        );
        futures::executor::block_on(repo.save_workspace(&primary)).expect("save primary");
        futures::executor::block_on(repo.save_workspace(&secondary)).expect("save secondary");
        let bundle_id = WorkspaceBundleId("baseline-bundle".to_string());
        futures::executor::block_on(repo.save_workspace_bundle(
            &WorkspaceBundle {
                id: bundle_id.clone(),
                name: "baseline".to_string(),
                primary_workspace_id: primary.id.clone(),
                state: WorkspaceBundleState::Ready,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
            &[
                WorkspaceBundleMember {
                    bundle_id: bundle_id.clone(),
                    workspace_id: primary.id.clone(),
                    created_for_bundle: true,
                    position: 0,
                },
                WorkspaceBundleMember {
                    bundle_id,
                    workspace_id: secondary.id.clone(),
                    created_for_bundle: true,
                    position: 1,
                },
            ],
        ))
        .expect("save workspace bundle");
        let session = Session {
            id: SessionId("baseline-session".to_string()),
            project_id: primary.project_id.clone(),
            workspace_id: primary.id.clone(),
            additional_workspace_ids: vec![secondary.id.clone()],
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        futures::executor::block_on(SessionRepo::save_session(&state, &session))
            .expect("save session");
        let turn_id = TurnId("baseline-turn".to_string());

        let capture =
            futures::executor::block_on(state.capture_turn_review_baseline(&session, &turn_id))
                .expect("capture durable baseline rows");

        assert_eq!(capture.snapshots.len(), 2);
        let rows = state
            .list_turn_change_sets(&session.id)
            .expect("list durable rows");
        assert_eq!(rows.len(), capture.snapshots.len());
        for snapshot in &capture.snapshots {
            let row = state
                .get_turn_change_set(&snapshot.snapshot_id)
                .expect("load durable row")
                .expect("returned reference has a durable row");
            assert_eq!(row.session_id, snapshot.session_id);
            assert_eq!(row.turn_id, snapshot.turn_id);
            assert_eq!(row.workspace_id, snapshot.workspace_id);
            assert_eq!(row.state, "unavailable");
        }
        let returned_ids = capture
            .snapshots
            .iter()
            .map(|snapshot| snapshot.snapshot_id.as_str())
            .collect::<HashSet<_>>();
        let persisted_ids = rows
            .iter()
            .map(|row| row.snapshot_id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(returned_ids, persisted_ids);
        assert!(capture
            .snapshots
            .iter()
            .any(|snapshot| snapshot.workspace_id == primary.id));
        assert!(capture
            .snapshots
            .iter()
            .any(|snapshot| snapshot.workspace_id == secondary.id));

        drop(repo);
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn baseline_root_resolution_error_still_returns_a_persisted_unavailable_ref() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-baseline-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let repo = SqliteWorkspaceRepo::open(&db_path).expect("open workspace repo");
        let primary = sample_workspace("missing-root-workspace", "/tmp/missing-root-workspace");
        futures::executor::block_on(repo.save_workspace(&primary)).expect("save primary");
        let session = Session {
            id: SessionId("missing-root-session".to_string()),
            project_id: primary.project_id.clone(),
            workspace_id: primary.id.clone(),
            // A second root without a ready DCC bundle makes root resolution
            // fail before any workspace capture is attempted.
            additional_workspace_ids: vec![WorkspaceId("missing-secondary".to_string())],
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        futures::executor::block_on(SessionRepo::save_session(&state, &session))
            .expect("save session");
        let turn_id = TurnId("missing-root-turn".to_string());

        let capture =
            futures::executor::block_on(state.capture_turn_review_baseline(&session, &turn_id))
                .expect("unavailable row is still durable");

        let report = futures::executor::block_on(state.begin_capture_v2_after_m3(
            &session,
            &turn_id,
            capture.clone(),
        ));
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        assert_eq!(report.disposition, CaptureV2StartDisposition::Skipped);
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        assert_eq!(report.disposition, CaptureV2StartDisposition::Disabled);

        assert_eq!(capture.snapshots.len(), 1);
        let snapshot = &capture.snapshots[0];
        assert_eq!(snapshot.session_id, session.id);
        assert_eq!(snapshot.turn_id, turn_id);
        assert_eq!(snapshot.workspace_id, session.workspace_id);
        let row = state
            .get_turn_change_set(&snapshot.snapshot_id)
            .expect("load durable unavailable row")
            .expect("returned reference has a durable row");
        assert_eq!(row.state, "unavailable");
        assert_eq!(row.session_id, snapshot.session_id);
        assert_eq!(row.turn_id, snapshot.turn_id);
        assert_eq!(row.workspace_id, snapshot.workspace_id);

        drop(repo);
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn capture_v2_start_rejects_empty_baseline_without_initializing_runtime() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-capture-v2-empty-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let session = sample_session("capture-v2-empty");
        let report = futures::executor::block_on(state.begin_capture_v2_after_m3(
            &session,
            &TurnId("empty-turn".to_string()),
            M3BaselineCapture::default(),
        ));
        #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
        assert_eq!(report.disposition, CaptureV2StartDisposition::Skipped);
        #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
        assert_eq!(report.disposition, CaptureV2StartDisposition::Disabled);
        assert_eq!(report.active_captures, 0);
        assert_eq!(report.failed_captures, 0);
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[cfg(not(all(target_os = "macos", feature = "guarded-undo-capture-v2")))]
    #[test]
    fn capture_v2_feature_off_returns_before_row_or_root_lookup() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-capture-v2-off-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let session = sample_session("never-persisted");
        let turn_id = TurnId("never-persisted-turn".to_owned());
        let baseline = M3BaselineCapture {
            snapshots: vec![M3SnapshotRef {
                snapshot_id: "never-persisted-snapshot".to_owned(),
                session_id: session.id.clone(),
                turn_id: turn_id.clone(),
                workspace_id: session.workspace_id.clone(),
            }],
            root_bindings: vec![M3RootBinding {
                workspace_id: session.workspace_id.clone(),
                workspace_absolute: PathBuf::from("/definitely-not-looked-up"),
            }],
        };
        let report = futures::executor::block_on(
            state.begin_capture_v2_after_m3(&session, &turn_id, baseline),
        );
        assert_eq!(report.disposition, CaptureV2StartDisposition::Disabled);
        assert!(state
            .get_turn_change_set("never-persisted-snapshot")
            .unwrap()
            .is_none());
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[test]
    fn capture_v2_rejects_worktree_mapping_change_after_m3() {
        let db_root = tempfile::tempdir_in("/private/tmp").unwrap();
        let app_data = tempfile::tempdir_in("/private/tmp").unwrap();
        let original_root = tempfile::tempdir_in("/private/tmp").unwrap();
        let replacement_root = tempfile::tempdir_in("/private/tmp").unwrap();
        let original = std::fs::canonicalize(original_root.path()).unwrap();
        let replacement = std::fs::canonicalize(replacement_root.path()).unwrap();
        for arguments in [
            vec!["init", "-q"],
            vec!["config", "user.name", "DCC Test"],
            vec!["config", "user.email", "dcc@example.invalid"],
        ] {
            assert!(std::process::Command::new("/usr/bin/git")
                .current_dir(&original)
                .args(arguments)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(original.join("tracked.txt"), b"before\n").unwrap();
        assert!(std::process::Command::new("/usr/bin/git")
            .current_dir(&original)
            .args(["add", "--", "tracked.txt"])
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("/usr/bin/git")
            .current_dir(&original)
            .args(["commit", "--no-gpg-sign", "--no-verify", "-qm", "initial"])
            .status()
            .unwrap()
            .success());

        let db_path = std::fs::canonicalize(db_root.path())
            .unwrap()
            .join("sessions.sqlite");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).unwrap(),
        );
        let repo = SqliteWorkspaceRepo::open(&db_path).unwrap();
        let workspace = sample_workspace("mapping-swap", original.to_str().unwrap());
        futures::executor::block_on(repo.save_workspace(&workspace)).unwrap();
        let session = Session {
            id: SessionId("mapping-swap-session".to_owned()),
            project_id: workspace.project_id.clone(),
            workspace_id: workspace.id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_owned(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        futures::executor::block_on(SessionRepo::save_session(&state, &session)).unwrap();
        let turn_id = TurnId("mapping-swap-turn".to_owned());
        let baseline =
            futures::executor::block_on(state.capture_turn_review_baseline(&session, &turn_id))
                .unwrap();
        assert_eq!(baseline.root_bindings.len(), 1);

        let replacement_workspace = sample_workspace("mapping-swap", replacement.to_str().unwrap());
        futures::executor::block_on(repo.save_workspace(&replacement_workspace)).unwrap();
        let report = futures::executor::block_on(
            state.begin_capture_v2_after_m3(&session, &turn_id, baseline),
        );
        assert_eq!(report.disposition, CaptureV2StartDisposition::Skipped);

        drop(repo);
        drop(state);
    }

    #[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
    #[tokio::test(flavor = "current_thread")]
    async fn recovery_roots_skip_stale_unrelated_and_fallback_to_root_path() {
        let db_root = tempfile::tempdir_in("/private/tmp").unwrap();
        let app_data = tempfile::tempdir_in("/private/tmp").unwrap();
        let current_root = tempfile::tempdir_in("/private/tmp").unwrap();
        let fallback_root = tempfile::tempdir_in("/private/tmp").unwrap();
        let current = std::fs::canonicalize(current_root.path()).unwrap();
        let fallback = std::fs::canonicalize(fallback_root.path()).unwrap();
        let db_path = std::fs::canonicalize(db_root.path())
            .unwrap()
            .join("sessions.sqlite");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).unwrap(),
        );
        let repo = SqliteWorkspaceRepo::open(&db_path).unwrap();

        let current_workspace = sample_workspace("recovery-current", current.to_str().unwrap());
        let stale_workspace = sample_workspace(
            "recovery-stale",
            "/private/tmp/dcc-definitely-missing-stale-worktree",
        );
        let mut fallback_workspace =
            sample_workspace("recovery-fallback", fallback.to_str().unwrap());
        fallback_workspace.worktree_path = None;
        fallback_workspace.root_path = fallback.to_string_lossy().into_owned();
        for workspace in [&current_workspace, &stale_workspace, &fallback_workspace] {
            repo.save_workspace(workspace).await.unwrap();
        }
        let session = Session {
            id: SessionId("recovery-current-session".to_owned()),
            project_id: current_workspace.project_id.clone(),
            workspace_id: current_workspace.id.clone(),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_owned(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        SessionRepo::save_session(&state, &session).await.unwrap();

        let roots = state
            .capture_v2_recovery_roots(&session)
            .await
            .expect("bounded recovery roots");
        assert!(roots.contains(&current));
        assert!(roots.contains(&fallback));
        assert!(!roots
            .iter()
            .any(|root| root.ends_with("dcc-definitely-missing-stale-worktree")));

        drop(repo);
        drop(state);
    }

    #[test]
    fn capture_v2_root_policy_is_absolute_and_component_safe() {
        assert_eq!(
            SessionCommandState::lexical_absolute_root(Path::new("/tmp/./workspace")),
            Some(PathBuf::from("/tmp/workspace"))
        );
        assert_eq!(
            SessionCommandState::lexical_absolute_root(Path::new("relative/workspace")),
            None
        );
        assert_eq!(
            SessionCommandState::lexical_absolute_root(Path::new("/tmp/../workspace")),
            None
        );
    }

    #[test]
    fn states_share_runtime_for_same_physical_scope() {
        let root = tempfile::tempdir().expect("runtime test root");
        let physical_root = std::fs::canonicalize(root.path()).expect("physical root");
        let db_path = physical_root.join("state.sqlite");
        let app_data = physical_root.join("app-data");
        let first = SessionCommandState::new_headless(db_path.clone(), app_data.clone());
        let second = SessionCommandState::new_headless(db_path, app_data);
        assert!(Arc::ptr_eq(
            &first.process_runtime(),
            &second.process_runtime()
        ));
        assert!(Arc::ptr_eq(
            &first.process_runtime().terminal_arbiter(),
            &second.process_runtime().terminal_arbiter()
        ));
        assert!(Arc::ptr_eq(&first.store, &second.store));
    }

    #[derive(Default)]
    struct FakeEphemeralMcpProjection {
        fail: AtomicBool,
        projects: AtomicUsize,
        next_lease: AtomicUsize,
        revocations: Mutex<Vec<(String, String)>>,
        config: Mutex<Option<ProviderMcpServerConfig>>,
    }

    impl EphemeralMcpProjection for FakeEphemeralMcpProjection {
        fn project_for_session(
            &self,
            _session: &Session,
        ) -> Result<Option<EphemeralMcpProjectionLease>> {
            self.projects.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(dcc_core::CoreError::Provider(
                    "synthetic projector failure".to_string(),
                ));
            }
            let lease_id = format!(
                "lease-{}",
                self.next_lease.fetch_add(1, Ordering::SeqCst) + 1
            );
            self.config
                .lock()
                .map(|config| {
                    config
                        .clone()
                        .map(|server| EphemeralMcpProjectionLease { server, lease_id })
                })
                .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))
        }

        fn revoke_session(&self, session_id: &SessionId, lease_id: &str) {
            self.revocations
                .lock()
                .expect("revocation log")
                .push((session_id.0.clone(), lease_id.to_string()));
        }
    }

    fn fake_ephemeral_server(definition_id: &str, server_name: &str) -> ProviderMcpServerConfig {
        ProviderMcpServerConfig {
            definition_id: McpDefinitionId(definition_id.to_string()),
            server_name: server_name.to_string(),
            transport: ProviderMcpTransport::Http {
                url: "http://127.0.0.1:1/mcp".to_string(),
                headers: Vec::new(),
            },
            oauth_state: None,
            tool_policies: Vec::new(),
        }
    }

    #[test]
    fn ephemeral_projection_is_shared_gated_and_revoked_without_replacement() {
        let root = tempfile::tempdir().expect("runtime test root");
        let root = std::fs::canonicalize(root.path()).expect("physical root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let projector = Arc::new(FakeEphemeralMcpProjection {
            config: Mutex::new(Some(fake_ephemeral_server(
                "dcc-browser",
                "dcc-browser-webview",
            ))),
            ..Default::default()
        });
        state
            .install_ephemeral_mcp_projection(projector.clone())
            .expect("first installation should succeed");
        assert!(state
            .install_ephemeral_mcp_projection(Arc::new(FakeEphemeralMcpProjection::default()))
            .is_err());

        let session = sample_session("ephemeral-projection");
        let projected = state
            .project_ephemeral_mcp_server(Some("audited-runtime"), &session)
            .expect("projection should resolve")
            .expect("audited provider should receive projection");
        assert_eq!(projected.server.definition_id.0, "dcc-browser");
        assert_eq!(projected.lease_id, "lease-1");
        assert!(state
            .project_ephemeral_mcp_server(None, &session)
            .expect("unsupported provider should not call projector")
            .is_none());
        assert_eq!(projector.projects.load(Ordering::SeqCst), 1);

        state.revoke_ephemeral_mcp_projection(&session.id, Some(&projected.lease_id));
        assert_eq!(
            projector.revocations.lock().unwrap().as_slice(),
            &[(session.id.0.clone(), "lease-1".to_string())]
        );

        let winner = state
            .project_ephemeral_mcp_server(Some("audited-runtime"), &session)
            .expect("second projection should resolve")
            .expect("second audited projection should exist");
        assert_eq!(winner.lease_id, "lease-2");
        let binding = ProviderSessionBinding {
            provider_id: "codex".to_string(),
            handle: SessionHandle {
                provider_id: ProviderId("codex".to_string()),
                session_id: session.id.clone(),
                handle_id: "winner-handle".to_string(),
            },
            current_turn_id: Arc::new(AsyncMutex::new(None)),
            terminal_lock: Arc::new(AsyncMutex::new(())),
            terminal_token: Arc::new(TerminalTokenState::default()),
            usage_turn_id: Arc::new(AsyncMutex::new(None)),
            assistant_messages: Arc::new(AsyncMutex::new(AssistantMessageTracker::default())),
            projected_mcp_definition_ids: Arc::new(HashSet::new()),
            ephemeral_mcp_lease_id: Some(winner.lease_id.clone()),
        };
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session.id.clone(), binding.clone());
        futures::executor::block_on(state.remove_binding_if_same(&session.id, &binding))
            .expect("winner cleanup should succeed");
        assert_eq!(
            projector.revocations.lock().unwrap().as_slice(),
            &[
                (session.id.0.clone(), "lease-1".to_string()),
                (session.id.0.clone(), "lease-2".to_string()),
            ]
        );
    }

    #[test]
    fn ephemeral_projection_error_degrades_without_blocking_base_flow() {
        let root = tempfile::tempdir().expect("runtime test root");
        let root = std::fs::canonicalize(root.path()).expect("physical root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let projector = Arc::new(FakeEphemeralMcpProjection {
            fail: AtomicBool::new(true),
            ..Default::default()
        });
        state
            .install_ephemeral_mcp_projection(projector.clone())
            .expect("projection installation should succeed");

        let session = sample_session("ephemeral-projection-failure");
        assert!(state
            .project_ephemeral_mcp_server(Some("audited-runtime"), &session)
            .expect("projection failure should degrade")
            .is_none());
        assert_eq!(projector.projects.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ephemeral_projection_cannot_collide_with_persistent_server_identity() {
        for (persistent_definition, persistent_name) in [
            ("dcc-browser", "other-server"),
            ("other-definition", "dcc-browser-webview"),
        ] {
            let mut servers = vec![fake_ephemeral_server(
                persistent_definition,
                persistent_name,
            )];
            let error = append_ephemeral_mcp_server(
                &mut servers,
                fake_ephemeral_server("dcc-browser", "dcc-browser-webview"),
            )
            .expect_err("persistent identity collision must fail closed");
            assert!(error.to_string().contains("collides"));
            assert_eq!(servers.len(), 1);
        }

        let mut servers = vec![fake_ephemeral_server("persistent", "persistent-server")];
        append_ephemeral_mcp_server(
            &mut servers,
            fake_ephemeral_server("dcc-browser", "dcc-browser-webview"),
        )
        .expect("distinct persistent and ephemeral identities should append");
        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn terminal_token_blocks_replacement_until_raii_drop() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-token-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_headless(
            db_path,
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let session_id = SessionId("token-session".to_string());
        let turn_id = TurnId("token-turn".to_string());
        let binding = ProviderSessionBinding {
            provider_id: "codex".to_string(),
            handle: SessionHandle {
                provider_id: ProviderId("codex".to_string()),
                session_id: session_id.clone(),
                handle_id: "token-handle".to_string(),
            },
            current_turn_id: Arc::new(AsyncMutex::new(Some(turn_id.0.clone()))),
            terminal_lock: Arc::new(AsyncMutex::new(())),
            terminal_token: Arc::new(TerminalTokenState::default()),
            usage_turn_id: Arc::new(AsyncMutex::new(Some(turn_id.0.clone()))),
            assistant_messages: Arc::new(AsyncMutex::new(AssistantMessageTracker::default())),
            projected_mcp_definition_ids: Arc::new(HashSet::new()),
            ephemeral_mcp_lease_id: None,
        };
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session_id.clone(), binding.clone());
        let token = futures::executor::block_on(state.acquire_terminal_token(
            &session_id,
            &turn_id,
            &binding,
        ))
        .expect("acquire terminal token");
        assert!(futures::executor::block_on(
            state.set_active_turn(&session_id, Some("replacement".to_string()),)
        )
        .is_err());
        drop(token);
        futures::executor::block_on(
            state.set_active_turn(&session_id, Some("replacement".to_string())),
        )
        .expect("replacement after token drop");
    }

    #[test]
    fn aborted_leader_does_not_flush_assistant_completion_events() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-abort-flush-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let session = sample_session("abort-flush");
        let workspace_root = tempfile::tempdir().expect("workspace root");
        let workspace = sample_workspace(
            &session.workspace_id.0,
            workspace_root.path().to_str().expect("workspace path"),
        );
        let workspace_repo = SqliteWorkspaceRepo::open(&db_path).expect("workspace repo");
        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");
        futures::executor::block_on(SessionRepo::save_session(&state, &session))
            .expect("save session");
        let turn_id = TurnId("abort-turn".to_string());
        let binding = ProviderSessionBinding {
            provider_id: "codex".to_string(),
            handle: SessionHandle {
                provider_id: ProviderId("codex".to_string()),
                session_id: session.id.clone(),
                handle_id: "abort-handle".to_string(),
            },
            current_turn_id: Arc::new(AsyncMutex::new(Some(turn_id.0.clone()))),
            terminal_lock: Arc::new(AsyncMutex::new(())),
            terminal_token: Arc::new(TerminalTokenState::default()),
            usage_turn_id: Arc::new(AsyncMutex::new(Some(turn_id.0.clone()))),
            assistant_messages: Arc::new(AsyncMutex::new(AssistantMessageTracker::default())),
            projected_mcp_definition_ids: Arc::new(HashSet::new()),
            ephemeral_mcp_lease_id: None,
        };
        binding
            .assistant_messages
            .try_lock()
            .expect("tracker")
            .active
            .insert("assistant-1".to_string(), AssistantMessagePhase::Unknown);
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session.id.clone(), binding);
        futures::executor::block_on(state.terminalize_turn(
            &session.id,
            &turn_id,
            TerminalRequest::Aborted {
                reason: Some("cancelled".to_string()),
                source: TerminalSource::ProviderFailed,
            },
        ))
        .expect("aborted terminal");
        let events = futures::executor::block_on(SessionEventRepo::list_events_by_session(
            &state,
            &session.id,
        ))
        .expect("events");
        assert!(events
            .iter()
            .any(|event| matches!(&event.kind, SessionEventKind::TurnAborted { .. })));
        assert!(!events.iter().any(|event| matches!(
            &event.kind,
            SessionEventKind::TurnAssistantMessageCompleted { .. }
        )));
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn publish_failure_still_cleans_terminal_binding_after_commit() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-publish-failure-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_with_event_bus(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
            Arc::new(FailingEventBus),
        );
        let session = sample_session("publish-failure");
        let workspace_root = tempfile::tempdir().expect("workspace root");
        let workspace = sample_workspace(
            &session.workspace_id.0,
            workspace_root.path().to_str().expect("workspace path"),
        );
        let workspace_repo = SqliteWorkspaceRepo::open(&db_path).expect("workspace repo");
        futures::executor::block_on(workspace_repo.save_workspace(&workspace))
            .expect("save workspace");
        futures::executor::block_on(SessionRepo::save_session(&state, &session))
            .expect("save session");
        let turn_id = TurnId("publish-turn".to_string());
        let binding = ProviderSessionBinding {
            provider_id: "codex".to_string(),
            handle: SessionHandle {
                provider_id: ProviderId("codex".to_string()),
                session_id: session.id.clone(),
                handle_id: "publish-handle".to_string(),
            },
            current_turn_id: Arc::new(AsyncMutex::new(Some(turn_id.0.clone()))),
            terminal_lock: Arc::new(AsyncMutex::new(())),
            terminal_token: Arc::new(TerminalTokenState::default()),
            usage_turn_id: Arc::new(AsyncMutex::new(Some(turn_id.0.clone()))),
            assistant_messages: Arc::new(AsyncMutex::new(AssistantMessageTracker::default())),
            projected_mcp_definition_ids: Arc::new(HashSet::new()),
            ephemeral_mcp_lease_id: None,
        };
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session.id.clone(), binding);
        let result = futures::executor::block_on(state.terminalize_turn(
            &session.id,
            &turn_id,
            TerminalRequest::Completed,
        ));
        let result = result.expect("durable terminal survives publish failure");
        assert!(result.inserted);
        let events = futures::executor::block_on(SessionEventRepo::list_events_by_session(
            &state,
            &session.id,
        ))
        .expect("events");
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(&event.kind, SessionEventKind::TurnCompleted { .. }))
                .count(),
            1
        );
        let binding = state
            .provider_binding(&session.id)
            .expect("binding lookup")
            .expect("binding retained for completed queue");
        assert!(binding
            .current_turn_id
            .try_lock()
            .expect("turn lock")
            .is_none());
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn unbound_started_abort_preserves_older_active_binding() {
        let db_path = physical_db_path(
            std::env::temp_dir().join(format!("dcc-unbound-abort-{}.sqlite", Uuid::new_v4())),
        );
        let app_data = tempfile::tempdir().expect("app data directory");
        let state = SessionCommandState::new_headless(
            db_path.clone(),
            std::fs::canonicalize(app_data.path()).expect("physical app data"),
        );
        let session = sample_session("unbound-abort");
        futures::executor::block_on(SessionRepo::save_session(&state, &session))
            .expect("save session");
        let old_turn = TurnId("old-turn".to_string());
        let new_turn = TurnId("new-turn".to_string());
        let binding = ProviderSessionBinding {
            provider_id: "codex".to_string(),
            handle: SessionHandle {
                provider_id: ProviderId("codex".to_string()),
                session_id: session.id.clone(),
                handle_id: "old-handle".to_string(),
            },
            current_turn_id: Arc::new(AsyncMutex::new(Some(old_turn.0.clone()))),
            terminal_lock: Arc::new(AsyncMutex::new(())),
            terminal_token: Arc::new(TerminalTokenState::default()),
            usage_turn_id: Arc::new(AsyncMutex::new(Some(old_turn.0.clone()))),
            assistant_messages: Arc::new(AsyncMutex::new(AssistantMessageTracker::default())),
            projected_mcp_definition_ids: Arc::new(HashSet::new()),
            ephemeral_mcp_lease_id: None,
        };
        state
            .store
            .lock()
            .expect("store")
            .provider_sessions
            .insert(session.id.clone(), binding);
        futures::executor::block_on(state.emit_unbound_started_turn_aborted(
            &session.id,
            &new_turn,
            Some("binding token busy".to_string()),
        ))
        .expect("unbound turn abort");
        let current = state
            .provider_binding(&session.id)
            .expect("binding lookup")
            .expect("old binding remains");
        assert_eq!(
            futures::executor::block_on(current.current_turn_id.lock()).as_deref(),
            Some(old_turn.0.as_str())
        );
        let events = futures::executor::block_on(SessionEventRepo::list_events_by_session(
            &state,
            &session.id,
        ))
        .expect("events");
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            SessionEventKind::TurnAborted { turn_id, .. } if turn_id == &new_turn
        )));
        drop(state);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn distinct_runtime_scopes_are_isolated_and_first_run_creates_app_data() {
        let root = tempfile::tempdir().expect("runtime test root");
        let physical_root = std::fs::canonicalize(root.path()).expect("physical root");
        let first_app_data = physical_root.join("first").join("nested-app-data");
        let second_app_data = physical_root.join("second").join("nested-app-data");
        let first_db = physical_root.join("first.sqlite");
        let second_db = physical_root.join("second.sqlite");
        assert!(!first_app_data.exists());
        let first = SessionCommandState::new_headless(first_db.clone(), first_app_data.clone());
        let second = SessionCommandState::new_headless(second_db, second_app_data);
        assert!(first_app_data.is_dir());
        assert!(first_db.is_file());
        assert!(!Arc::ptr_eq(
            &first.process_runtime(),
            &second.process_runtime()
        ));
        assert!(!Arc::ptr_eq(
            &first.process_runtime().terminal_arbiter(),
            &second.process_runtime().terminal_arbiter()
        ));
    }

    #[test]
    #[cfg(unix)]
    fn state_rejects_intermediate_and_final_symlink_inputs_without_paths() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("runtime test root");
        let physical_root = std::fs::canonicalize(root.path()).expect("physical root");
        let real_app = physical_root.join("real-app");
        std::fs::create_dir(&real_app).expect("real app data");
        let app_alias = physical_root.join("app-alias");
        symlink(&real_app, &app_alias).expect("app alias");
        let real_parent = physical_root.join("real-parent");
        std::fs::create_dir(&real_parent).expect("real parent");
        let parent_alias = physical_root.join("parent-alias");
        symlink(&real_parent, &parent_alias).expect("parent alias");

        let intermediate_db = parent_alias.join("intermediate.sqlite");
        let result = std::panic::catch_unwind(|| {
            SessionCommandState::new_headless(intermediate_db, real_app.clone())
        });
        let payload = match result {
            Ok(_) => panic!("intermediate symlink must fail"),
            Err(payload) => payload,
        };
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"failed to initialize session runtime")
        );
        let final_db = physical_root.join("final.sqlite");
        let result =
            std::panic::catch_unwind(|| SessionCommandState::new_headless(final_db, app_alias));
        let payload = match result {
            Ok(_) => panic!("final symlink must fail"),
            Err(payload) => payload,
        };
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"failed to initialize session runtime")
        );
    }

    #[test]
    fn relative_runtime_paths_are_made_absolute_without_canonicalization() {
        let relative = PathBuf::from("relative/runtime.sqlite");
        let absolute = lexical_absolute_path(&relative);
        assert!(absolute.is_absolute());
        assert!(absolute.ends_with(relative));
    }

    fn delivery_recovery_test_state(name: &str) -> (WorkspaceCommandState, tempfile::TempDir) {
        let root = tempfile::tempdir().expect("runtime test root");
        let physical_root = std::fs::canonicalize(root.path()).expect("physical test root");
        let db_path = physical_root.join(format!("{name}.sqlite"));
        let app_data = physical_root.join("app-data");
        let session = SessionCommandState::new_headless(db_path, app_data);
        (WorkspaceCommandState::from_session(&session), root)
    }

    fn delivery_recovery_test_snapshot(token: &str) -> WorkspaceDeliveryFailureSnapshot {
        WorkspaceDeliveryFailureSnapshot {
            attempt_token: token.to_string(),
            workspace_root: "/tmp/delivery-recovery-test".to_string(),
            branch: Some("work".to_string()),
            head_sha: Some("a".repeat(40)),
            operation: WorkspaceDeliveryFailureOperation::Push,
            classification: WorkspaceDeliveryFailureClassification::Transport,
            remote: Some("origin".to_string()),
            operation_target: None,
            push_target: None,
            output: "temporary failure".to_string(),
            output_truncated: false,
            changed_files: Vec::new(),
            changed_files_truncated: false,
            external_url: None,
            available_actions: Vec::new(),
            created_at: token.to_string(),
        }
    }

    #[test]
    fn delivery_recovery_claim_is_single_flight_for_same_token() {
        let (state, _root) = delivery_recovery_test_state("single-flight");
        state.record_delivery_failure(delivery_recovery_test_snapshot("attempt-1"));
        let first = state
            .claim_delivery_recovery(
                "/tmp/delivery-recovery-test",
                WorkspaceDeliveryFailureOperation::Push,
                "attempt-1",
            )
            .expect("first recovery claim");

        let state = Arc::new(state);
        let concurrent = Arc::clone(&state);
        let second = std::thread::spawn(move || {
            concurrent.claim_delivery_recovery(
                "/tmp/delivery-recovery-test",
                WorkspaceDeliveryFailureOperation::Push,
                "attempt-1",
            )
        })
        .join()
        .expect("recovery claimant thread");
        assert!(second.is_err());
        drop(first);
        assert!(state
            .claim_delivery_recovery(
                "/tmp/delivery-recovery-test",
                WorkspaceDeliveryFailureOperation::Push,
                "attempt-1",
            )
            .is_ok());
    }

    #[test]
    fn delivery_recovery_claim_clear_is_token_compare_and_swap() {
        let (state, _root) = delivery_recovery_test_state("clear-cas");
        state.record_delivery_failure(delivery_recovery_test_snapshot("attempt-old"));
        let claim = state
            .claim_delivery_recovery(
                "/tmp/delivery-recovery-test",
                WorkspaceDeliveryFailureOperation::Push,
                "attempt-old",
            )
            .expect("old recovery claim");

        let mut replacement = delivery_recovery_test_snapshot("attempt-new");
        replacement.output = "different temporary failure".to_string();
        state.record_delivery_failure(replacement);
        claim
            .clear_current_snapshot()
            .expect("clear old snapshot only");
        assert!(
            state
                .claim_delivery_recovery(
                    "/tmp/delivery-recovery-test",
                    WorkspaceDeliveryFailureOperation::Push,
                    "attempt-new",
                )
                .is_err(),
            "old claim must still serialize the replacement"
        );
        drop(claim);
        assert!(state
            .claim_delivery_recovery(
                "/tmp/delivery-recovery-test",
                WorkspaceDeliveryFailureOperation::Push,
                "attempt-new",
            )
            .is_ok());
    }

    #[test]
    fn poisoned_delivery_recovery_state_fails_closed() {
        let (state, _root) = delivery_recovery_test_state("poisoned");
        state.record_delivery_failure(delivery_recovery_test_snapshot("attempt-1"));
        let poisoned = Arc::clone(&state.delivery_failures);
        let _ = std::thread::spawn(move || {
            let _guard = poisoned.lock().expect("recovery state lock");
            panic!("poison recovery state for fail-closed test");
        })
        .join();
        assert!(state
            .claim_delivery_recovery(
                "/tmp/delivery-recovery-test",
                WorkspaceDeliveryFailureOperation::Push,
                "attempt-1",
            )
            .is_err());
    }

    #[test]
    fn browser_location_wrappers_delegate_to_the_shared_session_database() {
        let root = tempfile::tempdir().expect("browser location test root");
        let app_data = root.path().join("app-data");
        std::fs::create_dir_all(&app_data).expect("create app data");
        let db_path = physical_db_path(root.path().join("state.sqlite"));
        let state = SessionCommandState::new_headless(
            db_path,
            std::fs::canonicalize(app_data).expect("canonical app data"),
        );
        state
            .save_browser_location(
                "workspace",
                None,
                "https://example.test/workbench",
                100,
                200,
            )
            .expect("save browser location");
        assert_eq!(
            state
                .load_browser_location("workspace", None, 199)
                .expect("load browser location"),
            Some("https://example.test/workbench".to_string())
        );
        assert!(state
            .delete_browser_location("workspace", None)
            .expect("delete browser location"));
        assert_eq!(
            state
                .load_browser_location("workspace", None, 199)
                .expect("load deleted browser location"),
            None
        );
    }
}
