use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use dcc_core::{
    application::{
        list_turn_queue, mark_queued_turn_dispatched, prepare_session_for_turn,
        resolve_session_mcp_servers, send_turn as run_send_turn,
        send_turn_selection_differs_from_session, ResolveSessionMcpInput, SendTurnInput,
        StartThreadInput,
    },
    domain::{
        delegation::{Delegation, DelegationId, DelegationStatus},
        mcp::{
            mcp_oauth_resource_fingerprint, McpDefinitionId, McpErrorCategory, McpOauthGrant,
            McpRuntimeError, McpRuntimeState, McpRuntimeStatus, McpSecretReferenceId, McpTransport,
        },
        project::{Project, ProjectId},
        provider::{McpOauthSupport, ProviderEvent, ProviderId, SessionHandle},
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
        AppendEventOutcome, CredentialStore, DelegationRepo, EventBus, Input, McpRepo, ProjectRepo,
        Provider, ProviderMcpOauthStart, ProviderMcpServerConfig, ProviderRuntimeConfig,
        RepositoryRepo, SessionConfig, SessionEventRepo, SessionRepo, ThreadRepo, UsageRepo,
        WorkspaceBundleRepo, WorkspaceRepo,
    },
    Result,
};
use dcc_infra::{
    credential_store::SystemCredentialStore,
    db::{SqliteSessionRepo, SqliteWorkspaceRepo},
    mcp_db::SqliteMcpRepo,
};

use crate::turn_review::{
    capture_baseline, capture_result, cleanup_all_snapshot_quarantines, cleanup_snapshot,
    current_snapshot_matches, observed_validations_for_turn, GitTurnBaseline,
    TURN_REVIEW_CAPTURE_VERSION,
};

use crate::delivery_failure::{
    sanitize_delivery_failure_output, WorkspaceDeliveryFailureOperation,
    WorkspaceDeliveryFailureSnapshot,
};
use crate::events::TauriEventBus;
use crate::process_runtime_registry::{ProcessRuntime, ProcessRuntimeRegistry};
use crate::terminal_arbiter::{
    PersistThenCommitError, TerminalArbiterError, TerminalClaimResult, TerminalIntent, TerminalKey,
};
use dcc_providers::provider_runtime;

const DELIVERY_FAILURE_WORKSPACE_LIMIT: usize = 64;

type DeliveryFailureStore =
    HashMap<String, HashMap<WorkspaceDeliveryFailureOperation, WorkspaceDeliveryFailureSnapshot>>;

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

#[derive(Clone, Debug)]
pub struct WorkspaceCommandState {
    pub db_path: PathBuf,
    delivery_failures: Arc<Mutex<DeliveryFailureStore>>,
}

impl WorkspaceCommandState {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            delivery_failures: Arc::new(Mutex::new(HashMap::new())),
        }
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

        if !store.contains_key(&root) && store.len() >= DELIVERY_FAILURE_WORKSPACE_LIMIT {
            let oldest_root = store
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
                store.remove(&oldest_root);
            }
        }

        store
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
            .get_mut(root)
            .map(|operations| {
                operations.remove(&operation);
                operations.is_empty()
            })
            .unwrap_or(false);
        if remove_root {
            store.remove(root);
        }
    }

    pub(crate) fn clear_delivery_failures(&self, workspace_root: &str) {
        let Ok(mut store) = self.delivery_failures.lock() else {
            return;
        };
        store.remove(workspace_root.trim());
    }

    pub(crate) fn has_delivery_failure(&self, workspace_root: &str) -> bool {
        self.delivery_failures
            .lock()
            .ok()
            .and_then(|store| {
                store
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
            .get(workspace_root.trim())?
            .values()
            .filter(|failure| {
                failure.branch.as_deref() == branch && failure.head_sha.as_deref() == head_sha
            })
            .max_by(|left, right| left.created_at.cmp(&right.created_at))
            .cloned()
    }
}

#[derive(Clone)]
pub struct SessionCommandState {
    app_data_dir: PathBuf,
    db_path: PathBuf,
    session_repo: SqliteSessionRepo,
    _event_bus: Arc<dyn EventBus>,
    store: Arc<Mutex<SessionStore>>,
    runtime: Arc<ProcessRuntime>,
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
    projected_mcp_definition_ids: Arc<HashSet<McpDefinitionId>>,
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
    Passive,
    Quiesce,
    Cancel,
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
        if recover_interrupted {
            cleanup_all_snapshot_quarantines(&app_data_dir.join("turn-review").join("snapshots"));
            let _ = session_repo
                .recover_interrupted_turn_change_sets(&Utc::now().to_rfc3339())
                .unwrap_or_default();
        }
        Self {
            app_data_dir,
            session_repo,
            db_path,
            _event_bus: event_bus,
            store: runtime.session_store(),
            runtime,
        }
    }

    pub fn process_runtime(&self) -> Arc<ProcessRuntime> {
        Arc::clone(&self.runtime)
    }

    pub(crate) fn db_path(&self) -> &std::path::Path {
        &self.db_path
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

    fn provider_runtime_config(
        &self,
        provider_id: &str,
        runtime: Option<&ProviderRuntimeConfig>,
    ) -> Result<ProviderRuntimeConfig> {
        let runtime = runtime.cloned().unwrap_or_default();
        if self.is_legacy_managed_provider_home(provider_id, &runtime) {
            return Ok(ProviderRuntimeConfig::default());
        }
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
        let provider = provider_runtime(&provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!("unknown provider runtime: {provider_id}"))
        })?;
        if !provider.capabilities().supports_read_only_delegation {
            return Err(dcc_core::CoreError::InvalidInput(format!(
                "provider {provider_id} does not support read-only review runs"
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
        provider
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
            .await?;

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
        let _ = provider.cancel(&handle).await;
        let response = result??;
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
                });
            }
        };
        let mut snapshots = Vec::with_capacity(roots.len());
        for (workspace_id, root) in roots {
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
            if !keep_quarantine {
                cleanup_snapshot(&snapshot_root);
            }
        }
        Ok(M3BaselineCapture { snapshots })
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
        if matches!(outcome, AppendEventOutcome::Inserted(_)) {
            self.publish(core_event).await?;
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
                    return Ok((
                        Self::terminal_outcome(&existing)?,
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
                            if let Some(provider) = provider_runtime(&binding.provider_id) {
                                let _ = provider.cancel(&binding.handle).await;
                            }
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
                    if partial {
                        eprintln!("[DCC] aborted turn review capture failed: {error}");
                    } else {
                        return Err(error);
                    }
                }
                let append = self.append_session_event(session_id, kind).await?;
                let (record, inserted) = match append {
                    AppendEventOutcome::Inserted(record) => (record, true),
                    AppendEventOutcome::Existing(record) => (record, false),
                };
                Ok((
                    Self::terminal_outcome(&record)?,
                    TerminalPersistence { record, inserted },
                ))
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
                    self.publish(dcc_core::ports::events::CoreEvent::SessionTurnCompleted {
                        session_id: payload.record.session_id.0.clone(),
                        turn_id: turn_id.0.clone(),
                    })
                    .await
                }
                SessionEventKind::TurnAborted { turn_id, reason } => {
                    self.publish(dcc_core::ports::events::CoreEvent::SessionTurnAborted {
                        session_id: payload.record.session_id.0.clone(),
                        turn_id: turn_id.0.clone(),
                        reason: reason.clone(),
                    })
                    .await
                }
                _ => Ok(()),
            }
        } else {
            Ok(())
        };
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
            if matches!(outcome, AppendEventOutcome::Inserted(_))
                && self
                    .publish(
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
                        source: TerminalSource::Passive,
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
                    source: TerminalSource::Passive,
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
                    source: TerminalSource::Passive,
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

    pub async fn attach_provider_session(&self, session: &Session) -> Result<()> {
        if self.provider_binding(&session.id)?.is_some() {
            return Ok(());
        }

        let provider = provider_runtime(&session.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                session.provider_id
            ))
        })?;
        let (working_directory, additional_working_directories) = self
            .resolve_session_working_directories(
                session,
                provider.capabilities().supports_multi_root,
            )
            .await?;
        let provider_runtime =
            self.provider_runtime_config(&session.provider_id, session.provider_runtime.as_ref())?;
        let mcp_projection_version = provider.dcc_mcp_projection_version().map(str::to_string);
        let mcp_servers = self
            .resolve_provider_mcp_servers(session, provider.as_ref())
            .await?;
        let projected_definition_ids = mcp_servers
            .iter()
            .map(|server| server.definition_id.clone())
            .collect::<Vec<_>>();

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
        };

        let won_binding = {
            let mut store = self.lock_store()?;
            match store.provider_sessions.entry(session.id.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(binding.clone());
                    true
                }
                Entry::Occupied(_) => false,
            }
        };
        if !won_binding {
            // Another state attached the same session while this adapter was
            // preparing. Do not overwrite its binding or publish loser
            // statuses; dispose only the handle we prepared.
            let _ = provider.cancel(&handle).await;
            return Ok(());
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
        let current = self
            .peek_session(&input.session_id)
            .await?
            .ok_or_else(|| dcc_core::CoreError::Repository("session not found".to_string()))?;
        if send_turn_selection_differs_from_session(&current, input) {
            let _ = self.cancel_provider_session(&input.session_id).await;
        }
        let session = prepare_session_for_turn(self, input).await?;
        self.attach_provider_session(&session).await?;
        Ok(session)
    }

    pub fn session_mcp_oauth_support(&self, session_id: &SessionId) -> Result<McpOauthSupport> {
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
        Ok(provider.capabilities().mcp_oauth_support)
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

    pub async fn validate_start_thread_scope(&self, input: &StartThreadInput) -> Result<()> {
        if input.additional_workspace_ids.is_empty() {
            return Ok(());
        }
        let provider = provider_runtime(&input.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                input.provider_id
            ))
        })?;
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
            provider.capabilities().supports_multi_root,
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
                            if let Err(error) = state.dispatch_next_queued_turn(&session_id).await {
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

    /// Dispatch the oldest durable follow-up after a provider completes. The
    /// queue is projected from session events, so it survives UI remounts and
    /// app restarts instead of living in composer state.
    pub async fn dispatch_next_queued_turn(&self, session_id: &SessionId) -> Result<bool> {
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
        };
        let provider_input = dcc_core::ports::ProviderTurnInput {
            prompt: input.prompt.clone(),
            tool_instructions: input.tool_instructions.clone(),
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
        if let Err(error) = self
            .capture_turn_review_baseline(&output.session, &turn_id)
            .await
        {
            eprintln!("[DCC] queued turn review baseline persistence failed: {error}");
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
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        if !provider.capabilities().supports_steering {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} does not support steering an active turn",
                binding.provider_id
            )));
        }
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
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        if !provider.capabilities().supports_native_subagent_steering {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} does not support steering native subagents",
                binding.provider_id
            )));
        }
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
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        if !provider.capabilities().supports_native_subagent_interrupt {
            return Err(dcc_core::CoreError::Provider(format!(
                "provider {} does not support interrupting native subagents",
                binding.provider_id
            )));
        }
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
        let provider = provider_runtime(&binding.provider_id).ok_or_else(|| {
            dcc_core::CoreError::Provider(format!(
                "unknown provider runtime: {}",
                binding.provider_id
            ))
        })?;
        provider
            .start_mcp_oauth(&binding.handle, definition_id)
            .await
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
                let _ = provider.cancel(&binding.handle).await;
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
            {
                let mut store = self.lock_store()?;
                store.provider_sessions.remove(id);
            }
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
impl EventBus for SessionCommandState {
    async fn publish(&self, event: dcc_core::ports::events::CoreEvent) -> Result<()> {
        self.runtime.publish_event(event).await
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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
                source: TerminalSource::Passive,
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
}
